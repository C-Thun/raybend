//! `catalog.db` 侧的导入落库：`import_runs` / `import_items` / 资产登记 / 序号计数。
//!
//! [`CatalogSink`] 是 [`ImportSink`] 的真实实现。它把执行器要做的每一件事
//! **攒起来**，到 `commit()` 时在一个写事务里做掉（`plans/M1-6.md` §3.3 的「批事务」）：
//!
//! ```text
//! 攒（Vec<Op>）                    提交（一个 write_tx）
//!   record_plan  → InsertItem…      → BEGIN; 逐条 apply; COMMIT
//!   register     → Register         ── 一条一提交会让大目录慢得没法看，
//!   mark         → Mark                整批一次又让崩溃续跑变差（默认每 200 条一提交）
//! ```
//!
//! 两条必须守住的顺序（`plans/M1-6.md` §3.3）：
//!
//! 1. **`pending` 先落，`imported` 后落** —— 中间被杀时，重开能凭 `target_rel`
//!    + 状态判断这条到底落没落；
//! 2. **`begin_run` 立刻提交**（不攒）—— 不然崩溃之后根本不知道上次在导什么。
//!
//! 资产登记**不走 `apply_diff`**：它的配对键是「同目录 + 同名主体」，而
//! `REPOSITORY.md` §4.1 的 `_RAW/` 约定下 RAW 与位图天生不同目录 ——
//! 直接用它会把一张照片拆成两条资产（网格里两个格子）。这里改成
//! 「把 `_RAW/` 折算回位图那个目录再找资产」，其余（新建资产、插文件行）用的还是
//! `store::assets` 里那几个函数。

use std::collections::HashMap;

use rusqlite::params;

use crate::error::Result;
use crate::import::plan::{KnownSources, PlannedItem, Sequences, SourceFile};
use crate::import::runner::{ImportSink, RunCounts, RunRequest};
use crate::media::diff::DiskFile;
use crate::media::exif::{self, ExifData, TakenAt, TakenAtSource};
use crate::media::kind::MediaKind;
use crate::store::assets;
use crate::store::db::CatalogDb;
use crate::store::file_id::FileId;
use crate::store::path_semantics::PathForms;

/// 攒下来、等 `commit()` 一起做的一条动作。
#[derive(Debug, Clone, PartialEq)]
enum Op {
    /// 规划结果落成 `import_items` 行（pending / skipped / failed）。
    InsertItem {
        run_id: i64,
        source_path: String,
        target_rel: Option<String>,
        status: String,
        reason: Option<String>,
    },
    /// 登记一个已复制的文件。
    Register {
        run_id: i64,
        target_rel: String,
        kind: MediaKind,
        size_bytes: u64,
        mtime_ms: Option<i64>,
        /// 副本的**创建时间**（右栏「创建日期」；文件系统不给就是 `None`）。
        created_ms: Option<i64>,
        /// 库内**副本**的身份（读不到就 `None`）。
        copy_identity: Option<FileId>,
        /// 源文件绝对路径（溯源 + 兜底判重）。
        source_path: String,
        /// 源文件身份。
        source_identity: Option<FileId>,
        /// 从**库内副本**读到的 EXIF（读不到时是默认值 —— 那就什么都不写）。
        ///
        /// 装箱：`ExifData` 比这条枚举里别的载荷大一大截，直接放进去会让所有
        /// `Op` 都跟着变大（clippy 的 `large_enum_variant`）。
        exif: Box<ExifData>,
        /// 由那份 EXIF 定出的拍摄时间（含来源与时区偏移）。
        taken: Option<TakenAt>,
    },
    /// 更新一条的状态。
    Mark {
        run_id: i64,
        target_rel: String,
        status: String,
        reason: Option<String>,
    },
    /// 序号计数写回。
    SaveSequences(Vec<(String, usize, u64)>),
    /// 收尾一个 run。
    FinishRun {
        run_id: i64,
        state: String,
        counts: RunCounts,
    },
}

/// 真实的落库出口（`catalog.db`，走单写者）。
pub struct CatalogSink<'a> {
    catalog: &'a CatalogDb,
    now_ms: i64,
    ops: Vec<Op>,
}

impl<'a> CatalogSink<'a> {
    /// 绑到一个已经打开的库。
    #[must_use]
    pub fn new(catalog: &'a CatalogDb, now_ms: i64) -> Self {
        Self {
            catalog,
            now_ms,
            ops: Vec::new(),
        }
    }

    /// 还攒着几条没提交（诊断与测试用）。
    #[must_use]
    pub fn pending_ops(&self) -> usize {
        self.ops.len()
    }

    /// 把攒下的动作一次做完。
    fn flush(&mut self) -> Result<()> {
        if self.ops.is_empty() {
            return Ok(());
        }
        let ops = std::mem::take(&mut self.ops);
        let now = self.now_ms;
        self.catalog.write_tx(move |tx| {
            for op in &ops {
                apply(tx, op, now)?;
            }
            Ok(())
        })
    }
}

impl ImportSink for CatalogSink<'_> {
    fn known_sources(&mut self) -> Result<KnownSources> {
        self.flush()?;
        self.catalog.read(load_known_sources)
    }

    fn sequences(&mut self) -> Result<Sequences> {
        self.flush()?;
        self.catalog.read(load_sequences)
    }

    fn save_sequences(&mut self, sequences: &Sequences) -> Result<()> {
        self.ops.push(Op::SaveSequences(sequences.snapshot()));
        Ok(())
    }

    fn begin_run(&mut self, request: &RunRequest) -> Result<i64> {
        // 顺序要紧：先把上一段做完，再开新 run
        self.flush()?;
        let source_root = request.source_root.display().to_string();
        let template = request.template_source.clone();
        let include_subdirs = request.include_subdirs;
        let now = self.now_ms;
        self.catalog.write(move |conn| {
            conn.execute(
                "INSERT INTO import_runs
                    (source_root, template, include_subdirs, started_at, state, imported, skipped, failed)
                 VALUES (?1, ?2, ?3, ?4, 'running', 0, 0, 0)",
                params![source_root, template, i64::from(include_subdirs), now],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

    fn record_plan(
        &mut self,
        run_id: i64,
        files: &[SourceFile],
        items: &[PlannedItem],
    ) -> Result<()> {
        for item in items {
            self.ops.push(Op::InsertItem {
                run_id,
                source_path: files[item.index].abs_path.display().to_string(),
                target_rel: item.target_rel().map(ToString::to_string),
                status: item.status_str().to_string(),
                reason: item.reason(),
            });
        }
        Ok(())
    }

    fn stale_pending(&mut self) -> Result<HashMap<String, String>> {
        self.flush()?;
        self.catalog.read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT i.target_rel, i.source_path
                   FROM import_items i
                   JOIN import_runs r ON r.id = i.run_id
                  WHERE i.status = 'pending'
                    AND i.target_rel IS NOT NULL
                    AND r.state = 'running'",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut out = HashMap::new();
            for row in rows {
                let (target, source) = row?;
                out.insert(target, source);
            }
            Ok(out)
        })
    }

    fn register(
        &mut self,
        run_id: i64,
        item: &PlannedItem,
        file: &SourceFile,
        copy_identity: Option<FileId>,
    ) -> Result<()> {
        let Some(target_rel) = item.target_rel() else {
            return Ok(());
        };
        /*
         * 元数据在这里读（**库内副本**、在写事务之外）——
         * 以前这一块根本没做，于是导入进库的照片 `taken_at` 永远是 NULL
         * （人类 2026-09-18 上报「库里的时间全是 NULL」，根因就在这里）。
         *
         * 读的是**库里那份**：源文件可能在导入后被移走，而用户看到的就是库里这份。
         * 兜底用的 mtime 要传**源文件的**（`file.mtime_ms`）—— 副本的 mtime 是复制时间，
         * 拿它当拍摄时间是错的（而且与导入网格显示的那个值对不上）。
         */
        let abs = self.catalog.root().join(target_rel);
        let exif = exif::read_file_for(&abs);
        let file_name = target_rel.rsplit('/').next().unwrap_or(target_rel);
        let taken = exif::resolve_taken_at(Some(&exif), file_name, file.mtime_ms);
        self.ops.push(Op::Register {
            run_id,
            target_rel: target_rel.to_string(),
            kind: file.kind,
            size_bytes: file.size_bytes,
            mtime_ms: file.mtime_ms,
            created_ms: file.created_ms,
            copy_identity,
            source_path: file.abs_path.display().to_string(),
            source_identity: file.identity,
            exif: Box::new(exif),
            taken,
        });
        Ok(())
    }

    fn mark(
        &mut self,
        run_id: i64,
        item: &PlannedItem,
        status: &str,
        reason: Option<&str>,
    ) -> Result<()> {
        let Some(target_rel) = item.target_rel() else {
            return Ok(());
        };
        self.ops.push(Op::Mark {
            run_id,
            target_rel: target_rel.to_string(),
            status: status.to_string(),
            reason: reason.map(ToString::to_string),
        });
        Ok(())
    }

    fn finish_run(&mut self, run_id: i64, state: &str, counts: &RunCounts) -> Result<()> {
        self.ops.push(Op::FinishRun {
            run_id,
            state: state.to_string(),
            counts: *counts,
        });
        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        self.flush()
    }
}

/// 在一个写事务/连接里做掉一条动作。
fn apply(conn: &rusqlite::Connection, op: &Op, now_ms: i64) -> Result<()> {
    match op {
        Op::InsertItem {
            run_id,
            source_path,
            target_rel,
            status,
            reason,
        } => {
            conn.execute(
                "INSERT INTO import_items
                    (run_id, asset_id, source_path, target_rel, status, reason, created_at)
                 VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6)",
                params![run_id, source_path, target_rel, status, reason, now_ms],
            )?;
        }
        Op::Register {
            run_id,
            target_rel,
            kind,
            size_bytes,
            mtime_ms,
            created_ms,
            copy_identity,
            source_path,
            source_identity,
            exif,
            taken,
        } => {
            let forms = PathForms::new(target_rel);
            let asset_id = find_or_create_asset(conn, target_rel, now_ms)?;
            let disk = DiskFile {
                rel_path: target_rel.clone(),
                rel_path_folded: forms.folded().to_string(),
                kind: *kind,
                size_bytes: *size_bytes,
                mtime_ms: *mtime_ms,
                created_ms: *created_ms,
                identity: *copy_identity,
            };
            assets::insert_file(conn, asset_id, &disk, assets::role_of(*kind), now_ms)?;
            assets::set_source(
                conn,
                forms.folded(),
                source_path,
                *source_identity,
                now_ms,
            )?;
            /*
             * EXIF 只由**位图**写，RAW 只在「这个资产没有位图」时才写。
             *
             * 为什么：同一张照片的位图与 RAW 共用一个 `assets` 行，而 RAW 那边
             * 能读到的字段往往更少（镜头名、部分曝光字段）—— 让后登记的那个去覆盖，
             * 会把位图读到的信息冲成空。时间同理，只是多一层「不许降级」（见下）。
             */
            let has_bitmap: i64 = conn.query_row(
                "SELECT count(*) FROM asset_files WHERE asset_id = ?1 AND role = 'bitmap'",
                [asset_id],
                |row| row.get(0),
            )?;
            if matches!(kind, MediaKind::Image) || has_bitmap == 0 {
                let effective = keep_better_taken(conn, asset_id, *taken)?;
                assets::apply_exif(conn, asset_id, exif, effective, now_ms)?;
            }
            conn.execute(
                "UPDATE import_items
                    SET asset_id = ?1, status = 'copied', reason = NULL
                  WHERE run_id = ?2 AND target_rel = ?3",
                params![asset_id, run_id, target_rel],
            )?;
        }
        Op::Mark {
            run_id,
            target_rel,
            status,
            reason,
        } => {
            conn.execute(
                "UPDATE import_items SET status = ?1, reason = ?2
                  WHERE run_id = ?3 AND target_rel = ?4",
                params![status, reason, run_id, target_rel],
            )?;
        }
        Op::SaveSequences(rows) => {
            for (directory, width, value) in rows {
                conn.execute(
                    "INSERT INTO seq_counters(directory, width, value, updated_at)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(directory, width)
                     DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                    params![
                        directory,
                        i64::try_from(*width).unwrap_or(9),
                        i64::try_from(*value).unwrap_or(i64::MAX),
                        now_ms
                    ],
                )?;
            }
        }
        Op::FinishRun {
            run_id,
            state,
            counts,
        } => {
            conn.execute(
                "UPDATE import_runs
                    SET finished_at = ?1, state = ?2, imported = ?3, skipped = ?4, failed = ?5
                  WHERE id = ?6",
                params![
                    now_ms,
                    state,
                    i64::try_from(counts.imported).unwrap_or(i64::MAX),
                    i64::try_from(counts.skipped).unwrap_or(i64::MAX),
                    i64::try_from(counts.failed).unwrap_or(i64::MAX),
                    run_id
                ],
            )?;
        }
    }
    Ok(())
}

/// 时间的「**不许降级**」规则：现有时间比这次读到的好，就保留现有的。
///
/// 位图与 RAW 共用一个 `assets` 行，而登记顺序由规划决定、并不保证；
/// 若 RAW 那侧只读到 mtime（相机没给 RAW 写时间）而位图那侧读到了真 EXIF 时间，
/// 「后写的赢」就会把好时间冲掉。所以按来源定级，只允许往上换：
///
/// `EXIF(3) > 同名位图继承(2) > 文件名(1) > 文件 mtime(0)`。
/// 来源列缺失的老数据（早期版本写过的行）按最可信处理 —— 不确定就不动它。
fn keep_better_taken(
    conn: &rusqlite::Connection,
    asset_id: i64,
    incoming: Option<TakenAt>,
) -> Result<Option<TakenAt>> {
    use rusqlite::OptionalExtension;

    let current: Option<(Option<i64>, Option<String>, Option<i64>)> = conn
        .query_row(
            "SELECT taken_at, taken_at_source, taken_at_offset_min FROM assets WHERE id = ?1",
            [asset_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((Some(millis), source, offset_min)) = current else {
        return Ok(incoming);
    };
    let current = TakenAt {
        millis,
        offset_min: offset_min.and_then(|v| i32::try_from(v).ok()),
        source: match source.as_deref() {
            Some("file_mtime") => TakenAtSource::FileMtime,
            Some("filename") => TakenAtSource::Filename,
            Some("sibling") => TakenAtSource::Sibling,
            // `exif` 与认不出来的写法（含 NULL）都当最可信
            _ => TakenAtSource::Exif,
        },
    };
    let rank = exif::source_rank;
    if rank(current.source) >= incoming.map_or(-1, |t| rank(t.source)) {
        Ok(Some(current))
    } else {
        Ok(incoming)
    }
}

/// 找这张照片的资产：**把 `_RAW/` 折算回位图那个目录**再找；没有就新建一个。
fn find_or_create_asset(conn: &rusqlite::Connection, target_rel: &str, now_ms: i64) -> Result<i64> {
    let (dir, file_name) = match target_rel.rsplit_once('/') {
        Some((dir, name)) => (dir, name),
        None => ("", target_rel),
    };
    // `photos/2026-08-15/_RAW` → 配对目录是 `photos/2026-08-15`（大小写不敏感：
    // 规划出来的路径保留原文，而库里的路径是折叠的）
    let pairing_dir = strip_raw_segment(dir);
    let stem = crate::media::kind::stem_folded(file_name);

    match assets::find_asset_for_group(conn, &fold(pairing_dir), &stem)? {
        Some(id) => Ok(id),
        None => assets::insert_asset(conn, now_ms),
    }
}

/// 折掉最后那一段 `_RAW`（只有最后一段才算）。
fn strip_raw_segment(dir: &str) -> &str {
    const SUFFIX: &str = "/_RAW";
    match dir.len().checked_sub(SUFFIX.len()) {
        Some(at) if dir[at..].eq_ignore_ascii_case(SUFFIX) => &dir[..at],
        _ => dir,
    }
}

/// 库里记着「哪些源已经导进来过」（`REPOSITORY.md` §4.3）。
fn load_known_sources(conn: &rusqlite::Connection) -> Result<KnownSources> {
    let mut known = KnownSources::new();
    let mut stmt = conn.prepare(
        "SELECT source_path, size_bytes, mtime_ms, source_volume_serial, source_file_id
           FROM asset_files
          WHERE missing_since IS NULL
            AND (source_path IS NOT NULL OR source_volume_serial IS NOT NULL)",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let source_path: Option<String> = row.get(0)?;
        let size: Option<i64> = row.get(1)?;
        let mtime: Option<i64> = row.get(2)?;
        let volume: Option<i64> = row.get(3)?;
        let blob: Option<Vec<u8>> = row.get(4)?;

        if let Some(path) = source_path {
            known.insert_fallback(
                &path,
                u64::try_from(size.unwrap_or(0)).unwrap_or(0),
                mtime,
            );
        }
        if let (Some(volume), Some(blob)) = (volume, blob)
            && let Some(id) = FileId::from_blob(u64::try_from(volume).unwrap_or(0), &blob)
        {
            known.insert_identity(id);
        }
    }
    Ok(known)
}

/// 库里记着的序号计数。
fn load_sequences(conn: &rusqlite::Connection) -> Result<Sequences> {
    let mut sequences = Sequences::new();
    let mut stmt = conn.prepare("SELECT directory, width, value FROM seq_counters")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let directory: String = row.get(0)?;
        let width: i64 = row.get(1)?;
        let value: i64 = row.get(2)?;
        sequences.seed(
            &directory,
            usize::try_from(width).unwrap_or(3),
            u64::try_from(value).unwrap_or(0),
        );
    }
    Ok(sequences)
}

fn fold(path: &str) -> String {
    PathForms::new(path).folded().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::plan::{ItemOutcome, PlannedItem, Role, Sequences, SourceExtras};
    use crate::media::scan::ScannedFile;
    use crate::store::db::OpenOpts;
    use std::path::{Path, PathBuf};

    const T0: i64 = 1_789_516_800_000;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().expect("临时目录")
    }

    fn catalog(dir: &Path) -> CatalogDb {
        CatalogDb::create(dir, "测试库", None, OpenOpts::unbacked_up(T0)).expect("建库")
    }

    fn source_file(rel: &str, kind: MediaKind, abs: &str, size: u64) -> SourceFile {
        let name = rel.rsplit('/').next().unwrap_or(rel).to_string();
        SourceFile::from_scanned(
            &ScannedFile {
                abs_path: PathBuf::from(abs),
                rel_path: rel.to_string(),
                ext: crate::media::kind::extension(&name),
                kind,
                stem_folded: crate::media::kind::stem_folded(&name),
                file_name: name,
                size_bytes: size,
                mtime_ms: Some(T0 - 1000),
            },
            SourceExtras {
                identity: Some(FileId::new(5, [3u8; 16])),
                taken_at: Some(T0 - 5000),
                brand: None,
                model: None,
            },
        )
    }

    fn planned(index: usize, source_rel: &str, target: &str, role: Role) -> PlannedItem {
        PlannedItem {
            index,
            source_rel: source_rel.to_string(),
            role: Some(role),
            outcome: ItemOutcome::Plan {
                target_rel: target.to_string(),
            },
            missing: Vec::new(),
        }
    }

    fn request(root: &str) -> RunRequest {
        RunRequest {
            index: 0,
            source_root: PathBuf::from(root),
            photos_dir: "photos".to_string(),
            template: crate::import::template::parse(":CYEAR/:FILENAME").expect("模版"),
            template_source: ":CYEAR/:FILENAME".to_string(),
            include_subdirs: true,
            avoid_duplicates: true,
            excluded: std::sync::Arc::new(std::collections::HashSet::new()),
        }
    }

    fn counts(imported: u64, skipped: u64, failed: u64) -> RunCounts {
        RunCounts {
            total: imported + skipped + failed,
            imported,
            skipped,
            duplicates: 0,
            failed,
            bytes: 1234,
        }
    }

    /* ══════════════════════════════════════════════════════════ */

    #[test]
    fn a_run_row_is_written_immediately() {
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let run_id = sink.begin_run(&request("/src")).expect("开 run");
        assert!(run_id > 0);
        assert_eq!(sink.pending_ops(), 0, "begin_run 不攒，立刻落库");

        let (state, root, template, finished): (String, String, String, Option<i64>) = db
            .read(|conn| {
                conn.query_row(
                    "SELECT state, source_root, template, finished_at FROM import_runs WHERE id = ?1",
                    [run_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .map_err(Into::into)
            })
            .expect("查 run");
        assert_eq!(state, "running");
        assert_eq!(root, "/src");
        assert_eq!(template, ":CYEAR/:FILENAME", "模版要留痕（模版会变）");
        assert_eq!(finished, None);
    }

    #[test]
    fn plan_items_are_stored_with_their_status() {
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let run_id = sink.begin_run(&request("/src")).unwrap();
        let files = [
            source_file("a.jpg", MediaKind::Image, "/src/a.jpg", 10),
            source_file("b.jpg", MediaKind::Image, "/src/b.jpg", 20),
        ];
        let items = vec![
            planned(0, "a.jpg", "photos/2026/a.jpg", Role::Bitmap),
            PlannedItem {
                index: 1,
                source_rel: "b.jpg".to_string(),
                role: Some(Role::Bitmap),
                outcome: ItemOutcome::Skipped("已在库中（按文件身份判重）".to_string()),
                missing: Vec::new(),
            },
        ];
        sink.record_plan(run_id, &files, &items).unwrap();
        assert_eq!(sink.pending_ops(), 2, "攒着，等 commit");
        sink.commit().unwrap();
        assert_eq!(sink.pending_ops(), 0);

        let rows: Vec<(String, Option<String>, String, Option<String>)> = db
            .read(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT source_path, target_rel, status, reason FROM import_items
                      WHERE run_id = ?1 ORDER BY id",
                )?;
                let rows = stmt.query_map([run_id], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })?;
                Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
            })
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "/src/a.jpg", "存的是源**绝对**路径（溯源）");
        assert_eq!(rows[0].1.as_deref(), Some("photos/2026/a.jpg"));
        assert_eq!(rows[0].2, "pending", "规划出来的先落 pending");
        assert_eq!(rows[1].2, "skipped");
        assert!(rows[1].3.is_some(), "跳过要带原因");
    }

    #[test]
    fn registering_a_bitmap_creates_the_asset_and_records_the_source() {
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let run_id = sink.begin_run(&request("/src")).unwrap();
        let file = source_file("a.jpg", MediaKind::Image, "/src/2026/a.jpg", 10);
        let item = planned(0, "a.jpg", "photos/2026/a.jpg", Role::Bitmap);
        sink.record_plan(run_id, std::slice::from_ref(&file), std::slice::from_ref(&item))
            .unwrap();
        sink.register(run_id, &item, &file, Some(FileId::new(9, [1u8; 16])))
            .unwrap();
        sink.mark(run_id, &item, "imported", None).unwrap();
        sink.commit().unwrap();

        /// 登记之后库里该有的样子。
        struct AfterRegistration {
            assets: i64,
            files: i64,
            role: String,
            source_path: Option<String>,
            source_path_folded: Option<String>,
            source_volume: Option<i64>,
            item_status: String,
            item_asset_id: Option<i64>,
        }

        let after: AfterRegistration = db
            .read(|conn| {
                let assets_n: i64 =
                    conn.query_row("SELECT count(*) FROM assets", [], |r| r.get(0))?;
                let files_n: i64 =
                    conn.query_row("SELECT count(*) FROM asset_files", [], |r| r.get(0))?;
                let row = conn.query_row(
                    "SELECT role, source_path, source_path_folded, source_volume_serial
                       FROM asset_files WHERE rel_path_folded = 'photos/2026/a.jpg'",
                    [],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, Option<String>>(1)?,
                            r.get::<_, Option<String>>(2)?,
                            r.get::<_, Option<i64>>(3)?,
                        ))
                    },
                )?;
                let (status, asset_id): (String, Option<i64>) = conn.query_row(
                    "SELECT status, asset_id FROM import_items WHERE run_id = ?1",
                    [run_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                Ok(AfterRegistration {
                    assets: assets_n,
                    files: files_n,
                    role: row.0,
                    source_path: row.1,
                    source_path_folded: row.2,
                    source_volume: row.3,
                    item_status: status,
                    item_asset_id: asset_id,
                })
            })
            .unwrap();

        assert_eq!(after.assets, 1);
        assert_eq!(after.files, 1);
        assert_eq!(after.role, "bitmap");
        assert_eq!(after.source_path.as_deref(), Some("/src/2026/a.jpg"));
        assert_eq!(after.source_path_folded.as_deref(), Some("/src/2026/a.jpg"));
        assert_eq!(after.source_volume, Some(5), "源身份要落库");
        assert_eq!(after.item_status, "imported");
        assert!(after.item_asset_id.is_some(), "import_items 要关联到资产");
    }

    #[test]
    fn raw_under_raw_dir_joins_the_bitmap_asset() {
        // `_RAW/` 折算：位图与它的 RAW 必须是**同一条资产**，
        // 否则网格里一张照片会出现两个格子
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let run_id = sink.begin_run(&request("/src")).unwrap();
        let bitmap = source_file("P0001.png", MediaKind::Image, "/src/P0001.png", 10);
        let raw = source_file("P0001.ORF", MediaKind::Raw, "/src/P0001.ORF", 20);
        let items = [
            planned(0, "P0001.png", "photos/2026/MYP0001.png", Role::Bitmap),
            planned(1, "P0001.ORF", "photos/2026/_RAW/MYP0001.ORF", Role::Raw),
        ];
        let files = [bitmap, raw];
        sink.record_plan(run_id, &files, &items).unwrap();
        sink.register(run_id, &items[0], &files[0], None).unwrap();
        sink.register(run_id, &items[1], &files[1], None).unwrap();
        sink.commit().unwrap();

        let (assets_n, files_n, roles): (i64, i64, Vec<String>) = db
            .read(|conn| {
                let assets_n: i64 =
                    conn.query_row("SELECT count(*) FROM assets", [], |r| r.get(0))?;
                let files_n: i64 =
                    conn.query_row("SELECT count(*) FROM asset_files", [], |r| r.get(0))?;
                let mut stmt =
                    conn.prepare("SELECT role FROM asset_files ORDER BY id")?;
                let roles = stmt
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok((assets_n, files_n, roles))
            })
            .unwrap();
        assert_eq!(assets_n, 1, "一张照片一条资产");
        assert_eq!(files_n, 2);
        assert_eq!(roles, vec!["bitmap", "raw"]);
    }

    #[test]
    fn raw_without_a_bitmap_gets_its_own_asset() {
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let run_id = sink.begin_run(&request("/src")).unwrap();
        let raw = source_file("only.ORF", MediaKind::Raw, "/src/only.ORF", 20);
        let item = planned(0, "only.ORF", "photos/2026/MYonly.ORF", Role::Raw);
        sink.record_plan(run_id, std::slice::from_ref(&raw), std::slice::from_ref(&item))
            .unwrap();
        sink.register(run_id, &item, &raw, None).unwrap();
        sink.commit().unwrap();

        let (assets_n, files_n): (i64, i64) = db
            .read(|conn| {
                Ok((
                    conn.query_row("SELECT count(*) FROM assets", [], |r| r.get(0))?,
                    conn.query_row("SELECT count(*) FROM asset_files", [], |r| r.get(0))?,
                ))
            })
            .unwrap();
        assert_eq!((assets_n, files_n), (1, 1));
    }

    #[test]
    fn a_second_import_into_the_same_directory_reuses_the_existing_asset() {
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let run_id = sink.begin_run(&request("/src")).unwrap();
        // 先导位图
        let bitmap = source_file("P1.png", MediaKind::Image, "/src/P1.png", 10);
        let item_b = planned(0, "P1.png", "photos/2026/MYP1.png", Role::Bitmap);
        sink.record_plan(run_id, std::slice::from_ref(&bitmap), std::slice::from_ref(&item_b))
            .unwrap();
        sink.register(run_id, &item_b, &bitmap, None).unwrap();
        sink.commit().unwrap();

        // 再导同名的 RAW（正常目录，不是 _RAW/）—— 应当挂到同一条资产上
        let raw = source_file("P1.ORF", MediaKind::Raw, "/src/P1.ORF", 20);
        let item_r = planned(0, "P1.ORF", "photos/2026/MYP1.ORF", Role::Raw);
        sink.record_plan(run_id, std::slice::from_ref(&raw), std::slice::from_ref(&item_r))
            .unwrap();
        sink.register(run_id, &item_r, &raw, None).unwrap();
        sink.commit().unwrap();

        let (assets_n, files_n): (i64, i64) = db
            .read(|conn| {
                Ok((
                    conn.query_row("SELECT count(*) FROM assets", [], |r| r.get(0))?,
                    conn.query_row("SELECT count(*) FROM asset_files", [], |r| r.get(0))?,
                ))
            })
            .unwrap();
        assert_eq!((assets_n, files_n), (1, 2), "位图 + 后补的 RAW = 一张照片");
    }

    #[test]
    fn known_sources_round_trips_through_the_database() {
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let run_id = sink.begin_run(&request("/src")).unwrap();
        let file = source_file("a.jpg", MediaKind::Image, "/src/2026/a.jpg", 10);
        let item = planned(0, "a.jpg", "photos/2026/a.jpg", Role::Bitmap);
        sink.record_plan(run_id, std::slice::from_ref(&file), std::slice::from_ref(&item))
            .unwrap();
        sink.register(run_id, &item, &file, None).unwrap();
        sink.commit().unwrap();

        let mut fresh = CatalogSink::new(&db, T0);
        let known = fresh.known_sources().unwrap();
        assert!(known.contains(&file), "身份与兜底键都要能命中");
        // 路径一样但大小不同 → 不算同一张
        let other = SourceFile {
            size_bytes: 999,
            ..source_file("a.jpg", MediaKind::Image, "/src/2026/a.jpg", 10)
        };
        assert!(known.contains(&other), "身份命中优先（大小不同也认）");
    }

    #[test]
    fn a_missing_repo_copy_does_not_count_as_known() {
        // 库里的副本没了（missing）→ 允许再导一次把它找回来
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let run_id = sink.begin_run(&request("/src")).unwrap();
        let file = source_file("a.jpg", MediaKind::Image, "/src/a.jpg", 10);
        let item = planned(0, "a.jpg", "photos/2026/a.jpg", Role::Bitmap);
        sink.record_plan(run_id, std::slice::from_ref(&file), std::slice::from_ref(&item))
            .unwrap();
        sink.register(run_id, &item, &file, None).unwrap();
        sink.commit().unwrap();
        db.write(|conn| assets::mark_missing(conn, 1, T0)).unwrap();

        let mut fresh = CatalogSink::new(&db, T0);
        assert!(
            !fresh.known_sources().unwrap().contains(&file),
            "标了 missing 的不算「已在库中」"
        );
    }

    #[test]
    fn sequences_survive_a_round_trip() {
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let mut sequences = Sequences::new();
        sequences.seed("photos/2026-08-15", 3, 41);
        sequences.next("photos/2026-08-15", 4);
        sink.save_sequences(&sequences).unwrap();
        sink.commit().unwrap();

        let mut fresh = CatalogSink::new(&db, T0);
        let loaded = fresh.sequences().unwrap();
        assert_eq!(loaded.last("photos/2026-08-15", 3), 41);
        assert_eq!(loaded.last("photos/2026-08-15", 4), 1, "宽度不同 = 不同计数器");

        // 再存一次是覆盖而不是插重
        let mut again = Sequences::new();
        again.seed("photos/2026-08-15", 3, 42);
        sink.save_sequences(&again).unwrap();
        sink.commit().unwrap();
        let rows: i64 = db
            .read(|conn| {
                conn.query_row("SELECT count(*) FROM seq_counters", [], |r| r.get(0))
                    .map_err(Into::into)
            })
            .unwrap();
        assert_eq!(rows, 2);
        assert_eq!(
            CatalogSink::new(&db, T0).sequences().unwrap().last("photos/2026-08-15", 3),
            42
        );
    }

    #[test]
    fn stale_pending_only_comes_from_unfinished_runs() {
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let run_a = sink.begin_run(&request("/src/a")).unwrap();
        let files = [source_file("a.jpg", MediaKind::Image, "/src/a/a.jpg", 10)];
        let items = [planned(0, "a.jpg", "photos/2026/a.jpg", Role::Bitmap)];
        sink.record_plan(run_a, &files, &items).unwrap();
        sink.commit().unwrap();

        // 还没收尾的 run → 算「半成品」
        let stale = sink.stale_pending().unwrap();
        assert_eq!(
            stale.get("photos/2026/a.jpg").map(String::as_str),
            Some("/src/a/a.jpg")
        );

        // 收尾之后就不算了（那一条是正常结束的）
        sink.finish_run(run_a, "done", &counts(0, 0, 0)).unwrap();
        sink.commit().unwrap();
        assert!(sink.stale_pending().unwrap().is_empty());

        // 另一个 run 里失败的条目也不算
        let run_b = sink.begin_run(&request("/src/b")).unwrap();
        let item = PlannedItem {
            index: 0,
            source_rel: "b.jpg".to_string(),
            role: Some(Role::Bitmap),
            outcome: ItemOutcome::Failed("路径不合法".to_string()),
            missing: Vec::new(),
        };
        sink.record_plan(run_b, &[source_file("b.jpg", MediaKind::Image, "/src/b/b.jpg", 5)], &[item])
            .unwrap();
        sink.commit().unwrap();
        assert!(sink.stale_pending().unwrap().is_empty(), "只有 pending 才算半成品");
    }

    #[test]
    fn finish_run_records_state_and_counts() {
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let run_id = sink.begin_run(&request("/src")).unwrap();
        sink.finish_run(run_id, "cancelled", &counts(3, 1, 2)).unwrap();
        sink.commit().unwrap();

        let (state, imported, skipped, failed, finished): (String, i64, i64, i64, Option<i64>) =
            db.read(|conn| {
                conn.query_row(
                    "SELECT state, imported, skipped, failed, finished_at
                       FROM import_runs WHERE id = ?1",
                    [run_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .map_err(Into::into)
            })
            .unwrap();
        assert_eq!((state.as_str(), imported, skipped, failed), ("cancelled", 3, 1, 2));
        assert_eq!(finished, Some(T0));
    }

    #[test]
    fn commit_with_nothing_pending_is_a_noop() {
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        sink.commit().unwrap();
        assert_eq!(sink.pending_ops(), 0);
    }

    #[test]
    fn raw_registered_before_its_bitmap_still_ends_up_in_one_asset() {
        // 反过来的顺序（先 RAW 后位图）也要配上对：一边是源目录里先扫到谁不确定，
        // 另一边是「先导了 RAW、过些天又导了它的 JPEG」这种真实场景
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let run_id = sink.begin_run(&request("/src")).unwrap();
        let raw = source_file("P1.ORF", MediaKind::Raw, "/src/P1.ORF", 20);
        let bitmap = source_file("P1.png", MediaKind::Image, "/src/P1.png", 10);
        let item_raw = planned(0, "P1.ORF", "photos/2026/_RAW/MYP1.ORF", Role::Raw);
        let item_bitmap = planned(1, "P1.png", "photos/2026/MYP1.png", Role::Bitmap);
        let files = [raw, bitmap];
        let items = [item_raw, item_bitmap];
        sink.record_plan(run_id, &files, &items).unwrap();
        // 先 RAW
        sink.register(run_id, &items[0], &files[0], None).unwrap();
        sink.commit().unwrap();
        // 再位图
        sink.register(run_id, &items[1], &files[1], None).unwrap();
        sink.commit().unwrap();

        let (assets_n, files_n, roles): (i64, i64, Vec<String>) = db
            .read(|conn| {
                let assets_n: i64 =
                    conn.query_row("SELECT count(*) FROM assets", [], |r| r.get(0))?;
                let files_n: i64 =
                    conn.query_row("SELECT count(*) FROM asset_files", [], |r| r.get(0))?;
                let mut stmt = conn.prepare("SELECT role FROM asset_files ORDER BY id")?;
                let roles = stmt
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok((assets_n, files_n, roles))
            })
            .unwrap();
        assert_eq!(assets_n, 1, "一张照片一条资产");
        assert_eq!(files_n, 2);
        assert_eq!(roles, vec!["raw", "bitmap"]);
    }

    #[test]
    fn raw_segment_matching_is_case_insensitive() {
        // 库里存的是折叠路径（小写），调用方手里可能是原文 —— 两头都得认
        assert_eq!(strip_raw_segment("photos/2026/_RAW"), "photos/2026");
        assert_eq!(strip_raw_segment("photos/2026/_raw"), "photos/2026");
        assert_eq!(strip_raw_segment("photos/2026"), "photos/2026");
        assert_eq!(strip_raw_segment("_RAW"), "_RAW", "根目录下这段不算");
        assert_eq!(strip_raw_segment("photos/_RAWx"), "photos/_RAWx", "不是整段");
    }
}
