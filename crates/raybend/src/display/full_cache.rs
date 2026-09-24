//! **库内大图缓存**：`<库根>/cache/full/<asset_id>/<issue>-v<pipeline>.avif`。
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
//! ```
//!
//! * `issue` —— `sooc` / `raw` / `latest`（**SOOC 不进缓存**：那就是原文件本身）；
//! * **`base` —— 编辑基准**（`sooc` / `raw`）：同一张照片在两种基准下渲染出的是
//!   **两张不同的图**（人类 2026-09-24 让基准可切），所以它必须进文件名 ——
//!   否则切了基准之后读到的是另一基准渲染出来的旧图，而且**看不出是错的**
//!   （`IMAGING.md` §4.3 登记了这个坑）；
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

use crate::error::{Error, Result};
use crate::store::develop::EditBase;

/// 库根下缓存目录的名字（用户看得见，起个直白的）。
pub const CACHE_DIR: &str = "cache";
/// 大图缓存所在的子目录（与将来的其它缓存分开）。
pub const FULL_DIR: &str = "full";

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

    /// 某个资产某个 issue **某个编辑基准**的缓存文件路径。
    #[must_use]
    pub fn path_for(
        &self,
        asset_id: i64,
        issue: &str,
        base: EditBase,
        pipeline_version: u32,
    ) -> PathBuf {
        self.root
            .join(asset_id.to_string())
            .join(format!("{issue}-{}-v{pipeline_version}.avif", base.as_str()))
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
        let path = self.path_for(asset_id, issue, base, pipeline_version);
        let bytes = std::fs::read(&path).ok()?;
        if bytes.is_empty() { None } else { Some(bytes) }
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
        let path = self.path_for(asset_id, issue, base, pipeline_version);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("avif.tmp");
        if let Err(error) = std::fs::write(&temp, data) {
            let _ = std::fs::remove_file(&temp);
            return Err(Error::Io(error));
        }
        if let Err(error) = std::fs::rename(&temp, &path) {
            let _ = std::fs::remove_file(&temp);
            return Err(Error::Io(error));
        }
        Ok(())
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
            if std::fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
        let _ = std::fs::remove_dir(&dir); // 空了才删得掉；删不掉也无所谓
        removed
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
        assert!(dir.path().join("cache").join("full").is_dir(), "目录要建出来");
        assert_eq!(cache.root(), dir.path().join("cache").join("full"));
    }

    #[test]
    fn write_then_read_round_trips_and_is_atomic() {
        let (_dir, cache) = cache();
        assert!(cache.read(7, "latest", EditBase::Raw, 6).is_none(), "一开始没有");
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
        cache.write(1, "latest", EditBase::Raw, 6, b"v6").expect("写");
        cache.write(1, "raw", EditBase::Raw, 6, b"raw").expect("写");
        cache.write(1, "latest", EditBase::Raw, 7, b"v7").expect("写");
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
        cache.write(9, "latest", EditBase::Raw, 6, b"from raw").expect("写");
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
        assert!(names.contains(&"latest-raw-v6.avif".to_string()), "{names:?}");
        assert!(names.contains(&"latest-sooc-v6.avif".to_string()), "{names:?}");
    }

    #[test]
    fn overwrite_replaces_the_same_file() {
        let (_dir, cache) = cache();
        cache.write(3, "latest", EditBase::Raw, 6, b"first").expect("写");
        cache
            .write(3, "latest", EditBase::Raw, 6, b"second")
            .expect("再写");
        assert_eq!(
            cache.read(3, "latest", EditBase::Raw, 6).as_deref(),
            Some(&b"second"[..])
        );
    }

    #[test]
    fn invalidate_removes_only_that_asset() {
        let (_dir, cache) = cache();
        cache.write(1, "latest", EditBase::Raw, 6, b"a").expect("写");
        cache.write(1, "raw", EditBase::Raw, 6, b"b").expect("写");
        cache.write(2, "latest", EditBase::Raw, 6, b"c").expect("写");
        assert_eq!(cache.invalidate(1), 2);
        assert!(cache.read(1, "latest", EditBase::Raw, 6).is_none());
        assert!(cache.read(1, "raw", EditBase::Raw, 6).is_none());
        assert_eq!(
            cache.read(2, "latest", EditBase::Raw, 6).as_deref(),
            Some(&b"c"[..])
        );
        assert_eq!(cache.invalidate(1), 0, "重复失效不算错");
    }

    #[test]
    fn clear_removes_everything_and_is_idempotent() {
        let (_dir, cache) = cache();
        cache.write(1, "latest", EditBase::Raw, 6, b"a").expect("写");
        cache.write(2, "latest", EditBase::Raw, 6, b"b").expect("写");
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
}
