//! 导入执行器：按阶段把一批源目录真的搬进库里。
//!
//! ```text
//! 每个源目录一个 run：
//!   ① 扫描（walk）→ ② 读元数据（enrich）→ ③ 规划（plan）→ ④ 复制 + 登记 → ⑤ 缩略图入队
//! ```
//!
//! ## 四条不能动的纪律
//!
//! 1. **跑在自己的线程里**（不是 `spawn_blocking`）：一批导入要活几分钟，
//!    占着线程池的槽位会让别的命令排队。
//! 2. **暂停/取消只在文件之间生效**：复制中途不打断 —— 半截文件比慢更糟。
//! 3. **每条源文件的顺序不能反**：
//!
//!    ```text
//!    import_items 先落一行 pending（target_rel 已定）
//!      → 复制（`.part` 中转，见 `fsops`）
//!      → 登记 assets / asset_files（复用 `apply_diff`）
//!      → 补来源列 → 改成 imported
//!    ```
//!
//!    这样任何时刻被杀，重开都能凭 `target_rel` + 状态判断这条到底落没落
//!    （`plans/M1-6.md` §3.3 的续跑语义）。
//! 4. **跳过的文件不消耗序号**、**单条失败不打断整批**。
//!
//! ## 注入点
//!
//! 数据库（[`ImportSink`]）、文件系统（[`FileOps`]）、扫描（[`Scanner`]）、
//! 缩略图入队（回调）全部从外面给 —— 于是「跑完整整一批」在单测里
//! 只用内存实现，一个真文件都不碰（`AGENTS.md` §2.10）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::Result;
use crate::import::fsops::{FileOps, ScanNeeds, Scanner};
use crate::import::plan::{
    self, FsProbe, KnownSources, PlanOptions, PlannedItem, Reserved, Sequences, SourceExtras,
    SourceFile,
};
use crate::import::progress::{
    BatchCommit, BatchHandle, BatchProgress, CurrentItem, ImportError, ImportStage, ImportState,
    RunProgress, Throttle,
};
use crate::import::template::Template;
use crate::media::scan::{Cancel, ScanOptions, ScannedFile};
use crate::store::file_id::FileId;

/// 读元数据的每批条数（每批之间报进度、让出取消的检查点）。
const ENRICH_CHUNK: usize = 200;

/// 暂停时的轮询间隔。
const PAUSE_POLL: Duration = Duration::from_millis(50);

/// 一批要处理的源目录（每个源目录一个 run）。
#[derive(Debug, Clone)]
pub struct RunRequest {
    /// 在这批里的序号（从 0 起；进度里的 `[2/3]` 用它）。
    pub index: usize,
    /// 源根目录。
    pub source_root: std::path::PathBuf,
    /// 库内放照片的目录名（一般是 `photos`）。
    pub photos_dir: String,
    /// 编译好的模版。
    pub template: Template,
    /// 模版原文（要写进 `import_runs.template` 留痕 —— 模版会变）。
    pub template_source: String,
    /// 是否透传子目录。
    pub include_subdirs: bool,
    /// 是否避免重复导入。
    pub avoid_duplicates: bool,
}

impl RunRequest {
    /// 规划参数。
    #[must_use]
    pub fn plan_options(&self) -> PlanOptions {
        PlanOptions {
            photos_dir: self.photos_dir.clone(),
            include_subdirs: self.include_subdirs,
            avoid_duplicates: self.avoid_duplicates,
        }
    }

    /// 扫描参数：**「不含子目录」= `max_depth: 0`**（根的直接子目录即 `TooDeep`）。
    #[must_use]
    pub fn scan_options(&self) -> ScanOptions {
        ScanOptions {
            max_depth: if self.include_subdirs { 32 } else { 0 },
            ..ScanOptions::default()
        }
    }
}

/// 一个 run 的计数（写进 `import_runs` 的也是这几个）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunCounts {
    /// 条目总数（含跳过与失败）。
    pub total: u64,
    /// 真的导进来了。
    pub imported: u64,
    /// 跳过（含重复）。
    pub skipped: u64,
    /// 其中「已在库中」的。
    pub duplicates: u64,
    /// 失败。
    pub failed: u64,
    /// 复制了多少字节。
    pub bytes: u64,
}

/// 整批的结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchOutcome {
    /// 最终状态。
    pub state: Option<ImportState>,
    /// 合计计数。
    pub counts: RunCounts,
    /// 每个 run 的 id。
    pub run_ids: Vec<i64>,
    /// 排进缩略图队列的库内路径（去重）。
    pub thumb_paths: Vec<String>,
}

/// 暂停 / 取消的控制（全批一个）。
///
/// 用原子量而不是 channel：暂停/取消是**状态**（可以被反复问），不是消息。
/// 界面按一下就要生效，不该等项目循环走到某个地方才被读到。
#[derive(Debug, Clone)]
pub struct Control {
    state: Arc<AtomicU8>,
}

impl Default for Control {
    fn default() -> Self {
        Self::new()
    }
}

impl Control {
    /// 新的控制（初始 = 运行中）。
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(AtomicU8::new(encode(ImportState::Running))),
        }
    }

    /// 当前状态。
    #[must_use]
    pub fn state(&self) -> ImportState {
        decode(self.state.load(Ordering::SeqCst))
    }

    /// 请求暂停（执行器会在下一个安全点停下）。
    pub fn pause(&self) {
        // 已经暂停/取消的就不再改（幂等；界面重复按也不会乱）
        if matches!(self.state(), ImportState::Running | ImportState::Pausing) {
            self.set(ImportState::Pausing);
        }
    }

    /// 继续。
    pub fn resume(&self) {
        if matches!(self.state(), ImportState::Paused | ImportState::Pausing) {
            self.set(ImportState::Running);
        }
    }

    /// 请求取消（已导入的部分**保留**）。
    pub fn cancel(&self) {
        if !self.state().is_final() {
            self.set(ImportState::Cancelling);
        }
    }

    /// 是不是正在取消（含已取消）。
    #[must_use]
    pub fn is_cancelling(&self) -> bool {
        matches!(
            self.state(),
            ImportState::Cancelling | ImportState::Cancelled
        )
    }

    fn set(&self, state: ImportState) {
        self.state.store(encode(state), Ordering::SeqCst);
    }
}

fn encode(state: ImportState) -> u8 {
    match state {
        ImportState::Running => 0,
        ImportState::Pausing => 1,
        ImportState::Paused => 2,
        ImportState::Cancelling => 3,
        ImportState::Cancelled => 4,
        ImportState::Done => 5,
        ImportState::Failed => 6,
    }
}

fn decode(value: u8) -> ImportState {
    match value {
        1 => ImportState::Pausing,
        2 => ImportState::Paused,
        3 => ImportState::Cancelling,
        4 => ImportState::Cancelled,
        5 => ImportState::Done,
        6 => ImportState::Failed,
        _ => ImportState::Running,
    }
}

/// 导入过程中所有落库动作的出口（真实实现见 `src-tauri`，用 `catalog.db`）。
///
/// 拆这么细不是过度设计：每一行都对着 `import_items.status` 的一次状态迁移，
/// 顺序错了崩溃续跑就会误判（`plans/M1-6.md` §3.3）。
pub trait ImportSink {
    /// 判重用的「已导入过的源」（关掉判重时不必调）。
    fn known_sources(&mut self) -> Result<KnownSources>;
    /// 序号分配器（从 `seq_counters` 播种）。
    fn sequences(&mut self) -> Result<Sequences>;
    /// 把序号写回库。
    fn save_sequences(&mut self, sequences: &Sequences) -> Result<()>;
    /// 开一个 run，返回 id（**必须立刻提交**：崩溃时才知道有这么个 run）。
    fn begin_run(&mut self, request: &RunRequest) -> Result<i64>;
    /// 把规划结果落成 `import_items` 行（pending / skipped / failed）。
    fn record_plan(
        &mut self,
        run_id: i64,
        files: &[SourceFile],
        items: &[PlannedItem],
    ) -> Result<()>;
    /// 上次留下的「复制了但没登记」的痕迹（`target_rel` → 源路径）。
    fn stale_pending(&mut self) -> Result<HashMap<String, String>>;
    /// 登记一个已复制的文件（`assets` / `asset_files` / 来源列）。
    fn register(
        &mut self,
        run_id: i64,
        item: &PlannedItem,
        file: &SourceFile,
        copy_identity: Option<FileId>,
    ) -> Result<()>;
    /// 更新一条的状态与原因。
    fn mark(
        &mut self,
        run_id: i64,
        item: &PlannedItem,
        status: &str,
        reason: Option<&str>,
    ) -> Result<()>;
    /// 收尾一个 run。
    fn finish_run(&mut self, run_id: i64, state: &str, counts: &RunCounts) -> Result<()>;
    /// 提交当前这一批（批事务的边界）。
    fn commit(&mut self) -> Result<()>;
}

/// 执行器需要的一切（都从外面给，便于换成内存实现单测）。
pub struct Deps<'a> {
    /// 落库出口。
    pub sink: &'a mut dyn ImportSink,
    /// 文件操作。
    pub ops: &'a dyn FileOps,
    /// 源目录扫描。
    pub scanner: &'a dyn Scanner,
    /// 暂停/取消。
    pub control: &'a Control,
    /// 共享进度（命令与事件读它）。
    pub handle: &'a BatchHandle,
    /// 进度变化时回调（**已节流**；状态变化不节流）。
    pub on_progress: &'a mut dyn FnMut(&BatchProgress),
    /// 把库内路径排进缩略图队列（外壳实现；核心不认识 `app.db`）。
    pub enqueue_thumbs: &'a mut dyn FnMut(&[String]) -> Result<usize>,
    /// 这一批的「现在」（Unix 毫秒；测试可注入固定值）。
    pub now_ms: i64,
    /// 进度事件的节流间隔（默认 100ms；单测传 0 = 每条都报，断言才稳定）。
    pub throttle: Duration,
}

/// 跑一整批（顺序跑每个源目录 —— 复制串行对 HDD 友好，错误顺序也可读）。
pub fn run_batch(mut deps: Deps<'_>, jobs: &[RunRequest]) -> BatchOutcome {
    let mut outcome = BatchOutcome::default();
    let now = deps.now_ms;

    if jobs.is_empty() {
        // 一条源目录都没有：直接算完成（界面上的按钮本该是禁用的，这里防御）
        deps.handle.with(|p| {
            p.state = ImportState::Done;
            p.finished_at = Some(now);
        });
        deps.emit(true);
        outcome.state = Some(ImportState::Done);
        return outcome;
    }

    for (idx, job) in jobs.iter().enumerate() {
        if deps.control.is_cancelling() {
            // 取消之后剩下的源目录**连 run 都不开**（免得在库里留下空 run）。
            // 一个 run 都没开的话，整批也得显示成「已取消」，不能停在「运行中」。
            let _ = idx;
            deps.handle.with(|p| {
                p.refresh_state();
                if p.runs.is_empty() {
                    p.state = ImportState::Cancelled;
                }
            });
            break;
        }
        match deps.run_one(job) {
            Ok(result) => {
                outcome.run_ids.push(result.run_id);
                outcome.counts.total += result.counts.total;
                outcome.counts.imported += result.counts.imported;
                outcome.counts.skipped += result.counts.skipped;
                outcome.counts.duplicates += result.counts.duplicates;
                outcome.counts.failed += result.counts.failed;
                outcome.counts.bytes += result.counts.bytes;
                for path in result.thumbs {
                    if !outcome.thumb_paths.contains(&path) {
                        outcome.thumb_paths.push(path);
                    }
                }
            }
            Err(error) => {
                // 一个源目录整个跑不起来（例如目录没了）：记一笔，继续下一个
                let reason = error.to_string();
                deps.handle.with(|p| {
                    if let Some(run) = p.runs.iter_mut().find(|r| r.source_root == display_root(job)) {
                        run.state = ImportState::Failed;
                        run.note = Some(reason.clone());
                    }
                    p.push_error(ImportError {
                        source: display_root(job),
                        target: None,
                        reason: reason.clone(),
                        status: "failed".to_string(),
                    });
                    p.refresh_state();
                });
                deps.emit(true);
            }
        }
    }

    let final_state = deps.handle.with(|p| {
        p.recompute();
        p.refresh_state();
        p.finished_at = Some(now);
        p.state
    });
    deps.emit(true);
    outcome.state = Some(final_state);
    outcome
}

/// 一个 run 的结果。
struct RunOutcome {
    run_id: i64,
    counts: RunCounts,
    thumbs: Vec<String>,
}

impl Deps<'_> {
    /// 跑一个源目录。
    fn run_one(&mut self, job: &RunRequest) -> Result<RunOutcome> {
        let root = display_root(job);
        let run_id = self.sink.begin_run(job)?;
        // run 行立刻落库：不然崩溃之后根本不知道「上次在导什么」
        self.sink.commit()?;
        self.handle.with(|p| {
            let mut run = RunProgress::new(run_id, root.clone());
            run.note = None;
            p.runs.push(run);
            p.current_run = Some(job.index);
            p.recompute();
            p.refresh_state();
        });
        self.emit(true);

        let mut counts = RunCounts::default();
        let cancel = Cancel::new();
        let mut throttle = Throttle::new(self.throttle);

        /* ── ① 扫描 ───────────────────────────────────── */
        let mut scanned: Vec<ScannedFile> = Vec::new();
        let control = self.control;
        let mut on_file = |file: ScannedFile| {
            if control.is_cancelling() {
                return false;
            }
            scanned.push(file);
            true
        };
        let walk = self.scanner.walk(
            &job.source_root,
            &job.scan_options(),
            &cancel,
            &mut on_file,
        )?;
        self.set_run(run_id, |run| {
            run.scanned = walk.files as u64;
        });
        self.emit(true);

        if walk.cancelled || self.control.is_cancelling() {
            return self.finish(run_id, job, ImportState::Cancelled, counts, Vec::new());
        }

        /* ── ② 读元数据（模版要用才读）─────────────────── */
        let needs = ScanNeeds::for_template(&job.template, job.avoid_duplicates);
        let mut extras: Vec<SourceExtras> = Vec::with_capacity(scanned.len());
        for chunk in scanned.chunks(ENRICH_CHUNK) {
            if self.cancel_requested(run_id) {
                return self.finish(run_id, job, ImportState::Cancelled, counts, Vec::new());
            }
            extras.extend(self.scanner.enrich(chunk, &needs, &cancel));
            let read = extras.len() as u64;
            self.set_run(run_id, |run| run.scanned = read);
            self.emit_if_due(&mut throttle);
        }
        let files: Vec<SourceFile> = scanned
            .iter()
            .zip(extras)
            .map(|(scan, extra)| SourceFile::from_scanned(scan, extra))
            .collect();

        /* ── ③ 规划 ───────────────────────────────────── */
        self.set_stage(run_id, ImportStage::Plan);
        self.emit(true);
        let known = if job.avoid_duplicates {
            self.sink.known_sources()?
        } else {
            KnownSources::new()
        };
        let mut sequences = self.sink.sequences()?;
        let options = job.plan_options();

        // 续跑：上次「复制了但还没登记」且**大小相符**的目标路径。
        // 交给规划器当「我们自己占着的」—— 否则重名规则会给它加 `_01`，
        // 同一张照片就有两份了（`plans/M1-6.md` §7 风险 10）。
        let stale = self.sink.stale_pending()?;
        let mut resumable: Reserved = Reserved::new();
        for (target, source_rel) in &stale {
            if let Some(file) = files.iter().find(|f| f.rel_path == *source_rel)
                && self.ops.size_of(target) == Some(file.size_bytes)
            {
                resumable.insert(target);
            }
        }

        let probe = OpsProbe(self.ops);
        let planned = plan::plan(
            &files,
            &job.template,
            &options,
            &probe,
            &known,
            &mut sequences,
            &resumable,
        );
        self.sink.record_plan(run_id, &files, &planned.items)?;
        self.sink.save_sequences(&sequences)?;
        self.sink.commit()?;

        counts.total = planned.items.len() as u64;
        counts.skipped = planned.counts.skipped as u64;
        counts.duplicates = planned.counts.duplicates as u64;
        counts.failed = planned.counts.failed as u64;
        self.set_run(run_id, |run| {
            run.stage = ImportStage::Import;
            run.total = counts.total;
            run.skipped = counts.skipped;
            run.duplicates = counts.duplicates;
            run.failed = counts.failed;
            // 跳过与失败的条目在规划阶段就已经处理完了
            run.done = counts.skipped + counts.failed;
        });
        self.emit(true);

        // 规划里失败的条目也要进错误清单（用户在弹窗里要能看到原因）
        self.collect_plan_errors(&planned.items);

        /* ── ④ 复制 + 登记 ─────────────────────────────── */
        let mut commit = BatchCommit::new(200, Duration::from_millis(500), Instant::now());
        let mut thumbs: Vec<String> = Vec::new();

        for item in planned.planned() {
            if self.wait_at_safe_point(run_id) {
                self.sink.save_sequences(&sequences)?;
                self.sink.commit()?;
                return self.finish(run_id, job, ImportState::Cancelled, counts, thumbs);
            }
            let Some(target) = item.target_rel() else {
                continue;
            };
            let file = &files[item.index];
            self.set_run(run_id, |run| {
                run.current = Some(CurrentItem {
                    source: file.rel_path.clone(),
                    target: Some(target.to_string()),
                });
            });

            // 续跑：上次「复制了但没登记」的痕迹 + 目标在且大小相符 → 直接登记，不重拷
            let resume_ok = stale.contains_key(target)
                && self.ops.size_of(target) == Some(file.size_bytes);

            if !resume_ok
                && let Err(error) = self.copy_one(file, target) {
                    let reason = error.to_string();
                    counts.failed += 1;
                    self.sink.mark(run_id, item, "failed", Some(&reason))?;
                    self.note_error(file, Some(target), &reason, "failed");
                    self.bump(run_id, 0, 0, 1);
                    self.emit_if_due(&mut throttle);
                    continue;
                }

            let copy_identity = self.ops.identity_of(target);
            match self.sink.register(run_id, item, file, copy_identity) {
                Ok(()) => {
                    self.sink.mark(run_id, item, "imported", None)?;
                    counts.imported += 1;
                    self.bump(run_id, 1, 0, 0);
                    if item.role != Some(plan::Role::Raw) {
                        thumbs.push(target.to_string());
                    }
                }
                Err(error) => {
                    let reason = error.to_string();
                    counts.failed += 1;
                    self.sink.mark(run_id, item, "failed", Some(&reason))?;
                    self.note_error(file, Some(target), &reason, "failed");
                    self.bump(run_id, 0, 0, 1);
                }
            }

            if commit.tick(Instant::now()) {
                self.sink.save_sequences(&sequences)?;
                self.sink.commit()?;
            }
            self.emit_if_due(&mut throttle);
        }

        self.set_run(run_id, |run| run.current = None);
        self.emit_if_due(&mut throttle);

        /* ── ⑤ 缩略图入队（入队即算这一阶段完成）────────── */
        self.set_stage(run_id, ImportStage::Thumbs);
        self.emit(true);
        if !thumbs.is_empty() {
            // 入队失败不该把导入算失败：文件已经进库了，缩略图补得回来
            if let Err(error) = (self.enqueue_thumbs)(&thumbs) {
                self.handle.with(|p| {
                    run_of(p, run_id).note = Some(format!("缩略图入队失败：{error}"));
                });
            }
        }

        self.sink.save_sequences(&sequences)?;
        self.finish(run_id, job, ImportState::Done, counts, thumbs)
    }

    /// 复制一个文件（先建目录）。
    fn copy_one(&mut self, file: &SourceFile, target: &str) -> Result<()> {
        let dir = target.rsplit_once('/').map(|(dir, _)| dir);
        if let Some(dir) = dir {
            self.ops.create_dir_all(dir)?;
        }
        let bytes = self.ops.copy(&file.abs_path, target)?;
        self.handle.with(|p| {
            // 字节数记在「当前那个跑着的 run」上
            if let Some(run) = p
                .runs
                .iter_mut()
                .find(|r| r.current.as_ref().is_some_and(|c| c.target.as_deref() == Some(target)))
            {
                run.bytes += bytes;
            }
        });
        Ok(())
    }

    /// 收尾一个 run（写库 + 更新进度）。
    fn finish(
        &mut self,
        run_id: i64,
        _job: &RunRequest,
        state: ImportState,
        counts: RunCounts,
        thumbs: Vec<String>,
    ) -> Result<RunOutcome> {
        let bytes = self.handle.with(|p| run_of(p, run_id).bytes);
        let mut counts = counts;
        counts.bytes = bytes;
        self.sink.finish_run(run_id, state.run_state(), &counts)?;
        self.sink.commit()?;
        self.set_run(run_id, |run| {
            run.state = state;
            run.stage = if state == ImportState::Done {
                ImportStage::Done
            } else {
                run.stage
            };
            run.current = None;
        });
        self.handle.with(|p| {
            p.refresh_state();
        });
        self.emit(true);
        Ok(RunOutcome {
            run_id,
            counts,
            thumbs,
        })
    }

    /// 规划阶段就失败的条目（路径不合法之类）要出现在错误清单里。
    fn collect_plan_errors(&mut self, items: &[PlannedItem]) {
        for item in items {
            if let Some(reason) = item.reason() {
                let status = item.status_str().to_string();
                if status == "skipped" {
                    continue; // 跳过不算错误，界面另有计数
                }
                self.handle.with(|p| {
                    p.push_error(ImportError {
                        source: item.source_rel.clone(),
                        target: item.target_rel().map(ToString::to_string),
                        reason,
                        status,
                    });
                });
            }
        }
    }

    fn note_error(&self, file: &SourceFile, target: Option<&str>, reason: &str, status: &str) {
        self.handle.with(|p| {
            p.push_error(ImportError {
                source: file.rel_path.clone(),
                target: target.map(ToString::to_string),
                reason: reason.to_string(),
                status: status.to_string(),
            });
        });
    }

    /* ── 进度小工具 ───────────────────────────────────── */

    fn set_run(&self, run_id: i64, f: impl FnOnce(&mut RunProgress)) {
        self.handle.with(|p| {
            f(run_of(p, run_id));
            p.recompute();
        });
    }

    fn set_stage(&self, run_id: i64, stage: ImportStage) {
        self.set_run(run_id, |run| {
            run.stage = stage;
            if stage == ImportStage::Done {
                run.state = ImportState::Done;
            }
        });
    }

    /// 处理完一条：更新计数（`done` 总是跟着走）。
    fn bump(&self, run_id: i64, imported: u64, skipped: u64, failed: u64) {
        self.set_run(run_id, |run| {
            run.imported += imported;
            run.skipped += skipped;
            run.failed += failed;
            run.done += imported + skipped + failed;
        });
    }

    fn emit(&mut self, force: bool) {
        let snapshot = self.handle.snapshot();
        if force {
            (self.on_progress)(&snapshot);
        }
    }

    fn emit_if_due(&mut self, throttle: &mut Throttle) {
        if throttle.ready_at(Instant::now()) {
            self.emit(true);
        }
    }

    /// 安全点：暂停时停在这儿等；返回 `true` = 该收工了（被取消）。
    fn wait_at_safe_point(&mut self, run_id: i64) -> bool {
        if self.cancel_requested(run_id) {
            return true;
        }
        if !matches!(
            self.control.state(),
            ImportState::Pausing | ImportState::Paused
        ) {
            return false;
        }
        // 真的停下来了才宣布「已暂停」
        self.set_run(run_id, |run| run.state = ImportState::Paused);
        self.handle.with(BatchProgress::refresh_state);
        self.emit(true);

        while matches!(
            self.control.state(),
            ImportState::Pausing | ImportState::Paused
        ) {
            if self.control.is_cancelling() {
                return true;
            }
            std::thread::sleep(PAUSE_POLL);
        }
        self.set_run(run_id, |run| run.state = ImportState::Running);
        self.handle.with(BatchProgress::refresh_state);
        self.emit(true);
        false
    }

    fn cancel_requested(&self, run_id: i64) -> bool {
        if self.control.is_cancelling() {
            self.set_run(run_id, |run| run.state = ImportState::Cancelling);
            return true;
        }
        false
    }
}

/// 把 `&dyn FileOps` 当规划要的 `FsProbe` 用（只用到「文件在不在」这一件事）。
struct OpsProbe<'a>(&'a dyn FileOps);

impl FsProbe for OpsProbe<'_> {
    fn file_exists(&self, rel_path: &str) -> bool {
        self.0.exists(rel_path)
    }
}

fn run_of(progress: &mut BatchProgress, run_id: i64) -> &mut RunProgress {
    // run 一定在（刚 push 过）；真找不到就补一个，绝不 panic（进度不该能弄崩导入）
    if let Some(pos) = progress.runs.iter().position(|r| r.run_id == run_id) {
        return &mut progress.runs[pos];
    }
    progress
        .runs
        .push(RunProgress::new(run_id, format!("(未知源) run {run_id}")));
    let last = progress.runs.len() - 1;
    &mut progress.runs[last]
}

fn display_root(job: &RunRequest) -> String {
    job.source_root.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::fsops::MemoryFs;
    use crate::import::plan::SourceExtras;
    use crate::import::progress::BatchProgress;
    use crate::import::template::parse as parse_template;
    
    use crate::media::scan::ScannedFile;
    use crate::store::file_id::FileId;
    use std::path::PathBuf;

    const T0: i64 = 1_789_516_800_000;
    /// 2026-08-15T12:00:00Z
    const TAKEN: i64 = 1_786_795_200_000;
    const SRC: &str = "/src";

    /* ══════════════════════════════════════════════════════════
     * 测试替身：内存 catalog
     * ══════════════════════════════════════════════════════════ */

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RecordedRun {
        id: i64,
        source_root: String,
        template: String,
        state: String,
        counts: RunCounts,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RecordedItem {
        run_id: i64,
        source: String,
        target: Option<String>,
        status: String,
        reason: Option<String>,
        /// 登记时写下的「库内副本身份」。
        copy_identity: Option<FileId>,
        /// 登记时写下的「源身份」。
        source_identity: Option<FileId>,
    }

    #[derive(Default)]
    struct MemorySink {
        runs: Vec<RecordedRun>,
        items: Vec<RecordedItem>,
        commits: usize,
        known: KnownSources,
        stale: HashMap<String, String>,
        saved_sequences: Vec<Vec<(String, usize, u64)>>,
        next_id: i64,
        /// 让某个源文件的登记失败（测「单条失败不打断」）。
        fail_register_for: Option<String>,
    }

    impl MemorySink {
        fn item(&self, source: &str) -> &RecordedItem {
            self.items
                .iter()
                .find(|i| i.source == source)
                .unwrap_or_else(|| panic!("没有这条记录：{source}（现有 {:?}）", self.sources()))
        }

        fn sources(&self) -> Vec<&str> {
            self.items.iter().map(|i| i.source.as_str()).collect()
        }

        fn statuses(&self) -> Vec<(String, String)> {
            self.items
                .iter()
                .map(|i| (i.source.clone(), i.status.clone()))
                .collect()
        }

        fn run(&self, id: i64) -> &RecordedRun {
            self.runs.iter().find(|r| r.id == id).expect("有那个 run")
        }
    }

    impl ImportSink for MemorySink {
        fn known_sources(&mut self) -> Result<KnownSources> {
            Ok(self.known.clone())
        }

        fn sequences(&mut self) -> Result<Sequences> {
            Ok(Sequences::new())
        }

        fn save_sequences(&mut self, sequences: &Sequences) -> Result<()> {
            self.saved_sequences.push(sequences.snapshot());
            Ok(())
        }

        fn begin_run(&mut self, request: &RunRequest) -> Result<i64> {
            self.next_id += 1;
            self.runs.push(RecordedRun {
                id: self.next_id,
                source_root: request.source_root.display().to_string(),
                template: request.template_source.clone(),
                state: "running".to_string(),
                counts: RunCounts::default(),
            });
            Ok(self.next_id)
        }

        fn record_plan(
            &mut self,
            run_id: i64,
            files: &[SourceFile],
            items: &[PlannedItem],
        ) -> Result<()> {
            for item in items {
                self.items.push(RecordedItem {
                    run_id,
                    source: files[item.index].rel_path.clone(),
                    target: item.target_rel().map(ToString::to_string),
                    status: item.status_str().to_string(),
                    reason: item.reason(),
                    copy_identity: None,
                    source_identity: None,
                });
            }
            Ok(())
        }

        fn stale_pending(&mut self) -> Result<HashMap<String, String>> {
            Ok(self.stale.clone())
        }

        fn register(
            &mut self,
            _run_id: i64,
            item: &PlannedItem,
            file: &SourceFile,
            copy_identity: Option<FileId>,
        ) -> Result<()> {
            if self.fail_register_for.as_deref() == Some(file.rel_path.as_str()) {
                return Err(crate::error::Error::Unsupported("索引里已有这条路径".into()));
            }
            if let Some(record) = self.items.iter_mut().find(|i| i.source == file.rel_path) {
                record.copy_identity = copy_identity;
                record.source_identity = file.identity;
            }
            let _ = item;
            Ok(())
        }

        fn mark(
            &mut self,
            _run_id: i64,
            item: &PlannedItem,
            status: &str,
            reason: Option<&str>,
        ) -> Result<()> {
            if let Some(record) = self
                .items
                .iter_mut()
                .find(|i| i.target == item.target_rel().map(ToString::to_string))
            {
                record.status = status.to_string();
                record.reason = reason.map(ToString::to_string);
            }
            Ok(())
        }

        fn finish_run(
            &mut self,
            run_id: i64,
            state: &str,
            counts: &RunCounts,
        ) -> Result<()> {
            if let Some(run) = self.runs.iter_mut().find(|r| r.id == run_id) {
                run.state = state.to_string();
                run.counts = *counts;
            }
            Ok(())
        }

        fn commit(&mut self) -> Result<()> {
            self.commits += 1;
            Ok(())
        }
    }

    /* ══════════════════════════════════════════════════════════
     * 搭场景
     * ══════════════════════════════════════════════════════════ */

    fn scanned(rel: &str, size: u64) -> ScannedFile {
        scanned_at(&format!("{SRC}/root"), rel, size)
    }

    fn scanned_at(root: &str, rel: &str, size: u64) -> ScannedFile {
        let name = rel.rsplit('/').next().unwrap_or(rel).to_string();
        ScannedFile {
            abs_path: PathBuf::from(format!("{root}/{rel}")),
            rel_path: rel.to_string(),
            ext: crate::media::kind::extension(&name),
            kind: crate::media::kind::kind_of_file(&name),
            stem_folded: crate::media::kind::stem_folded(&name),
            file_name: name,
            size_bytes: size,
            mtime_ms: Some(1_700_000_000_000),
        }
    }

    /// 一个「源目录里有这些文件」的世界（照片内容都在）。
    fn world(files: &[(&str, u64)]) -> MemoryFs {
        let mut fs = MemoryFs::new();
        let list: Vec<ScannedFile> = files.iter().map(|(rel, size)| scanned(rel, *size)).collect();
        for (rel, size) in files {
            fs.add_source(format!("{SRC}/root/{rel}"), &vec![0u8; *size as usize]);
            fs.set_extras(
                rel,
                SourceExtras {
                    identity: None,
                    taken_at: Some(TAKEN),
                    brand: None,
                    model: None,
                },
            );
        }
        fs.set_scan(SRC, list);
        fs
    }

    struct Harness {
        fs: MemoryFs,
        sink: MemorySink,
        control: Control,
        handle: BatchHandle,
        events: Vec<BatchProgress>,
        thumbs: Vec<Vec<String>>,
        thumb_fails: bool,
    }

    impl Harness {
        fn new(fs: MemoryFs) -> Self {
            Self {
                fs,
                sink: MemorySink::default(),
                control: Control::new(),
                handle: BatchHandle::new(BatchProgress::new("b1", T0)),
                events: Vec::new(),
                thumbs: Vec::new(),
                thumb_fails: false,
            }
        }

        fn job(&self, template: &str) -> RunRequest {
            RunRequest {
                index: 0,
                source_root: PathBuf::from(SRC),
                photos_dir: "photos".to_string(),
                template: parse_template(template).expect("模版"),
                template_source: template.to_string(),
                include_subdirs: true,
                avoid_duplicates: true,
            }
        }

        /// 跑一批（`hook` 在每次进度回调里被调，用来注入暂停/取消）。
        fn run_with(&mut self, jobs: &[RunRequest], mut hook: impl FnMut(&BatchProgress, &Control)) -> BatchOutcome {
            let Harness {
                fs,
                sink,
                control,
                handle,
                events,
                thumbs,
                thumb_fails,
            } = self;
            let mut on_progress = |p: &BatchProgress| {
                events.push(p.clone());
                hook(p, control);
            };
            let mut enqueue = |paths: &[String]| -> Result<usize> {
                thumbs.push(paths.to_vec());
                if *thumb_fails {
                    return Err(crate::error::Error::Unsupported("队列写不进去".into()));
                }
                Ok(paths.len())
            };
            let deps = Deps {
                sink,
                ops: fs,
                scanner: fs,
                control,
                handle,
                on_progress: &mut on_progress,
                enqueue_thumbs: &mut enqueue,
                now_ms: T0,
                // 0 = 每条都报，单测的断言才不依赖 100ms 的节流
                throttle: Duration::ZERO,
            };
            run_batch(deps, jobs)
        }

        fn run(&mut self, jobs: &[RunRequest]) -> BatchOutcome {
            self.run_with(jobs, |_, _| {})
        }
    }

    const TPL: &str = ":CYEAR-:CMONTH-:CDAY/MY:FILENAME";

    /* ══════════════════════════════════════════════════════════
     * 主流程
     * ══════════════════════════════════════════════════════════ */

    #[test]
    fn a_full_run_copies_registers_and_enqueues_thumbnails() {
        let fs = world(&[("P0001.png", 10), ("P0001.ORF", 20)]);
        let mut h = Harness::new(fs);
        let outcome = h.run(&[h.job(TPL)]);

        assert_eq!(outcome.state, Some(ImportState::Done));
        assert_eq!(outcome.counts.imported, 2);
        assert_eq!(outcome.counts.failed, 0);
        assert_eq!(h.fs.paths(), vec![
            "photos/2026-08-15/MYP0001.png",
            "photos/2026-08-15/_RAW/MYP0001.ORF",
        ]);
        // 落库：两条都是 imported
        assert_eq!(
            h.sink.statuses(),
            vec![
                ("P0001.png".to_string(), "imported".to_string()),
                ("P0001.ORF".to_string(), "imported".to_string()),
            ]
        );
        assert_eq!(h.sink.run(1).state, "done");
        assert_eq!(h.sink.run(1).counts.imported, 2);
        assert_eq!(h.sink.run(1).template, TPL, "模版要留痕");
        // 缩略图：只排队位图（RAW 不做网格缩略图）
        assert_eq!(h.thumbs.len(), 1);
        assert_eq!(h.thumbs[0], vec!["photos/2026-08-15/MYP0001.png"]);
        // 批次收尾
        assert_eq!(h.handle.snapshot().state, ImportState::Done);
        assert!(h.handle.snapshot().finished_at.is_some());
        assert_eq!(h.handle.snapshot().percent(), Some(100.0));
        assert!(h.sink.commits >= 3, "run 开、规划、收尾都要提交");
    }

    #[test]
    fn progress_visits_every_stage_and_reports_counts() {
        let fs = world(&[("a.jpg", 5), ("b.jpg", 6)]);
        let mut h = Harness::new(fs);
        h.run(&[h.job(TPL)]);

        let stages: Vec<ImportStage> = h.events.iter().map(|e| e.stage).collect();
        assert!(stages.contains(&ImportStage::Scan), "{stages:?}");
        assert!(stages.contains(&ImportStage::Plan), "{stages:?}");
        assert!(stages.contains(&ImportStage::Import), "{stages:?}");
        assert!(stages.contains(&ImportStage::Thumbs), "{stages:?}");
        assert_eq!(stages.last(), Some(&ImportStage::Done));

        let last = h.events.last().expect("有进度");
        assert_eq!(last.imported, 2);
        assert_eq!(last.total, 2);
        assert_eq!(last.done, 2);
        assert_eq!(last.current_run, Some(0));
        assert_eq!(last.runs.len(), 1);
        assert_eq!(last.runs[0].source_root, SRC);
    }

    #[test]
    fn one_bad_file_does_not_stop_the_batch() {
        let fs = world(&[("a.jpg", 5), ("bad.jpg", 7), ("c.jpg", 9)]);
        let mut h = Harness::new(fs);
        h.fs.fail_source(format!("{SRC}/root/bad.jpg"));
        let outcome = h.run(&[h.job(TPL)]);

        assert_eq!(outcome.counts.imported, 2);
        assert_eq!(outcome.counts.failed, 1);
        assert_eq!(outcome.state, Some(ImportState::Done), "有失败也要走到完成");
        assert_eq!(h.sink.item("bad.jpg").status, "failed");
        assert!(
            h.sink.item("bad.jpg").reason.as_deref().unwrap_or_default().contains("读不了"),
            "{:?}",
            h.sink.item("bad.jpg").reason
        );
        // 错误清单里有它，且计数对得上
        let snap = h.handle.snapshot();
        assert_eq!(snap.errors_total, 1);
        assert_eq!(snap.errors[0].status, "failed");
        assert_eq!(snap.failed, 1);
        // 另外两条照常进库
        assert_eq!(h.fs.file_count(), 2);
    }

    #[test]
    fn a_registration_failure_is_recorded_per_file() {
        let fs = world(&[("a.jpg", 5), ("b.jpg", 6)]);
        let mut h = Harness::new(fs);
        h.sink.fail_register_for = Some("b.jpg".to_string());
        let outcome = h.run(&[h.job(TPL)]);

        assert_eq!(outcome.counts.imported, 1);
        assert_eq!(outcome.counts.failed, 1);
        assert_eq!(h.sink.item("b.jpg").status, "failed");
        assert!(h.handle.snapshot().errors_total >= 1);
    }

    #[test]
    fn duplicates_are_skipped_without_copying() {
        let fs = world(&[("a.jpg", 5)]);
        let mut h = Harness::new(fs);
        let mut known = KnownSources::new();
        known.insert_fallback("/src/root/a.jpg", 5, Some(1_700_000_000_000));
        h.sink.known = known;

        let outcome = h.run(&[h.job(TPL)]);
        assert_eq!(outcome.counts.imported, 0);
        assert_eq!(outcome.counts.skipped, 1);
        assert_eq!(outcome.counts.duplicates, 1);
        assert!(h.fs.copied().is_empty(), "重复的不该再拷一份");
        assert_eq!(h.sink.item("a.jpg").status, "skipped");
        let snap = h.handle.snapshot();
        assert_eq!(snap.skipped, 1);
        assert_eq!(snap.duplicates, 1, "其中重复多少要单独看得出来");
    }

    #[test]
    fn avoid_duplicates_off_does_not_even_ask_for_known_sources() {
        let fs = world(&[("a.jpg", 5)]);
        let mut h = Harness::new(fs);
        let mut known = KnownSources::new();
        known.insert_fallback("/src/root/a.jpg", 5, Some(1_700_000_000_000));
        h.sink.known = known;
        let mut job = h.job(TPL);
        job.avoid_duplicates = false;

        let outcome = h.run(&[job]);
        assert_eq!(outcome.counts.imported, 1, "关掉判重=照样导一份");
        assert_eq!(outcome.counts.skipped, 0);
    }

    /* ══════════════════════════════════════════════════════════
     * 暂停与取消
     * ══════════════════════════════════════════════════════════ */

    #[test]
    fn cancel_keeps_what_was_already_imported() {
        let fs = world(&[("a.jpg", 5), ("b.jpg", 6), ("c.jpg", 7)]);
        let mut h = Harness::new(fs);
        let outcome = h.run_with(&[h.job(TPL)], |p, control| {
            // 第一条导入完成之后就取消
            if p.imported == 1 {
                control.cancel();
            }
        });

        assert_eq!(outcome.state, Some(ImportState::Cancelled));
        assert_eq!(h.sink.run(1).state, "cancelled");
        assert_eq!(h.fs.file_count(), 1, "已经导进来的那份**保留**");
        assert_eq!(outcome.counts.imported, 1);
        // 剩下的走不到（没被标成失败）
        assert_eq!(h.sink.item("b.jpg").status, "pending");
        assert_eq!(h.sink.item("c.jpg").status, "pending");
        assert!(h.thumbs.is_empty(), "取消之后不排缩略图");
    }

    #[test]
    fn cancel_before_the_run_leaves_everything_untouched() {
        let fs = world(&[("a.jpg", 5)]);
        let mut h = Harness::new(fs);
        h.control.cancel();
        let outcome = h.run(&[h.job(TPL)]);

        assert_eq!(outcome.state, Some(ImportState::Cancelled));
        assert!(
            h.fs.copied().is_empty() && h.fs.file_count() == 0,
            "取消在开工前 → 一个文件都不碰"
        );
        assert_eq!(
            h.sink.runs.len(),
            0,
            "连 run 都不开：库里不该留下空 run 行"
        );
        assert_eq!(h.handle.snapshot().state, ImportState::Cancelled, "整批要显示成已取消");
        assert_eq!(h.sink.commits, 0, "什么都没做，不该有提交");
    }

    #[test]
    fn pause_blocks_at_a_safe_point_and_resume_continues() {
        let fs = world(&[("a.jpg", 5), ("b.jpg", 6)]);
        let mut h = Harness::new(fs);

        let control = h.control.clone();
        // 从另一个线程恢复：单线程测试里不能自己等自己
        let resumer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(80));
            control.resume();
        });

        // 只暂停一次：暂停态会一路报出来，每报一次都再按一次暂停就没完没了
        let mut paused = false;
        let outcome = h.run_with(&[h.job(TPL)], |p, control| {
            if p.imported >= 1 && !paused {
                paused = true;
                control.pause();
            }
        });
        resumer.join().expect("恢复线程");

        assert_eq!(outcome.state, Some(ImportState::Done), "暂停之后要能接着跑完");
        assert_eq!(outcome.counts.imported, 2);
        assert!(
            h.events.iter().any(|e| e.state == ImportState::Paused),
            "暂停要真的被宣布出去（不然界面看不到）"
        );
        assert!(
            h.events.iter().any(|e| e.runs.iter().any(|r| r.state == ImportState::Paused)),
            "run 级也要有暂停态"
        );
        assert_eq!(h.control.state(), ImportState::Running, "恢复后回到运行中");
    }

    /* ══════════════════════════════════════════════════════════
     * 续跑与多目录
     * ══════════════════════════════════════════════════════════ */

    #[test]
    fn stale_pending_items_are_registered_without_copying_again() {
        // 上次崩溃在「复制完了但还没登记」的那一步：目标文件在、大小相符
        let fs = world(&[("a.jpg", 5)]);
        let mut h = Harness::new(fs);
        h.fs.add_existing("photos/2026-08-15/MYa.jpg", &[0u8; 5]);
        h.sink.stale.insert(
            "photos/2026-08-15/MYa.jpg".to_string(),
            "a.jpg".to_string(),
        );

        let outcome = h.run(&[h.job(TPL)]);
        assert_eq!(outcome.counts.imported, 1, "续跑要把它补登记上");
        assert!(h.fs.copied().is_empty(), "不该重拷一份");
        assert_eq!(h.sink.item("a.jpg").status, "imported");
    }

    #[test]
    fn stale_pending_with_a_wrong_size_does_not_count_as_ours() {
        let fs = world(&[("a.jpg", 5)]);
        let mut h = Harness::new(fs);
        // 大小对不上 → 这文件不是我们写的（我们只会留 .part）
        h.fs.add_existing("photos/2026-08-15/MYa.jpg", &[0u8; 3]);
        h.sink.stale.insert(
            "photos/2026-08-15/MYa.jpg".to_string(),
            "a.jpg".to_string(),
        );

        let outcome = h.run(&[h.job(TPL)]);
        assert_eq!(outcome.counts.imported, 1);
        assert_eq!(outcome.counts.failed, 0);
        // 别人的文件不动，我们落到旁边的 `_01` 上
        assert_eq!(
            h.fs.paths(),
            vec![
                "photos/2026-08-15/MYa.jpg".to_string(),
                "photos/2026-08-15/MYa_01.jpg".to_string(),
            ]
        );
        assert_eq!(
            h.fs.bytes_of("photos/2026-08-15/MYa.jpg").map(|b| b.len()),
            Some(3),
            "原来那个文件必须原样"
        );
    }

    #[test]
    fn several_source_directories_become_several_runs() {
        let mut fs = MemoryFs::new();
        for (root, files) in [
            ("/src/a", vec![("x.jpg", 5u64)]),
            ("/src/b", vec![("y.jpg", 6u64), ("z.jpg", 7)]),
        ] {
            let list: Vec<ScannedFile> = files
                .iter()
                .map(|(rel, size)| scanned_at(&format!("{root}/root"), rel, *size))
                .collect();
            for (rel, size) in &files {
                fs.add_source(format!("{root}/root/{rel}"), &vec![0u8; *size as usize]);
                fs.set_extras(
                    rel,
                    SourceExtras {
                        taken_at: Some(TAKEN),
                        ..SourceExtras::default()
                    },
                );
            }
            fs.set_scan(root, list);
        }
        let mut h = Harness::new(fs);
        let job_a = RunRequest {
            index: 0,
            source_root: PathBuf::from("/src/a"),
            ..h.job(":FILENAME")
        };
        let job_b = RunRequest {
            index: 1,
            source_root: PathBuf::from("/src/b"),
            ..h.job(":FILENAME")
        };
        let outcome = h.run(&[job_a, job_b]);

        assert_eq!(outcome.run_ids, vec![1, 2], "每个源目录一个 run");
        assert_eq!(outcome.counts.imported, 3);
        let snap = h.handle.snapshot();
        assert_eq!(snap.runs.len(), 2);
        assert_eq!(snap.runs[0].source_root, "/src/a");
        assert_eq!(snap.runs[1].source_root, "/src/b");
        assert_eq!(snap.total, 3);
        assert_eq!(snap.current_run, Some(1), "最后停在第二个目录");
        assert_eq!(h.sink.run(1).state, "done");
        assert_eq!(h.sink.run(2).state, "done");
    }

    #[test]
    fn an_empty_batch_is_done_not_stuck() {
        let fs = MemoryFs::new();
        let mut h = Harness::new(fs);
        let outcome = h.run(&[]);
        assert_eq!(outcome.state, Some(ImportState::Done));
        assert_eq!(h.handle.snapshot().state, ImportState::Done);
        assert_eq!(h.sink.runs.len(), 0);
    }

    #[test]
    fn a_failing_thumbnail_queue_does_not_fail_the_import() {
        let fs = world(&[("a.jpg", 5)]);
        let mut h = Harness::new(fs);
        h.thumb_fails = true;
        let outcome = h.run(&[h.job(TPL)]);

        assert_eq!(outcome.counts.imported, 1, "文件已经进库了，缩略图不该拖垮它");
        assert_eq!(outcome.state, Some(ImportState::Done));
        assert_eq!(h.sink.run(1).state, "done");
        let snap = h.handle.snapshot();
        assert!(
            snap.runs[0].note.as_deref().unwrap_or_default().contains("缩略图"),
            "{:?}",
            snap.runs[0].note
        );
    }

    #[test]
    fn sequence_values_are_saved_back_to_the_store() {
        let fs = world(&[("a.jpg", 5), ("b.jpg", 6)]);
        let mut h = Harness::new(fs);
        h.run(&[h.job(":CYEAR/SEQ:SEQ000")]);

        let saved = h.sink.saved_sequences.last().expect("要写回序号");
        assert_eq!(saved.len(), 1, "一个目标目录一个计数");
        assert_eq!(saved[0].1, 3, "宽度 3");
        assert_eq!(saved[0].2, 2, "取到第 2 个号");
    }

    #[test]
    fn source_identity_is_recorded_on_registration() {
        let mut fs = world(&[("a.jpg", 5)]);
        let source_id = FileId::new(3, [1u8; 16]);
        fs.set_extras(
            "a.jpg",
            SourceExtras {
                identity: Some(source_id),
                taken_at: Some(TAKEN),
                ..SourceExtras::default()
            },
        );
        let mut h = Harness::new(fs);
        h.run(&[h.job(TPL)]);

        assert_eq!(
            h.sink.item("a.jpg").source_identity,
            Some(source_id),
            "源身份要落库（下次判重靠它）"
        );
    }

    #[test]
    fn control_is_idempotent_and_final_states_are_sticky() {
        let control = Control::new();
        control.cancel();
        control.cancel();
        assert_eq!(control.state(), ImportState::Cancelling);
        control.pause();
        assert_eq!(control.state(), ImportState::Cancelling, "取消之后暂停不生效");
        control.resume();
        assert_eq!(control.state(), ImportState::Cancelling, "取消之后恢复不生效");

        let control = Control::new();
        control.pause();
        assert_eq!(control.state(), ImportState::Pausing);
        control.pause();
        assert_eq!(control.state(), ImportState::Pausing);
        control.resume();
        assert_eq!(control.state(), ImportState::Running);
    }
}
