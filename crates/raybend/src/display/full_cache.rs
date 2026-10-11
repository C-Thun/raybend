//! **库内大图缓存**：`<库根>/cache/full/<asset_id>/<issue>-<base>-v<pipeline>.avif`。
//!
//! # 两档（2026-10-11 加 `Full`）
//!
//! | 档 | 用途 | 扩展名 | 体积 |
//! | --- | --- | --- | --- |
//! | [`CacheClass::Preview`] | **载入占位**（长边 1920） | `.avif` | 几百 KB |
//! | [`CacheClass::Full`] | **看图真身**（该变体自己的像素） | `.jpg`（q92） | ~9 MB / 20MP |
//!
//! 两档同目录、同命名前缀（`latest-` / `issue-<id>-`），只差文件名里的 `-full` 与扩展名 ——
//! 所以失效函数按**前缀**删时两档一起走，不会漏掉真身档。
//!
//! # 为什么放在库里（而不是 app data）
//!
//! 人类 2026-09-24 定：大图缓存跟着**库**走 —— 库搬到哪、换台机器、接外置盘，
//! 打开就有图，不用重新渲染一遍。代价是它会被用户看见（`cache/` 是库根下一个普通目录），
//! 所以目录名取得直白、文件可读。
//!
//! 缩略图（网格 / 胶片带）仍走 `thumbs.db`（`AGENTS.md` §6.5）—— 那是几万行的小 BLOB，
//! 塞进 SQLite 更合适；这里放的是**每张几百 KB 的大图**。
//!
//! # 命名与失效
//!
//! ```text
//! <库根>/cache/full/1234/latest-raw-v6.avif    ← 基于 RAW 编辑的结果
//! <库根>/cache/full/1234/latest-sooc-v6.avif   ← 基于 SOOC 编辑的结果
//! <库根>/cache/full/1234/issue-42-<hash>-raw-v6.avif ← 定稿 42 的独立快照
//! ```
//!
//! * `issue` —— `latest` 或 `issue-<id>-<profile_hash>`；SOOC 不进缓存；
//! * **`base` —— 编辑基准**（`sooc` / `raw`）：同一张照片在两种基准下渲染出的是
//!   **两张不同的图**（人类 2026-09-24 让基准可切），所以它必须进文件名 ——
//!   否则切了基准之后读到的是另一基准渲染出来的旧图，而且**看不出是错的**
//!   （`memory/FUNCTION-IMAGING.md` §4.3 登记了这个坑）；
//! * `v<pipeline>` —— 渲染管线版本（`PIPELINE_VERSION`）。算法一改，旧文件自动变孤儿，
//!   与缩略图那套 `render_sig` 同一个思路；
//! * 编辑参数**不进文件名**：编辑一次覆盖同一个 `latest-<base>-*.avif`，
//!   省掉「按参数散列堆积成千上万份过期大图」这个麻烦。
//!
//! 基准是从**源文件**推出来的（[`EditBase::of_file`]：RAW → `raw`，其余 → `sooc`），
//! 不靠调用方自己传 —— 渲染用的哪个文件，就是哪个基准。
//!
//! # 它是**派生数据**（红线不变）
//!
//! 整个 `cache/` 目录删掉：功能降级（重新渲染一遍），但完全可用（`AGENTS.md` §6.5）。

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::error::Result;
use crate::store::develop::EditBase;

/// 库根下缓存目录的名字（用户看得见，起个直白的）。
pub const CACHE_DIR: &str = "cache";
/// 大图缓存所在的子目录（与将来的其它缓存分开）。
pub const FULL_DIR: &str = "full";

/// 真身档的总量上限：**5GB 或可用空间 5% 中取小者**（`specs/view-true-image.md` §0.4）。
///
/// 单张 20MP 真身档 ~9MB，5GB ≈ 500–600 张；可用空间紧张时自动收紧。
/// 一处常量，改它就行（崔总 2026-10-11：「设置一个范围，滚动清除更新即可」）。
pub const FULL_CACHE_LIMIT_BYTES: u64 = 5 * 1024 * 1024 * 1024;
/// 可用空间的占比上限（与 [`FULL_CACHE_LIMIT_BYTES`] 取小）。
pub const FULL_CACHE_LIMIT_SHARE: f64 = 0.05;
/// 容量清扫的最小间隔（进程内；不进看图热路径）。
const SWEEP_INTERVAL: Duration = Duration::from_secs(300);
/// 命中缓存时多久摸一次 mtime —— mtime 是 LRU 的近似（Windows 的「最后访问时间」默认关着）。
const TOUCH_AFTER: Duration = Duration::from_secs(3600);

/// 缓存条目属于哪一档（决定扩展名与容量清扫的删除优先级）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheClass {
    /// 载入占位（长边 1920，AVIF）。
    Preview,
    /// 看图真身（该变体自己的像素，JPEG q92）。
    Full,
}

impl CacheClass {
    /// 文件扩展名。
    #[must_use]
    pub const fn ext(self) -> &'static str {
        match self {
            Self::Preview => "avif",
            Self::Full => "jpg",
        }
    }

    /// 文件名里的档标记（Preview 不带，保持旧文件可读）。
    #[must_use]
    const fn marker(self) -> &'static str {
        match self {
            Self::Preview => "",
            Self::Full => "-full",
        }
    }

    /// 从文件名反推档位（容量清扫要按档分优先级）。
    #[must_use]
    fn of_file_name(name: &str) -> Self {
        if name.contains("-full-v") {
            Self::Full
        } else {
            Self::Preview
        }
    }

    /// 容量清扫的删除优先级：真身档先删（它最大，且删了只是重算一张）。
    const fn sweep_rank(self) -> u8 {
        match self {
            Self::Full => 0,
            Self::Preview => 1,
        }
    }
}

/// 库内大图缓存。
#[derive(Debug, Clone)]
pub struct FullCache {
    /// `<库根>/cache/full`
    root: PathBuf,
}

impl FullCache {
    /// 打开（必要时建目录）。
    ///
    /// # Errors
    /// 目录建不出来（权限、只读盘）。
    pub fn open(repo_root: &Path) -> Result<Self> {
        let root = repo_root.join(CACHE_DIR).join(FULL_DIR);
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// 缓存根目录（诊断 / 设置面板用）。
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// latest / 定稿共享的源版本命名规则；迟到结果写到旧版本，后续请求不会命中。
    #[must_use]
    pub fn source_name(issue: &str, signature: &str) -> String {
        format!("{issue}-src{signature}")
    }

    /// 某个资产某个 issue **某个编辑基准**的缓存文件路径（占位档，向后兼容）。
    #[must_use]
    pub fn path_for(
        &self,
        asset_id: i64,
        issue: &str,
        base: EditBase,
        pipeline_version: u32,
    ) -> PathBuf {
        self.path_for_class(asset_id, issue, base, CacheClass::Preview, pipeline_version)
    }

    /// 同上，带档位（真身档文件名里多一个 `-full`，扩展名 `.jpg`）。
    #[must_use]
    pub fn path_for_class(
        &self,
        asset_id: i64,
        issue: &str,
        base: EditBase,
        class: CacheClass,
        pipeline_version: u32,
    ) -> PathBuf {
        self.root.join(asset_id.to_string()).join(format!(
            "{issue}-{}{}-v{pipeline_version}.{}",
            base.as_str(),
            class.marker(),
            class.ext()
        ))
    }

    /// 读缓存（不存在 / 读不动都返回 `None` —— 缓存不该让界面报错）。
    #[must_use]
    pub fn read(
        &self,
        asset_id: i64,
        issue: &str,
        base: EditBase,
        pipeline_version: u32,
    ) -> Option<Vec<u8>> {
        self.read_class(asset_id, issue, base, CacheClass::Preview, pipeline_version)
    }

    /// 读缓存（带档位）。命中时**顺手把 mtime 摸新**（限每小时一次）——
    /// 容量清扫靠它当 LRU 的近似：Windows 默认不记「最后访问时间」，只有 mtime 靠得住。
    #[must_use]
    pub fn read_class(
        &self,
        asset_id: i64,
        issue: &str,
        base: EditBase,
        class: CacheClass,
        pipeline_version: u32,
    ) -> Option<Vec<u8>> {
        let path = self.path_for_class(asset_id, issue, base, class, pipeline_version);
        let bytes = std::fs::read(&path).ok()?;
        if bytes.is_empty() {
            return None;
        }
        touch_if_stale(&path);
        Some(bytes)
    }

    /// 写缓存（**原子**：先写 `.tmp` 再改名，避免半个文件被读到）。
    ///
    /// # Errors
    /// 目录建不出来 / 写不动（磁盘满、只读盘）。**调用方可以忽略** ——
    /// 缓存写失败只意味着下次还要再渲染一遍。
    pub fn write(
        &self,
        asset_id: i64,
        issue: &str,
        base: EditBase,
        pipeline_version: u32,
        data: &[u8],
    ) -> Result<()> {
        self.write_class(asset_id, issue, base, CacheClass::Preview, pipeline_version, data)
    }

    /// 写缓存（带档位）。
    ///
    /// # Errors
    /// 同 [`Self::write`]。
    pub fn write_class(
        &self,
        asset_id: i64,
        issue: &str,
        base: EditBase,
        class: CacheClass,
        pipeline_version: u32,
        data: &[u8],
    ) -> Result<()> {
        let path = self.path_for_class(asset_id, issue, base, class, pipeline_version);
        crate::fs_atomic::write(&path, data)
    }

    /// 删掉某个资产的全部大图缓存（**编辑落库后调**：旧结果立刻作废）。
    ///
    /// **两个基准一起删**：编辑栈是共用的，改了参数两种基准的渲染都过期了。
    ///
    /// 返回删掉几个文件。删不掉不算错（下次写会覆盖）。
    pub fn invalidate(&self, asset_id: i64) -> usize {
        let dir = self.root.join(asset_id.to_string());
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return 0;
        };
        let mut removed = 0;
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with("latest-")
                && std::fs::remove_file(entry.path()).is_ok()
            {
                removed += 1;
            }
        }
        let _ = std::fs::remove_dir(&dir);
        removed
    }

    /// **按资产、按档位保留最新 `keep` 份 `latest-*`**（按 mtime，从新到旧；其余删掉）。
    ///
    /// 分档保留是必须的（2026-10-11）：占位档与真身档都在 `latest-` 前缀下，
    /// 若按文件数一刀切，一张新真身档就会把还没用过的占位档挤掉（占位档是
    /// 「秒开」的那一半，不能为了省几百 KB 把它删了）。
    /// **不进热路径**：写新快照之后顺手做。
    /// 返回删掉几个文件；删不掉不算错。
    #[must_use]
    pub fn gc_latest(&self, asset_id: i64, keep: usize) -> usize {
        let dir = self.root.join(asset_id.to_string());
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return 0;
        };
        let mut by_class: [Vec<(SystemTime, PathBuf)>; 2] = [Vec::new(), Vec::new()];
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with("latest-") {
                continue;
            }
            let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) else {
                continue;
            };
            let class = CacheClass::of_file_name(&name);
            by_class[class.sweep_rank() as usize].push((modified, entry.path()));
        }
        let mut removed = 0;
        for group in &mut by_class {
            group.sort_by(|a, b| b.0.cmp(&a.0));
            for (_, path) in group.drain(..).skip(keep) {
                if std::fs::remove_file(path).is_ok() {
                    removed += 1;
                }
            }
        }
        removed
    }

    /// 源文件变化使所有派生图失效；profile 与 issue 本身保持不变。
    ///
    /// **不限扩展名**（2026-10-11）：真身档是 `.jpg`，只看 `.avif` 会漏删；
    /// 这个函数的语义是「该资产的派生图全不要了」，本来就与格式无关。
    pub fn invalidate_source(&self, asset_id: i64) -> Result<usize> {
        let dir = self.root.join(asset_id.to_string());
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e.into()),
        };
        let mut removed = 0;
        for entry in entries {
            let entry = entry?;
            if entry.path().is_file() {
                std::fs::remove_file(entry.path())?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// 只删除一个定稿的预览；latest 编辑不能触碰其它定稿的快照。
    pub fn remove_issue(&self, asset_id: i64, issue_id: i64) -> usize {
        let dir = self.root.join(asset_id.to_string());
        let prefix = format!("issue-{issue_id}-");
        let Ok(entries) = std::fs::read_dir(dir) else {
            return 0;
        };
        entries
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
            .filter(|entry| std::fs::remove_file(entry.path()).is_ok())
            .count()
    }

    /// 清掉整个大图缓存（设置里的「清理缓存」）。返回删掉几个文件。
    pub fn clear(&self) -> usize {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return 0;
        };
        let mut removed = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Ok(inner) = std::fs::read_dir(&path) {
                    for file in inner.flatten() {
                        if std::fs::remove_file(file.path()).is_ok() {
                            removed += 1;
                        }
                    }
                }
                let _ = std::fs::remove_dir(&path);
            } else if std::fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
        removed
    }

    /// 当前占用字节数（诊断 / 容量清扫的门槛判断）。
    #[must_use]
    pub fn used_bytes(&self) -> u64 {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return 0;
        };
        let mut total: u64 = 0;
        for entry in entries.flatten() {
            let Ok(inner) = std::fs::read_dir(entry.path()) else {
                continue;
            };
            for file in inner.flatten() {
                if let Ok(meta) = file.metadata() {
                    total = total.saturating_add(meta.len());
                }
            }
        }
        total
    }

    /// **滚动清扫**：总量超过 `max_bytes` 就按「真身档优先、最旧优先」删到上限以内。
    ///
    /// 运行时机由 [`Self::maybe_enforce_capacity`] 控制（写缓存之后、有间隔地跑），
    /// **不进看图热路径**。返回删掉几个文件。
    pub fn enforce_capacity(&self, max_bytes: u64) -> usize {
        let Ok(assets) = std::fs::read_dir(&self.root) else {
            return 0;
        };
        let mut files: Vec<(u8, SystemTime, u64, PathBuf)> = Vec::new();
        let mut total = 0u64;
        for asset in assets.flatten() {
            let Ok(entries) = std::fs::read_dir(asset.path()) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(meta) = entry.metadata() else { continue };
                if !meta.is_file() {
                    continue;
                }
                let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                total = total.saturating_add(meta.len());
                files.push((
                    CacheClass::of_file_name(&entry.file_name().to_string_lossy()).sweep_rank(),
                    modified,
                    meta.len(),
                    entry.path(),
                ));
            }
        }
        if total <= max_bytes {
            return 0;
        }
        // 真身档先删（`sweep_rank` 小的在前），同档内最旧的先删。
        files.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        let mut removed = 0;
        for (_, _, size, path) in files {
            if total <= max_bytes {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(size);
                removed += 1;
            }
        }
        removed
    }

    /// 写缓存之后顺手跑一次清扫，但**有间隔**（进程内 5 分钟一次）。
    ///
    /// 上限 = `min(5GB, 可用空间 5%)`：可用空间拿不到（网络盘 / 怪 FS）就只用 5GB。
    /// 返回删掉几个文件（没跑清扫就是 0）。
    pub fn maybe_enforce_capacity(&self) -> usize {
        {
            let mut last = match LAST_SWEEP.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            let now = SystemTime::now();
            if let Some(previous) = *last
                && now
                    .duration_since(previous)
                    .is_ok_and(|elapsed| elapsed < SWEEP_INTERVAL)
            {
                return 0;
            }
            *last = Some(now);
        }
        let limit = crate::import::fsops::free_bytes_of(&self.root)
            .map_or(FULL_CACHE_LIMIT_BYTES, |free| {
                let share = (free as f64 * FULL_CACHE_LIMIT_SHARE) as u64;
                FULL_CACHE_LIMIT_BYTES.min(share)
            });
        self.enforce_capacity(limit)
    }
}

/// 进程内「上次清扫」时间（不跨进程；库能被多个会话打开，但清扫是幂等的）。
static LAST_SWEEP: std::sync::Mutex<Option<SystemTime>> = std::sync::Mutex::new(None);

/// 命中缓存时把 mtime 摸新（限 [`TOUCH_AFTER`] 一次）—— 给容量清扫当 LRU 用。
///
/// 失败就算了（只读盘 / 权限）：触不到时间只是让淘汰顺序糙一点。
fn touch_if_stale(path: &Path) {
    let Ok(meta) = path.metadata() else { return };
    let stale = meta
        .modified()
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age > TOUCH_AFTER);
    if !stale {
        return;
    }
    if let Ok(file) = std::fs::OpenOptions::new().write(true).open(path) {
        let _ = file.set_modified(SystemTime::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache() -> (tempfile::TempDir, FullCache) {
        let dir = tempfile::tempdir().expect("临时目录");
        let cache = FullCache::open(dir.path()).expect("建缓存");
        (dir, cache)
    }

    #[test]
    fn open_creates_the_directory_inside_the_repository() {
        let (dir, cache) = cache();
        assert!(
            dir.path().join("cache").join("full").is_dir(),
            "目录要建出来"
        );
        assert_eq!(cache.root(), dir.path().join("cache").join("full"));
    }

    #[test]
    fn write_then_read_round_trips_and_is_atomic() {
        let (_dir, cache) = cache();
        assert!(
            cache.read(7, "latest", EditBase::Raw, 6).is_none(),
            "一开始没有"
        );
        cache
            .write(7, "latest", EditBase::Raw, 6, b"hello avif")
            .expect("写");
        assert_eq!(
            cache.read(7, "latest", EditBase::Raw, 6).as_deref(),
            Some(&b"hello avif"[..])
        );
        // 临时文件不许留下
        let leftovers: Vec<_> = std::fs::read_dir(cache.root().join("7"))
            .expect("目录")
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "tmp"))
            .collect();
        assert!(leftovers.is_empty(), "原子写不该留下 .tmp");
    }

    #[test]
    fn versions_issues_and_bases_do_not_collide() {
        let (_dir, cache) = cache();
        cache
            .write(1, "latest", EditBase::Raw, 6, b"v6")
            .expect("写");
        cache.write(1, "raw", EditBase::Raw, 6, b"raw").expect("写");
        cache
            .write(1, "latest", EditBase::Raw, 7, b"v7")
            .expect("写");
        assert_eq!(
            cache.read(1, "latest", EditBase::Raw, 6).as_deref(),
            Some(&b"v6"[..])
        );
        assert_eq!(
            cache.read(1, "raw", EditBase::Raw, 6).as_deref(),
            Some(&b"raw"[..])
        );
        assert_eq!(
            cache.read(1, "latest", EditBase::Raw, 7).as_deref(),
            Some(&b"v7"[..])
        );
        // 不同资产互不影响
        assert!(cache.read(2, "latest", EditBase::Raw, 6).is_none());
    }

    /// **两个编辑基准各存一份**（人类 2026-09-24 让基准可切之后必须成立）：
    /// 同一张照片、同一个 issue、同一个管线版本，基准不同就是两张不同的图。
    #[test]
    fn edit_bases_have_their_own_files() {
        let (_dir, cache) = cache();
        cache
            .write(9, "latest", EditBase::Raw, 6, b"from raw")
            .expect("写");
        cache
            .write(9, "latest", EditBase::Sooc, 6, b"from sooc")
            .expect("写");
        assert_eq!(
            cache.read(9, "latest", EditBase::Raw, 6).as_deref(),
            Some(&b"from raw"[..]),
            "切基准不许读到另一基准的图"
        );
        assert_eq!(
            cache.read(9, "latest", EditBase::Sooc, 6).as_deref(),
            Some(&b"from sooc"[..])
        );
        // 文件名里两个基准都要看得见（用户直接看 cache/ 目录）
        let names: Vec<String> = std::fs::read_dir(cache.root().join("9"))
            .expect("目录")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.contains(&"latest-raw-v6.avif".to_string()),
            "{names:?}"
        );
        assert!(
            names.contains(&"latest-sooc-v6.avif".to_string()),
            "{names:?}"
        );
    }

    #[test]
    fn overwrite_replaces_the_same_file() {
        let (_dir, cache) = cache();
        cache
            .write(3, "latest", EditBase::Raw, 6, b"first")
            .expect("写");
        cache
            .write(3, "latest", EditBase::Raw, 6, b"second")
            .expect("再写");
        assert_eq!(
            cache.read(3, "latest", EditBase::Raw, 6).as_deref(),
            Some(&b"second"[..])
        );
    }

    #[test]
    fn invalidate_removes_only_latest_for_that_asset() {
        let (_dir, cache) = cache();
        cache
            .write(1, "latest", EditBase::Raw, 6, b"a")
            .expect("写");
        cache.write(1, "raw", EditBase::Raw, 6, b"b").expect("写");
        cache
            .write(2, "latest", EditBase::Raw, 6, b"c")
            .expect("写");
        assert_eq!(cache.invalidate(1), 1);
        assert!(cache.read(1, "latest", EditBase::Raw, 6).is_none());
        assert_eq!(
            cache.read(1, "raw", EditBase::Raw, 6).as_deref(),
            Some(&b"b"[..])
        );
        assert_eq!(
            cache.read(2, "latest", EditBase::Raw, 6).as_deref(),
            Some(&b"c"[..])
        );
        assert_eq!(cache.invalidate(1), 0, "重复失效不算错");
    }

    #[test]
    fn latest_invalidation_preserves_named_issue_until_explicit_delete() {
        let (_dir, cache) = cache();
        cache
            .write(1, "latest", EditBase::Raw, 6, b"working")
            .unwrap();
        cache
            .write(1, "issue-42-abc", EditBase::Raw, 6, b"snapshot")
            .unwrap();
        assert_eq!(cache.invalidate(1), 1);
        assert_eq!(
            cache.read(1, "issue-42-abc", EditBase::Raw, 6).as_deref(),
            Some(&b"snapshot"[..])
        );
        assert_eq!(cache.remove_issue(1, 42), 1);
        assert!(cache.read(1, "issue-42-abc", EditBase::Raw, 6).is_none());
    }

    #[test]
    fn clear_removes_everything_and_is_idempotent() {
        let (_dir, cache) = cache();
        cache
            .write(1, "latest", EditBase::Raw, 6, b"a")
            .expect("写");
        cache
            .write(2, "latest", EditBase::Raw, 6, b"b")
            .expect("写");
        assert_eq!(cache.clear(), 2);
        assert!(cache.read(1, "latest", EditBase::Raw, 6).is_none());
        assert_eq!(cache.clear(), 0);
        assert!(cache.root().is_dir(), "清空不该把目录本身删掉");
    }

    #[test]
    fn empty_files_are_treated_as_missing() {
        let (_dir, cache) = cache();
        cache
            .write(9, "latest", EditBase::Sooc, 6, b"")
            .expect("写空");
        assert!(
            cache.read(9, "latest", EditBase::Sooc, 6).is_none(),
            "空文件当没有（半个文件的兜底）"
        );
    }
    #[test]
    fn changed_source_invalidates_all_issues_only_for_that_asset() {
        let dir = tempfile::tempdir().unwrap();
        let cache = FullCache::open(dir.path()).unwrap();
        cache
            .write(1, "latest-srcabc", EditBase::Raw, 6, b"a")
            .unwrap();
        cache
            .write(1, "issue-42-hash-srcabc", EditBase::Sooc, 6, b"b")
            .unwrap();
        cache
            .write(2, "latest-srcabc", EditBase::Raw, 6, b"c")
            .unwrap();
        assert_eq!(cache.invalidate_source(1).unwrap(), 2);
        assert!(cache.read(2, "latest-srcabc", EditBase::Raw, 6).is_some());
        assert_eq!(cache.invalidate_source(1).unwrap(), 0);
    }

    #[test]
    fn gc_keeps_only_the_newest_latest_snapshots() {
        let (_dir, cache) = cache();
        for (index, name) in [
            "latest-aaaaaaaaaaaa",
            "latest-bbbbbbbbbbbb",
            "latest-cccccccccccc",
        ]
        .iter()
        .enumerate()
        {
            cache
                .write(5, name, EditBase::Raw, 6, format!("v{index}").as_bytes())
                .expect("写");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        cache
            .write(5, "issue-1-hash", EditBase::Raw, 6, b"issue")
            .expect("写");
        assert_eq!(cache.gc_latest(5, 2), 1, "三份 latest 保留两份");
        assert!(
            cache.read(5, "latest-aaaaaaaaaaaa", EditBase::Raw, 6).is_none(),
            "最老的那份被回收"
        );
        assert!(cache.read(5, "latest-bbbbbbbbbbbb", EditBase::Raw, 6).is_some());
        assert!(cache.read(5, "latest-cccccccccccc", EditBase::Raw, 6).is_some());
        assert!(
            cache.read(5, "issue-1-hash", EditBase::Raw, 6).is_some(),
            "定稿快照不归它管"
        );
        assert_eq!(cache.gc_latest(5, 2), 0, "幂等");
    }

    /*
     * 真身档（2026-10-11 加）—— 三件事必须钉住，不然上线就漏删 / 互相挤掉：
     *   ① 扩展名与档标记（`.jpg` + `-full`）；
     *   ② 失效函数**不限扩展名**（源变了要连真身档一起删）；
     *   ③ `gc_latest` **按档各保留 N 份**（一张新真身档不许把占位档挤掉）。
     */
    #[test]
    fn full_class_has_its_own_extension_and_invalidates_with_the_rest() {
        let (_dir, cache) = cache();
        let preview = cache.path_for(1, "latest-srcabc", EditBase::Raw, 6);
        let full = cache.path_for_class(1, "latest-srcabc", EditBase::Raw, CacheClass::Full, 6);
        assert!(preview.to_string_lossy().ends_with("-v6.avif"));
        assert!(full.to_string_lossy().ends_with("-full-v6.jpg"));
        cache.write(1, "latest-srcabc", EditBase::Raw, 6, b"p").unwrap();
        cache
            .write_class(1, "latest-srcabc", EditBase::Raw, CacheClass::Full, 6, b"f")
            .unwrap();
        assert!(cache.read_class(1, "latest-srcabc", EditBase::Raw, CacheClass::Full, 6).is_some());
        assert_eq!(
            cache.invalidate_source(1).unwrap(),
            2,
            "源变了：真身档（.jpg）也要删 —— 只看 .avif 的旧实现会漏"
        );
        assert!(cache.read(1, "latest-srcabc", EditBase::Raw, 6).is_none());
        assert!(
            cache
                .read_class(1, "latest-srcabc", EditBase::Raw, CacheClass::Full, 6)
                .is_none()
        );
    }

    #[test]
    fn gc_latest_keeps_each_class_separately() {
        let (_dir, cache) = cache();
        for (index, name) in ["latest-aaaaaaaaaaaa", "latest-bbbbbbbbbbbb"].iter().enumerate() {
            cache
                .write(7, name, EditBase::Raw, 6, format!("p{index}").as_bytes())
                .unwrap();
            cache
                .write_class(7, name, EditBase::Raw, CacheClass::Full, 6, format!("f{index}").as_bytes())
                .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(cache.gc_latest(7, 1), 2, "每档各留最新 1 份 ⇒ 各删 1 份");
        assert!(
            cache.read(7, "latest-aaaaaaaaaaaa", EditBase::Raw, 6).is_none(),
            "旧的占位档被回收"
        );
        assert!(
            cache.read_class(7, "latest-aaaaaaaaaaaa", EditBase::Raw, CacheClass::Full, 6).is_none(),
            "旧的真身档也被回收"
        );
        assert!(cache.read(7, "latest-bbbbbbbbbbbb", EditBase::Raw, 6).is_some());
        assert!(
            cache
                .read_class(7, "latest-bbbbbbbbbbbb", EditBase::Raw, CacheClass::Full, 6)
                .is_some()
        );
    }

    #[test]
    fn capacity_sweep_deletes_full_class_first_and_keeps_the_tiny_placeholders() {
        let (_dir, cache) = cache();
        // 三个资产：两个真身档（各 1000 字节）+ 一个占位档（1000 字节）
        cache.write_class(1, "latest-a", EditBase::Raw, CacheClass::Full, 6, &vec![1u8; 1000]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        cache.write_class(2, "latest-b", EditBase::Raw, CacheClass::Full, 6, &vec![2u8; 1000]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        cache.write(3, "latest-c", EditBase::Raw, 6, &vec![3u8; 1000]).unwrap();
        assert_eq!(cache.used_bytes(), 3000);

        // 上限 1500：真身档先删（最旧的先走），占位档留着
        let removed = cache.enforce_capacity(1500);
        assert!(removed >= 1, "至少要删掉一份");
        assert!(cache.used_bytes() <= 1500);
        assert!(
            cache.read(3, "latest-c", EditBase::Raw, 6).is_some(),
            "占位档（几百 KB 但秒开的那一半）最后才动"
        );
    }

    #[test]
    fn capacity_sweep_is_a_noop_below_the_limit() {
        let (_dir, cache) = cache();
        cache.write(1, "latest-a", EditBase::Raw, 6, b"x").unwrap();
        assert_eq!(cache.enforce_capacity(FULL_CACHE_LIMIT_BYTES), 0);
        assert_eq!(cache.used_bytes(), 1);
    }
}
