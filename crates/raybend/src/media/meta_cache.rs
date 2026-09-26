//! 「浏览过的目录」的元信息**会话级内存缓存**。
//!
//! ## 为什么是内存、不是磁盘（2026-09-16 人类判断）
//!
//! 读头实测只要 **0.022 ms/张**（ext4；见 `media::meta` 的模块文档），为它落盘不划算。
//! 真正要避免的只有一件事：**反复切目录时重复扫盘**。所以：
//!
//! * 缓存活在本进程里，**退出即丢**；
//! * 每个目录带 **TTL（默认 6 小时）** —— 防的只是「一直不关应用」的极端情况；
//! * 过期 ≠ 全量重扫：过期后仍逐条比对 `(file_size, mtime_ms)`，**只有变了的才重读**；
//! * 容量双上限（目录数 / 总条目数），按最近使用淘汰，浏览几百个目录也不会无限涨。
//!
//! ## 线程模型
//!
//! 所有方法都是 `&mut self` —— 上层（`src-tauri`）包一层 `Mutex` 就能用，
//! 与 `SourcesThumbs` 的样式一致。时钟是**注入**的（`now: Instant`），
//! 这样 TTL 才测得动（与仓库里到处传 `now_ms` 的习惯一致）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::error::Result;
use crate::media::meta::PhotoMeta;

/// 默认 TTL：6 小时（人类指定）。
pub const DEFAULT_TTL: Duration = Duration::from_secs(6 * 60 * 60);
/// 默认最多缓存多少个目录（按最近使用淘汰）。
pub const DEFAULT_MAX_DIRS: usize = 256;
/// 默认最多缓存多少条照片元信息（超了就从最久没用的目录开始整目录淘汰）。
pub const DEFAULT_MAX_ENTRIES: usize = 200_000;

/// 调用方给出的一条「这个文件现在是什么样」——来自目录列表，不需要再读一次磁盘。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaInput {
    /// 目录内的相对名（非递归扫描下就是文件名）
    pub relative: String,
    /// 文件字节数
    pub file_size: u64,
    /// 修改时间（Unix 毫秒）
    pub mtime_ms: i64,
}

impl MetaInput {
    /// 与缓存里的那条是否「同一个文件」——**判据就是大小 + 修改时间**。
    #[must_use]
    fn matches(&self, cached: &PhotoMeta) -> bool {
        cached.file_size == self.file_size && cached.mtime_ms == self.mtime_ms
    }
}

/// 一次 `ensure_dir` 的结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetaBatch {
    /// 目录内每个文件当前的元信息（顺序与传入的 `files` 一致）
    pub metas: Vec<PhotoMeta>,
    /// 这次真的去读头的条数
    pub read_count: usize,
    /// 直接命中缓存的条数
    pub cached_count: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MetaCacheStats {
    pub dirs: usize,
    pub entries: usize,
    /// 累计读过多少个头（用于基准与排错）
    pub total_reads: u64,
    /// 累计命中多少次缓存
    pub total_hits: u64,
}

#[derive(Debug, Clone, Copy)]
struct CachedMeta {
    meta: PhotoMeta,
    /// 这条是**什么时候读的** —— 逐条年龄，比「整个目录一起过期」更精细
    read_at: Instant,
}

#[derive(Debug)]
struct DirEntry {
    /// 最近使用序号（越大越新）——淘汰用
    used: u64,
    metas: HashMap<String, CachedMeta>,
}

/// 会话级缓存本体。
#[derive(Debug)]
pub struct MetaCache {
    ttl: Duration,
    max_dirs: usize,
    max_entries: usize,
    tick: u64,
    dirs: HashMap<PathBuf, DirEntry>,
    total_reads: u64,
    total_hits: u64,
}

impl Default for MetaCache {
    fn default() -> Self {
        Self::new()
    }
}

impl MetaCache {
    #[must_use]
    pub fn new() -> Self {
        Self::with_limits(DEFAULT_TTL, DEFAULT_MAX_DIRS, DEFAULT_MAX_ENTRIES)
    }

    #[must_use]
    pub fn with_limits(ttl: Duration, max_dirs: usize, max_entries: usize) -> Self {
        Self {
            ttl,
            max_dirs: max_dirs.max(1),
            max_entries: max_entries.max(1),
            tick: 0,
            dirs: HashMap::new(),
            total_reads: 0,
            total_hits: 0,
        }
    }

    /// 确保 `dir` 下这批文件的元信息都是新鲜的，返回与 `files` **一一对应**的结果。
    ///
    /// `reader` 负责真的去读一个文件的头（测试里注入假实现）。
    pub fn ensure_dir<F>(
        &mut self,
        dir: &Path,
        files: &[MetaInput],
        now: Instant,
        mut reader: F,
    ) -> Result<MetaBatch>
    where
        F: FnMut(&Path) -> Result<PhotoMeta>,
    {
        self.tick += 1;
        let tick = self.tick;

        let mut batch = MetaBatch {
            metas: Vec::with_capacity(files.len()),
            read_count: 0,
            cached_count: 0,
        };
        // 先把旧表拿出来（避免与 self 的借用打架），处理完再放回去
        let previous = self
            .dirs
            .remove(dir)
            .map(|entry| entry.metas)
            .unwrap_or_default();
        let mut fresh: HashMap<String, CachedMeta> = HashMap::with_capacity(files.len());

        for file in files {
            /*
             * 复用条件有两条：
             *   ① **指纹相同**（大小 + 修改时间没变）—— 平时就靠它，切来切去不重扫盘；
             *   ② **这条还没超龄**（逐条年龄 < TTL）—— 防「一个进程开好几天」时，
             *      指纹恰好不变的病态改动（同大小同时间换了内容）一直不被发现。
             */
            let reusable = previous
                .get(&file.relative)
                .filter(|cached| {
                    file.matches(&cached.meta)
                        && now.duration_since(cached.read_at) < self.ttl
                })
                .copied();
            let (mut meta, read_at) = match reusable {
                Some(cached) => {
                    batch.cached_count += 1;
                    // **保留它原来是什么时候读的** —— 复用不等于刷新年龄，
                    // 否则「逐条 TTL」永远不会触发（这条被测试抓到过）
                    (cached.meta, cached.read_at)
                }
                None => {
                    batch.read_count += 1;
                    (reader(&dir.join(&file.relative))?, now)
                }
            };
            /*
             * **指纹字段一律以目录列表为准**：读头只负责「尺寸与方向」，
             * 大小/修改时间取自本次 `read_dir` 的结果 ——
             * 它是最新的，而且下次比对时用的也是它，两边天然一致。
             *
             * （一开始我把读头里的大小/时间原样存下来，纯函数测试立刻发现
             * 「比对的来源」和「存储的来源」是两个地方，迟早对不上。）
             */
            meta.file_size = file.file_size;
            meta.mtime_ms = file.mtime_ms;
            fresh.insert(
                file.relative.clone(),
                CachedMeta { meta, read_at },
            );
            batch.metas.push(meta);
        }

        self.total_reads += u64::try_from(batch.read_count).unwrap_or(0);
        self.total_hits += u64::try_from(batch.cached_count).unwrap_or(0);
        self.dirs.insert(
            dir.to_path_buf(),
            DirEntry {
                used: tick,
                metas: fresh,
            },
        );
        self.evict();
        Ok(batch)
    }

    /// 丢掉某个目录的缓存（外部改动后强制重读时用）。
    pub fn invalidate_dir(&mut self, dir: &Path) {
        self.dirs.remove(dir);
    }

    /// 清空（测试与「手动刷新」用）。
    pub fn clear(&mut self) {
        self.dirs.clear();
    }

    #[must_use]
    pub fn stats(&self) -> MetaCacheStats {
        MetaCacheStats {
            dirs: self.dirs.len(),
            entries: self.dirs.values().map(|entry| entry.metas.len()).sum(),
            total_reads: self.total_reads,
            total_hits: self.total_hits,
        }
    }

    /// 双上限淘汰：先按目录数，再按总条目数；都从**最久没用**的目录开始整目录丢。
    fn evict(&mut self) {
        while self.dirs.len() > self.max_dirs {
            self.drop_lru();
        }
        while self.entries() > self.max_entries && self.dirs.len() > 1 {
            self.drop_lru();
        }
    }

    fn entries(&self) -> usize {
        self.dirs.values().map(|entry| entry.metas.len()).sum()
    }

    fn drop_lru(&mut self) {
        let Some(oldest) = self
            .dirs
            .iter()
            .min_by_key(|(_, entry)| entry.used)
            .map(|(path, _)| path.clone())
        else {
            return;
        };
        self.dirs.remove(&oldest);
    }
}

#[cfg(test)]
mod tests {
    use super::{MetaCache, MetaInput};
    use crate::media::meta::{PhotoMeta, UNKNOWN_EDGE};
    use std::cell::Cell;
    use std::path::Path;
    use std::time::{Duration, Instant};

    fn input(name: &str, file_size: u64, mtime_ms: i64) -> MetaInput {
        MetaInput {
            relative: name.to_string(),
            file_size,
            mtime_ms,
        }
    }

    /// 假读头：计数 + 按文件名返回，便于断言「读了哪些」
    fn reader<'a>(
        calls: &'a Cell<usize>,
        fail_on: Option<&'a str>,
    ) -> impl FnMut(&Path) -> crate::error::Result<PhotoMeta> + 'a {
        move |path: &Path| {
            calls.set(calls.get() + 1);
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if fail_on == Some(name.as_str()) {
                return Err(crate::error::Error::PathNotFound(path.to_path_buf()));
            }
            Ok(PhotoMeta {
                width: 4000,
                height: 3000,
                orientation: 1,
                file_size: 111,
                mtime_ms: 222,
            })
        }
    }

    #[test]
    fn 首次全部读_再次全部命中() {
        let mut cache = MetaCache::new();
        let dir = Path::new(r"D:\Photos");
        let now = Instant::now();
        let files = vec![input("a.jpg", 10, 100), input("b.jpg", 20, 200)];

        let calls = Cell::new(0);
        let first = cache.ensure_dir(dir, &files, now, reader(&calls, None)).unwrap();
        assert_eq!(first.read_count, 2, "冷启动要读两条");
        assert_eq!(first.cached_count, 0);
        assert_eq!(calls.get(), 2);

        let second = cache
            .ensure_dir(dir, &files, now + Duration::from_secs(60), reader(&calls, None))
            .unwrap();
        assert_eq!(second.read_count, 0, "同一批文件不该再读");
        assert_eq!(second.cached_count, 2);
        assert_eq!(calls.get(), 2, "总读取次数不该增加");
    }

    #[test]
    fn 只有变化的文件才重读() {
        let mut cache = MetaCache::new();
        let dir = Path::new(r"D:\Photos");
        let now = Instant::now();
        let before = vec![input("a.jpg", 10, 100), input("b.jpg", 20, 200)];
        let calls = Cell::new(0);
        cache.ensure_dir(dir, &before, now, reader(&calls, None)).unwrap();

        // a 的修改时间变了；b 原样；c 是新文件
        let after = vec![
            input("a.jpg", 10, 999),
            input("b.jpg", 20, 200),
            input("c.jpg", 30, 300),
        ];
        let batch = cache
            .ensure_dir(dir, &after, now + Duration::from_secs(5), reader(&calls, None))
            .unwrap();
        assert_eq!(batch.read_count, 2, "a 变了、c 是新的 → 读两条");
        assert_eq!(batch.cached_count, 1, "b 命中");
        assert_eq!(batch.metas.len(), 3);
    }

    #[test]
    fn 消失的文件不再留在缓存里() {
        let mut cache = MetaCache::new();
        let dir = Path::new(r"D:\Photos");
        let now = Instant::now();
        let calls = Cell::new(0);
        cache
            .ensure_dir(dir, &[input("a.jpg", 10, 100), input("b.jpg", 20, 200)], now, reader(&calls, None))
            .unwrap();
        let batch = cache
            .ensure_dir(dir, &[input("a.jpg", 10, 100)], now, reader(&calls, None))
            .unwrap();
        assert_eq!(batch.metas.len(), 1);
        assert_eq!(cache.stats().entries, 1, "被删掉的 b 不该还占着条目");
    }

    #[test]
    fn ttl内只重读变了的_超龄则即使指纹相同也重读() {
        let ttl = Duration::from_secs(60);
        let mut cache = MetaCache::with_limits(ttl, 8, 100);
        let dir = Path::new(r"D:\Photos");
        let now = Instant::now();
        let calls = Cell::new(0);
        cache
            .ensure_dir(
                dir,
                &[input("a.jpg", 10, 100), input("b.jpg", 20, 200)],
                now,
                reader(&calls, None),
            )
            .unwrap();
        assert_eq!(calls.get(), 2);

        // TTL 之内：只有变了的 b 重读，a 复用（这正是「反复切目录不重扫盘」）
        let within = now + Duration::from_secs(30);
        let batch = cache
            .ensure_dir(
                dir,
                &[input("a.jpg", 10, 100), input("b.jpg", 20, 777)],
                within,
                reader(&calls, None),
            )
            .unwrap();
        assert_eq!(batch.read_count, 1, "只有 b 要重读");
        assert_eq!(batch.cached_count, 1);
        assert_eq!(calls.get(), 3);

        // 超出 TTL：即使 a 的指纹没变，也要重读一次（保险阀：防「同大小同时间换了内容」）
        let beyond = now + ttl + Duration::from_secs(1);
        let batch = cache
            .ensure_dir(
                dir,
                &[input("a.jpg", 10, 100), input("b.jpg", 20, 777)],
                beyond,
                reader(&calls, None),
            )
            .unwrap();
        /*
         * 超龄的只有 a（它是 t=0 读的，到 t=61 已超过 60s 的 TTL）——
         * 尽管它的指纹一直没变，也要重读一次：这就是保险阀的意义。
         * b 在 t=30 刚重读过，年龄还在 TTL 内，可以继续复用。
         */
        assert_eq!(batch.read_count, 1, "超龄的 a 即使指纹相同也要重读");
        assert_eq!(batch.cached_count, 1, "还年轻的 b 继续复用");
    }

    #[test]
    fn 读头失败会冒出来_不会静默给出错的元信息() {
        let mut cache = MetaCache::new();
        let dir = Path::new(r"D:\Photos");
        let calls = Cell::new(0);
        let result = cache.ensure_dir(
            dir,
            &[input("bad.jpg", 10, 100)],
            Instant::now(),
            reader(&calls, Some("bad.jpg")),
        );
        assert!(result.is_err(), "读不出来要冒错，别伪造");
    }

    #[test]
    fn 目录数超上限时淘汰最久没用的() {
        let mut cache = MetaCache::with_limits(Duration::from_secs(60), 2, 100);
        let now = Instant::now();
        let calls = Cell::new(0);
        for (index, name) in ["A", "B", "C"].iter().enumerate() {
            let dir = Path::new(r"D:\Photos").join(name);
            cache
                .ensure_dir(
                    &dir,
                    &[input("a.jpg", 1, 1)],
                    now + Duration::from_secs(index as u64),
                    reader(&calls, None),
                )
                .unwrap();
        }
        assert_eq!(cache.stats().dirs, 2, "上限是 2 个目录");

        // A 最久没用 → 已经被丢掉；重新问 A 会再读一次
        let before = calls.get();
        cache
            .ensure_dir(
                &Path::new(r"D:\Photos").join("A"),
                &[input("a.jpg", 1, 1)],
                now + Duration::from_secs(10),
                reader(&calls, None),
            )
            .unwrap();
        assert_eq!(calls.get(), before + 1, "A 应当已经被淘汰，需要重读");
    }

    #[test]
    fn 总条目数超上限时也淘汰() {
        let mut cache = MetaCache::with_limits(Duration::from_secs(60), 100, 3);
        let now = Instant::now();
        let calls = Cell::new(0);
        let files: Vec<MetaInput> = (0..3).map(|i| input(&format!("{i}.jpg"), 1, 1)).collect();
        cache
            .ensure_dir(Path::new(r"D:\A"), &files, now, reader(&calls, None))
            .unwrap();
        cache
            .ensure_dir(
                Path::new(r"D:\B"),
                &files,
                now + Duration::from_secs(1),
                reader(&calls, None),
            )
            .unwrap();
        assert!(cache.stats().entries <= 3, "总条目数不该超过上限");
        assert_eq!(cache.stats().dirs, 1, "整目录淘汰，保留最近用的那个");
    }

    #[test]
    fn 手动失效后要重读() {
        let mut cache = MetaCache::new();
        let dir = Path::new(r"D:\Photos");
        let now = Instant::now();
        let calls = Cell::new(0);
        cache.ensure_dir(dir, &[input("a.jpg", 10, 100)], now, reader(&calls, None)).unwrap();
        cache.ensure_dir(dir, &[input("a.jpg", 10, 100)], now, reader(&calls, None)).unwrap();
        assert_eq!(calls.get(), 1, "第二次命中");

        cache.invalidate_dir(dir);
        cache.ensure_dir(dir, &[input("a.jpg", 10, 100)], now, reader(&calls, None)).unwrap();
        assert_eq!(calls.get(), 2, "失效之后必须重读");
    }

    #[test]
    fn 空目录与未知尺寸都不炸() {
        let mut cache = MetaCache::new();
        let calls = Cell::new(0);
        let empty = cache
            .ensure_dir(Path::new(r"D:\Empty"), &[], Instant::now(), reader(&calls, None))
            .unwrap();
        assert_eq!(empty.metas.len(), 0);
        assert_eq!(empty.read_count, 0);

        // 读不出来尺寸的文件：缓存里如实存「0×0」，不要假装有尺寸
        let mut cache2 = MetaCache::new();
        let batch = cache2
            .ensure_dir(
                Path::new(r"D:\X"),
                &[input("weird.jpg", 5, 5)],
                Instant::now(),
                |_: &Path| {
                    Ok(PhotoMeta {
                        width: 0,
                        height: 0,
                        orientation: 1,
                        file_size: 5,
                        mtime_ms: 5,
                    })
                },
            )
            .unwrap();
        assert_eq!(batch.metas[0].width, UNKNOWN_EDGE);
    }

    // ---------- 基准（留在测试里，跑得快、还能防回归）----------

    #[test]
    fn 一千条规模_热路径的开销可以忽略() {
        /*
         * 基准的意义（`specs/photo-meta-and-tile-display.md` §六 step 9）：
         * 「首次读头」与「会话内二次进入」差多少。
         *
         * 真机读头的数字在计划里实测过（ext4 0.022ms/张、9p 挂载 5.35ms/张），
         * 这里量的是**缓存本身**：冷路径必须调 1000 次读头、热路径必须一次都不调，
         * 且热路径的墙钟开销要小到可以忽略（给它一个宽松上限，避免偶发抖动误报）。
         */
        const N: usize = 1000;
        let mut cache = MetaCache::new();
        let dir = Path::new(r"D:\Photos\Big");
        let now = Instant::now();
        let files: Vec<MetaInput> = (0..N)
            .map(|index| input(&format!("P{index:05}.JPG"), 1024, 1_700_000_000_000))
            .collect();
        let calls = Cell::new(0);
        let read_meta = |path: &Path| {
            calls.set(calls.get() + 1);
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            Ok(PhotoMeta {
                width: 4000,
                height: 3000,
                orientation: 1,
                file_size: 1024,
                mtime_ms: 1_700_000_000_000,
            })
            .inspect(|_meta: &PhotoMeta| {
                let _ = name;
            })
        };

        let cold_start = Instant::now();
        let cold = cache.ensure_dir(dir, &files, now, read_meta).unwrap();
        let cold_ms = cold_start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(cold.read_count, N, "冷路径要读满 {N} 条");
        assert_eq!(calls.get(), N);

        let warm_start = Instant::now();
        let warm = cache
            .ensure_dir(dir, &files, now + Duration::from_secs(60), read_meta)
            .unwrap();
        let warm_ms = warm_start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(warm.read_count, 0, "热路径一次读头都不该发生");
        assert_eq!(warm.cached_count, N);
        assert_eq!(calls.get(), N, "读头次数不该增加");

        println!("[meta-cache 基准] {N} 条：冷 {cold_ms:.2} ms（含 1000 次假读头）、热 {warm_ms:.2} ms");
        // 宽松上限：热路径只是哈希查找，1000 条在几十微秒量级；给 200ms 只为防抖动
        assert!(
            warm_ms < 200.0,
            "热路径 {warm_ms:.2} ms 太慢了（应当只是查表）"
        );
    }
}
