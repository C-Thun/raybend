//! 「重建数据」：把库在磁盘上的真相**重新对齐**到 `catalog.db`。
//!
//! 人类 2026-09-19 要的入口（库卡片齿轮里的那个按钮）。它花时间，但能自愈三类问题：
//!
//! 1. **元数据缺**：2026-09-18 之前的导入不写 EXIF，老库那批资产的
//!    `taken_at` / `width` / `height` / `orientation` 全是 NULL
//!    （根因见 [`super::backfill`] 的文件头）—— 表现为时间分组乱、tile 比例不对；
//! 2. **文件在程序外面被增删**：有人直接把照片拷进/挪出库目录 —— 库里该多出来的没多、
//!    该少的还留着；
//! 3. **计数与磁盘对不上**：`app.db` 里的相片/图片数（`photos_count` / `images_count`）。
//!
//! 分工：**这个模块只管 catalog 那一半**（扫盘 ↔ 资产表 ↔ 元数据）。
//! 计数住在 `app.db`（`directories` 表），由调用方（外壳层）在同一个流程里重算 ——
//! store 层不碰 `app.db`（它只认自己那一个库）。

use std::path::Path;

use crate::error::Result;
use crate::media::diff::{self, DiskFile};
use crate::media::scan::{self, Cancel, ScanOptions};

use super::backfill;
use super::{assets, db::CatalogDb};

/// 重扫的结果（给日志与界面上的摘要用）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RescanReport {
    /// 扫到多少个文件（`photos/` 之下）。
    pub scanned: usize,
    /// 新登记的文件（磁盘上有、库里没记录）。
    pub registered: usize,
    /// 新标记为「磁盘上找不到」。
    pub missing: usize,
    /// 之前标记缺失、这次又找到了。
    pub returned: usize,
    /// 路径变了但认出来是同一个文件。
    pub renamed: usize,
    /// 补上元数据的**资产**数。
    pub metadata_filled: usize,
}

/// 重扫一个库：`photos/` → 与 `asset_files` 对比 → 落库 → 重读元数据。
///
/// `root` 是库根，`photos_dir` 是库内落地目录（默认 `photos`）。
/// 两步都落在**同一个库**上，但顺序要紧：先把文件对齐（新文件得先有行），
/// 再补元数据（否则新登记的行也会被当成「没有元数据」白跑一趟）。
pub fn rescan_library(
    catalog: &CatalogDb,
    root: &Path,
    photos_dir: &str,
    now_ms: i64,
) -> Result<RescanReport> {
    rescan_library_with_progress(catalog, root, photos_dir, now_ms, &mut |_| {})
}

/// 重建过程中的**进度事实**（命令层把它翻成事件给前端；这里不认识 Tauri）。
///
/// `phase` 是**机器可读**的阶段名（`scan` / `apply` / `metadata`），
/// 句子由前端按当前语言组织 —— 后端不拼人话（i18n 纪律）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RebuildProgress {
    pub phase: &'static str,
    /// 这一阶段已完成多少
    pub done: usize,
    /// 这一阶段总共多少（扫描阶段要扫完才知道 ⇒ 那时 `done == total`）
    pub total: usize,
}

/// 带进度的重扫（人类 2026-09-19：重建数据要能看到进展，不能黑箱几十秒）。
///
/// 每个**阶段结束**报一次，加上扫描过程中**每 N 个文件**报一次 ——
/// 大库最耗时的是扫盘，那段没有反馈，人就会以为卡死了。
pub fn rescan_library_with_progress(
    catalog: &CatalogDb,
    root: &Path,
    photos_dir: &str,
    now_ms: i64,
    progress: &mut dyn FnMut(RebuildProgress),
) -> Result<RescanReport> {
    let base = if photos_dir.is_empty() {
        root.to_path_buf()
    } else {
        root.join(photos_dir)
    };

    // ① 扫盘（**不跟随符号链接**：库目录里放个链接指到自己会扫不完）
    let cancel = Cancel::new();
    let (scanned, _outcome) = if base.is_dir() {
        let mut seen = 0usize;
        let mut files = Vec::new();
        let outcome = scan::scan(&base, &ScanOptions::default(), &cancel, |event| {
            if let scan::ScanEvent::File(file) = event {
                seen += 1;
                files.push(file);
                // 每 200 个报一次：太密会把事件通道打满、太疏又像卡住
                if seen.is_multiple_of(200) {
                    progress(RebuildProgress {
                        phase: "scan",
                        done: seen,
                        total: seen,
                    });
                }
            }
            Ok(())
        })?;
        progress(RebuildProgress {
            phase: "scan",
            done: files.len(),
            total: files.len(),
        });
        (files, outcome)
    } else {
        (Vec::new(), scan::ScanOutcome::default())
    };

    /*
     * 磁盘文件 → `DiskFile`：**相对路径要补上 `photos/` 前缀**。
     * 扫描给的是「相对扫描根」的路径，而库里存的是「相对库根」的路径
     * （`photos/2026-08-15/MY0001.JPG`）—— 这里少写一层，对比会全部落进 `new_files`，
     * 表现为「重建一次，库里多出一整套重复照片」。
     */
    let disk: Vec<DiskFile> = scanned
        .iter()
        .map(|file| {
            let rel = if photos_dir.is_empty() {
                file.rel_path.clone()
            } else {
                format!("{photos_dir}/{}", file.rel_path)
            };
            DiskFile::new(rel, file.size_bytes, file.mtime_ms)
        })
        .collect();

    /*
     * ② 与库里的记录对比 → 落库。
     *
     * 对比与落库**放在同一个写闭包里**：写闭包要跨线程（`'static`），
     * 与其把 `disk` / `plan` clone 进去，不如让「读现状 → 算差异 → 落库」一气呵成 ——
     * 中间不会被别人的导入插进来改一行（那会让 diff 算出的计划与库里的现状对不上）。
     */
    let scanned_count = disk.len();
    let applied = catalog.write(move |conn| {
        let rows = assets::list_files(conn)?;
        let known: Vec<diff::DbFile> = rows.iter().map(assets::FileRow::to_db_file).collect();
        let plan = diff::diff(&disk, &known);
        assets::apply_diff(conn, &plan, &disk, now_ms)
    })?;

    progress(RebuildProgress {
        phase: "apply",
        done: scanned_count,
        total: scanned_count,
    });

    // ③ 元数据全量重读：重建的语义是“以磁盘为准”，非空但错误的方向/尺寸也必须修正。
    let filled = backfill::refresh_metadata(catalog, now_ms)?;
    progress(RebuildProgress {
        phase: "metadata",
        done: filled.filled,
        total: scanned_count,
    });

    Ok(RescanReport {
        scanned: scanned_count,
        registered: applied.new_files,
        missing: applied.missing,
        returned: applied.returned,
        renamed: applied.renamed,
        metadata_filled: filled.filled,
    })
}
