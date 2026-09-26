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

use super::{file_id::FileId, fts, path_semantics::PathForms, repository};
use crate::error::Error;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use crate::error::Result;
use crate::media::diff::{self, DiskFile};
use crate::media::scan::{self, Cancel, ScanOptions};

use super::backfill;
use super::{assets, db::CatalogDb};

/// 重扫的结果（给日志与界面上的摘要用）。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
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
    pub modified: usize,
    /// 变化的资产（含缺失）；缓存与清单只刷新这些。
    pub changed_assets: Vec<i64>,
    /// 变化前后路径，库内相对路径。
    pub changed_paths: Vec<String>,
    pub directory_counts: Vec<(String, i64, i64)>,
    pub reset_counts: bool,
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
    synchronize(catalog, root, photos_dir, None, now_ms, progress).map(|(report, _)| report)
}

/// 范围是本目录和它的 `_RAW`，不递归其它子目录。
pub fn rescan_scope(
    catalog: &CatalogDb,
    photos_dir: &str,
    scope: &str,
    now_ms: i64,
) -> Result<(RescanReport, repository::Counts)> {
    synchronize(
        catalog,
        catalog.root(),
        photos_dir,
        Some(scope),
        now_ms,
        &mut |_| {},
    )
}

const PENDING_KEY: &str = "catalog-sync.pending.v1";
#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Pending {
    assets: BTreeSet<i64>,
    paths: BTreeSet<String>,
    counts: BTreeMap<String, (i64, i64)>,
    reset_counts: bool,
}
fn pending(conn: &rusqlite::Connection) -> Result<Pending> {
    repository::read_meta(conn, PENDING_KEY)?
        .map(|value| serde_json::from_str(&value).map_err(Error::from))
        .transpose()
        .map(|p| p.unwrap_or_default())
}
/// 只有缓存失效和 app.db 计数均成功后才确认。失败/重启会重放已落库的变化。
pub fn acknowledge_changes(catalog: &CatalogDb, report: &RescanReport) -> Result<()> {
    let assets: BTreeSet<_> = report.changed_assets.iter().copied().collect();
    let paths: BTreeSet<_> = report.changed_paths.iter().cloned().collect();
    let counts: BTreeMap<_, _> = report
        .directory_counts
        .iter()
        .map(|(path, photos, images)| (path.clone(), (*photos, *images)))
        .collect();
    let reset_counts = report.reset_counts;
    catalog.write_tx(move |conn| {
        let pending = pending(conn)?;
        if pending.assets == assets
            && pending.paths == paths
            && pending.counts == counts
            && pending.reset_counts == reset_counts
        {
            conn.execute("DELETE FROM repository_meta WHERE key=?1", [PENDING_KEY])?;
        }
        Ok(())
    })
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Unsupported(message.into())
}

pub fn validate_scope(photos_dir: &str, scope: &str) -> Result<String> {
    let scope = scope.replace('\\', "/");
    if scope.contains(':')
        || scope.contains('\0')
        || scope.is_empty()
        || Path::new(&scope)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || scope
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !(scope == photos_dir || scope.starts_with(&format!("{photos_dir}/")))
    {
        return Err(invalid("扫描范围必须是 photos 内的相对目录"));
    }
    Ok(scope)
}

/// 不让未完成扫描产生负面事实；局部有意跳过子目录不算截断。
fn require_complete(outcome: &scan::ScanOutcome, recursive: bool) -> Result<()> {
    let truncated = recursive
        && outcome
            .skipped_by_reason
            .iter()
            .any(|(reason, count)| *reason == scan::SkipReason::TooDeep && *count > 0);
    if outcome.cancelled || !outcome.problems.is_empty() || truncated {
        return Err(invalid(format!(
            "目录扫描不完整，保留原记录：{:?}",
            outcome.problems
        )));
    }
    Ok(())
}

fn in_scope(path: &str, scope: &str) -> bool {
    let dir = path.rsplit_once('/').map_or("", |(dir, _)| dir);
    dir == scope || dir == format!("{scope}/_raw")
}

fn synchronize(
    catalog: &CatalogDb,
    root: &Path,
    photos_dir: &str,
    scope: Option<&str>,
    now_ms: i64,
    progress: &mut dyn FnMut(RebuildProgress),
) -> Result<(RescanReport, repository::Counts)> {
    // 解析过的库仍可能在读盘期间被卸载或替换：按真实文件里的 ID 复核。
    let verify = || -> Result<()> {
        let meta = repository::read_repository_meta(&root.join("catalog.db"))?;
        if meta.id != catalog.meta().id {
            return Err(invalid("库目录的身份已经改变，停止同步"));
        }
        Ok(())
    };
    verify()?;
    let photo_root = root.join(photos_dir);
    let canonical_photos = std::fs::canonicalize(&photo_root)?;
    let scope = scope.map(|s| validate_scope(photos_dir, s)).transpose()?;
    let initial = catalog.read(assets::list_files)?;
    let mut disk = Vec::new();
    let roots = if let Some(scope) = &scope {
        let raw = match std::fs::read_dir(root.join(scope)) {
            Ok(entries) => {
                let names = entries
                    .collect::<std::io::Result<Vec<_>>>()?
                    .into_iter()
                    .filter(|entry| {
                        entry
                            .file_name()
                            .to_string_lossy()
                            .eq_ignore_ascii_case("_RAW")
                    })
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect::<Vec<_>>();
                if names.len() > 1 {
                    return Err(invalid("多个 RAW 目录折叠后同名，停止同步"));
                }
                names.into_iter().next().unwrap_or_else(|| "_RAW".into())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => "_RAW".into(),
            Err(e) => return Err(e.into()),
        };
        vec![scope.clone(), format!("{scope}/{raw}")]
    } else {
        vec![photos_dir.to_string()]
    };
    for relative_root in &roots {
        let base = root.join(relative_root);
        match std::fs::symlink_metadata(&base) {
            Ok(meta) => {
                if meta.file_type().is_symlink() || !meta.is_dir() {
                    return Err(invalid("扫描目录不是普通目录"));
                }
                if !std::fs::canonicalize(&base)?.starts_with(&canonical_photos) {
                    return Err(invalid("扫描目录越过库边界"));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && scope.is_some() => {
                // 只有 photos 仍在线且父链可读时才承认目录被删除。
                let mut parent = base.parent();
                while let Some(path) = parent {
                    match std::fs::read_dir(path) {
                        Ok(_) => break,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                            parent = path.parent()
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
                continue;
            }
            Err(error) => return Err(error.into()),
        }
        let opts = ScanOptions {
            max_depth: if scope.is_some() { 0 } else { 32 },
            skip_empty_files: false,
            ..ScanOptions::default()
        };
        let outcome = scan::scan(&base, &opts, &Cancel::new(), |event| {
            if let scan::ScanEvent::File(file) = event {
                let rel = if relative_root.is_empty() {
                    file.rel_path.clone()
                } else {
                    format!("{relative_root}/{}", file.rel_path)
                };
                let mut entry = DiskFile::new(rel, file.size_bytes, file.mtime_ms);
                entry.created_ms = file.created_ms;
                entry.identity = FileId::try_read(&file.abs_path);
                disk.push(entry);
                if disk.len().is_multiple_of(200) {
                    progress(RebuildProgress {
                        phase: "scan",
                        done: disk.len(),
                        total: disk.len(),
                    });
                }
            }
            Ok(())
        })?;
        require_complete(&outcome, scope.is_none())?;
    }
    verify()?;
    // 不把扫描后的离线读成“所有文件被删”。
    std::fs::read_dir(&photo_root)?;
    // 扫描期间文件被改写/移走时拒绝提交这份快照；元数据 I/O 不占用写事务。
    for file in &disk {
        let path = root.join(&file.rel_path);
        let metadata = std::fs::symlink_metadata(&path)?;
        let mtime = metadata.modified().ok().map(super::time::from_system_time);
        let identity = FileId::try_read(&path);
        if !metadata.is_file()
            || metadata.len() != file.size_bytes
            || mtime != file.mtime_ms
            || file.identity.is_some() && identity != file.identity
        {
            return Err(invalid("扫描期间文件已变化，保留原记录，请重试"));
        }
    }
    let counts = repository::Counts {
        photos: disk
            .iter()
            .filter(|file| {
                !file
                    .rel_path_folded
                    .rsplit_once('/')
                    .is_some_and(|(dir, _)| dir.ends_with("/_raw"))
            })
            .count() as i64,
        images: disk.len() as i64,
    };
    let mut directory_counts = BTreeMap::<String, (i64, i64)>::new();
    for file in &disk {
        let dir = file.rel_path.rsplit_once('/').map_or("", |(dir, _)| dir);
        let parent = assets::normalize_raw_dir(dir);
        let counts = directory_counts.entry(parent.to_string()).or_default();
        counts.1 += 1;
        if parent == dir {
            counts.0 += 1;
        }
    }
    if let Some(scope) = &scope {
        directory_counts.entry(scope.clone()).or_default();
    }
    let reset_counts = scope.is_none();
    let directory_counts: Vec<_> = directory_counts
        .into_iter()
        .map(|(dir, (photos, images))| (dir, photos, images))
        .collect();
    let count = disk.len();
    progress(RebuildProgress {
        phase: "scan",
        done: count,
        total: count,
    });
    let root = root.to_path_buf();
    let folded_scope = scope
        .as_ref()
        .map(|s| PathForms::new(s).folded().to_string());
    let (applied, changed_assets, changed_paths, directory_counts, reset_counts) = catalog
        .write_tx(move |conn| {
            fts::ensure_fresh(conn)?;
            let rows = assets::list_files(conn)?;
            let before: BTreeMap<_, _> = initial.iter().map(|row| (row.row_id, row)).collect();
            let covered = |row: &assets::FileRow| {
                folded_scope
                    .as_ref()
                    .is_none_or(|s| in_scope(&row.rel_path_folded, s))
            };
            // 范围外只借身份作匹配，并要求旧位置确实已不存在；硬链接不能误当移动。
            let identities: BTreeSet<_> = disk
                .iter()
                .filter_map(|file| file.identity.map(|id| (id.volume_serial, id.file_id)))
                .collect();
            let known: Vec<_> =
                rows.iter()
                    .filter(|row| {
                        covered(row)
                            || row.identity.is_some_and(|id| {
                                identities.contains(&(id.volume_serial, id.file_id))
                            }) && std::fs::symlink_metadata(root.join(&row.rel_path))
                                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
                    })
                    .map(assets::FileRow::to_db_file)
                    .collect();
            let mut plan = diff::diff(&disk, &known);
            // 身份不明时宁可作为新文件；不启用相似名字猜配来移动已有 issue。
            plan.renamed
                .retain(|rename| rename.evidence != diff::Evidence::Heuristic);
            let matched: BTreeSet<_> = plan
                .renamed
                .iter()
                .map(|r| r.disk_index)
                .chain(
                    plan.unchanged
                        .iter()
                        .chain(&plan.modified)
                        .chain(&plan.returned)
                        .map(|m| m.disk_index),
                )
                .collect();
            plan.new_files = (0..disk.len())
                .filter(|index| !matched.contains(index))
                .collect();
            let matched_rows: BTreeSet<_> = plan
                .renamed
                .iter()
                .map(|r| r.row_id)
                .chain(
                    plan.unchanged
                        .iter()
                        .chain(&plan.modified)
                        .chain(&plan.returned)
                        .map(|m| m.row_id),
                )
                .collect();
            plan.missing = rows
                .iter()
                .filter(|row| {
                    covered(row)
                        && !row.missing
                        && before.get(&row.row_id).is_some_and(|old| **old == **row)
                        && !matched_rows.contains(&row.row_id)
                })
                .map(|row| row.row_id)
                .collect();
            // 最后确认负面事实：文件在枚举后回来了就保留记录，权限错误则整笔回滚。
            let mut confirmed_missing = Vec::new();
            let by_id: BTreeMap<_, _> = rows.iter().map(|row| (row.row_id, row)).collect();
            for id in &plan.missing {
                let row = by_id[id];
                match std::fs::symlink_metadata(root.join(&row.rel_path)) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        confirmed_missing.push(*id)
                    }
                    Err(error) => return Err(error.into()),
                    Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
                        confirmed_missing.push(*id)
                    }
                    Ok(_) => {}
                }
            }
            plan.missing = confirmed_missing;
            let changed_rows: BTreeSet<_> = plan
                .missing
                .iter()
                .copied()
                .chain(plan.renamed.iter().map(|r| r.row_id))
                .chain(plan.modified.iter().chain(&plan.returned).map(|m| m.row_id))
                .collect();
            let mut ids = BTreeSet::new();
            let mut paths = BTreeSet::new();
            for row in &rows {
                if changed_rows.contains(&row.row_id) {
                    ids.insert(row.asset_id);
                    paths.insert(row.rel_path.clone());
                }
            }
            for index in plan
                .new_files
                .iter()
                .copied()
                .chain(
                    plan.modified
                        .iter()
                        .chain(&plan.returned)
                        .map(|m| m.disk_index),
                )
                .chain(plan.renamed.iter().map(|r| r.disk_index))
            {
                paths.insert(disk[index].rel_path.clone());
            }
            let applied = assets::apply_diff(conn, &plan, &disk, now_ms)?;
            for row in assets::list_files(conn)? {
                if paths.contains(&row.rel_path) {
                    ids.insert(row.asset_id);
                }
            }
            for id in &ids {
                fts::refresh_asset(conn, *id)?;
            }
            let mut pending = pending(conn)?;
            pending.assets.extend(ids);
            pending.paths.extend(paths);
            for (path, photos, images) in directory_counts {
                pending.counts.insert(path, (photos, images));
            }
            pending.reset_counts |= reset_counts;
            repository::write_meta(conn, PENDING_KEY, &serde_json::to_string(&pending)?)?;
            Ok((
                applied,
                pending.assets,
                pending.paths,
                pending
                    .counts
                    .into_iter()
                    .map(|(path, (photos, images))| (path, photos, images))
                    .collect(),
                pending.reset_counts,
            ))
        })?;
    progress(RebuildProgress {
        phase: "apply",
        done: count,
        total: count,
    });
    let filled = if scope.is_none() {
        backfill::refresh_metadata(catalog, now_ms)?
    } else {
        backfill::refresh_assets(catalog, now_ms, &changed_assets)?
    };
    progress(RebuildProgress {
        phase: "metadata",
        done: filled.filled,
        total: count,
    });
    Ok((
        RescanReport {
            scanned: count,
            registered: applied.new_files,
            missing: applied.missing,
            returned: applied.returned,
            renamed: applied.renamed,
            modified: applied.modified,
            metadata_filled: filled.filled,
            changed_assets: changed_assets.into_iter().collect(),
            changed_paths: changed_paths.into_iter().collect(),
            directory_counts,
            reset_counts,
        },
        counts,
    ))
}

#[cfg(test)]
mod tests {
    use super::super::db::OpenOpts;
    use super::*;
    fn catalog() -> (tempfile::TempDir, CatalogDb) {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("photos")).unwrap();
        let db =
            CatalogDb::create(root.path(), "同步测试", None, OpenOpts::unbacked_up(1)).unwrap();
        (root, db)
    }
    fn photo(root: &Path, path: &str) {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        image::RgbImage::from_pixel(3, 2, image::Rgb([10, 20, 30]))
            .save(path)
            .unwrap();
    }
    fn files(db: &CatalogDb) -> Vec<assets::FileRow> {
        db.read(assets::list_files).unwrap()
    }
    fn scan(db: &CatalogDb, scope: &str) -> RescanReport {
        let report = rescan_scope(db, "photos", scope, 10).unwrap().0;
        acknowledge_changes(db, &report).unwrap();
        report
    }
    #[test]
    fn empty_and_paired_scopes_do_not_touch_other_directories() {
        let (root, db) = catalog();
        std::fs::create_dir(root.path().join("photos/空目录")).unwrap();
        assert_eq!(scan(&db, "photos/空目录").scanned, 0);
        photo(root.path(), "photos/中文/相片.JPG");
        std::fs::create_dir(root.path().join("photos/中文/_RAW")).unwrap();
        std::fs::write(root.path().join("photos/中文/_RAW/相片.NEF"), b"raw").unwrap();
        let (report, counts) = rescan_scope(&db, "photos", "photos/中文", 10).unwrap();
        acknowledge_changes(&db, &report).unwrap();
        assert_eq!(report.registered, 2);
        assert_eq!(
            counts,
            repository::Counts {
                photos: 1,
                images: 2
            }
        );
        assert_eq!(files(&db)[0].asset_id, files(&db)[1].asset_id);
        assert_eq!(scan(&db, "photos/空目录").missing, 0);
        assert!(
            files(&db)
                .iter()
                .all(|row| !row.missing && row.identity.is_some())
        );
        assert_eq!(scan(&db, "photos/中文").changed_assets.len(), 0);
    }
    #[test]
    fn identity_move_keeps_asset_and_marks_only_covered_missing() {
        let (root, db) = catalog();
        photo(root.path(), "photos/旧/IMG_001.jpg");
        scan(&db, "photos/旧");
        let original = files(&db)[0].clone();
        db.write_tx(move |conn| {
            conn.execute(
                "UPDATE assets SET rating = 5 WHERE id = ?1",
                [original.asset_id],
            )?;
            Ok(())
        })
        .unwrap();
        std::fs::create_dir(root.path().join("photos/新")).unwrap();
        std::fs::rename(
            root.path().join("photos/旧/IMG_001.jpg"),
            root.path().join("photos/新/改名.jpg"),
        )
        .unwrap();
        assert_eq!(scan(&db, "photos/旧").missing, 1);
        assert_eq!(scan(&db, "photos/新").renamed, 1);
        let moved = files(&db);
        assert_eq!(moved.len(), 1);
        assert_eq!(moved[0].asset_id, original.asset_id);
        assert!(!moved[0].missing);
        assert_eq!(
            db.read(|conn| Ok(conn.query_row(
                "SELECT rating FROM assets WHERE id=?1",
                [original.asset_id],
                |r| r.get::<_, i64>(0)
            )?))
            .unwrap(),
            5
        );
        let name = db
            .read(|conn| {
                Ok(conn.query_row(
                    "SELECT file_name FROM assets_fts WHERE rowid=?1",
                    [original.asset_id],
                    |r| r.get::<_, String>(0),
                )?)
            })
            .unwrap();
        assert_eq!(name, "改名.jpg");
    }
    #[test]
    fn replaced_identity_is_stored_and_next_scan_is_quiet() {
        let (root, db) = catalog();
        photo(root.path(), "photos/a.jpg");
        scan(&db, "photos");
        let old = files(&db)[0].clone();
        photo(root.path(), "photos/b.jpg");
        std::fs::rename(
            root.path().join("photos/b.jpg"),
            root.path().join("photos/a.jpg"),
        )
        .unwrap();
        assert_eq!(scan(&db, "photos").modified, 1);
        assert_ne!(files(&db)[0].identity, old.identity);
        assert_eq!(scan(&db, "photos").modified, 0);
    }
    #[test]
    fn missing_directory_and_returned_file_are_safe() {
        let (root, db) = catalog();
        photo(root.path(), "photos/a/x.jpg");
        scan(&db, "photos/a");
        std::fs::rename(root.path().join("photos/a"), root.path().join("parked")).unwrap();
        assert_eq!(scan(&db, "photos/a").missing, 1);
        std::fs::rename(root.path().join("parked"), root.path().join("photos/a")).unwrap();
        assert_eq!(scan(&db, "photos/a").returned, 1);
        assert!(!files(&db)[0].missing);
    }
    #[test]
    fn offline_and_invalid_scope_leave_catalog_unchanged() {
        let (root, db) = catalog();
        photo(root.path(), "photos/x.jpg");
        scan(&db, "photos");
        let before = files(&db);
        for scope in [
            "",
            "../photos",
            "/photos",
            "photos/../cache",
            "photos//a",
            "photos/./a",
            "C:/photos",
            "cache",
            "photos/a\0",
        ] {
            assert!(rescan_scope(&db, "photos", scope, 20).is_err(), "{scope:?}");
        }
        std::fs::rename(root.path().join("photos"), root.path().join("offline")).unwrap();
        assert!(rescan_scope(&db, "photos", "photos", 20).is_err());
        assert!(rescan_library(&db, root.path(), "photos", 20).is_err());
        assert_eq!(files(&db), before);
    }
    #[test]
    fn partial_cancelled_and_truncated_scans_are_not_empty_truth() {
        let mut outcome = scan::ScanOutcome::default();
        require_complete(&outcome, true).unwrap();
        outcome.cancelled = true;
        assert!(require_complete(&outcome, false).is_err());
        outcome.cancelled = false;
        outcome.problems.push(scan::ScanProblem {
            rel_path: "a".into(),
            message: "permission denied".into(),
        });
        assert!(require_complete(&outcome, false).is_err());
        outcome.problems.clear();
        outcome
            .skipped_by_reason
            .push((scan::SkipReason::TooDeep, 1));
        assert!(require_complete(&outcome, true).is_err());
        require_complete(&outcome, false).unwrap();
    }
    #[test]
    fn concurrent_registration_is_not_marked_missing() {
        let (root, db) = catalog();
        photo(root.path(), "photos/a.jpg");
        let mut injected = false;
        synchronize(
            &db,
            root.path(),
            "photos",
            Some("photos"),
            10,
            &mut |progress| {
                if progress.phase == "scan" && !injected {
                    injected = true;
                    db.write_tx(|conn| {
                        let id = assets::insert_asset(conn, 11)?;
                        assets::insert_file(
                            conn,
                            id,
                            &DiskFile::new("photos/concurrent.jpg", 12, Some(11)),
                            "bitmap",
                            11,
                        )?;
                        Ok(())
                    })
                    .unwrap();
                }
            },
        )
        .unwrap();
        assert!(files(&db).iter().all(|row| !row.missing));
    }
    #[cfg(unix)]
    #[test]
    fn symlink_scope_and_hardlink_do_not_steal_assets() {
        let (root, db) = catalog();
        photo(root.path(), "photos/a/x.jpg");
        scan(&db, "photos/a");
        std::os::unix::fs::symlink(
            root.path().join("photos/a"),
            root.path().join("photos/link"),
        )
        .unwrap();
        assert!(rescan_scope(&db, "photos", "photos/link", 20).is_err());
        std::fs::create_dir(root.path().join("photos/b")).unwrap();
        std::fs::hard_link(
            root.path().join("photos/a/x.jpg"),
            root.path().join("photos/b/x.jpg"),
        )
        .unwrap();
        assert_eq!(scan(&db, "photos/b").registered, 1);
        assert_eq!(files(&db).len(), 2);
    }
    #[test]
    fn swap_names_and_reuse_vacated_path_keep_identities() {
        let (root, db) = catalog();
        photo(root.path(), "photos/a.jpg");
        photo(root.path(), "photos/b.jpg");
        scan(&db, "photos");
        let original = files(&db);
        std::fs::rename(
            root.path().join("photos/a.jpg"),
            root.path().join("temp.jpg"),
        )
        .unwrap();
        std::fs::rename(
            root.path().join("photos/b.jpg"),
            root.path().join("photos/a.jpg"),
        )
        .unwrap();
        std::fs::rename(
            root.path().join("temp.jpg"),
            root.path().join("photos/b.jpg"),
        )
        .unwrap();
        assert_eq!(scan(&db, "photos").renamed, 2);
        let swapped = files(&db);
        assert_eq!(swapped[0].asset_id, original[0].asset_id);
        assert_eq!(swapped[0].rel_path, "photos/b.jpg");
        std::fs::rename(
            root.path().join("photos/b.jpg"),
            root.path().join("photos/c.jpg"),
        )
        .unwrap();
        photo(root.path(), "photos/b.jpg");
        let report = scan(&db, "photos");
        assert_eq!((report.renamed, report.registered), (1, 1));
        assert_eq!(files(&db).len(), 3);
    }
    #[test]
    fn pending_changes_and_counts_survive_restart_until_acknowledged() {
        let (root, db) = catalog();
        photo(root.path(), "photos/a/x.jpg");
        photo(root.path(), "photos/b/y.jpg");
        let first = rescan_scope(&db, "photos", "photos/a", 10).unwrap().0;
        assert_eq!(first.registered, 1);
        let second = rescan_scope(&db, "photos", "photos/b", 11).unwrap().0;
        assert_eq!(second.changed_assets.len(), 2);
        assert_eq!(second.directory_counts.len(), 2);
        drop(db);
        let db = CatalogDb::open(root.path(), OpenOpts::unbacked_up(12)).unwrap();
        let replay = rescan_scope(&db, "photos", "photos/a", 13).unwrap().0;
        assert_eq!(replay.changed_assets, second.changed_assets);
        assert_eq!(replay.directory_counts, second.directory_counts);
        acknowledge_changes(&db, &replay).unwrap();
        assert!(scan(&db, "photos/a").changed_assets.is_empty());
    }
    #[test]
    fn raw_case_and_case_only_rename_are_reconciled() {
        let (root, db) = catalog();
        photo(root.path(), "photos/中文/x.JPG");
        std::fs::create_dir(root.path().join("photos/中文/_raw")).unwrap();
        std::fs::write(root.path().join("photos/中文/_raw/x.NEF"), b"raw").unwrap();
        assert_eq!(scan(&db, "photos/中文").registered, 2);
        assert_eq!(files(&db)[0].asset_id, files(&db)[1].asset_id);
        std::fs::rename(
            root.path().join("photos/中文/x.JPG"),
            root.path().join("photos/中文/X.jpg"),
        )
        .unwrap();
        assert_eq!(scan(&db, "photos/中文").renamed, 1);
        assert!(files(&db).iter().all(|file| !file.missing));
    }
    #[test]
    fn move_preserves_named_issue_profile() {
        let (root, db) = catalog();
        photo(root.path(), "photos/旧/x.jpg");
        scan(&db, "photos/旧");
        let asset = files(&db)[0].asset_id;
        let issue = db
            .write_tx(move |conn| {
                let mut stack = super::super::develop::DevelopStack::default();
                stack.params.insert("exposure".into(), 1.0);
                super::super::issues::create(conn, asset, "中文定稿", &stack, 11)
            })
            .unwrap();
        std::fs::create_dir(root.path().join("photos/新")).unwrap();
        std::fs::rename(
            root.path().join("photos/旧/x.jpg"),
            root.path().join("photos/新/改名.jpg"),
        )
        .unwrap();
        assert_eq!(scan(&db, "photos/新").renamed, 1);
        let preserved = db
            .read(move |conn| super::super::issues::get(conn, asset, issue.id))
            .unwrap()
            .unwrap();
        assert_eq!(preserved, issue);
    }
    #[cfg(unix)]
    #[test]
    fn hardlinks_in_the_same_scope_are_not_spurious_renames() {
        let (root, db) = catalog();
        photo(root.path(), "photos/a.jpg");
        std::fs::hard_link(
            root.path().join("photos/a.jpg"),
            root.path().join("photos/b.jpg"),
        )
        .unwrap();
        assert_eq!(scan(&db, "photos").registered, 2);
        let report = scan(&db, "photos");
        assert_eq!(
            (report.renamed, report.registered, report.missing),
            (0, 0, 0)
        );
    }
}
