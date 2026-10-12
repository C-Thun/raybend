//! `catalog.db` 侧的导入落库：`import_runs` / `import_items` / 资产登记 / 序号计数。
//!
//! [`CatalogSink`] 是 [`ImportSink`] 的真实实现。它把执行器要做的每一件事
//! **攒起来**，到 `commit()` 时在一个写事务里做掉（`specs/M1-6.md` §3.3 的「批事务」）：
//!
//! ```text
//! 攒（Vec<Op>）                    提交（一个 write_tx）
//!   record_plan  → InsertItem…      → BEGIN; 逐条 apply; COMMIT
//!   register     → Register         ── 一条一提交会让大目录慢得没法看，
//!   mark         → Mark                整批一次又让崩溃续跑变差（默认每 200 条一提交）
//! ```
//!
//! 两条必须守住的顺序（`specs/M1-6.md` §3.3）：
//!
//! 1. **`pending` 先落，`imported` 后落** —— 中间被杀时，重开能凭 `target_rel`
//!    + 状态判断这条到底落没落；
//! 2. **`begin_run` 立刻提交**（不攒）—— 不然崩溃之后根本不知道上次在导什么。
//!
//! 资产登记**不走 `apply_diff`**：它的配对键是「同目录 + 同名主体」，而
//! `memory/FUNCTION-REPOSITORY.md` §4.1 的 `_RAW/` 约定下 RAW 与位图天生不同目录 ——
//! 直接用它会把一张照片拆成两条资产（网格里两个格子）。这里改成
//! 「把 `_RAW/` 折算回位图那个目录再找资产」，其余（新建资产、插文件行）用的还是
//! `store::assets` 里那几个函数。

use std::collections::HashMap;

use rusqlite::params;

use crate::error::Result;
use crate::import::plan::{
    KnownSources, KnownTargets, LibraryEntry, PlannedItem, Sequences, SourceFile,
};
use crate::import::runner::{ImportSink, RunCounts, RunRequest};
use crate::media::diff::DiskFile;
use crate::media::exif::{self, ExifData, TakenAt, TakenAtSource};
use crate::media::fingerprint::{ContentFingerprint, HashKind};
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
        /// 该文件的**展示尺寸 + 自己的方向**（`media::meta::read_photo_meta`，
        /// 与 EXIF 同一次登记里读的头；位图与 RAW 各一份，`specs/display-size-source.md` §3.2）。
        /// 读不到尺寸时是 `None` —— 那些列就留 NULL（消费方回退）。
        display: Option<crate::media::meta::PhotoMeta>,
        /// 库内副本的内容指纹（位图全文 / RAW 采样；`None` = 没算成）。
        fingerprint: Option<ContentFingerprint>,
        /// 这个资产所在的文件组是不是「整组完整导入」（边车采纳的门，见 `NewAsset`）。
        complete: bool,
    },
    /// 回填一条内容指纹（判重时顺手为库内老行建立的）。
    SetContentHash {
        rel_path: String,
        fingerprint: ContentFingerprint,
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
    recovery: Option<&'a crate::store::session::TaskCatalog>,
    now_ms: i64,
    ops: Vec<Op>,
    /// 本批**新建**的资产（sidecar 采纳用；`take_new_assets` 取走）。
    new_assets: Vec<NewAsset>,
}

/// 新登记时**真创建**了 `assets` 行的照片（往已有资产上补文件不算）。
/// `source_file_abs` = 源文件绝对路径 —— sidecar 探测在源目录（`_RAW/` 折算后）。
#[derive(Debug, Clone, PartialEq)]
pub struct NewAsset {
    pub asset_id: i64,
    pub target_rel: String,
    pub source_file_abs: String,
    /// 这个资产所在的**文件组**是不是「整组完整导入」了（没跳过、没失败、没改名）。
    ///
    /// 边车（`.xmp`）采纳靠它把门：拆开的情况（例：位图改名 `_01`、RAW 跳过）
    /// 不算完整导入，不采纳配置（`specs/import-dedup-fingerprint.md` §9）。
    pub complete: bool,
}

impl<'a> CatalogSink<'a> {
    /// 绑到一个已经打开的库。
    #[must_use]
    pub fn new(catalog: &'a CatalogDb, now_ms: i64) -> Self {
        Self {
            catalog,
            recovery: None,
            now_ms,
            ops: Vec::new(),
            new_assets: Vec::new(),
        }
    }

    pub fn recovering(
        catalog: &'a CatalogDb,
        task: &'a crate::store::session::TaskCatalog,
        now_ms: i64,
    ) -> Self {
        Self {
            catalog,
            recovery: Some(task),
            now_ms,
            ops: Vec::new(),
            new_assets: Vec::new(),
        }
    }
    fn active(&self) -> ActiveCatalog<'_> {
        match self.recovery {
            Some(task) => ActiveCatalog::Shared(task.current()),
            None => ActiveCatalog::Borrowed(self.catalog),
        }
    }
    /// 还攒着几条没提交（诊断与测试用）。
    #[must_use]
    pub fn pending_ops(&self) -> usize {
        self.ops.len()
    }

    /// 取走本批**新建**的资产（sidecar 采纳用，`specs/xmp-w1.md` §7；只回真创建的那些）。
    pub fn take_new_assets(&mut self) -> Vec<NewAsset> {
        std::mem::take(&mut self.new_assets)
    }

    /// 把攒下的动作一次做完。
    fn flush(&mut self) -> Result<()> {
        if self.ops.is_empty() {
            return Ok(());
        }
        let ops = self.ops.clone();
        let now = self.now_ms;
        let created = self.active().write_tx(move |tx| {
            let mut created = Vec::new();
            for op in &ops {
                apply(tx, op, now, &mut created)?;
            }
            let mut runs = std::collections::HashSet::new();
            for op in &ops {
                match op { Op::InsertItem { run_id, .. } | Op::Register { run_id, .. } | Op::Mark { run_id, .. } => { runs.insert(*run_id); }, _ => {} }
            }
            for id in runs {
                tx.execute("UPDATE import_runs SET imported=(SELECT count(*) FROM import_items WHERE run_id=?1 AND status='imported'), skipped=(SELECT count(*) FROM import_items WHERE run_id=?1 AND status='skipped'), failed=(SELECT count(*) FROM import_items WHERE run_id=?1 AND status='failed') WHERE id=?1 AND state='running'", [id])?;
            }
            Ok(created)
        })?;
        self.new_assets.extend(created);
        self.ops.clear();
        Ok(())
    }
}

enum ActiveCatalog<'a> {
    Borrowed(&'a CatalogDb),
    Shared(std::sync::Arc<CatalogDb>),
}
impl std::ops::Deref for ActiveCatalog<'_> {
    type Target = CatalogDb;
    fn deref(&self) -> &CatalogDb {
        match self {
            Self::Borrowed(db) => db,
            Self::Shared(db) => db,
        }
    }
}

impl ImportSink for CatalogSink<'_> {
    fn check_session(&self) -> Result<()> {
        self.active().ensure_current()
    }
    fn known_sources(&mut self) -> Result<KnownSources> {
        self.flush()?;
        self.active().read(load_known_sources)
    }

    fn known_targets(&mut self) -> Result<KnownTargets> {
        self.flush()?;
        self.active().read(|conn| {
            // 含标了缺失的行：盘上找不到不代表这个**路径**能再用 ——
            // 库里那行还在，向同一路径插新行依旧会撞 `UNIQUE(rel_path_folded)`。
            // 顺手把大小与已有指纹带出来：判「同一份内容」时能直接用，不必再读盘。
            let mut stmt = conn.prepare(
                "SELECT rel_path, size_bytes, content_hash, content_hash_kind FROM asset_files",
            )?;
            let rows = stmt.query_map([], |row| {
                let rel_path: String = row.get(0)?;
                let size: Option<i64> = row.get(1)?;
                let hex: Option<String> = row.get(2)?;
                let kind: Option<String> = row.get(3)?;
                Ok(LibraryEntry {
                    rel_path,
                    size_bytes: size.map(|value| value.max(0) as u64),
                    fingerprint: match (hex, kind) {
                        (Some(hex), Some(kind)) => {
                            HashKind::parse(&kind).map(|kind| ContentFingerprint::new(kind, hex))
                        }
                        _ => None,
                    },
                })
            })?;
            let mut targets = KnownTargets::new();
            for row in rows {
                targets.insert(row?);
            }
            Ok(targets)
        })
    }

    fn interrupted_sources(&mut self) -> Result<KnownSources> {
        self.flush()?;
        self.active()
            .read(|conn| load_known_sources_filtered(conn, true))
    }
    fn sequences(&mut self) -> Result<Sequences> {
        self.flush()?;
        self.active().read(load_sequences)
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
        self.active().write(move |conn| {
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
        self.active().read(|conn| {
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
        fingerprint: Option<ContentFingerprint>,
    ) -> Result<()> {
        let Some(target_rel) = item.target_rel() else {
            return Ok(());
        };
        /*
         * 冲突兜底①（数据库侧）：这个目标路径库里已经有人占着了吗 ——
         * 导入写进 `photos/` 会触发文件监听差分，`store::rebuild` 可能
         * **抢在我们前面**把同名文件登记进库（`specs/import-dedup-fingerprint.md` §1.3）。
         *
         * 判据是「角色 + 大小」：一致 → 照常推 op（`apply` 会认领那一行，不再插）；
         * 不一致 → 真冲突，直接返回错误 —— 执行器按「单条失败」处理，
         * 不碰库里的记录，也不带崩整批。
         */
        if let Some(existing) = self
            .active()
            .read(|conn| assets::find_file_by_rel_path(conn, target_rel))?
            && !existing.matches_copy(assets::role_of(file.kind), file.size_bytes)
        {
            return Err(crate::Error::ContentConflict(target_rel.to_string()));
        }
        /*
         * 元数据在这里读（**库内副本**、在写事务之外）——
         * 以前这一块根本没做，于是导入进库的照片 `taken_at` 永远是 NULL
         * （人类 2026-09-18 上报「库里的时间全是 NULL」，根因就在这里）。
         *
         * 读的是**库里那份**：源文件可能在导入后被移走，而用户看到的就是库里这份。
         * 兜底用的 mtime 要传**源文件的**（`file.mtime_ms`）—— 副本的 mtime 是复制时间，
         * 拿它当拍摄时间是错的（而且与导入网格显示的那个值对不上）。
         */
        let abs = self.active().root().join(target_rel);
        let exif = exif::read_file_for(&abs);
        /*
         * 展示尺寸与方向**只读这一次头**（`specs/display-size-source.md` §3.2）：
         * `read_photo_meta` 自己会先试真栅格格式的头部、再退 EXIF —— 与 EXIF 那次
         * 读法分开（`image_dimensions` 拿到的尺寸比 EXIF 字段可信）。
         * sidecar（`.xmp`）没有像素，不读。
         */
        let display = match file.kind {
            MediaKind::Other => None,
            // RAW 的展示尺寸 = **出图尺寸**（rawler 显影管线的输出），不是文件头整帧
            // —— 与懒回填 / 批量回填同一个口径（`store::sizes`）。
            _ => crate::media::meta::read_photo_meta(&abs)
                .ok()
                .map(|meta| crate::store::sizes::apply_display_size_source(&abs, meta)),
        };
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
            display,
            fingerprint,
            complete: item.complete,
        });
        Ok(())
    }

    fn set_content_hash(
        &mut self,
        rel_path: &str,
        fingerprint: &ContentFingerprint,
    ) -> Result<()> {
        self.ops.push(Op::SetContentHash {
            rel_path: rel_path.to_string(),
            fingerprint: fingerprint.clone(),
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

/// 在一个写事务/连接里做掉一条动作。`new_assets` 收集本批真创建的资产（Register 分支）。
fn apply(
    conn: &rusqlite::Connection,
    op: &Op,
    now_ms: i64,
    new_assets: &mut Vec<NewAsset>,
) -> Result<()> {
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
                 SELECT ?1, NULL, ?2, ?3, ?4, ?5, ?6 WHERE NOT EXISTS(SELECT 1 FROM import_items WHERE run_id=?1 AND source_path=?2 AND target_rel IS ?3)",
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
            display,
            fingerprint,
            complete,
        } => {
            let facts = CopyFacts {
                target_rel,
                role: assets::role_of(*kind),
                size_bytes: *size_bytes,
                mtime_ms: *mtime_ms,
                created_ms: *created_ms,
                identity: *copy_identity,
                source_path,
                source_identity: *source_identity,
                fingerprint: fingerprint.clone(),
                complete: *complete,
            };
            // 冲突兜底②：库里有行就认领（同步竞态、我们自己的半成品都走这条）。
            if let Some(existing) = assets::find_file_by_rel_path(conn, target_rel)? {
                return adopt_occupied(conn, *run_id, &facts, &existing, now_ms);
            }
            /*
             * 真插入：整段包在 savepoint 里。万一还有别的写者抢在前头，
             * 回滚到 savepoint（连新建的 `assets` 行一起撤）后再走同一条兜底 ——
             * **绝不**让一条冲突把整个批事务带走（2026-10-12 真机事故）。
             */
            conn.execute_batch("SAVEPOINT rb_register")?;
            match register_fresh(
                conn, *run_id, &facts, *kind, exif, *taken, *display, now_ms, new_assets,
            ) {
                Ok(()) => conn.execute_batch("RELEASE rb_register")?,
                Err(error) if unique_violation(&error) => {
                    conn.execute_batch("ROLLBACK TO rb_register; RELEASE rb_register")?;
                    match assets::find_file_by_rel_path(conn, target_rel)? {
                        Some(existing) => {
                            adopt_occupied(conn, *run_id, &facts, &existing, now_ms)?;
                        }
                        // 行又没了（另一个写者又把它删了）：把原始错误交出去，
                        // 由执行器记入失败清单。
                        None => return Err(error),
                    }
                }
                Err(error) => return Err(error),
            }
        }
        Op::SetContentHash {
            rel_path,
            fingerprint,
        } => {
            // 幂等：库里已经有指纹就不覆盖（`set_content_hash` 里带了守卫）
            assets::set_content_hash(conn, rel_path, fingerprint, now_ms)?;
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
                     DO UPDATE SET value = max(seq_counters.value, excluded.value), updated_at = excluded.updated_at",
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
fn find_or_create_asset(
    conn: &rusqlite::Connection,
    target_rel: &str,
    now_ms: i64,
) -> Result<(i64, bool)> {
    let (dir, file_name) = match target_rel.rsplit_once('/') {
        Some((dir, name)) => (dir, name),
        None => ("", target_rel),
    };
    // `photos/2026-08-15/_RAW` → 配对目录是 `photos/2026-08-15`（大小写不敏感：
    // 规划出来的路径保留原文，而库里的路径是折叠的）
    let pairing_dir = strip_raw_segment(dir);
    let stem = crate::media::kind::stem_folded(file_name);

    match assets::find_asset_for_group(conn, &fold(pairing_dir), &stem)? {
        Some(id) => Ok((id, false)),
        None => Ok((assets::insert_asset(conn, now_ms)?, true)),
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

/// 一份「刚复制到位的副本」的事实（登记与认领都要用）。
struct CopyFacts<'a> {
    target_rel: &'a str,
    role: &'a str,
    size_bytes: u64,
    mtime_ms: Option<i64>,
    created_ms: Option<i64>,
    identity: Option<FileId>,
    source_path: &'a str,
    source_identity: Option<FileId>,
    /// 库内副本的内容指纹（位图全文 / RAW 采样）。
    fingerprint: Option<ContentFingerprint>,
    /// 这个资产所在的文件组是不是「整组完整导入」（边车采纳的门）。
    complete: bool,
}

/// 真插一行新记录（`assets` 需要时新建）—— 这条路径当时确实空着。
#[allow(clippy::too_many_arguments)]
fn register_fresh(
    conn: &rusqlite::Connection,
    run_id: i64,
    facts: &CopyFacts<'_>,
    kind: MediaKind,
    exif: &ExifData,
    taken: Option<TakenAt>,
    display: Option<crate::media::meta::PhotoMeta>,
    now_ms: i64,
    new_assets: &mut Vec<NewAsset>,
) -> Result<()> {
    let forms = PathForms::new(facts.target_rel);
    let (asset_id, created) = find_or_create_asset(conn, facts.target_rel, now_ms)?;
    if created {
        new_assets.push(NewAsset {
            asset_id,
            target_rel: facts.target_rel.to_string(),
            source_file_abs: facts.source_path.to_string(),
            complete: facts.complete,
        });
    }
    let disk = DiskFile {
        rel_path: facts.target_rel.to_string(),
        rel_path_folded: forms.folded().to_string(),
        kind,
        size_bytes: facts.size_bytes,
        mtime_ms: facts.mtime_ms,
        created_ms: facts.created_ms,
        identity: facts.identity,
    };
    assets::insert_file(
        conn,
        asset_id,
        &disk,
        facts.role,
        display,
        facts.fingerprint.as_ref(),
        now_ms,
    )?;
    assets::set_source(
        conn,
        forms.folded(),
        facts.source_path,
        facts.source_identity,
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
        let effective = keep_better_taken(conn, asset_id, taken)?;
        assets::apply_exif(conn, asset_id, exif, effective, now_ms)?;
    }
    mark_item_imported(conn, run_id, facts.target_rel, asset_id)
}

/// 目标路径**已经被登记过**时的兜底（`specs/import-dedup-fingerprint.md` §7）。
///
/// * 与刚复制到位的这份是同一份文件（角色 + 大小一致）→ **认领**：不插新行，
///   把来源列补上、把副本的新身份/时间写回去、把这一条挂到它所属的 `asset_id`；
/// * 否则只把**这一条**标失败（原因写清楚），库与磁盘都不动。
fn adopt_occupied(
    conn: &rusqlite::Connection,
    run_id: i64,
    facts: &CopyFacts<'_>,
    existing: &assets::ExistingFile,
    now_ms: i64,
) -> Result<()> {
    if !existing.matches_copy(facts.role, facts.size_bytes) {
        return fail_item(
            conn,
            run_id,
            facts.target_rel,
            &format!("库内已有同名路径、内容不同：{}", facts.target_rel),
        );
    }
    assets::update_stat(
        conn,
        existing.row_id,
        facts.size_bytes,
        facts.mtime_ms,
        facts.created_ms,
        now_ms,
    )?;
    assets::update_identity(conn, existing.row_id, facts.identity)?;
    assets::clear_missing(conn, existing.row_id, now_ms)?;
    // 库里那行还没指纹而我们刚算了一份 → 顺手补上（下次判重省一次读盘）。
    // 已经有指纹就不动：同一份内容的指纹不会变（`set_content_hash` 自己也是幂等的）。
    if existing.fingerprint.is_none()
        && let Some(fingerprint) = &facts.fingerprint
    {
        assets::set_content_hash(conn, facts.target_rel, fingerprint, now_ms)?;
    }
    assets::set_source(
        conn,
        PathForms::new(facts.target_rel).folded(),
        facts.source_path,
        facts.source_identity,
        now_ms,
    )?;
    mark_item_imported(conn, run_id, facts.target_rel, existing.asset_id)
}

/// 登记成功：条目状态**一步到 `imported`**（`specs/M1-6.md` §3.3：`copied` 与
/// `imported` 之间没有需要区分的时刻；分两步写反而会让兜底标的 `failed`
/// 被后一条 `Mark(imported)` 盖掉）。
fn mark_item_imported(
    conn: &rusqlite::Connection,
    run_id: i64,
    target_rel: &str,
    asset_id: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE import_items SET asset_id = ?1, status = 'imported', reason = NULL
          WHERE run_id = ?2 AND target_rel = ?3",
        params![asset_id, run_id, target_rel],
    )?;
    Ok(())
}

/// 只让这一条失败（写进错误清单的原因文案由调用方给）。
fn fail_item(
    conn: &rusqlite::Connection,
    run_id: i64,
    target_rel: &str,
    reason: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE import_items SET status = 'failed', reason = ?1
          WHERE run_id = ?2 AND target_rel = ?3",
        params![reason, run_id, target_rel],
    )?;
    Ok(())
}

/// 是不是「唯一约束冲突」（`UNIQUE constraint failed: asset_files.rel_path_folded`）。
///
/// 只认这一种：外键、非空之类的约束错误是**别的问题**，不该被当成「路径被占」。
fn unique_violation(error: &crate::Error) -> bool {
    matches!(
        error,
        crate::Error::Database(rusqlite::Error::SqliteFailure(inner, _))
            if inner.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
    )
}

/// 库里记着「哪些源已经导进来过」（`memory/FUNCTION-REPOSITORY.md` §4.3）。
fn load_known_sources(conn: &rusqlite::Connection) -> Result<KnownSources> {
    load_known_sources_filtered(conn, false)
}
fn load_known_sources_filtered(
    conn: &rusqlite::Connection,
    interrupted: bool,
) -> Result<KnownSources> {
    let mut known = KnownSources::new();
    let mut stmt = conn.prepare(
        "SELECT source_path, size_bytes, mtime_ms, source_volume_serial, source_file_id
           FROM asset_files f
          WHERE missing_since IS NULL AND (source_path IS NOT NULL OR source_volume_serial IS NOT NULL)
            AND (?1=0 OR EXISTS(SELECT 1 FROM import_items i JOIN import_runs r ON r.id=i.run_id WHERE i.asset_id=f.asset_id AND i.target_rel=f.rel_path AND i.status='imported' AND r.state='running'))",
    )?;
    let mut rows = stmt.query([interrupted])?;
    while let Some(row) = rows.next()? {
        let source_path: Option<String> = row.get(0)?;
        let size: Option<i64> = row.get(1)?;
        let mtime: Option<i64> = row.get(2)?;
        let volume: Option<i64> = row.get(3)?;
        let blob: Option<Vec<u8>> = row.get(4)?;

        if let Some(path) = source_path {
            known.insert_fallback(&path, u64::try_from(size.unwrap_or(0)).unwrap_or(0), mtime);
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
                created_ms: None,
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
            // 测试里默认当「整组完整导入」：想验拆开的情形显式改。
            complete: true,
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

    /* ══════════════════════════════════════════════════════════════
     * 冲突兜底（`specs/import-dedup-fingerprint.md` §7）
     *
     * 2026-10-12 真机事故：导入写进 `photos/` 触发文件监听差分，
     * `store::rebuild` 抢先把同一个路径登记进库 —— 导入自己的 `Op::Register`
     * 再插一条就撞 `UNIQUE(rel_path_folded)`，一条冲突把**整批**带走了。
     * ══════════════════════════════════════════════════════════════ */

    /// 「别人」把同一个路径登记进去了（大小一致）→ 导入要**认领**那一行。
    #[test]
    fn register_adopts_a_row_that_someone_else_registered_first() {
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let run_id = sink.begin_run(&request("/src")).unwrap();
        let file = source_file("a.jpg", MediaKind::Image, "/src/2026/a.jpg", 10);
        let item = planned(0, "a.jpg", "photos/2026/a.jpg", Role::Bitmap);
        sink.record_plan(run_id, std::slice::from_ref(&file), std::slice::from_ref(&item))
            .unwrap();
        sink.commit().unwrap();

        // 差分同步先登记的那一行：路径一样、大小一样，但**来源列还空着**。
        let asset_id = db
            .write(|conn| {
                let asset = assets::insert_asset(conn, T0)?;
                let disk = DiskFile::new("photos/2026/a.jpg", 10, Some(T0 - 1000));
                assets::insert_file(conn, asset, &disk, "bitmap", None, None, T0)?;
                Ok(asset)
            })
            .unwrap();

        sink.register(run_id, &item, &file, Some(FileId::new(7, [7u8; 16])), None)
            .expect("大小一致 = 我们自己刚写下的那份，应该认领");
        sink.commit().expect("认领不该报错");

        let (rows, status, item_asset, source, volume, source_volume) = db
            .read(|conn| {
                let rows: i64 = conn.query_row(
                    "SELECT count(*) FROM asset_files WHERE rel_path_folded = 'photos/2026/a.jpg'",
                    [],
                    |r| r.get(0),
                )?;
                let (status, asset, source, volume, source_volume): (
                    String,
                    Option<i64>,
                    Option<String>,
                    Option<i64>,
                    Option<i64>,
                ) = conn.query_row(
                    "SELECT i.status, i.asset_id, f.source_path, f.volume_serial, f.source_volume_serial
                       FROM import_items i JOIN asset_files f ON f.rel_path_folded = i.target_rel
                      WHERE i.run_id = ?1",
                    [run_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )?;
                Ok((rows, status, asset, source, volume, source_volume))
            })
            .unwrap();
        assert_eq!(rows, 1, "认领不该再插第二条");
        assert_eq!(status, "imported");
        assert_eq!(item_asset, Some(asset_id), "这条要挂到已存在的那张照片上");
        assert_eq!(source.as_deref(), Some("/src/2026/a.jpg"), "来源列要补上（判重靠它）");
        assert_eq!(volume, Some(7), "副本的新身份要写进 volume_serial");
        assert_eq!(source_volume, Some(5), "源身份写进 source_volume_serial（两列分开）");
    }

    /// 冲突发生在**推 op 之后、提交之前**（文件监听差分抢跑）：
    /// 只让这一条失败，**其余照常登记、批照样提交**。
    #[test]
    fn a_conflicting_row_fails_one_item_without_killing_the_batch() {
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let run_id = sink.begin_run(&request("/src")).unwrap();
        let files = [
            source_file("a.jpg", MediaKind::Image, "/src/a.jpg", 10),
            source_file("b.jpg", MediaKind::Image, "/src/b.jpg", 20),
        ];
        let items = [
            planned(0, "a.jpg", "photos/a.jpg", Role::Bitmap),
            planned(1, "b.jpg", "photos/b.jpg", Role::Bitmap),
        ];
        sink.record_plan(run_id, &files, &items).unwrap();
        sink.commit().unwrap();

        // 两条登记 op 已经攒下（推的时候库里还是空的）—— 这时别人把 `photos/b.jpg` 占了。
        sink.register(run_id, &items[0], &files[0], None, None).unwrap();
        sink.register(run_id, &items[1], &files[1], None, None).unwrap();
        db.write(|conn| {
            let asset = assets::insert_asset(conn, T0)?;
            let disk = DiskFile::new("photos/b.jpg", 99, Some(T0 - 1000));
            assets::insert_file(conn, asset, &disk, "bitmap", None, None, T0)?;
            Ok(())
        })
        .unwrap();

        sink.commit().expect("单条冲突绝不能带崩整批");

        let (b_rows, a_rows, statuses) = db
            .read(|conn| {
                let count = |path: &str| -> Result<i64> {
                    Ok(conn.query_row(
                        "SELECT count(*) FROM asset_files WHERE rel_path_folded = ?1",
                        [path],
                        |r| r.get(0),
                    )?)
                };
                let mut stmt =
                    conn.prepare("SELECT status FROM import_items WHERE run_id = ?1 ORDER BY id")?;
                let statuses = stmt
                    .query_map([run_id], |r| r.get::<_, String>(0))?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok((count("photos/b.jpg")?, count("photos/a.jpg")?, statuses))
            })
            .unwrap();
        assert_eq!(a_rows, 1, "不冲突的那条照常进来");
        assert_eq!(b_rows, 1, "冲突那条保留别人登记的行，不覆盖也不新增");
        assert_eq!(statuses, vec!["imported", "failed"]);

        let (failed, reason) = db
            .read(|conn| {
                Ok(conn.query_row(
                    "SELECT failed, (SELECT reason FROM import_items WHERE run_id = ?1 AND status = 'failed')
                       FROM import_runs WHERE id = ?1",
                    [run_id],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?)),
                )?)
            })
            .unwrap();
        assert_eq!(failed, 1, "计数要如实反应那一条的失败");
        let reason = reason.expect("失败要带原因");
        assert!(reason.contains("内容不同"), "原因要能给人看懂：{reason}");
    }

    /// 推 op 那一刻库里有行且**不是同一份**（大小不同）→ 当场拒掉，
    /// 由执行器走「单条失败」，不浪费时间读 EXIF、也不碰库。
    #[test]
    fn register_rejects_a_path_occupied_by_a_different_file() {
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let run_id = sink.begin_run(&request("/src")).unwrap();
        let file = source_file("a.jpg", MediaKind::Image, "/src/a.jpg", 10);
        let item = planned(0, "a.jpg", "photos/a.jpg", Role::Bitmap);
        sink.record_plan(run_id, std::slice::from_ref(&file), std::slice::from_ref(&item))
            .unwrap();
        sink.commit().unwrap();
        db.write(|conn| {
            let asset = assets::insert_asset(conn, T0)?;
            let disk = DiskFile::new("photos/a.jpg", 99, Some(T0 - 1000));
            assets::insert_file(conn, asset, &disk, "bitmap", None, None, T0)?;
            Ok(())
        })
        .unwrap();

        let error = sink
            .register(run_id, &item, &file, None, None)
            .expect_err("同一路径上不是同一份文件，不能闷头认领");
        assert!(matches!(error, crate::Error::ContentConflict(_)), "{error:?}");
        assert_eq!(sink.pending_ops(), 0, "被拒的条目不该留下半截 op");
        sink.commit().unwrap();
        let rows = db
            .read(|conn| {
                Ok(conn.query_row(
                    "SELECT count(*) FROM asset_files WHERE rel_path_folded = 'photos/a.jpg'",
                    [],
                    |r| r.get::<_, i64>(0),
                )?)
            })
            .unwrap();
        assert_eq!(rows, 1, "库里的那一行不能被碰");
    }

    /// `known_targets` 交给规划的东西：**库里所有**路径，含标了缺失的行
    /// （盘上找不到不代表这个路径能再用 —— 库里那行还在，插新行依旧撞唯一索引）。
    #[test]
    fn known_targets_include_every_registered_path_even_missing_ones() {
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let run_id = sink.begin_run(&request("/src")).unwrap();
        let files = [
            source_file("a.jpg", MediaKind::Image, "/src/a.jpg", 10),
            source_file("b.jpg", MediaKind::Image, "/src/b.jpg", 10),
        ];
        let items = [
            planned(0, "a.jpg", "photos/A.jpg", Role::Bitmap),
            planned(1, "b.jpg", "photos/B.jpg", Role::Bitmap),
        ];
        sink.record_plan(run_id, &files, &items).unwrap();
        sink.register(run_id, &items[0], &files[0], None, None)
            .unwrap();
        sink.register(run_id, &items[1], &files[1], None, None)
            .unwrap();
        sink.commit().unwrap();
        db.write(|conn| {
            conn.execute(
                "UPDATE asset_files SET missing_since = ?1 WHERE rel_path_folded = 'photos/b.jpg'",
                [T0],
            )?;
            Ok(())
        })
        .unwrap();

        let targets = sink.known_targets().unwrap();
        assert!(targets.contains("photos/A.jpg"));
        assert!(targets.contains("photos/a.jpg"), "折叠比较（库里列是小写）");
        assert!(targets.contains("photos/B.jpg"), "标了缺失的行也占着这个路径");
    }

    #[test]
    fn replay_is_idempotent_and_deferred_source_never_rewinds_sequence() {
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let id = sink.begin_run(&request("/中文源")).unwrap();
        let file = source_file("a.jpg", MediaKind::Image, "/中文源/a.jpg", 3);
        let item = planned(0, "a.jpg", "photos/a.jpg", Role::Bitmap);
        sink.record_plan(id, std::slice::from_ref(&file), std::slice::from_ref(&item))
            .unwrap();
        let replay = sink.ops.clone();
        sink.commit().unwrap();
        sink.ops = replay;
        sink.commit().unwrap();
        assert_eq!(
            db.read(
                |c| Ok(c.query_row("SELECT count(*) FROM import_items", [], |r| r
                    .get::<_, i64>(0))?)
            )
            .unwrap(),
            1
        );
        let mut latest = Sequences::new();
        latest.seed("photos", 3, 10);
        sink.save_sequences(&latest).unwrap();
        sink.commit().unwrap();
        let mut delayed = Sequences::new();
        delayed.seed("photos", 3, 2);
        sink.save_sequences(&delayed).unwrap();
        sink.commit().unwrap();
        assert_eq!(
            db.read(|c| Ok(
                c.query_row("SELECT value FROM seq_counters", [], |r| r.get::<_, i64>(0))?
            ))
            .unwrap(),
            10
        );
    }
    #[test]
    fn failed_commit_retains_operations_for_verified_rebind_and_partial_counts() {
        use crate::store::session::{CatalogSessions, TaskCatalog};
        use std::sync::Arc;
        let dir = tmp();
        let root = dir.path().join("库");
        let db = catalog(&root);
        let id = db.meta().id.clone();
        drop(db);
        let sessions = CatalogSessions::default();
        let old = sessions
            .acquire(&id, || Ok(root.clone()), OpenOpts::unbacked_up(T0))
            .unwrap();
        let task = TaskCatalog::new(Arc::clone(&old));
        let mut sink = CatalogSink::recovering(&old, &task, T0);
        let run = sink.begin_run(&request("/src")).unwrap();
        let file = source_file("a.jpg", MediaKind::Image, "/src/a.jpg", 5);
        let item = planned(0, "a.jpg", "photos/a.jpg", Role::Bitmap);
        sink.record_plan(
            run,
            std::slice::from_ref(&file),
            std::slice::from_ref(&item),
        )
        .unwrap();
        sink.register(run, &item, &file, None, None).unwrap();
        sink.mark(run, &item, "imported", None).unwrap();
        let pending = sink.pending_ops();
        sessions.invalidate(&id).unwrap();
        assert!(sink.commit().is_err());
        assert_eq!(sink.pending_ops(), pending);
        let next = sessions
            .acquire(&id, || Ok(root.clone()), OpenOpts::unbacked_up(T0))
            .unwrap();
        task.install(next.clone()).unwrap();
        sink.commit().unwrap();
        assert_eq!(sink.pending_ops(), 0);
        assert_eq!(
            next.read(|c| Ok(c.query_row(
                "SELECT imported FROM import_runs WHERE id=?1",
                [run],
                |r| r.get::<_, i64>(0)
            )?))
            .unwrap(),
            1
        );
        assert!(sink.interrupted_sources().unwrap().contains(&file));
        sink.finish_run(run, "done", &counts(1, 0, 0)).unwrap();
        sink.commit().unwrap();
        assert!(!sink.interrupted_sources().unwrap().contains(&file));
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
                complete: false,
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
    fn new_assets_track_creation_only_not_extension() {
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let run_id = sink.begin_run(&request("/src")).unwrap();
        // 位图先落：真创建资产
        let bitmap = source_file("a.jpg", MediaKind::Image, "/src/2026/a.jpg", 10);
        let bitmap_item = planned(0, "a.jpg", "photos/2026/a.jpg", Role::Bitmap);
        sink.record_plan(run_id, std::slice::from_ref(&bitmap), std::slice::from_ref(&bitmap_item)).unwrap();
        sink.register(run_id, &bitmap_item, &bitmap, None, None).unwrap();
        sink.commit().unwrap();
        let created = sink.take_new_assets();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].target_rel, "photos/2026/a.jpg");
        assert_eq!(created[0].source_file_abs, "/src/2026/a.jpg");

        // 后补同主体 RAW：挂到同一资产，不再产生新资产记录
        let raw = source_file("a.orf", MediaKind::Raw, "/src/2026/a.orf", 20);
        let raw_item = planned(0, "a.orf", "photos/2026/_RAW/a.orf", Role::Raw);
        sink.record_plan(run_id, std::slice::from_ref(&raw), std::slice::from_ref(&raw_item)).unwrap();
        sink.register(run_id, &raw_item, &raw, None, None).unwrap();
        sink.commit().unwrap();
        assert!(sink.take_new_assets().is_empty(), "往已有资产补文件不算新建");

        let assets: i64 = db
            .read(|conn| {
                Ok(conn.query_row("SELECT count(*) FROM assets", [], |r| r.get(0))?)
            })
            .unwrap();
        assert_eq!(assets, 1);
    }

    #[test]
    fn registering_a_bitmap_creates_the_asset_and_records_the_source() {
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let run_id = sink.begin_run(&request("/src")).unwrap();
        let file = source_file("a.jpg", MediaKind::Image, "/src/2026/a.jpg", 10);
        let item = planned(0, "a.jpg", "photos/2026/a.jpg", Role::Bitmap);
        sink.record_plan(
            run_id,
            std::slice::from_ref(&file),
            std::slice::from_ref(&item),
        )
        .unwrap();
        sink.register(run_id, &item, &file, Some(FileId::new(9, [1u8; 16])), None)
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
    fn importing_records_each_files_own_display_size_and_orientation() {
        // `specs/display-size-source.md` §3.2：导入时给**每个文件**各写一份展示尺寸 + 自己的方向
        // （位图与 RAW 可以不同；不用 `assets.orientation` 代替）。
        let dir = tmp();
        let db = catalog(dir.path());
        let photos = dir.path().join("photos/2026");
        std::fs::create_dir_all(photos.join("_RAW")).unwrap();
        // 位图：普通 JPEG 40×20（无方向）
        image::RgbImage::new(40, 20)
            .save(photos.join("MYP0001.jpg"))
            .unwrap();
        // RAW：裸 TIFF 6000×4000 + 方向 6（竖拍 ⇒ 展示 4000×6000）
        write_bare_tiff(&photos.join("_RAW/MYP0001.rw2"), 6, 6000, 4000);

        let mut sink = CatalogSink::new(&db, T0);
        let run_id = sink.begin_run(&request("/src")).unwrap();
        let bitmap = source_file("MYP0001.jpg", MediaKind::Image, "/src/MYP0001.jpg", 10);
        let raw = source_file("MYP0001.rw2", MediaKind::Raw, "/src/MYP0001.rw2", 20);
        let items = [
            planned(0, "MYP0001.jpg", "photos/2026/MYP0001.jpg", Role::Bitmap),
            planned(1, "MYP0001.rw2", "photos/2026/_RAW/MYP0001.rw2", Role::Raw),
        ];
        let files = [bitmap, raw];
        sink.record_plan(run_id, &files, &items).unwrap();
        sink.register(run_id, &items[0], &files[0], None, None).unwrap();
        sink.register(run_id, &items[1], &files[1], None, None).unwrap();
        sink.commit().unwrap();

        let rows: Vec<(String, Option<i64>, Option<i64>, Option<i64>)> = db
            .read(|conn| {
                let mut statement = conn.prepare(
                    "SELECT role, display_width, display_height, orientation FROM asset_files ORDER BY role",
                )?;
                Ok(statement
                    .query_map([], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .unwrap();
        assert_eq!(
            rows,
            vec![
                ("bitmap".into(), Some(40), Some(20), Some(1)),
                ("raw".into(), Some(4000), Some(6000), Some(6)),
            ],
            "位图与 RAW 各写自己的那一份"
        );
    }

    /// 裸 TIFF：`MM` 大端 + 一个 IFD，含 Orientation / ImageWidth / ImageLength
    /// （与 `media::meta` 的 RAW 读法同一形状）。
    fn write_bare_tiff(path: &Path, orientation: u16, width: u32, height: u32) {
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(b"MM\x00\x2a");
        bytes.extend_from_slice(&8u32.to_be_bytes());
        let entries: [(u16, u16, u32); 3] = [
            (0x0112, 3, u32::from(orientation)),
            (0x0100, 4, width),
            (0x0101, 4, height),
        ];
        bytes.extend_from_slice(&(entries.len() as u16).to_be_bytes());
        for (tag, kind, value) in entries {
            bytes.extend_from_slice(&tag.to_be_bytes());
            bytes.extend_from_slice(&kind.to_be_bytes());
            bytes.extend_from_slice(&1u32.to_be_bytes());
            let mut slot = [0u8; 4];
            if kind == 3 {
                slot[..2].copy_from_slice(&(value as u16).to_be_bytes());
            } else {
                slot.copy_from_slice(&value.to_be_bytes());
            }
            bytes.extend_from_slice(&slot);
        }
        bytes.extend_from_slice(&0u32.to_be_bytes());
        std::fs::write(path, bytes).unwrap();
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
        sink.register(run_id, &items[0], &files[0], None, None).unwrap();
        sink.register(run_id, &items[1], &files[1], None, None).unwrap();
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
        sink.record_plan(
            run_id,
            std::slice::from_ref(&raw),
            std::slice::from_ref(&item),
        )
        .unwrap();
        sink.register(run_id, &item, &raw, None, None).unwrap();
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
        sink.record_plan(
            run_id,
            std::slice::from_ref(&bitmap),
            std::slice::from_ref(&item_b),
        )
        .unwrap();
        sink.register(run_id, &item_b, &bitmap, None, None).unwrap();
        sink.commit().unwrap();

        // 再导同名的 RAW（正常目录，不是 _RAW/）—— 应当挂到同一条资产上
        let raw = source_file("P1.ORF", MediaKind::Raw, "/src/P1.ORF", 20);
        let item_r = planned(0, "P1.ORF", "photos/2026/MYP1.ORF", Role::Raw);
        sink.record_plan(
            run_id,
            std::slice::from_ref(&raw),
            std::slice::from_ref(&item_r),
        )
        .unwrap();
        sink.register(run_id, &item_r, &raw, None, None).unwrap();
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
        sink.record_plan(
            run_id,
            std::slice::from_ref(&file),
            std::slice::from_ref(&item),
        )
        .unwrap();
        sink.register(run_id, &item, &file, None, None).unwrap();
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
        sink.record_plan(
            run_id,
            std::slice::from_ref(&file),
            std::slice::from_ref(&item),
        )
        .unwrap();
        sink.register(run_id, &item, &file, None, None).unwrap();
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
        assert_eq!(
            loaded.last("photos/2026-08-15", 4),
            1,
            "宽度不同 = 不同计数器"
        );

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
            CatalogSink::new(&db, T0)
                .sequences()
                .unwrap()
                .last("photos/2026-08-15", 3),
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
            complete: false,
        };
        sink.record_plan(
            run_b,
            &[source_file("b.jpg", MediaKind::Image, "/src/b/b.jpg", 5)],
            &[item],
        )
        .unwrap();
        sink.commit().unwrap();
        assert!(
            sink.stale_pending().unwrap().is_empty(),
            "只有 pending 才算半成品"
        );
    }

    #[test]
    fn finish_run_records_state_and_counts() {
        let dir = tmp();
        let db = catalog(dir.path());
        let mut sink = CatalogSink::new(&db, T0);
        let run_id = sink.begin_run(&request("/src")).unwrap();
        sink.finish_run(run_id, "cancelled", &counts(3, 1, 2))
            .unwrap();
        sink.commit().unwrap();

        let (state, imported, skipped, failed, finished): (String, i64, i64, i64, Option<i64>) = db
            .read(|conn| {
                conn.query_row(
                    "SELECT state, imported, skipped, failed, finished_at
                       FROM import_runs WHERE id = ?1",
                    [run_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .map_err(Into::into)
            })
            .unwrap();
        assert_eq!(
            (state.as_str(), imported, skipped, failed),
            ("cancelled", 3, 1, 2)
        );
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
        sink.register(run_id, &items[0], &files[0], None, None).unwrap();
        sink.commit().unwrap();
        // 再位图
        sink.register(run_id, &items[1], &files[1], None, None).unwrap();
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
        assert_eq!(
            strip_raw_segment("photos/_RAWx"),
            "photos/_RAWx",
            "不是整段"
        );
    }
}
