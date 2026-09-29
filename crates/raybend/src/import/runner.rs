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
//!    （`specs/M1-6.md` §3.3 的续跑语义）。
//! 4. **跳过的文件不消耗序号**、**单条失败不打断整批**。
//!
//! ## 注入点
//!
//! 数据库（[`ImportSink`]）、文件系统（[`FileOps`]）、扫描（[`Scanner`]）、
//! 缩略图入队（回调）全部从外面给 —— 于是「跑完整整一批」在单测里
//! 只用内存实现，一个真文件都不碰（`AGENTS.md` §2.10）。

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
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
    /// **本次不导入的文件**（绝对路径，用户在网格里排除掉的）。
    ///
    /// 为什么用绝对路径而不是「源目录 + 相对路径」：排除是**跨目录、跨源**的
    /// 一件事（多源导入时用户来回切目录，排除不能丢），而界面里的照片 id 本来就是
    /// 后端给的绝对路径 —— 直接用它，前后端不会有两套口径。
    ///
    /// 用 `Arc` 而不是每个源克隆一份：排除清单可能上千条，而它整批只读。
    pub excluded: std::sync::Arc<std::collections::HashSet<String>>,
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
        ImportState::Waiting => 7,
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
        7 => ImportState::Waiting,
        _ => ImportState::Running,
    }
}

/// 导入过程中所有落库动作的出口（真实实现见 `src-tauri`，用 `catalog.db`）。
///
/// 拆这么细不是过度设计：每一行都对着 `import_items.status` 的一次状态迁移，
/// 顺序错了崩溃续跑就会误判（`specs/M1-6.md` §3.3）。
pub trait ImportSink {
    /// 文件之间的租约闸门；源失败仍可继续，目标会话失效必须停止整批。
    fn check_session(&self) -> Result<()> {
        Ok(())
    }
    /// 判重用的「已导入过的源」（关掉判重时不必调）。
    fn known_sources(&mut self) -> Result<KnownSources>;
    fn interrupted_sources(&mut self) -> Result<KnownSources> {
        Ok(KnownSources::new())
    }
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
    /// 生产设备恢复闸门；无恢复适配器的合成调用保持失效即停止语义。
    pub recovery: Option<&'a dyn StorageRecovery>,
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

/// 来源与目标身份核对由同一生产适配器提供，不把路径存在当作恢复。
pub trait StorageRecovery {
    fn source_ready(&self, root: &std::path::Path) -> bool;
    fn target_ready(&self) -> bool;
    fn wait(&self) {
        std::thread::sleep(Duration::from_millis(500));
    }
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

    // 逐个来源保存规划和复制游标；失联来源让出执行机会，不拖住其它来源。
    let mut work: Vec<Option<Work>> = (0..jobs.len()).map(|_| None).collect();
    let mut queue: VecDeque<usize> = (0..jobs.len()).collect();
    deps.handle.with(|p| {
        p.runs = jobs
            .iter()
            .enumerate()
            .map(|(i, job)| RunProgress::new(-(i as i64) - 1, display_root(job)))
            .collect();
    });
    let mut deferred = 0usize;
    while let Some(idx) = queue.pop_front() {
        let job = &jobs[idx];
        if deps.control.is_cancelling() {
            deps.handle.with(|p| {
                for run in &mut p.runs {
                    if !run.state.is_final() {
                        run.state = ImportState::Cancelled;
                    }
                }
                p.refresh_state();
            });
            break;
        }
        deps.handle.with(|p| p.current_run = Some(idx));
        if let Err(error) = deps.ensure_target() {
            if deps.control.is_cancelling() {
                deps.handle.with(|p| {
                    for run in &mut p.runs {
                        if !run.state.is_final() {
                            run.state = ImportState::Cancelled;
                        }
                    }
                });
            } else {
                deps.handle.with(|p| {
                    for run in &mut p.runs {
                        if !run.state.is_final() {
                            run.state = ImportState::Failed;
                        }
                    }
                    p.push_error(ImportError {
                        source: display_root(job),
                        target: None,
                        reason: error.to_string(),
                        status: "failed".into(),
                    });
                });
            }
            break;
        }
        let source_ready = deps
            .recovery
            .is_none_or(|gate| gate.source_ready(&job.source_root));
        let result = if source_ready {
            deps.run_one(job, &mut work[idx])
        } else {
            Ok(None)
        };
        match result {
            Ok(Some(result)) => {
                deferred = 0;
                for path in result.thumbs {
                    if !outcome.thumb_paths.contains(&path) {
                        outcome.thumb_paths.push(path);
                    }
                }
            }
            Ok(None) => {
                deps.set_waiting(idx, "storage.wait.source");
                queue.push_back(idx);
                deferred += 1;
            }
            Err(error) if deps.control.is_cancelling() => {
                let _ = error;
                deps.handle.with(|p| {
                    for run in &mut p.runs {
                        if !run.state.is_final() {
                            run.state = ImportState::Cancelled;
                        }
                    }
                });
                break;
            }
            Err(_error)
                if deps
                    .recovery
                    .is_some_and(|gate| !gate.source_ready(&job.source_root)) =>
            {
                deps.set_waiting(idx, "storage.wait.source");
                queue.push_back(idx);
                deferred += 1;
            }
            Err(error) => {
                let reason = error.to_string();
                deps.handle.with(|p| {
                    p.runs[idx].state = ImportState::Failed;
                    p.runs[idx].note = Some(reason.clone());
                    p.push_error(ImportError {
                        source: display_root(job),
                        target: None,
                        reason,
                        status: "failed".into(),
                    });
                    p.refresh_state();
                });
                deps.emit(true);
                deferred = 0;
                if deps.sink.check_session().is_err() && deps.recovery.is_none() {
                    break;
                }
            }
        }
        if !queue.is_empty() && deferred >= queue.len() {
            if let Some(gate) = deps.recovery {
                gate.wait();
            }
            deferred = 0;
        }
    }

    let expired = deps.sink.check_session().is_err();
    let final_state = deps.handle.with(|p| {
        p.recompute();
        p.refresh_state();
        p.finished_at = Some(now);
        if deps.control.is_cancelling() {
            p.state = ImportState::Cancelled;
        } else if expired {
            p.state = ImportState::Failed;
        }
        p.state
    });
    deps.emit(true);
    outcome.state = Some(final_state);
    let progress = deps.handle.snapshot();
    outcome.counts = RunCounts {
        total: progress.total,
        imported: progress.imported,
        skipped: progress.skipped,
        duplicates: progress.duplicates,
        failed: progress.failed,
        bytes: progress.bytes,
    };
    outcome.run_ids = progress
        .runs
        .iter()
        .filter(|run| run.run_id > 0)
        .map(|run| run.run_id)
        .collect();
    outcome
}

/// 一个 run 的结果。
struct RunOutcome {
    thumbs: Vec<String>,
}
struct Work {
    run_id: i64,
    files: Vec<SourceFile>,
    items: Vec<PlannedItem>,
    sequences: Sequences,
    counts: RunCounts,
    thumbs: Vec<String>,
    cursor: usize,
    prepared: bool,
}

impl Deps<'_> {
    /// 跑一个源目录。
    fn run_one(
        &mut self,
        job: &RunRequest,
        context: &mut Option<Work>,
    ) -> Result<Option<RunOutcome>> {
        if context.is_none() {
            let run_id = self.target_call(|sink| sink.begin_run(job))?;
            self.target_call(|sink| sink.commit())?;
            self.handle.with(|p| {
                p.runs[job.index] = RunProgress::new(run_id, display_root(job));
                p.current_run = Some(job.index);
                p.refresh_state();
            });
            *context = Some(Work {
                run_id,
                files: Vec::new(),
                items: Vec::new(),
                sequences: Sequences::default(),
                counts: RunCounts::default(),
                thumbs: Vec::new(),
                cursor: 0,
                prepared: false,
            });
            self.emit(true);
        }
        let work = context.as_mut().expect("work was initialized");
        let run_id = work.run_id;
        self.set_run(run_id, |run| {
            run.state = ImportState::Running;
            if run
                .note
                .as_deref()
                .is_some_and(|note| note.starts_with("storage.wait."))
            {
                run.note = None;
            }
        });
        if !work.prepared {
            self.prepare(job, work)?;
            if self.control.is_cancelling() {
                return self
                    .finish(
                        run_id,
                        job,
                        ImportState::Cancelled,
                        work.counts,
                        std::mem::take(&mut work.thumbs),
                    )
                    .map(Some);
            }
        }
        let mut throttle = Throttle::new(self.throttle);
        let mut commit = BatchCommit::new(200, Duration::from_millis(500), Instant::now());
        while work.cursor < work.items.len() {
            self.ensure_target()?;
            if self.wait_at_safe_point(run_id) {
                self.target_call(|sink| sink.save_sequences(&work.sequences))?;
                self.target_call(|sink| sink.commit())?;
                return self
                    .finish(
                        run_id,
                        job,
                        ImportState::Cancelled,
                        work.counts,
                        std::mem::take(&mut work.thumbs),
                    )
                    .map(Some);
            }
            if self
                .recovery
                .is_some_and(|gate| !gate.source_ready(&job.source_root))
            {
                self.target_call(|sink| sink.commit())?;
                return Ok(None);
            }
            let item = &work.items[work.cursor];
            let Some(target) = item.target_rel() else {
                work.cursor += 1;
                continue;
            };
            if item.status_str() != "pending" {
                work.cursor += 1;
                continue;
            }
            let file = &work.files[item.index];
            self.set_run(run_id, |run| {
                run.current = Some(CurrentItem {
                    source: file.rel_path.clone(),
                    target: Some(target.to_string()),
                })
            });
            // 完整内容核验后才能接纳 pending；正常未落地的文件直接复制。
            let copied = self.ops.exists(target) && self.ops.verified_copy(&file.abs_path, target);
            if !copied || !self.ops.source_unchanged(file) {
                let copy = if !self.ops.source_unchanged(file) {
                    Err(crate::Error::Unsupported("来源文件在扫描后发生变化".into()))
                } else {
                    self.copy_one(file, target).and_then(|_| {
                        if self.ops.source_unchanged(file) {
                            Ok(())
                        } else {
                            Err(crate::Error::Unsupported(
                                "来源文件在复制期间发生变化".into(),
                            ))
                        }
                    })
                };
                if let Err(error) = copy {
                    if self
                        .recovery
                        .is_some_and(|gate| !gate.source_ready(&job.source_root))
                    {
                        self.target_call(|sink| sink.commit())?;
                        return Ok(None);
                    }
                    if self.sink.check_session().is_err() && self.recovery.is_some() {
                        self.ensure_target()?;
                        continue;
                    }
                    let reason = error.to_string();
                    work.counts.failed += 1;
                    self.target_call(|sink| sink.mark(run_id, item, "failed", Some(&reason)))?;
                    self.note_error(file, Some(target), &reason, "failed");
                    self.bump(run_id, 0, 0, 1);
                    work.cursor += 1;
                    self.emit_if_due(&mut throttle);
                    continue;
                }
            }
            let copy_identity = self.ops.identity_of(target);
            match self.target_call(|sink| sink.register(run_id, item, file, copy_identity)) {
                Ok(()) => {
                    self.target_call(|sink| sink.mark(run_id, item, "imported", None))?;
                    work.counts.imported += 1;
                    self.bump(run_id, 1, 0, 0);
                    if item.role != Some(plan::Role::Raw) {
                        work.thumbs.push(target.to_string());
                    }
                }
                Err(error) => {
                    let reason = error.to_string();
                    work.counts.failed += 1;
                    self.target_call(|sink| sink.mark(run_id, item, "failed", Some(&reason)))?;
                    self.note_error(file, Some(target), &reason, "failed");
                    self.bump(run_id, 0, 0, 1);
                }
            }
            work.cursor += 1;
            if commit.tick(Instant::now()) {
                self.target_call(|sink| sink.save_sequences(&work.sequences))?;
                self.target_call(|sink| sink.commit())?;
            }
            self.emit_if_due(&mut throttle);
        }
        self.set_run(run_id, |run| run.current = None);
        self.set_stage(run_id, ImportStage::Thumbs);
        self.emit(true);
        if !work.thumbs.is_empty()
            && let Err(error) = (self.enqueue_thumbs)(&work.thumbs) {
                self.set_run(run_id, |run| {
                    run.note = Some(format!("缩略图入队失败：{error}"))
                });
            }
        self.target_call(|sink| sink.save_sequences(&work.sequences))?;
        self.finish(
            run_id,
            job,
            ImportState::Done,
            work.counts,
            std::mem::take(&mut work.thumbs),
        )
        .map(Some)
    }

    fn prepare(&mut self, job: &RunRequest, work: &mut Work) -> Result<()> {
        let run_id = work.run_id;
        let cancel = Cancel::new();
        let mut throttle = Throttle::new(self.throttle);
        self.set_stage(run_id, ImportStage::Scan);
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
        let walk =
            self.scanner
                .walk(&job.source_root, &job.scan_options(), &cancel, &mut on_file)?;
        self.set_run(run_id, |run| {
            run.scanned = walk.files as u64;
        });
        self.emit(true);

        if walk.cancelled || self.control.is_cancelling() {
            self.control.cancel();
            return Ok(());
        }

        /*
         * ①′ 把用户**排除**的文件丢掉。
         *
         * 位置选在扫描之后、读元数据之前：既不用改扫描器（`on_file` 返回 false 是
         * 「中止扫描」，不是「跳过这张」—— 语义差得远），也省掉了对排除项的白读元数据。
         * 剔除后**不计入 total / skipped** —— 它们本来就不属于这一批
         * （「不导入这张」不等于「尝试过但跳过了」）。
         */
        if !job.excluded.is_empty() {
            scanned.retain(|file| {
                !job.excluded
                    .contains(file.abs_path.to_string_lossy().as_ref())
            });
            let kept = scanned.len() as u64;
            self.set_run(run_id, |run| run.scanned = kept);
            self.emit(true);
        }

        /* ── ② 读元数据（模版要用才读）─────────────────── */
        if self
            .recovery
            .is_some_and(|gate| !gate.source_ready(&job.source_root))
        {
            return Err(crate::Error::PathNotFound(job.source_root.clone()));
        }
        let mut needs = ScanNeeds::for_template(&job.template, job.avoid_duplicates);
        needs.identity |= self.recovery.is_some();
        let mut extras: Vec<SourceExtras> = Vec::with_capacity(scanned.len());
        for chunk in scanned.chunks(ENRICH_CHUNK) {
            if self.cancel_requested(run_id) {
                return Ok(());
            }
            extras.extend(self.scanner.enrich(chunk, &needs, &cancel));
            let read = extras.len() as u64;
            self.set_run(run_id, |run| run.scanned = read);
            self.emit_if_due(&mut throttle);
        }
        let mut files: Vec<SourceFile> = scanned
            .iter()
            .zip(extras)
            .map(|(scan, extra)| SourceFile::from_scanned(scan, extra))
            .collect();

        /* ── ③ 规划 ───────────────────────────────────── */
        self.set_stage(run_id, ImportStage::Plan);
        self.emit(true);
        let known = if job.avoid_duplicates {
            self.target_call(|sink| sink.known_sources())?
        } else {
            KnownSources::new()
        };
        let interrupted = self.target_call(|sink| sink.interrupted_sources())?;
        files.retain(|file| !interrupted.contains(file));
        let mut sequences = self.target_call(|sink| sink.sequences())?;
        let options = job.plan_options();

        // 续跑：上次「复制了但还没登记」且**大小相符**的目标路径。
        // 交给规划器当「我们自己占着的」—— 否则重名规则会给它加 `_01`，
        // 同一张照片就有两份了（`specs/M1-6.md` §7 风险 10）。
        let stale = self.target_call(|sink| sink.stale_pending())?;
        let mut resumable: Reserved = Reserved::new();
        let mut adopt: plan::Adopt = plan::Adopt::new();
        for (target, source_abs) in &stale {
            // 比对的是**绝对路径**：`import_items.source_path` 存的是绝对路径（`sink.rs` 写的），
            // 曾经拿它跟 `rel_path` 比 —— 于是这段「续跑要落回原名字」的逻辑在真机上一直是空转的。
            let Some(file) = files.iter().find(|f| {
                crate::store::path_semantics::PathForms::new(f.abs_path.to_string_lossy().as_ref())
                    .folded()
                    == crate::store::path_semantics::PathForms::new(source_abs).folded()
            }) else {
                continue;
            };
            // 只有「上次真的把文件放到那儿了（大小相符）」才认 —— 光有条 pending 行不算，
            // 那种情况照常编号即可（计数器是单调的，不会撞名）。
            if self.ops.size_of(target) == Some(file.size_bytes)
                && self.ops.verified_copy(&file.abs_path, target)
            {
                resumable.insert(target);
                adopt.insert(file.rel_path.clone(), target.clone());
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
            &adopt,
        );
        if self
            .recovery
            .is_some_and(|gate| !gate.source_ready(&job.source_root))
        {
            return Err(crate::Error::PathNotFound(job.source_root.clone()));
        }
        self.target_call(|sink| sink.record_plan(run_id, &files, &planned.items))?;
        self.target_call(|sink| sink.save_sequences(&sequences))?;
        self.target_call(|sink| sink.commit())?;

        work.counts.total = planned.items.len() as u64;
        work.counts.skipped = planned.counts.skipped as u64;
        work.counts.duplicates = planned.counts.duplicates as u64;
        work.counts.failed = planned.counts.failed as u64;
        self.set_run(run_id, |run| {
            run.stage = ImportStage::Import;
            run.total = work.counts.total;
            run.skipped = work.counts.skipped;
            run.duplicates = work.counts.duplicates;
            run.failed = work.counts.failed;
            run.done = work.counts.skipped + work.counts.failed;
        });
        self.collect_plan_errors(&planned.items);
        work.files = files;
        work.items = planned.items;
        work.sequences = sequences;
        work.prepared = true;
        self.emit(true);
        Ok(())
    }

    fn set_waiting(&mut self, index: usize, reason: &str) {
        let changed = self.handle.with(|p| {
            let run = &mut p.runs[index];
            let changed = run.state != ImportState::Waiting || run.note.as_deref() != Some(reason);
            run.state = ImportState::Waiting;
            run.note = Some(reason.into());
            p.refresh_state();
            changed
        });
        if changed {
            self.emit(true);
        }
    }
    fn ensure_target(&mut self) -> Result<()> {
        if self.sink.check_session().is_ok() {
            return Ok(());
        }
        let Some(gate) = self.recovery else {
            return self.sink.check_session();
        };
        while !self.control.is_cancelling() {
            if gate.target_ready() && self.sink.check_session().is_ok() {
                self.handle.with(|p| {
                    for run in &mut p.runs {
                        if run.note.as_deref() == Some("storage.wait.repository") {
                            run.state = ImportState::Running;
                            run.note = None;
                        }
                    }
                    p.refresh_state();
                });
                self.emit(true);
                return Ok(());
            }
            let changed = self.handle.with(|p| {
                let mut changed = false;
                for run in &mut p.runs {
                    if !run.state.is_final() {
                        changed |= run.note.as_deref() != Some("storage.wait.repository");
                        run.state = ImportState::Waiting;
                        run.note = Some("storage.wait.repository".into());
                    }
                }
                p.refresh_state();
                changed
            });
            if changed {
                self.emit(true);
            }
            gate.wait();
        }
        Err(crate::Error::Unsupported("导入已取消".into()))
    }
    fn target_call<T>(
        &mut self,
        mut action: impl FnMut(&mut dyn ImportSink) -> Result<T>,
    ) -> Result<T> {
        loop {
            self.ensure_target()?;
            match action(self.sink) {
                Err(_) if self.recovery.is_some() && self.sink.check_session().is_err() => continue,
                result => return result,
            }
        }
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
            if let Some(run) = p.runs.iter_mut().find(|r| {
                r.current
                    .as_ref()
                    .is_some_and(|c| c.target.as_deref() == Some(target))
            }) {
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
        self.target_call(|sink| sink.finish_run(run_id, state.run_state(), &counts))?;
        self.target_call(|sink| sink.commit())?;
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
        Ok(RunOutcome { thumbs })
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
        session_valid: Option<Arc<std::sync::atomic::AtomicBool>>,
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
        fn check_session(&self) -> Result<()> {
            if self
                .session_valid
                .as_ref()
                .is_some_and(|flag| !flag.load(Ordering::Acquire))
            {
                Err(crate::Error::SessionExpired)
            } else {
                Ok(())
            }
        }
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
                return Err(crate::error::Error::Unsupported(
                    "索引里已有这条路径".into(),
                ));
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

        fn finish_run(&mut self, run_id: i64, state: &str, counts: &RunCounts) -> Result<()> {
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
            created_ms: None,
        }
    }

    /// 一个「源目录里有这些文件」的世界（照片内容都在）。
    fn world(files: &[(&str, u64)]) -> MemoryFs {
        let mut fs = MemoryFs::new();
        let list: Vec<ScannedFile> = files
            .iter()
            .map(|(rel, size)| scanned(rel, *size))
            .collect();
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
                excluded: std::sync::Arc::new(std::collections::HashSet::new()),
            }
        }

        /// 跑一批（`hook` 在每次进度回调里被调，用来注入暂停/取消）。
        fn run_with(
            &mut self,
            jobs: &[RunRequest],
            hook: impl FnMut(&BatchProgress, &Control),
        ) -> BatchOutcome {
            self.run_recovering(jobs, None, hook)
        }
        fn run_recovering(
            &mut self,
            jobs: &[RunRequest],
            recovery: Option<&dyn StorageRecovery>,
            mut hook: impl FnMut(&BatchProgress, &Control),
        ) -> BatchOutcome {
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
                recovery,
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

    struct RecoveryTest {
        source: std::cell::Cell<bool>,
        target: std::cell::Cell<bool>,
        waits: std::cell::Cell<usize>,
    }
    impl StorageRecovery for RecoveryTest {
        fn source_ready(&self, root: &std::path::Path) -> bool {
            root.to_string_lossy().contains("other") || self.source.get()
        }
        fn target_ready(&self) -> bool {
            self.target.get()
        }
        fn wait(&self) {
            self.waits.set(self.waits.get() + 1);
            self.source.set(true);
            self.target.set(true);
        }
    }
    #[test]
    fn waiting_source_defers_only_itself_and_continues_from_safe_cursor() {
        let mut h = Harness::new(world(&[("a.jpg", 5), ("b.jpg", 6)]));
        h.fs.add_source("/other/c.jpg", &[1, 2, 3]);
        h.fs.set_scan("/other", vec![scanned_at("/other", "c.jpg", 3)]);
        let first = h.job(":FILENAME");
        let mut second = first.clone();
        second.index = 1;
        second.source_root = "/other".into();
        let gate = RecoveryTest {
            source: true.into(),
            target: true.into(),
            waits: 0.into(),
        };
        h.run_recovering(&[first, second], Some(&gate), |p, _| {
            if p.imported == 1 && gate.waits.get() == 0 {
                gate.source.set(false);
            }
        });
        assert_eq!(h.handle.snapshot().state, ImportState::Done);
        assert_eq!(h.handle.snapshot().imported, 3);
        assert!(h.events.iter().any(|p| p.imported == 2
            && p.runs[0].state == ImportState::Waiting
            && p.runs[1].state == ImportState::Done));
        assert_eq!(h.fs.copied().len(), 3);
        assert_eq!(gate.waits.get(), 1);
    }
    #[test]
    fn cancellation_while_source_waits_does_not_create_a_run_or_failure() {
        let mut h = Harness::new(world(&[("a.jpg", 5)]));
        let job = h.job(":FILENAME");
        let gate = RecoveryTest {
            source: false.into(),
            target: true.into(),
            waits: 0.into(),
        };
        let result = h.run_recovering(&[job], Some(&gate), |p, c| {
            if p.state == ImportState::Waiting {
                c.cancel();
            }
        });
        assert_eq!(result.state, Some(ImportState::Cancelled));
        assert!(result.run_ids.is_empty());
        assert_eq!(result.counts.failed, 0);
        assert_eq!(h.fs.copied().len(), 0);
    }
    #[test]
    fn real_target_disconnect_preserves_buffer_and_rebinds_original_entity() {
        use crate::import::fsops::{FsScanner, RepoFs};
        use crate::import::sink::CatalogSink;
        use crate::store::db::{CatalogDb, OpenOpts};
        use crate::store::session::{CatalogSessions, TaskCatalog};
        use std::sync::Arc;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let root = dir.path().join("库");
        let detached = dir.path().join("detached");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("a.jpg"), [1u8, 2, 3]).unwrap();
        std::fs::write(source.join("b.jpg"), [4u8, 5, 6]).unwrap();
        let created = CatalogDb::create(&root, "库", None, OpenOpts::new(None, 0)).unwrap();
        let id = created.meta().id.clone();
        drop(created);
        let sessions = CatalogSessions::default();
        let db = sessions
            .acquire(&id, || Ok(root.clone()), OpenOpts::new(None, 0))
            .unwrap();
        let task = Arc::new(TaskCatalog::new(db.clone()));
        struct Gate<'a> {
            task: &'a TaskCatalog,
            sessions: &'a CatalogSessions,
            root: &'a std::path::Path,
            detached: &'a std::path::Path,
            id: &'a str,
            attempts: std::cell::Cell<usize>,
        }
        impl StorageRecovery for Gate<'_> {
            fn source_ready(&self, _: &std::path::Path) -> bool {
                true
            }
            fn target_ready(&self) -> bool {
                let attempt = self.attempts.get();
                self.attempts.set(attempt + 1);
                if attempt == 0 {
                    self.sessions.invalidate(self.id).unwrap();
                    return false;
                }
                if self.detached.exists() {
                    std::fs::rename(self.detached, self.root).unwrap();
                }
                self.sessions
                    .acquire(self.id, || Ok(self.root.into()), OpenOpts::new(None, 0))
                    .is_ok_and(|db| self.task.install(db).is_ok())
            }
            fn wait(&self) {}
        }
        let gate = Gate {
            task: &task,
            sessions: &sessions,
            root: &root,
            detached: &detached,
            id: &id,
            attempts: 0.into(),
        };
        let mut sink = CatalogSink::recovering(&db, &task, 0);
        let ops = RepoFs::for_task(task.clone());
        let scanner = FsScanner;
        let control = Control::new();
        let handle = BatchHandle::new(BatchProgress::new("real", 0));
        let mut disconnected = false;
        let mut waiting = false;
        let mut events = |p: &BatchProgress| {
            if p.imported == 1 && !disconnected {
                std::fs::rename(&root, &detached).unwrap();
                disconnected = true;
            }
            waiting |= p.state == ImportState::Waiting;
        };
        let mut thumbs = |_: &[String]| Ok(0);
        let mut job = Harness::new(MemoryFs::new()).job(":FILENAME");
        job.source_root = source;
        let result = run_batch(
            Deps {
                sink: &mut sink,
                ops: &ops,
                scanner: &scanner,
                control: &control,
                handle: &handle,
                on_progress: &mut events,
                enqueue_thumbs: &mut thumbs,
                recovery: Some(&gate),
                now_ms: 0,
                throttle: Duration::ZERO,
            },
            &[job],
        );
        assert!(waiting);
        assert_eq!(result.state, Some(ImportState::Done));
        assert_eq!(result.counts.imported, 2);
        assert_eq!(result.counts.failed, 0);
        assert_eq!(
            task.current()
                .read(
                    |c| Ok(c.query_row("SELECT count(*) FROM asset_files", [], |r| r
                        .get::<_, i64>(0))?)
                )
                .unwrap(),
            2
        );
    }
    const TPL: &str = ":CYEAR-:CMONTH-:CDAY/MY:FILENAME";

    /* ══════════════════════════════════════════════════════════
     * 主流程
     * ══════════════════════════════════════════════════════════ */

    #[test]
    fn expired_session_stops_remaining_items_and_sources_but_retains_success() {
        let mut h = Harness::new(world(&[("a.jpg", 5), ("b.jpg", 6)]));
        let valid = Arc::new(std::sync::atomic::AtomicBool::new(true));
        h.sink.session_valid = Some(Arc::clone(&valid));
        let mut next = h.job(TPL);
        next.index = 1;
        next.source_root = PathBuf::from("/next-source");
        let outcome = h.run_with(&[h.job(TPL), next], |progress, _| {
            if progress.imported == 1 {
                valid.store(false, Ordering::Release);
            }
        });
        assert_eq!(outcome.state, Some(ImportState::Failed));
        assert_eq!(outcome.counts.imported, 1);
        assert_eq!(h.fs.file_count(), 1);
        assert_eq!(h.sink.runs.len(), 1);
        assert_eq!(h.sink.item("a.jpg").status, "imported");
        assert!(h.handle.snapshot().errors_total > 0);
    }

    #[test]
    fn a_full_run_copies_registers_and_enqueues_thumbnails() {
        let fs = world(&[("P0001.png", 10), ("P0001.ORF", 20)]);
        let mut h = Harness::new(fs);
        let outcome = h.run(&[h.job(TPL)]);

        assert_eq!(outcome.state, Some(ImportState::Done));
        assert_eq!(outcome.counts.imported, 2);
        assert_eq!(outcome.counts.failed, 0);
        assert_eq!(
            h.fs.paths(),
            vec![
                "photos/2026-08-15/MYP0001.png",
                "photos/2026-08-15/_RAW/MYP0001.ORF",
            ]
        );
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
            h.sink
                .item("bad.jpg")
                .reason
                .as_deref()
                .unwrap_or_default()
                .contains("读不了"),
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
        assert_eq!(h.sink.runs.len(), 0, "连 run 都不开：库里不该留下空 run 行");
        assert_eq!(
            h.handle.snapshot().state,
            ImportState::Cancelled,
            "整批要显示成已取消"
        );
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

        assert_eq!(
            outcome.state,
            Some(ImportState::Done),
            "暂停之后要能接着跑完"
        );
        assert_eq!(outcome.counts.imported, 2);
        assert!(
            h.events.iter().any(|e| e.state == ImportState::Paused),
            "暂停要真的被宣布出去（不然界面看不到）"
        );
        assert!(
            h.events
                .iter()
                .any(|e| e.runs.iter().any(|r| r.state == ImportState::Paused)),
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
        // `import_items.source_path` 存的是**绝对路径**（`sink.rs` 写的），
        // 所以这里的夹具也得是绝对路径 —— 曾经这里是相对路径，于是「续跑落回原名字」
        // 在真机上一直是空转的（比较永远不成立）。
        h.sink.stale.insert(
            "photos/2026-08-15/MYa.jpg".to_string(),
            format!("{SRC}/root/a.jpg"),
        );

        let outcome = h.run(&[h.job(TPL)]);
        assert_eq!(outcome.counts.imported, 1, "续跑要把它补登记上");
        assert!(h.fs.copied().is_empty(), "不该重拷一份");
        assert_eq!(h.sink.item("a.jpg").status, "imported");
        assert_eq!(
            h.fs.paths(),
            vec!["photos/2026-08-15/MYa.jpg".to_string()],
            "要落回上次那个名字；编个新号就会在磁盘上多出一份没人认识的副本"
        );
    }

    #[test]
    fn stale_pending_with_a_wrong_size_does_not_count_as_ours() {
        let fs = world(&[("a.jpg", 5)]);
        let mut h = Harness::new(fs);
        // 大小对不上 → 这文件不是我们写的（我们只会留 .part）
        h.fs.add_existing("photos/2026-08-15/MYa.jpg", &[0u8; 3]);
        h.sink.stale.insert(
            "photos/2026-08-15/MYa.jpg".to_string(),
            format!("{SRC}/root/a.jpg"),
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
    fn excluded_files_are_dropped_before_planning() {
        // 两张照片，用户排除了其中一张：它不该被登记、也不该进任何计数。
        let mut h = Harness::new(world(&[("a.jpg", 5), ("b.jpg", 6)]));
        let mut job = h.job(":FILENAME");
        job.excluded = std::sync::Arc::new(["/src/root/b.jpg".to_string()].into_iter().collect());

        let outcome = h.run(&[job]);

        assert_eq!(h.sink.sources(), vec!["a.jpg"], "被排除的文件不该被登记");
        assert_eq!(outcome.counts.imported, 1);
        assert_eq!(outcome.counts.total, 1, "被排除的不计入 total");
        assert_eq!(
            outcome.counts.skipped, 0,
            "排除不是「跳过」——它本来就不属于这批"
        );
        let snap = h.handle.snapshot();
        assert_eq!(snap.total, 1);
        assert_eq!(snap.runs[0].scanned, 1, "扫描计数也按剔除后的算");
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

        assert_eq!(
            outcome.counts.imported, 1,
            "文件已经进库了，缩略图不该拖垮它"
        );
        assert_eq!(outcome.state, Some(ImportState::Done));
        assert_eq!(h.sink.run(1).state, "done");
        let snap = h.handle.snapshot();
        assert!(
            snap.runs[0]
                .note
                .as_deref()
                .unwrap_or_default()
                .contains("缩略图"),
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
        assert_eq!(
            control.state(),
            ImportState::Cancelling,
            "取消之后暂停不生效"
        );
        control.resume();
        assert_eq!(
            control.state(),
            ImportState::Cancelling,
            "取消之后恢复不生效"
        );

        let control = Control::new();
        control.pause();
        assert_eq!(control.state(), ImportState::Pausing);
        control.pause();
        assert_eq!(control.state(), ImportState::Pausing);
        control.resume();
        assert_eq!(control.state(), ImportState::Running);
    }
}
