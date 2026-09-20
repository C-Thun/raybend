//! 老库的元数据回填，以及「重建数据」使用的全量元数据刷新。
//!
//! ## 为什么需要它
//!
//! 2026-09-18 之前的导入**根本不写 EXIF**（根因见 `import/sink.rs` 的 `register`，
//! 那里原本只登记文件、不碰元数据列）。于是那段时间导入的库里：
//! `taken_at` / `camera_model` / 曝光 / 尺寸全是 NULL ——
//! 浏览的时间分组、按时间筛选、右栏信息栏都因此是空的（人类 2026-09-18 上报）。
//!
//! 新导入已经走对（同一提交里补的）；这份代码负责把**老库**补齐。
//!
//! ## 只碰「从文件推出来」的东西
//!
//! 写的是 [`assets::apply_exif`] 那一组列（时间 / 器材 / 曝光 / 尺寸 / 朝向），
//! **绝不碰用户写的**（`author` / `description` / `rating` / 色标 / 标签 / 锁）。
//! 普通回填只处理 `taken_at IS NULL`；重建则刷新全部在线资产，修复非空但错误的方向/尺寸。
//!
//! ## 顺序与并发
//!
//! * 读盘在写事务**之外**（I/O 不该占着写者）；
//! * 一个库一次写事务（`write_tx`），且逐行再确认一次 `taken_at IS NULL`
//!   （回填期间用户又改了行就不覆盖）；
//! * 一个资产有多个文件时，**以位图为准**（RAW 那边能读到的字段往往更少），
//!   只有 RAW 没有位图时才用 RAW 的读数。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::media::exif::{self, source_rank, ExifData, TakenAt};
use crate::media::meta::{orientation_swaps_axes, read_photo_meta};
use crate::store::assets;
use crate::store::db::CatalogDb;

/// 回填结果（给调用方打日志/报数用）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackfillReport {
    /// 候选资产数：普通回填是 `taken_at IS NULL`，全量刷新是全部在线资产。
    pub candidates: usize,
    /// 这次真正写入的资产数。
    pub filled: usize,
    /// 读完之后仍然没有任何可写文件元数据的资产数。
    pub still_missing: usize,
    /// 有文件不在磁盘上（离线盘 / 被删）——那些资产原样不动。
    pub unreadable: usize,
}

/// 一个候选资产下的一条文件记录。
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileRow {
    role: String,
    rel_path: String,
    mtime_ms: Option<i64>,
}

/// 回填一个库（真实现：从磁盘读 EXIF）。
pub fn backfill_metadata(catalog: &CatalogDb, now_ms: i64) -> Result<BackfillReport> {
    backfill_with(catalog, now_ms, |abs, name, mtime_ms| {
        if !abs.is_file() {
            return None;
        }
        let data = read_disk_metadata(abs);
        let taken = exif::resolve_taken_at(Some(&data), name, mtime_ms);
        Some((data, taken))
    })
}

/// 重建数据时使用：不只补 NULL，而是重新读取全部在线资产的文件元数据。
///
/// 只覆盖 `assets::apply_exif` 管辖的文件事实；星级、文字、标签、锁等用户数据不动。
pub fn refresh_metadata(catalog: &CatalogDb, now_ms: i64) -> Result<BackfillReport> {
    refresh_with(catalog, now_ms, |abs, name, mtime_ms| {
        if !abs.is_file() {
            return None;
        }
        let data = read_disk_metadata(abs);
        let taken = exif::resolve_taken_at(Some(&data), name, mtime_ms);
        Some((data, taken))
    })
}

/// EXIF 提供器材/时间；真实图像头提供更可信的尺寸。
///
/// `read_photo_meta` 返回已应用方向的展示尺寸，catalog 仍存原始像素轴，所以写入前换回去。
fn read_disk_metadata(path: &Path) -> ExifData {
    let mut data = exif::read_file_for(path);
    if let Ok(meta) = read_photo_meta(path) {
        let (width, height) = if orientation_swaps_axes(meta.orientation) {
            (meta.height, meta.width)
        } else {
            (meta.width, meta.height)
        };
        data.width = (width > 0).then_some(i64::from(width));
        data.height = (height > 0).then_some(i64::from(height));
        data.orientation = Some(i64::from(meta.orientation));
    }
    data
}

/// 可注入读法的版本（测试用假读法，不必造真文件）。
pub fn backfill_with<F>(catalog: &CatalogDb, now_ms: i64, read: F) -> Result<BackfillReport>
where
    F: Fn(&Path, &str, Option<i64>) -> Option<(ExifData, Option<TakenAt>)>,
{
    sync_with(catalog, now_ms, true, read)
}

/// 可注入读法的全量刷新（测试与重建共用）。
pub fn refresh_with<F>(catalog: &CatalogDb, now_ms: i64, read: F) -> Result<BackfillReport>
where
    F: Fn(&Path, &str, Option<i64>) -> Option<(ExifData, Option<TakenAt>)>,
{
    sync_with(catalog, now_ms, false, read)
}

fn sync_with<F>(
    catalog: &CatalogDb,
    now_ms: i64,
    only_missing: bool,
    read: F,
) -> Result<BackfillReport>
where
    F: Fn(&Path, &str, Option<i64>) -> Option<(ExifData, Option<TakenAt>)>,
{
    let root: PathBuf = catalog.root().to_path_buf();
    let grouped: BTreeMap<i64, Vec<FileRow>> = catalog.read(|conn| {
        let sql = if only_missing {
            "SELECT f.asset_id, f.role, f.rel_path, f.mtime_ms
               FROM asset_files f
               JOIN assets a ON a.id = f.asset_id
              WHERE a.taken_at IS NULL AND f.missing_since IS NULL
              ORDER BY f.asset_id, (f.role = 'raw'), f.rel_path"
        } else {
            "SELECT f.asset_id, f.role, f.rel_path, f.mtime_ms
               FROM asset_files f
              WHERE f.missing_since IS NULL
              ORDER BY f.asset_id, (f.role = 'raw'), f.rel_path"
        };
        // 位图排在 RAW 前面（`role = 'raw'` 为真时排后面）。
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                FileRow {
                    role: row.get(1)?,
                    rel_path: row.get(2)?,
                    mtime_ms: row.get(3)?,
                },
            ))
        })?;
        let mut grouped: BTreeMap<i64, Vec<FileRow>> = BTreeMap::new();
        for row in rows {
            let (asset_id, file) = row?;
            grouped.entry(asset_id).or_default().push(file);
        }
        Ok(grouped)
    })?;

    let mut report = BackfillReport {
        candidates: grouped.len(),
        ..BackfillReport::default()
    };
    let mut updates: Vec<(i64, ExifData, Option<TakenAt>)> = Vec::new();

    for (asset_id, files) in &grouped {
        let mut reads: Vec<(ExifData, Option<TakenAt>)> = Vec::new();
        let mut any_unreadable = false;
        for file in files {
            let abs = root.join(&file.rel_path);
            let name = file.rel_path.rsplit('/').next().unwrap_or(&file.rel_path);
            match read(&abs, name, file.mtime_ms) {
                Some(reading) => reads.push(reading),
                None => any_unreadable = true,
            }
        }
        if reads.is_empty() {
            // 一个文件都读不了：离线盘或被删 —— 原样留着，不算「读完仍没有」
            if any_unreadable {
                report.unreadable += 1;
            }
            continue;
        }
        /*
         * 时间取「来源最可信」的那个；同级别时取先到的（位图排在前面）。
         * 器材/曝光这些字段一律取**第一个读到的位图**那份（RAW 的读法字段更少）。
         */
        let mut best: Option<TakenAt> = None;
        for (_, taken) in &reads {
            let Some(candidate) = taken else { continue };
            let better = best.is_none_or(|current| {
                source_rank(candidate.source) > source_rank(current.source)
            });
            if better {
                best = Some(*candidate);
            }
        }
        let primary = reads
            .first()
            .map(|(data, _)| data.clone())
            .unwrap_or_default();
        if best.is_none() && primary == ExifData::default() {
            report.still_missing += 1;
            continue;
        }
        updates.push((*asset_id, primary, best));
    }

    if !updates.is_empty() {
        let total = updates.len();
        let count = catalog.write_tx(move |tx| {
            let mut written = 0usize;
            for (asset_id, data, taken) in &updates {
                if only_missing {
                    // 回填期间用户又改了这一行 → 不覆盖（只填空值）
                    let still_empty: bool = tx.query_row(
                        "SELECT taken_at IS NULL FROM assets WHERE id = ?1",
                        [asset_id],
                        |row| row.get(0),
                    )?;
                    if !still_empty {
                        continue;
                    }
                }
                assets::apply_exif(tx, *asset_id, data, *taken, now_ms)?;
                written += 1;
            }
            Ok(written)
        })?;
        report.filled = count;
        report.still_missing += total - count;
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use rusqlite::Connection;

    use super::*;
    use crate::media::exif::TakenAtSource;
    use crate::store::db::{CatalogDb, OpenOpts};
    use crate::store::migration::{self, Backups, DbKind};

    const T0: i64 = 1_789_000_000_000;

    fn taken(millis: i64, source: TakenAtSource) -> TakenAt {
        TakenAt {
            millis,
            offset_min: Some(480),
            source,
        }
    }

    /// 一个临时库：目录 + 已迁移的 `catalog.db`。
    fn temp_catalog() -> (tempfile::TempDir, CatalogDb) {
        let dir = tempfile::tempdir().expect("临时目录");
        let root = dir.path().to_path_buf();
        {
            let mut conn = Connection::open(root.join("catalog.db")).expect("开库");
            migration::apply(&mut conn, DbKind::Catalog, Backups::none(), T0).expect("迁移");
            // 库身份：`CatalogDb::open` 靠它认这是不是库（`repository.rs` 的 key）
            conn.execute(
                "INSERT INTO repository_meta(key, value) VALUES ('repository_id', '0BackfillTest001')",
                [],
            )
            .expect("写库身份");
        }
        let catalog = CatalogDb::open(&root, OpenOpts::unbacked_up(T0)).expect("打开库");
        (dir, catalog)
    }

    /// 造一个资产 + 一个文件行。
    fn asset_with_file(catalog: &CatalogDb, rel: &str, role: &str, mtime: Option<i64>) -> i64 {
        let rel = rel.to_string();
        let role = role.to_string();
        catalog
            .write(move |conn| {
                let id = assets::insert_asset(conn, T0)?;
                conn.execute(
                    "INSERT INTO asset_files
                        (asset_id, role, rel_path, rel_path_folded, ext, size_bytes, mtime_ms,
                         created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?3, 'jpg', 10, ?4, ?5, ?5)",
                    rusqlite::params![id, role, rel, mtime, T0],
                )?;
                Ok(id)
            })
            .expect("插入")
    }

    fn read_time(catalog: &CatalogDb, id: i64) -> (Option<i64>, Option<String>, Option<i64>) {
        catalog
            .read(move |conn| {
                Ok(conn.query_row(
                    "SELECT taken_at, taken_at_source, taken_at_offset_min FROM assets WHERE id = ?1",
                    [id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?)
            })
            .expect("读回")
    }

    #[test]
    fn fills_the_time_and_the_camera_from_the_file() {
        let (_dir, catalog) = temp_catalog();
        let id = asset_with_file(&catalog, "photos/a.jpg", "bitmap", None);
        let report = backfill_with(&catalog, T0 + 100, |_abs, _name, _mtime| {
            let data = ExifData {
                camera_model: Some("DC-G9".to_string()),
                ..ExifData::default()
            };
            Some((data, Some(taken(T0 - 5000, TakenAtSource::Exif))))
        })
        .expect("回填");

        assert_eq!(report.candidates, 1);
        assert_eq!(report.filled, 1);
        assert_eq!(report.still_missing, 0);
        let (at, source, offset) = read_time(&catalog, id);
        assert_eq!(at, Some(T0 - 5000));
        assert_eq!(source.as_deref(), Some("exif"));
        assert_eq!(offset, Some(480));
        let model: Option<String> = catalog
            .read(move |conn| {
                Ok(conn.query_row("SELECT camera_model FROM assets WHERE id = ?1", [id], |r| {
                    r.get(0)
                })?)
            })
            .expect("读回");

        assert_eq!(model.as_deref(), Some("DC-G9"));
    }

    #[test]
    fn an_existing_time_is_never_touched() {
        let (_dir, catalog) = temp_catalog();
        let id = asset_with_file(&catalog, "photos/a.jpg", "bitmap", None);
        catalog
            .write(move |conn| {
                conn.execute(
                    "UPDATE assets SET taken_at = ?1, taken_at_source = 'exif' WHERE id = ?2",
                    rusqlite::params![T0 - 1, id],
                )?;
                Ok(())
            })
            .expect("预置时间");

        let report = backfill_with(&catalog, T0 + 100, |_abs, _name, _mtime| {
            Some((ExifData::default(), Some(taken(T0 - 5000, TakenAtSource::Exif))))
        })
        .expect("回填");

        assert_eq!(report.candidates, 0, "已经有时间的资产不进候选");
        assert_eq!(report.filled, 0);
        assert_eq!(read_time(&catalog, id).0, Some(T0 - 1));
    }

    #[test]
    fn refresh_replaces_existing_wrong_dimensions_but_preserves_user_fields() {
        let (_dir, catalog) = temp_catalog();
        let id = asset_with_file(&catalog, "photos/portrait.jpg", "bitmap", None);
        catalog
            .write(move |conn| {
                conn.execute(
                    "UPDATE assets SET taken_at = ?1, width = 4000, height = 3000,
                         orientation = 1, rating = 5, description = '保留我'
                       WHERE id = ?2",
                    rusqlite::params![T0 - 1, id],
                )?;
                Ok(())
            })
            .expect("预置错误元数据");

        let report = refresh_with(&catalog, T0 + 100, |_abs, _name, _mtime| {
            Some((
                ExifData {
                    width: Some(6000),
                    height: Some(4000),
                    orientation: Some(6),
                    ..ExifData::default()
                },
                Some(taken(T0 - 2, TakenAtSource::Exif)),
            ))
        })
        .expect("全量刷新");

        assert_eq!(report.candidates, 1, "已有 taken_at 也必须进全量刷新");
        assert_eq!(report.filled, 1);
        let row: (
            Option<i64>,
            Option<i64>,
            Option<i64>,
            i64,
            Option<String>,
            Option<i64>,
        ) = catalog
            .read(move |conn| {
                Ok(conn.query_row(
                    "SELECT width, height, orientation, rating, description, taken_at
                       FROM assets WHERE id = ?1",
                    [id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                        ))
                    },
                )?)
            })
            .expect("读回");
        assert_eq!(row.0, Some(6000));
        assert_eq!(row.1, Some(4000));
        assert_eq!(row.2, Some(6));
        assert_eq!(row.3, 5, "星级不能被重建抹掉");
        assert_eq!(row.4.as_deref(), Some("保留我"), "文字不能被重建抹掉");
        assert_eq!(row.5, Some(T0 - 2), "文件事实应刷新");
    }

    #[test]
    fn the_bitmap_reading_wins_over_the_raw_one() {
        let (_dir, catalog) = temp_catalog();
        let id = asset_with_file(&catalog, "photos/a.jpg", "bitmap", None);
        // 同一个资产的 RAW 行（配对规则下两者共享一个资产）
        catalog
            .write(move |conn| {
                conn.execute(
                    "INSERT INTO asset_files
                        (asset_id, role, rel_path, rel_path_folded, ext, size_bytes, mtime_ms,
                         created_at, updated_at)
                     VALUES (?1, 'raw', 'photos/_RAW/a.rw2', 'photos/_raw/a.rw2', 'rw2', 90, NULL, ?2, ?2)",
                    rusqlite::params![id, T0],
                )?;
                Ok(())
            })
            .expect("插 RAW 行");

        let report = backfill_with(&catalog, T0 + 100, |abs, _name, _mtime| {
            // 位图给真时间，RAW 只能退到 mtime（典型情形）
            if abs.to_string_lossy().ends_with(".jpg") {
                Some((ExifData::default(), Some(taken(T0 - 9000, TakenAtSource::Exif))))
            } else {
                Some((ExifData::default(), Some(taken(T0, TakenAtSource::FileMtime))))
            }
        })
        .expect("回填");

        assert_eq!(report.filled, 1);
        let (at, source, _) = read_time(&catalog, id);
        assert_eq!(at, Some(T0 - 9000), "取来源更可信的那个（位图的 EXIF）");
        assert_eq!(source.as_deref(), Some("exif"));
    }

    #[test]
    fn a_missing_file_is_counted_and_left_alone() {
        let (_dir, catalog) = temp_catalog();
        let id = asset_with_file(&catalog, "photos/gone.jpg", "bitmap", Some(T0));
        let report = backfill_with(&catalog, T0 + 100, |_abs: &Path, _name, _mtime| None)
            .expect("回填");

        assert_eq!(report.candidates, 1);
        assert_eq!(report.unreadable, 1);
        assert_eq!(report.filled, 0);
        assert_eq!(read_time(&catalog, id).0, None);
    }

    #[test]
    fn a_file_without_any_readable_metadata_is_reported_as_still_missing() {
        let (_dir, catalog) = temp_catalog();
        let id = asset_with_file(&catalog, "photos/plain.jpg", "bitmap", None);
        let report = backfill_with(&catalog, T0 + 100, |_abs, _name, _mtime| {
            Some((ExifData::default(), None))
        })
        .expect("回填");

        assert_eq!(report.still_missing, 1);
        assert_eq!(report.filled, 0);
        assert_eq!(read_time(&catalog, id).0, None);
    }

    #[test]
    fn the_mtime_fallback_is_written_but_marked_as_such() {
        // 没有 EXIF 的照片不该被补成空白：退到文件名 / mtime，并如实记下来源
        let (_dir, catalog) = temp_catalog();
        let id = asset_with_file(&catalog, "photos/scan.jpg", "bitmap", Some(T0 - 3000));
        let report = backfill_with(&catalog, T0 + 100, |_abs, _name, mtime| {
            Some((
                ExifData::default(),
                mtime.map(|ms| taken(ms, TakenAtSource::FileMtime)),
            ))
        })
        .expect("回填");

        assert_eq!(report.filled, 1);
        let (at, source, offset) = read_time(&catalog, id);
        assert_eq!(at, Some(T0 - 3000));
        assert_eq!(source.as_deref(), Some("file_mtime"));
        assert_eq!(offset, Some(480), "来源是 mtime 也要带上偏移（这里由假读法给的）");
    }

    #[test]
    fn running_twice_is_idempotent_and_empty_libraries_are_fine() {
        let (_dir, catalog) = temp_catalog();
        let _ = asset_with_file(&catalog, "photos/a.jpg", "bitmap", None);
        let first = backfill_with(&catalog, T0 + 100, |_abs, _name, _mtime| {
            Some((ExifData::default(), Some(taken(T0 - 5000, TakenAtSource::Exif))))
        })
        .expect("第一次");
        let second = backfill_with(&catalog, T0 + 200, |_abs, _name, _mtime| {
            Some((ExifData::default(), Some(taken(T0 - 6000, TakenAtSource::Exif))))
        })
        .expect("第二次");

        assert_eq!(first.filled, 1);
        assert_eq!(second.candidates, 0, "填过的资产不再是候选");
        assert_eq!(second.filled, 0);

        let (_empty_dir, empty) = temp_catalog();
        let empty_report = backfill_metadata(&empty, T0).expect("空库也不该出错");
        assert_eq!(empty_report, BackfillReport::default());
    }

    #[test]
    fn unicode_paths_and_multiple_files_are_handled() {
        let (_dir, catalog) = temp_catalog();
        let id = asset_with_file(&catalog, "photos/2026-08-15 婚礼/IMG_0001.JPG", "bitmap", None);
        let report = backfill_with(&catalog, T0 + 100, |abs, name, _mtime| {
            // 路径拼对了没有：目录 + 文件名都要能对上
            assert!(abs.to_string_lossy().contains("婚礼"));
            assert_eq!(name, "IMG_0001.JPG");
            Some((ExifData::default(), Some(taken(T0 - 7, TakenAtSource::Exif))))
        })
        .expect("回填");
        assert_eq!(report.filled, 1);
        assert_eq!(read_time(&catalog, id).0, Some(T0 - 7));
    }
}
