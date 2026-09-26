//! 导入进度的状态与快照（`REPOSITORY.md` §4.4）。
//!
//! 一次「按下导入」= 一个 **batch**；batch 里**每个源目录一个 run**
//! （`source_root` 一列只装得下一个根目录，且各目录的计数天然独立 —— `specs/M1-6.md` §3.4）。
//! 于是有三级：
//!
//! ```text
//! BatchProgress   ← 界面要的一整份快照（总计 + 每个 run 的明细 + 错误清单）
//!   └─ RunProgress ← 一个源目录
//!        └─ CurrentItem / ImportError
//! ```
//!
//! 本模块只有**数据与纯函数**（汇总、截断、节流、批事务的时机判定），
//! 不认识磁盘、数据库与 Tauri —— 那些都在 `runner` 与外壳里。
//!
//! 并发约定：跑导入的线程**写**、命令与事件**读**，所以外面套一层
//! [`BatchHandle`]（`Arc<Mutex<..>>`）：读的时候拿一份克隆出去的快照。

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

/// 导入的四个阶段（`REPOSITORY.md` §4.4 的「当前阶段 / 总阶段」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportStage {
    /// 扫描源目录（还不知道总数）。
    Scan,
    /// 算目标路径（判重、序号、重名）。
    Plan,
    /// 复制 + 登记。
    Import,
    /// 把缩略图任务排进队列（入队即算这一阶段完成）。
    Thumbs,
    /// 整批结束。
    Done,
}

/// 整批的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportState {
    /// 正在跑。
    Running,
    /// 收到暂停请求，还没有停在安全点上。
    Pausing,
    /// 已停在安全点（文件之间）。
    Paused,
    /// 收到取消请求，正在收尾。
    Cancelling,
    /// 已取消：**已导入的部分保留**（`REPOSITORY.md` §4.5）。
    Cancelled,
    /// 全部完成。
    Done,
    /// 整批失败（例如库离线、模版非法）。
    Failed,
}

impl ImportState {
    /// 是不是已经结束（不会再变了）。
    #[must_use]
    pub fn is_final(self) -> bool {
        matches!(self, Self::Cancelled | Self::Done | Self::Failed)
    }

    /// 写进 `import_runs.state` 的词（库里只有这四种）。
    #[must_use]
    pub fn run_state(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            _ => "running",
        }
    }
}

/// 正在处理的文件（「它没卡住」这件事只能靠它证明）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentItem {
    /// 源路径（给人看的）。
    pub source: String,
    /// 库内目标路径（还没算出来时为 `None`）。
    pub target: Option<String>,
}

/// 一条错误（失败或跳过）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportError {
    /// 源路径。
    pub source: String,
    /// 库内目标路径。
    pub target: Option<String>,
    /// 原因（给人看的）。
    pub reason: String,
    /// `failed` 还是 `skipped`。
    pub status: String,
}

/// 一个源目录（一个 run）的进度。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunProgress {
    /// `import_runs.id`。
    pub run_id: i64,
    /// 源根目录。
    pub source_root: String,
    /// 阶段。
    pub stage: ImportStage,
    /// 状态。
    pub state: ImportState,
    /// 扫描到的文件数（扫描阶段就在涨）。
    pub scanned: u64,
    /// 要处理的条目总数（规划完才知道）。
    pub total: u64,
    /// 已处理（含跳过与失败）。
    pub done: u64,
    /// 已导入。
    pub imported: u64,
    /// 跳过（含重复）。
    pub skipped: u64,
    /// 其中「已在库中」的。
    pub duplicates: u64,
    /// 失败。
    pub failed: u64,
    /// 已复制字节（吞吐用）。
    pub bytes: u64,
    /// 当前文件。
    pub current: Option<CurrentItem>,
    /// run 级提示（例如「上次中断，正在补齐」）。
    pub note: Option<String>,
}

impl RunProgress {
    /// 新建一个还没开始的 run。
    #[must_use]
    pub fn new(run_id: i64, source_root: impl Into<String>) -> Self {
        Self {
            run_id,
            source_root: source_root.into(),
            stage: ImportStage::Scan,
            state: ImportState::Running,
            scanned: 0,
            total: 0,
            done: 0,
            imported: 0,
            skipped: 0,
            duplicates: 0,
            failed: 0,
            bytes: 0,
            current: None,
            note: None,
        }
    }
}

/// 整批的快照（事件载荷与 `import_status` 返回的就是它）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchProgress {
    /// 进程内的批次 id（重启后就没有了）。
    pub batch_id: String,
    /// 整批阶段（取最靠后的那个 run）。
    pub stage: ImportStage,
    /// 整批状态。
    pub state: ImportState,
    /// 每个源目录一条。
    pub runs: Vec<RunProgress>,
    /// 正在跑第几个（从 0 开始；`[2/3]` 那种提示用它）。
    pub current_run: Option<usize>,
    /// 合计。
    pub total: u64,
    /// 合计。
    pub done: u64,
    /// 合计。
    pub imported: u64,
    /// 合计。
    pub skipped: u64,
    /// 合计（其中「已在库中」的）。
    pub duplicates: u64,
    /// 合计。
    pub failed: u64,
    /// 合计。
    pub bytes: u64,
    /// 错误清单（**只留尾巴**，见 [`ERROR_LIST_CAP`]）。
    pub errors: Vec<ImportError>,
    /// 错误总数（清单被截断了也知道有多少）。
    pub errors_total: u64,
    /// 目标卷剩余空间（拿不到就是 `None`）。
    pub free_bytes: Option<u64>,
    /// 开始时间（Unix 毫秒）。
    pub started_at: i64,
    /// 结束时间。
    pub finished_at: Option<i64>,
}

/// 快照里最多留多少条错误（多了界面也看不过来，但计数一直准）。
pub const ERROR_LIST_CAP: usize = 200;

impl BatchProgress {
    /// 新的一批。
    #[must_use]
    pub fn new(batch_id: impl Into<String>, started_at: i64) -> Self {
        Self {
            batch_id: batch_id.into(),
            stage: ImportStage::Scan,
            state: ImportState::Running,
            runs: Vec::new(),
            current_run: None,
            total: 0,
            done: 0,
            imported: 0,
            skipped: 0,
            duplicates: 0,
            failed: 0,
            bytes: 0,
            errors: Vec::new(),
            errors_total: 0,
            free_bytes: None,
            started_at,
            finished_at: None,
        }
    }

    /// 从 `runs` 重新汇总（每次改完一个 run 就调一次）。
    pub fn recompute(&mut self) {
        let mut total = 0;
        let mut done = 0;
        let mut imported = 0;
        let mut skipped = 0;
        let mut duplicates = 0;
        let mut failed = 0;
        let mut bytes = 0;
        for run in &self.runs {
            total += run.total;
            done += run.done;
            imported += run.imported;
            skipped += run.skipped;
            duplicates += run.duplicates;
            failed += run.failed;
            bytes += run.bytes;
        }
        self.total = total;
        self.done = done;
        self.imported = imported;
        self.skipped = skipped;
        self.duplicates = duplicates;
        self.failed = failed;
        self.bytes = bytes;

        // 整批阶段取「最靠后」的那个 run：一个在导入、一个还没扫完时，
        // 进度条该按导入算，否则会往回跳
        self.stage = self
            .runs
            .iter()
            .map(|r| r.stage)
            .max_by_key(|s| stage_rank(*s))
            .unwrap_or(ImportStage::Scan);
    }

    /// 整批状态：全结束才算结束；有取消就是取消。
    pub fn refresh_state(&mut self) {
        if self.runs.is_empty() {
            return;
        }
        if self.runs.iter().any(|r| r.state == ImportState::Cancelled) {
            // 取消之后：用户按下取消那一刻就已经是终局，别的 run 也跟着停
            self.state = ImportState::Cancelled;
            return;
        }
        if self.runs.iter().any(|r| r.state == ImportState::Failed) {
            self.state = ImportState::Failed;
            return;
        }
        if self.runs.iter().all(|r| r.state == ImportState::Done) {
            self.state = ImportState::Done;
            return;
        }
        if self
            .runs
            .iter()
            .any(|r| matches!(r.state, ImportState::Running | ImportState::Pausing))
        {
            self.state = ImportState::Running;
            return;
        }
        self.state = ImportState::Paused;
    }

    /// 记一条错误（清单截断、计数不截断）。
    pub fn push_error(&mut self, error: ImportError) {
        self.errors_total += 1;
        if self.errors.len() >= ERROR_LIST_CAP {
            self.errors.remove(0);
        }
        self.errors.push(error);
    }

    /// 百分比：扫描/规划阶段**没有总数**，返回 `None`（界面用不确定进度条）。
    ///
    /// 不编造数字：拿「已扫描 N 个文件」硬凑一个百分比，只会让进度条先快后慢、
    /// 最后停在 90% —— 那是骗人的。
    #[must_use]
    pub fn percent(&self) -> Option<f64> {
        match self.stage {
            ImportStage::Scan | ImportStage::Plan => None,
            ImportStage::Thumbs => Some(100.0),
            ImportStage::Done => Some(100.0),
            ImportStage::Import => {
                if self.total == 0 {
                    Some(100.0)
                } else {
                    let ratio = self.done as f64 / self.total as f64;
                    Some((ratio.clamp(0.0, 1.0) * 100.0).round())
                }
            }
        }
    }

    /// 当前正在跑的那个 run。
    #[must_use]
    pub fn current(&self) -> Option<&RunProgress> {
        self.current_run.and_then(|idx| self.runs.get(idx))
    }
}

fn stage_rank(stage: ImportStage) -> u8 {
    match stage {
        ImportStage::Scan => 0,
        ImportStage::Plan => 1,
        ImportStage::Import => 2,
        ImportStage::Thumbs => 3,
        ImportStage::Done => 4,
    }
}

/// 共享的批次状态：runner 写、命令与事件读。
#[derive(Debug, Clone)]
pub struct BatchHandle {
    inner: Arc<Mutex<BatchProgress>>,
}

impl BatchHandle {
    /// 用一份初始快照建句柄。
    #[must_use]
    pub fn new(progress: BatchProgress) -> Self {
        Self {
            inner: Arc::new(Mutex::new(progress)),
        }
    }

    /// 在锁里改一改。
    pub fn with<R>(&self, f: impl FnOnce(&mut BatchProgress) -> R) -> R {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut guard)
    }

    /// 拿一份克隆出去的快照（**不要在锁里序列化**）。
    #[must_use]
    pub fn snapshot(&self) -> BatchProgress {
        self.with(|p| p.clone())
    }
}

/// 事件节流：大目录里「每个文件一条事件」会把 WebView 打爆。
///
/// 阶段切换 / 暂停 / 结束这些**状态变化**不受节流约束（`force` 走）。
#[derive(Debug)]
pub struct Throttle {
    interval: Duration,
    last: Option<Instant>,
}

impl Default for Throttle {
    fn default() -> Self {
        Self::new(Duration::from_millis(100))
    }
}

impl Throttle {
    /// 指定最小间隔。
    #[must_use]
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: None,
        }
    }

    /// 现在能不能发一条？（会记住这次的时间）
    pub fn ready_at(&mut self, now: Instant) -> bool {
        match self.last {
            Some(last) if now.duration_since(last) < self.interval => false,
            _ => {
                self.last = Some(now);
                true
            }
        }
    }
}

/// 批事务的时机：攒够 `every_items` 条、或过了 `every` 时间就提交一次。
///
/// 太碎（一条一提交）会让大目录慢得没法看；太粗（整批一次）会让进度与崩溃续跑都变差。
#[derive(Debug)]
pub struct BatchCommit {
    every_items: usize,
    every: Duration,
    items: usize,
    last: Instant,
}

impl BatchCommit {
    /// 默认：每 200 条或每 500 毫秒。
    #[must_use]
    pub fn new(every_items: usize, every: Duration, now: Instant) -> Self {
        Self {
            every_items: every_items.max(1),
            every,
            items: 0,
            last: now,
        }
    }

    /// 记一条已处理的条目，返回「现在该提交吗」。
    pub fn tick(&mut self, now: Instant) -> bool {
        self.items += 1;
        if self.items >= self.every_items || now.duration_since(self.last) >= self.every {
            self.items = 0;
            self.last = now;
            true
        } else {
            false
        }
    }

    /// 手动要一次提交（收尾、暂停、取消时用）。
    pub fn flush(&mut self, now: Instant) -> bool {
        if self.items == 0 {
            return false;
        }
        self.items = 0;
        self.last = now;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(id: i64, stage: ImportStage, state: ImportState) -> RunProgress {
        RunProgress {
            stage,
            state,
            ..RunProgress::new(id, format!("/src/{id}"))
        }
    }

    #[test]
    fn recompute_sums_every_run() {
        let mut batch = BatchProgress::new("b1", 1_000);
        batch.runs = vec![
            RunProgress {
                total: 10,
                done: 4,
                imported: 3,
                skipped: 1,
                failed: 0,
                bytes: 100,
                ..run(1, ImportStage::Import, ImportState::Running)
            },
            RunProgress {
                total: 5,
                done: 5,
                imported: 4,
                skipped: 0,
                failed: 1,
                bytes: 50,
                ..run(2, ImportStage::Done, ImportState::Done)
            },
        ];
        batch.recompute();
        assert_eq!(batch.total, 15);
        assert_eq!(batch.done, 9);
        assert_eq!(batch.imported, 7);
        assert_eq!(batch.skipped, 1);
        assert_eq!(batch.failed, 1);
        assert_eq!(batch.bytes, 150);
    }

    #[test]
    fn stage_takes_the_furthest_run() {
        let mut batch = BatchProgress::new("b", 0);
        batch.runs = vec![
            run(1, ImportStage::Scan, ImportState::Running),
            run(2, ImportStage::Import, ImportState::Running),
        ];
        batch.recompute();
        assert_eq!(batch.stage, ImportStage::Import, "进度不能往回跳");
    }

    #[test]
    fn state_follows_the_runs() {
        let mut batch = BatchProgress::new("b", 0);
        batch.runs = vec![
            run(1, ImportStage::Done, ImportState::Done),
            run(2, ImportStage::Done, ImportState::Done),
        ];
        batch.refresh_state();
        assert_eq!(batch.state, ImportState::Done);

        batch.runs[1].state = ImportState::Cancelled;
        batch.refresh_state();
        assert_eq!(batch.state, ImportState::Cancelled, "有取消就是取消");

        batch.runs = vec![
            run(1, ImportStage::Import, ImportState::Paused),
            run(2, ImportStage::Import, ImportState::Paused),
        ];
        batch.refresh_state();
        assert_eq!(batch.state, ImportState::Paused, "全停下才算暂停");

        batch.runs[0].state = ImportState::Running;
        batch.refresh_state();
        assert_eq!(batch.state, ImportState::Running, "还有一个在跑就是运行中");
    }

    #[test]
    fn percent_is_honest_about_not_knowing_the_total() {
        let mut batch = BatchProgress::new("b", 0);
        batch.runs = vec![run(1, ImportStage::Scan, ImportState::Running)];
        batch.recompute();
        assert_eq!(batch.percent(), None, "扫描阶段不编造百分比");

        batch.runs[0].stage = ImportStage::Plan;
        batch.recompute();
        assert_eq!(batch.percent(), None);

        batch.runs[0].stage = ImportStage::Import;
        batch.runs[0].total = 4;
        batch.runs[0].done = 1;
        batch.recompute();
        assert_eq!(batch.percent(), Some(25.0));

        batch.stage = ImportStage::Thumbs;
        assert_eq!(batch.percent(), Some(100.0));
    }

    #[test]
    fn percent_handles_an_empty_batch() {
        let mut batch = BatchProgress::new("b", 0);
        batch.stage = ImportStage::Import;
        assert_eq!(batch.percent(), Some(100.0), "一条都没有 = 不用等");
    }

    #[test]
    fn errors_are_capped_but_counted() {
        let mut batch = BatchProgress::new("b", 0);
        for i in 0..(ERROR_LIST_CAP + 5) {
            batch.push_error(ImportError {
                source: format!("a{i}.jpg"),
                target: None,
                reason: "读不了".to_string(),
                status: "failed".to_string(),
            });
        }
        assert_eq!(batch.errors.len(), ERROR_LIST_CAP, "清单留尾巴");
        assert_eq!(batch.errors_total as usize, ERROR_LIST_CAP + 5, "计数不截断");
        assert_eq!(
            batch.errors.last().map(|e| e.source.as_str()),
            Some("a204.jpg"),
            "留的是最后几条"
        );
    }

    #[test]
    fn handle_snapshots_without_holding_the_lock() {
        let handle = BatchHandle::new(BatchProgress::new("b", 1));
        handle.with(|p| {
            p.runs.push(run(1, ImportStage::Scan, ImportState::Running));
            p.recompute();
        });
        let snap = handle.snapshot();
        assert_eq!(snap.runs.len(), 1);
        // 改快照不影响句柄
        let mut again = handle.snapshot();
        again.runs.clear();
        assert_eq!(handle.snapshot().runs.len(), 1);
    }

    #[test]
    fn throttle_lets_the_first_one_through_then_waits() {
        let mut throttle = Throttle::new(Duration::from_millis(100));
        let t0 = Instant::now();
        assert!(throttle.ready_at(t0), "第一条立刻发");
        assert!(!throttle.ready_at(t0));
        assert!(!throttle.ready_at(t0 + Duration::from_millis(99)));
        assert!(throttle.ready_at(t0 + Duration::from_millis(100)));
        assert!(!throttle.ready_at(t0 + Duration::from_millis(150)));
        assert!(throttle.ready_at(t0 + Duration::from_millis(200)));
    }

    #[test]
    fn batch_commit_fires_on_count_or_time() {
        let t0 = Instant::now();
        let mut commit = BatchCommit::new(3, Duration::from_millis(500), t0);
        assert!(!commit.tick(t0));
        assert!(!commit.tick(t0));
        assert!(commit.tick(t0), "第 3 条 → 按条数提交");

        assert!(!commit.tick(t0 + Duration::from_millis(100)));
        assert!(
            commit.tick(t0 + Duration::from_millis(600)),
            "过了 500ms → 按时间提交"
        );

        assert!(!commit.flush(t0), "没有待提交的就不提交");
        commit.tick(t0 + Duration::from_millis(700));
        assert!(commit.flush(t0 + Duration::from_millis(701)), "收尾时强制提交");
    }

    #[test]
    fn run_state_words_match_the_schema_comment() {
        assert_eq!(ImportState::Running.run_state(), "running");
        assert_eq!(ImportState::Paused.run_state(), "running", "暂停不是库里的状态");
        assert_eq!(ImportState::Done.run_state(), "done");
        assert_eq!(ImportState::Cancelled.run_state(), "cancelled");
        assert_eq!(ImportState::Failed.run_state(), "failed");
        assert!(ImportState::Cancelled.is_final());
        assert!(!ImportState::Pausing.is_final());
    }
}
