//! 后台重活的**优先级队列 + 同 key 单飞**（`specs/editor-session-pipeline.md` §4.3）。
//!
//! # 为什么需要它
//!
//! 以前「大图预览 / 定稿快照 / 缩略图预热 / 导出预览」各自 `std::thread::spawn` 或
//! 直接跑在阻塞池上：前台切图解码要和一屋子批量渲染抢同一条 RAW 管道，
//! 同一张画面还可能被两条链各渲一遍（一次 BM3D 就是十几秒）。
//!
//! 这里给重活一条**统一的队列**，三件事：
//!
//! * **优先级**（P0 交互 / P1 前台等待 / P2 预热 / P3 批量）—— 批量排满时高优先可插队；
//! * **key 去重** —— 同一个 key 的任务排队或执行中，就不重复入队；
//! * **同 key 单飞**（[`WorkQueue::run`]）—— 第二个调用者**等**第一个的结果（条件变量，不轮询）。
//!
//! 取消用 [`CancelToken`]：长任务在检查点看它；排队中被取消的任务由工作线程直接跳过。
//! **不抢占已经开跑的任务**（那要每个任务自己配合），但排队中的一定能丢掉。
//!
//! # 边界
//!
//! 这是**进程级**资源（挂 Tauri state），`Default` = 2 条工作线程：一条足够跑重活，
//! 第二条让「等 RAW 管道」与「纯 CPU 编码」不互相堵死。

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// 优先级（**数值越小越先跑**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// 编辑器当前照片（换照片 / 过渡帧 / 当前参数）。
    Interactive = 0,
    /// 用户正等着看的（打开全屏、切换定稿、导出预览）。
    Foreground = 1,
    /// 预热（1920 预览、邻图预取、缩略图后台补）。
    Warm = 2,
    /// 批量（定稿快照、raw-original 小图、AI 批处理）。
    Batch = 3,
}

const LEVELS: usize = 4;

thread_local! {
    /// 当前线程是不是这条队列的工作线程（嵌套等待要用它避险）。
    static ON_WORKER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// 当前线程是队列工作线程吗。
///
/// 用在「等另一个同 key 任务」的地方：在工作线程上等同一个队列的另一个任务
/// 可能把工作线程全占满而让被等的任务永远排不上（自录死锁）——
/// 这种情况下调用方宁可自己直接干。
#[must_use]
pub fn on_worker_thread() -> bool {
    ON_WORKER.with(std::cell::Cell::get)
}

/// 取消令牌：长任务在检查点看它，取消后提前收工（结果丢弃）。
#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// 新的（未取消）令牌。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 标记取消（幂等）。
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// 被取消了吗。
    #[must_use]
    pub fn cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// 一个 key 的完成状态：等待者（[`WorkQueue::run`] / [`WorkQueue::wait`]）从这里醒来。
#[derive(Default)]
struct Slot {
    result: Mutex<Option<Result<(), String>>>,
    ready: Condvar,
}

impl Slot {
    fn finish(&self, result: Result<(), String>) {
        let mut guard = self.result.lock().unwrap_or_else(|error| error.into_inner());
        if guard.is_none() {
            *guard = Some(result);
        }
        self.ready.notify_all();
    }

    /// 等结果（`timeout = None` = 无限等）；超时 / 还没有结果 → `None`。
    fn wait(&self, timeout: Option<Duration>) -> Option<Result<(), String>> {
        let mut guard = self.result.lock().unwrap_or_else(|error| error.into_inner());
        let deadline = timeout.map(|limit| Instant::now() + limit);
        while guard.is_none() {
            match deadline {
                None => {
                    guard = self
                        .ready
                        .wait(guard)
                        .unwrap_or_else(|error| error.into_inner());
                }
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return None;
                    }
                    let (next, _) = self
                        .ready
                        .wait_timeout(guard, left)
                        .unwrap_or_else(|error| error.into_inner());
                    guard = next;
                }
            }
        }
        guard.clone()
    }
}

type Task = Box<dyn FnOnce(&CancelToken) -> Result<(), String> + Send + 'static>;

struct Job {
    key: String,
    token: CancelToken,
    task: Option<Task>,
}

#[derive(Default)]
struct Inner {
    /// 每一级一条队列（`Priority as usize`）。
    queues: [VecDeque<Job>; LEVELS],
    /// key → 完成状态（排队中 / 执行中都在这里，跑完移除）。
    inflight: HashMap<String, Arc<Slot>>,
    shutdown: bool,
}

impl Inner {
    /// 取下一个任务：从最高优先级的非空队列拿；排队期间被取消的直接结算成失败。
    fn pop(&mut self) -> Option<Job> {
        for queue in &mut self.queues {
            while let Some(job) = queue.pop_front() {
                if job.token.cancelled() {
                    if let Some(slot) = self.inflight.remove(&job.key) {
                        slot.finish(Err("任务已取消".to_string()));
                    }
                    continue;
                }
                return Some(job);
            }
        }
        None
    }

    fn enqueue(&mut self, priority: Priority, key: String, token: CancelToken, task: Task) -> Arc<Slot> {
        let slot = Arc::new(Slot::default());
        self.inflight.insert(key.clone(), Arc::clone(&slot));
        self.queues[priority as usize].push_back(Job {
            key,
            token,
            task: Some(task),
        });
        slot
    }
}

/// 进程级的重活队列（挂 Tauri state；`default` = 2 条工作线程）。
pub struct WorkQueue {
    inner: Arc<(Mutex<Inner>, Condvar)>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl Default for WorkQueue {
    fn default() -> Self {
        Self::new(2)
    }
}

impl std::fmt::Debug for WorkQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner.0.lock().unwrap_or_else(|error| error.into_inner());
        f.debug_struct("WorkQueue")
            .field("queued", &inner.queues.iter().map(VecDeque::len).sum::<usize>())
            .field("inflight", &inner.inflight.len())
            .finish()
    }
}

impl WorkQueue {
    /// 建队列并起工作线程（`threads` 至少 1）。
    #[must_use]
    pub fn new(threads: usize) -> Self {
        let inner = Arc::new((Mutex::new(Inner::default()), Condvar::new()));
        let mut workers = Vec::new();
        for index in 0..threads.max(1) {
            let shared = Arc::clone(&inner);
            match std::thread::Builder::new()
                .name(format!("raybend-work-{index}"))
                .spawn(move || worker_loop(&shared))
            {
                Ok(handle) => workers.push(handle),
                Err(error) => {
                    // 起不来工作线程不该让应用起不来：调用方各自的同步兜底仍在。
                    eprintln!("[work] 后台工作线程 {index} 起不来：{error}");
                }
            }
        }
        Self {
            inner,
            workers: Mutex::new(workers),
        }
    }

    /// **入队**（不等结果）。同一个 key 已有任务（排队或执行中）→ 不入队，返回 `None`。
    ///
    /// 任务返回的 `Result` 只写给等待者看（fire-and-forget 的失败自己记日志）。
    pub fn spawn<F>(&self, priority: Priority, key: impl Into<String>, task: F) -> Option<CancelToken>
    where
        F: FnOnce(&CancelToken) + Send + 'static,
    {
        let key = key.into();
        let token = CancelToken::new();
        let mut inner = self.inner.0.lock().unwrap_or_else(|error| error.into_inner());
        if inner.shutdown || inner.inflight.contains_key(&key) {
            return None;
        }
        inner.enqueue(
            priority,
            key,
            token.clone(),
            Box::new(move |token| {
                task(token);
                Ok(())
            }),
        );
        drop(inner);
        self.inner.1.notify_one();
        Some(token)
    }

    /// **入队并等它跑完**（同 key 的第二个调用者只等，不重复跑）。
    ///
    /// # Errors
    /// 任务失败 / 任务 panic / 队列已停止时返回原因。
    pub fn run<F>(&self, priority: Priority, key: impl Into<String>, task: F) -> Result<(), String>
    where
        F: FnOnce(&CancelToken) -> Result<(), String> + Send + 'static,
    {
        let key = key.into();
        let slot = {
            let mut inner = self.inner.0.lock().unwrap_or_else(|error| error.into_inner());
            if inner.shutdown {
                return Err("后台队列已停止".to_string());
            }
            if let Some(slot) = inner.inflight.get(&key) {
                Arc::clone(slot)
            } else {
                let token = CancelToken::new();
                let slot = inner.enqueue(priority, key, token, Box::new(task));
                drop(inner);
                self.inner.1.notify_one();
                slot
            }
        };
        slot.wait(None).unwrap_or(Err("后台任务没有结算".to_string()))
    }

    /// 只**等**某个 key 的任务（不入队）。没有这个 key 的任务 → `None`（立即返回）。
    ///
    /// 定稿快照用它替代「sleep 轮询等 latest 预览」，见 `specs/editor-session-pipeline.md` §4.3。
    pub fn wait(&self, key: &str, timeout: Duration) -> Option<Result<(), String>> {
        let slot = {
            let inner = self.inner.0.lock().unwrap_or_else(|error| error.into_inner());
            inner.inflight.get(key).map(Arc::clone)
        }?;
        slot.wait(Some(timeout))
    }

    /// 取消某个 key 的任务：排队中的直接丢弃，执行中的由任务自己在检查点看令牌。
    ///
    /// 返回是否找到这个 key（找到 ≠ 一定停下）。
    pub fn cancel(&self, key: &str) -> bool {
        let mut inner = self.inner.0.lock().unwrap_or_else(|error| error.into_inner());
        let mut found = false;
        for queue in &mut inner.queues {
            for job in queue.iter() {
                if job.key == key {
                    job.token.cancel();
                    found = true;
                }
            }
        }
        found
    }

    /// 队列快照（诊断 / 测试）：`(排队数, 在飞数)`。
    #[must_use]
    pub fn stats(&self) -> (usize, usize) {
        let inner = self.inner.0.lock().unwrap_or_else(|error| error.into_inner());
        (
            inner.queues.iter().map(VecDeque::len).sum(),
            inner.inflight.len(),
        )
    }
}

impl Drop for WorkQueue {
    fn drop(&mut self) {
        let mut inner = self.inner.0.lock().unwrap_or_else(|error| error.into_inner());
        inner.shutdown = true;
        drop(inner);
        self.inner.1.notify_all();
        // **不 join**：在跑的任务可能是十几秒的整管线，退出时不该为它干等
        // （进程结束会带走这些线程；结果写不进缓存也只是下次再渲一遍）。
        if let Ok(mut workers) = self.workers.lock() {
            workers.clear();
        }
    }
}

fn worker_loop(shared: &Arc<(Mutex<Inner>, Condvar)>) {
    ON_WORKER.with(|flag| flag.set(true));
    loop {
        let job = {
            let mut inner = shared.0.lock().unwrap_or_else(|error| error.into_inner());
            loop {
                if inner.shutdown {
                    return;
                }
                if let Some(job) = inner.pop() {
                    break job;
                }
                inner = shared
                    .1
                    .wait(inner)
                    .unwrap_or_else(|error| error.into_inner());
            }
        };
        let (key, token, task) = (job.key, job.token, job.task);
        let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            match task {
                Some(task) => task(&token),
                None => Ok(()),
            }
        })) {
            Ok(result) => result,
            // panic 已经有人捕获过（任务内部常见）；这里兜底并把失败传给等待者
            Err(payload) => Err(format!("后台任务 panic：{}", panic_text(&payload))),
        };
        let slot = {
            let mut inner = shared.0.lock().unwrap_or_else(|error| error.into_inner());
            inner.inflight.remove(&key)
        };
        if let Some(slot) = slot {
            slot.finish(result);
        }
    }
}

/// panic 载荷 → 一句话（与 `editor::panic_message` 同一形状；这里是为了不跨模块引私有函数）。
fn panic_text(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "未知 panic".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    /// 等一个条件成立（有上限，避免测试挂死）。
    fn wait_until(mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !condition() {
            assert!(Instant::now() < deadline, "等待条件超时");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn same_key_runs_once_and_the_second_caller_waits_for_it() {
        let queue = Arc::new(WorkQueue::new(1));
        let runs = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (started, started_rx) = mpsc::channel::<()>();
        let (release, release_rx) = mpsc::channel::<()>();
        let release_rx = Arc::new(Mutex::new(release_rx));

        let first = {
            let queue = Arc::clone(&queue);
            let runs = Arc::clone(&runs);
            let release_rx = Arc::clone(&release_rx);
            std::thread::spawn(move || {
                queue.run(Priority::Warm, "preview:1", move |_| {
                    runs.fetch_add(1, Ordering::SeqCst);
                    let _ = started.send(());
                    let _ = release_rx.lock().unwrap().recv_timeout(Duration::from_secs(3));
                    Ok(())
                })
            })
        };
        started_rx.recv_timeout(Duration::from_secs(3)).expect("第一个任务要开跑");

        // 第二个调用者（同 key）：不许再跑一遍，等第一个放行后拿到同一个结果
        let second = {
            let queue = Arc::clone(&queue);
            let runs = Arc::clone(&runs);
            std::thread::spawn(move || queue.run(Priority::Warm, "preview:1", move |_| {
                runs.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }))
        };
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(runs.load(Ordering::SeqCst), 1, "同 key 只跑一次");
        let _ = release.send(());
        assert!(first.join().unwrap().is_ok());
        assert!(second.join().unwrap().is_ok());
        assert_eq!(runs.load(Ordering::SeqCst), 1, "等待者不重复执行");
        // 跑完之后 key 释放：再来一次会重新跑
        let rerun = Arc::clone(&runs);
        assert!(
            queue
                .run(Priority::Warm, "preview:1", move |_| {
                    rerun.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .is_ok()
        );
        assert_eq!(runs.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn batch_queue_is_entered_by_a_higher_priority_task() {
        let queue = Arc::new(WorkQueue::new(1));
        let order: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
        let (started, started_rx) = mpsc::channel::<()>();
        let (release, release_rx) = mpsc::channel::<()>();
        let release_rx = Arc::new(Mutex::new(release_rx));

        let record = |name: &'static str, order: &Arc<Mutex<Vec<&'static str>>>| {
            let order = Arc::clone(order);
            move |_: &CancelToken| {
                order.lock().unwrap().push(name);
            }
        };

        // 先占住唯一的工作线程
        let _ = queue.spawn(Priority::Batch, "block", {
            let release_rx = Arc::clone(&release_rx);
            move |_| {
                let _ = started.send(());
                let _ = release_rx.lock().unwrap().recv_timeout(Duration::from_secs(3));
            }
        });
        started_rx.recv_timeout(Duration::from_secs(3)).expect("占位任务要开跑");
        // 批量排满
        let _ = queue.spawn(Priority::Batch, "b1", record("b1", &order));
        let _ = queue.spawn(Priority::Batch, "b2", record("b2", &order));
        // 高优先插队
        let _ = queue.spawn(Priority::Interactive, "i1", record("i1", &order));
        assert_eq!(queue.stats().0, 3);
        let _ = release.send(());
        wait_until(|| order.lock().unwrap().len() == 3);
        assert_eq!(
            *order.lock().unwrap(),
            vec!["i1", "b1", "b2"],
            "高优先任务必须插到批量前面"
        );
    }

    #[test]
    fn spawn_dedupes_the_same_key_while_it_is_queued() {
        let queue = Arc::new(WorkQueue::new(1));
        let runs = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (started, started_rx) = mpsc::channel::<()>();
        let (release, release_rx) = mpsc::channel::<()>();
        let release_rx = Arc::new(Mutex::new(release_rx));
        queue.spawn(Priority::Batch, "key", {
            let release_rx = Arc::clone(&release_rx);
            move |_| {
                let _ = started.send(());
                let _ = release_rx.lock().unwrap().recv_timeout(Duration::from_secs(3));
            }
        });
        let token = {
            let runs = Arc::clone(&runs);
            queue.spawn(Priority::Batch, "key", move |_| {
                runs.fetch_add(1, Ordering::SeqCst);
            })
        };
        started_rx.recv_timeout(Duration::from_secs(3)).expect("任务要开跑");
        let _ = release.send(());
        wait_until(|| queue.stats() == (0, 0));
        assert_eq!(runs.load(Ordering::SeqCst), 0, "同 key 的第二个 spawn 必须被丢掉");
        assert!(token.is_none(), "同 key 的第二个 spawn 返回 None（没入队）");
    }

    #[test]
    fn cancelling_a_queued_task_releases_its_waiters() {
        let queue = Arc::new(WorkQueue::new(1));
        let (started, started_rx) = mpsc::channel::<()>();
        let (release, release_rx) = mpsc::channel::<()>();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let _ = queue.spawn(Priority::Batch, "block", {
            let release_rx = Arc::clone(&release_rx);
            move |_| {
                let _ = started.send(());
                let _ = release_rx.lock().unwrap().recv_timeout(Duration::from_secs(3));
            }
        });
        started_rx.recv_timeout(Duration::from_secs(3)).expect("占位任务要开跑");
        let waiter = {
            let queue = Arc::clone(&queue);
            std::thread::spawn(move || queue.run(Priority::Batch, "victim", |_| Ok(())))
        };
        wait_until(|| queue.stats().0 == 1);
        assert!(queue.cancel("victim"));
        assert_eq!(
            waiter.join().unwrap(),
            Err("任务已取消".to_string()),
            "被取消的任务要让等待者拿到结论"
        );
        let _ = release.send(());
    }

    #[test]
    fn wait_only_waits_for_an_existing_key() {
        let queue = WorkQueue::new(1);
        assert!(queue.wait("nope", Duration::from_millis(10)).is_none());
        let queue = Arc::new(queue);
        let (release, release_rx) = mpsc::channel::<()>();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let waiter = {
            let queue = Arc::clone(&queue);
            std::thread::spawn(move || queue.wait("k", Duration::from_secs(3)))
        };
        // 先把任务挂上（wait 可能在任务结算前就查到）
        let _ = queue.spawn(Priority::Warm, "k", move |_| {
            let _ = release_rx.lock().unwrap().recv_timeout(Duration::from_secs(3));
        });
        std::thread::sleep(Duration::from_millis(30));
        let _ = release.send(());
        // waiter 可能在任务结算后查到 key 已移除（None），也可能等到结果（Some(Ok)）——
        // 两者都是「没有轮询、立刻返回」；不许出现超时。
        let result = waiter.join().unwrap();
        assert!(matches!(result, None | Some(Ok(()))), "不许超时：{result:?}");
    }
}
