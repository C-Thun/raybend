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

/// 未命中项的并行读线程数上限（`ensure_dir`）。
///
/// 为什么是 6：读头是 I/O + 一点解析，核多也不线性——6 条已经能把 300 张的目录
/// 压到百毫秒级，再往上收益很小还容易跟缩略图解码抢核（崔总 2026-10-11 口径：
/// 「读文件信息这一步就该是多线程的」）。
pub const META_READ_THREADS: usize = 6;

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

/// 一次读头顺手留下的**整块 EXIF**（flowbar 的 `flowinfo` 用它）。
///
/// 崔总 2026-10-11：既然为了拍摄时间已经解析了 EXIF，就把**整块**留下 ——
/// 于是鼠标点/停到某张图上时，EXIF 信息**直接从缓存出**，不再读一次盘。
#[derive(Debug, Clone)]
pub struct CachedExif {
    pub data: crate::media::exif::ExifData,
    pub fields: Vec<crate::media::exif::ExifField>,
}

#[derive(Debug, Clone)]
struct CachedMeta {
    meta: PhotoMeta,
    /// 这条是**什么时候读的** —— 逐条年龄，比「整个目录一起过期」更精细
    read_at: Instant,
    /// 这条的整块 EXIF（可能没有：`cached_only` 时代或只写了时间的那半条）
    exif: Option<CachedExif>,
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
    /// `reader` 负责真的去读一个文件的头（测试里注入假实现）。**要能跨线程调用**：
    /// 未命中那部分会并行读（崔总 2026-10-11：读文件信息这一步就该是多线程的），
    /// 结果按入参顺序落回，调用方看不出差别。
    pub fn ensure_dir<F>(
        &mut self,
        dir: &Path,
        files: &[MetaInput],
        now: Instant,
        reader: F,
    ) -> Result<MetaBatch>
    where
        F: Fn(&Path) -> Result<PhotoMeta> + Sync,
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

        /*
         * ① 先分类：能复用的直接落地、缺的记下标（下标 = 并行读之后**保序**落回的钥匙）。
         *
         * 复用条件有两条：
         *   ① **指纹相同**（大小 + 修改时间没变）—— 平时就靠它，切来切去不重扫盘；
         *   ② **这条还没超龄**（逐条年龄 < TTL）—— 防「一个进程开好几天」时，
         *      指纹恰好不变的病态改动（同大小同时间换了内容）一直不被发现。
         */
        let mut metas: Vec<Option<PhotoMeta>> = vec![None; files.len()];
        let mut read_ats: Vec<Instant> = vec![now; files.len()];
        // 命中的那些，把它们的 EXIF 原样接着用（别因为这次「只要尺寸」就把它丢了）
        let mut carry: Vec<Option<CachedExif>> = vec![None; files.len()];
        let mut missing: Vec<usize> = Vec::new();
        for (index, file) in files.iter().enumerate() {
            let reusable = previous
                .get(&file.relative)
                .filter(|cached| {
                    file.matches(&cached.meta)
                        && now.duration_since(cached.read_at) < self.ttl
                        /*
                         * **半条元信息不算命中**：出图路径顺路写回的那条只有拍摄时间
                         *（宽高留 0，它只读了 EXIF、没去嗅探像素数，见 `thumbs.rs`）。
                         * 要尺寸的调用方（库内浏览的 `dir_meta_ensure`）必须照常读一次，
                         * 否则会把「不知道」当成 0×0 的真相。
                         */
                        && cached.meta.has_size()
                })
                .cloned();
            match reusable {
                Some(cached) => {
                    batch.cached_count += 1;
                    // **保留它原来是什么时候读的** —— 复用不等于刷新年龄，
                    // 否则「逐条 TTL」永远不会触发（这条被测试抓到过）
                    metas[index] = Some(cached.meta);
                    read_ats[index] = cached.read_at;
                    carry[index] = cached.exif;
                }
                None => missing.push(index),
            }
        }

        // ② 缺的那部分并行读（一两个就不值得开线程了）
        batch.read_count = missing.len();
        let threads = std::thread::available_parallelism()
            .map_or(1, std::num::NonZeroUsize::get)
            .clamp(1, META_READ_THREADS);
        if missing.len() >= 2 && threads > 1 {
            let results: std::sync::Mutex<Vec<(usize, Result<PhotoMeta>)>> =
                std::sync::Mutex::new(Vec::with_capacity(missing.len()));
            let slice_len = missing.len().div_ceil(threads);
            std::thread::scope(|scope| {
                for slice in missing.chunks(slice_len) {
                    let results = &results;
                    let reader = &reader;
                    scope.spawn(move || {
                        for &index in slice {
                            let meta = reader(&dir.join(&files[index].relative));
                            // 读失败也照收，由主线程按原来的方式返回 Err
                            if let Ok(mut guard) = results.lock() {
                                guard.push((index, meta));
                            }
                        }
                    });
                }
            });
            // 中毒也能取出数据（读头失败只是某一张读不到，不该把整批带走）
            let collected = results.into_inner().unwrap_or_else(|poisoned| poisoned.into_inner());
            for (index, meta) in collected {
                metas[index] = Some(meta?);
            }
        } else {
            for &index in &missing {
                metas[index] = Some(reader(&dir.join(&files[index].relative))?);
            }
        }

        // ③ 组装（顺序 = 入参顺序）
        for (index, file) in files.iter().enumerate() {
            let mut meta = metas[index].ok_or_else(|| {
                crate::error::Error::PathNotFound(dir.join(&file.relative))
            })?;
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
                CachedMeta {
                    meta,
                    read_at: read_ats[index],
                    exif: carry[index].take(),
                },
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

    /// 按**文件身份**取这一条的整块 EXIF（零 I/O）。
    ///
    /// `flowbar` 的 `flowinfo` 用它：鼠标点/停上去时 EXIF 直接从缓存出，
    /// 不再读一次盘（见 `specs/source-time-inline.md`）。
    /// 身份对不上（文件被换过）或这条没有 EXIF → `None`，调用方该读就自己读。
    #[must_use]
    pub fn exif_for(
        &mut self,
        dir: &Path,
        relative: &str,
        file_size: u64,
        mtime_ms: i64,
    ) -> Option<CachedExif> {
        self.tick += 1;
        let tick = self.tick;
        let entry = self.dirs.get_mut(dir)?;
        entry.used = tick;
        let cached = entry.metas.get(relative)?;
        if cached.meta.file_size != file_size || cached.meta.mtime_ms != mtime_ms {
            return None;
        }
        cached.exif.clone()
    }

    /// 忘掉一整个目录（拔盘再插回来时用：缓存里的东西未必还是盘上的东西）。
    pub fn forget_dir(&mut self, dir: &Path) {
        self.dirs.remove(dir);
    }

    /// **只查缓存**：命中就给、没命中就给「读不到」（宽高 0、时间 `None`）。
    ///
    /// 崔总 2026-10-11 的口径（`specs/source-time-inline.md`）：界面要时间就**只查缓存**，
    /// 绝不为了补时间再去读文件 —— 真相是**出图那一次读**顺手放回来的
    ///（[`Self::remember`]）：屏幕外的那些先保持兜底时间，等它出图时自然补上。
    pub fn cached_only(&mut self, dir: &Path, files: &[MetaInput], now: Instant) -> MetaBatch {
        self.tick += 1;
        let tick = self.tick;
        let previous = self
            .dirs
            .remove(dir)
            .map(|entry| entry.metas)
            .unwrap_or_default();
        let mut fresh: HashMap<String, CachedMeta> = HashMap::with_capacity(files.len());
        let mut batch = MetaBatch {
            metas: Vec::with_capacity(files.len()),
            read_count: 0,
            cached_count: 0,
        };
        for file in files {
            let hit = previous.get(&file.relative).filter(|cached| {
                file.matches(&cached.meta) && now.duration_since(cached.read_at) < self.ttl
            });
            match hit {
                Some(cached) => {
                    batch.cached_count += 1;
                    fresh.insert(file.relative.clone(), cached.clone());
                    batch.metas.push(cached.meta);
                }
                None => {
                    // 没缓存：如实报「不知道」，让界面用它的兜底值顶着
                    batch.metas.push(PhotoMeta {
                        file_size: file.file_size,
                        mtime_ms: file.mtime_ms,
                        ..PhotoMeta::default()
                    });
                }
            }
        }
        self.total_hits += u64::try_from(batch.cached_count).unwrap_or(0);
        self.dirs.insert(
            dir.to_path_buf(),
            DirEntry {
                used: tick,
                metas: fresh,
            },
        );
        self.evict();
        batch
    }

    /// **把一条元信息放回缓存**（出图那一次读顺手带回来的）。
    ///
    /// 这是「时间搭在出图的读上」的落点：渲染时空闲的 EXIF 解析结果在这里落袋，
    /// 于是界面稍后只查缓存就能拿到（[`Self::cached_only`]），不再有任何第二条读文件的线路。
    pub fn remember(
        &mut self,
        dir: &Path,
        relative: &str,
        meta: PhotoMeta,
        exif: Option<CachedExif>,
        now: Instant,
    ) {
        if relative.is_empty() {
            return;
        }
        self.tick += 1;
        let tick = self.tick;
        let entry = self.dirs.entry(dir.to_path_buf()).or_insert_with(|| DirEntry {
            used: tick,
            metas: HashMap::new(),
        });
        entry.used = tick;
        entry.metas.insert(
            relative.to_string(),
            CachedMeta {
                meta,
                read_at: now,
                exif,
            },
        );
        self.evict();
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
    use std::sync::atomic::{AtomicUsize, Ordering};
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
        calls: &'a AtomicUsize,
        fail_on: Option<&'a str>,
    ) -> impl Fn(&Path) -> crate::error::Result<PhotoMeta> + 'a {
        move |path: &Path| {
            calls.fetch_add(1, Ordering::SeqCst);
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
                taken_at_ms: None,
                taken_at_source: None,
                taken_at_offset_min: None,
            })
        }
    }

    #[test]
    fn 只有时间的半条元信息_对要尺寸的调用方不算命中() {
        use crate::media::exif::TakenAtSource;

        let mut cache = MetaCache::new();
        let dir = Path::new(r"D:\Photos\Half");
        let files = vec![input("a.jpg", 10, 100)];
        let now = Instant::now();
        // 出图那一刻顺手放回来的：只有时间，没有尺寸
        cache.remember(
            dir,
            "a.jpg",
            PhotoMeta {
                taken_at_ms: Some(1_234),
                taken_at_source: Some(TakenAtSource::Exif),
                file_size: 10,
                mtime_ms: 100,
                ..PhotoMeta::default()
            },
            None,
            now,
        );

        // 只查缓存：时间在 → 命中（导入界面要的就是它）
        let cached = cache.cached_only(dir, &files, now);
        assert_eq!(cached.cached_count, 1);
        assert_eq!(cached.metas[0].taken_at_ms, Some(1_234));

        // 要尺寸的调用方：这条不算命中，得老老实实读
        let read = std::sync::atomic::AtomicUsize::new(0);
        let batch = cache
            .ensure_dir(dir, &files, now, |_: &Path| {
                read.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(PhotoMeta {
                    width: 4000,
                    height: 3000,
                    orientation: 1,
                    file_size: 10,
                    mtime_ms: 100,
                    ..PhotoMeta::default()
                })
            })
            .expect("读得到");
        assert_eq!(
            read.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "半条元信息不许当成尺寸命中"
        );
        assert_eq!(batch.metas[0].width, 4000);
    }

    #[test]
    fn 只查缓存_出图放回来的能命中_没放的就不知道() {
        use crate::media::exif::TakenAtSource;

        let mut cache = MetaCache::new();
        let dir = Path::new(r"D:\Photos\CacheOnly");
        let files = vec![input("a.jpg", 10, 100), input("b.jpg", 20, 200)];
        let now = Instant::now();

        // 空缓存：两条都如实报「不知道」，而且**一次文件都不读**
        let empty = cache.cached_only(dir, &files, now);
        assert_eq!(empty.read_count, 0);
        assert_eq!(empty.cached_count, 0);
        assert_eq!(empty.metas[0].width, 0);
        assert_eq!(empty.metas[0].taken_at_ms, None);

        // 「出图那一次读」顺手把 a 放回来（真机上就是 `thumbs.rs` 的 remember）
        cache.remember(
            dir,
            "a.jpg",
            PhotoMeta {
                width: 4000,
                height: 3000,
                orientation: 1,
                taken_at_ms: Some(1_234),
                taken_at_source: Some(TakenAtSource::Exif),
                taken_at_offset_min: Some(480),
                file_size: 10,
                mtime_ms: 100,
            },
            None,
            now,
        );

        let hit = cache.cached_only(dir, &files, now);
        assert_eq!(hit.cached_count, 1);
        assert_eq!(hit.metas[0].taken_at_ms, Some(1_234));
        assert_eq!(hit.metas[0].taken_at_offset_min, Some(480));
        assert_eq!(hit.metas[1].width, 0, "还没出过图的那条仍是「不知道」");

        // 文件换了内容（大小或时间变了）→ 不命中，别把旧时间当新的用
        let changed = vec![input("a.jpg", 11, 100)];
        assert_eq!(
            cache.cached_only(dir, &changed, now).cached_count,
            0,
            "指纹变了就不该命中"
        );
    }

    #[test]
    fn 并行读保持入参顺序与长度() {
        // 5 张 = 会走并行那一条路（>=2 且线程数 >1）。
        // 每张按名字给不同的宽，顺序错了立刻看得出来。
        let mut cache = MetaCache::new();
        let dir = Path::new(r"D:\Photos\Par");
        let files: Vec<MetaInput> = (0..5)
            .map(|i| input(&format!("p{i}.jpg"), 10 + i as u64, 100))
            .collect();
        let batch = cache
            .ensure_dir(dir, &files, Instant::now(), |path: &Path| {
                let name = path
                    .file_name()
                    .expect("有文件名")
                    .to_string_lossy()
                    .into_owned();
                let index: u32 = name
                    .trim_start_matches('p')
                    .trim_end_matches(".jpg")
                    .parse()
                    .expect("测试文件名字长这样：p3.jpg");
                Ok(PhotoMeta {
                    width: 100 + index,
                    height: 50,
                    orientation: 1,
                    taken_at_ms: Some(1_700_000_000_000 + i64::from(index)),
                    taken_at_source: Some(crate::media::exif::TakenAtSource::Exif),
                    taken_at_offset_min: None,
                    file_size: 0,
                    mtime_ms: 0,
                })
            })
            .expect("读得到");
        assert_eq!(batch.read_count, 5);
        for (index, meta) in batch.metas.iter().enumerate() {
            assert_eq!(meta.width, 100 + index as u32, "第 {index} 条顺序不对");
            assert_eq!(meta.taken_at_ms, Some(1_700_000_000_000 + index as i64));
        }
    }

    #[test]
    fn 第二次调用全部命中_时间也一起命中() {
        let mut cache = MetaCache::new();
        let dir = Path::new(r"D:\Photos\Warm");
        let files = vec![input("a.jpg", 10, 100), input("b.jpg", 20, 200)];
        let read = |_: &Path| {
            Ok(PhotoMeta {
                width: 4000,
                height: 3000,
                orientation: 1,
                taken_at_ms: Some(1_700_000_000_000),
                taken_at_source: Some(crate::media::exif::TakenAtSource::Exif),
                taken_at_offset_min: Some(480),
                file_size: 0,
                mtime_ms: 0,
            })
        };
        let cold = cache
            .ensure_dir(dir, &files, Instant::now(), read)
            .expect("冷读");
        assert_eq!(cold.read_count, 2);
        let warm = cache
            .ensure_dir(dir, &files, Instant::now() + Duration::from_secs(1), read)
            .expect("热读");
        assert_eq!(warm.read_count, 0, "第二次一条都不该重读");
        assert_eq!(warm.cached_count, 2);
        assert_eq!(warm.metas[0].taken_at_ms, Some(1_700_000_000_000));
        assert_eq!(warm.metas[0].taken_at_offset_min, Some(480));
    }

    #[test]
    fn 首次全部读_再次全部命中() {
        let mut cache = MetaCache::new();
        let dir = Path::new(r"D:\Photos");
        let now = Instant::now();
        let files = vec![input("a.jpg", 10, 100), input("b.jpg", 20, 200)];

        let calls = AtomicUsize::new(0);
        let first = cache.ensure_dir(dir, &files, now, reader(&calls, None)).unwrap();
        assert_eq!(first.read_count, 2, "冷启动要读两条");
        assert_eq!(first.cached_count, 0);
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        let second = cache
            .ensure_dir(dir, &files, now + Duration::from_secs(60), reader(&calls, None))
            .unwrap();
        assert_eq!(second.read_count, 0, "同一批文件不该再读");
        assert_eq!(second.cached_count, 2);
        assert_eq!(calls.load(Ordering::SeqCst), 2, "总读取次数不该增加");
    }

    #[test]
    fn 只有变化的文件才重读() {
        let mut cache = MetaCache::new();
        let dir = Path::new(r"D:\Photos");
        let now = Instant::now();
        let before = vec![input("a.jpg", 10, 100), input("b.jpg", 20, 200)];
        let calls = AtomicUsize::new(0);
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
        let calls = AtomicUsize::new(0);
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
        let calls = AtomicUsize::new(0);
        cache
            .ensure_dir(
                dir,
                &[input("a.jpg", 10, 100), input("b.jpg", 20, 200)],
                now,
                reader(&calls, None),
            )
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);

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
        assert_eq!(calls.load(Ordering::SeqCst), 3);

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
        let calls = AtomicUsize::new(0);
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
        let calls = AtomicUsize::new(0);
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
        let before = calls.load(Ordering::SeqCst);
        cache
            .ensure_dir(
                &Path::new(r"D:\Photos").join("A"),
                &[input("a.jpg", 1, 1)],
                now + Duration::from_secs(10),
                reader(&calls, None),
            )
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), before + 1, "A 应当已经被淘汰，需要重读");
    }

    #[test]
    fn 总条目数超上限时也淘汰() {
        let mut cache = MetaCache::with_limits(Duration::from_secs(60), 100, 3);
        let now = Instant::now();
        let calls = AtomicUsize::new(0);
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
        let calls = AtomicUsize::new(0);
        cache.ensure_dir(dir, &[input("a.jpg", 10, 100)], now, reader(&calls, None)).unwrap();
        cache.ensure_dir(dir, &[input("a.jpg", 10, 100)], now, reader(&calls, None)).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "第二次命中");

        cache.invalidate_dir(dir);
        cache.ensure_dir(dir, &[input("a.jpg", 10, 100)], now, reader(&calls, None)).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2, "失效之后必须重读");
    }

    #[test]
    fn 空目录与未知尺寸都不炸() {
        let mut cache = MetaCache::new();
        let calls = AtomicUsize::new(0);
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
                        taken_at_ms: None,
                        taken_at_source: None,
                        taken_at_offset_min: None,
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
        let calls = AtomicUsize::new(0);
        let read_meta = |path: &Path| {
            calls.fetch_add(1, Ordering::SeqCst);
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            Ok(PhotoMeta {
                width: 4000,
                height: 3000,
                orientation: 1,
                file_size: 1024,
                mtime_ms: 1_700_000_000_000,
                taken_at_ms: None,
                taken_at_source: None,
                taken_at_offset_min: None,
            })
            .inspect(|_meta: &PhotoMeta| {
                let _ = name;
            })
        };

        let cold_start = Instant::now();
        let cold = cache.ensure_dir(dir, &files, now, read_meta).unwrap();
        let cold_ms = cold_start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(cold.read_count, N, "冷路径要读满 {N} 条");
        assert_eq!(calls.load(Ordering::SeqCst), N);

        let warm_start = Instant::now();
        let warm = cache
            .ensure_dir(dir, &files, now + Duration::from_secs(60), read_meta)
            .unwrap();
        let warm_ms = warm_start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(warm.read_count, 0, "热路径一次读头都不该发生");
        assert_eq!(warm.cached_count, N);
        assert_eq!(calls.load(Ordering::SeqCst), N, "读头次数不该增加");

        println!("[meta-cache 基准] {N} 条：冷 {cold_ms:.2} ms（含 1000 次假读头）、热 {warm_ms:.2} ms");
        // 宽松上限：热路径只是哈希查找，1000 条在几十微秒量级；给 200ms 只为防抖动
        assert!(
            warm_ms < 200.0,
            "热路径 {warm_ms:.2} ms 太慢了（应当只是查表）"
        );
    }
}
