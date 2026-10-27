//! 图像渲染的**同 key 单飞**（2026-10-27）。
//!
//! 缩略图 / 1920 占位 / 真身可能同时被多个入口请求：网格、看图两段加载、
//! 对比预载、退出编辑时的预览刷新。缓存只能合并「已经完成」的请求，
//! 挡不住「缓存还没写好，第二张又进来」的并发重复渲染。
//!
//! 这里只负责**执行中的那一次**：首个调用者成为 leader，后续同 key 调用者
//! 等它完成后拿同一个结果。它不是缓存：leader 结算后 key 立即释放，
//! 下一次请求仍应先查真正的缓存（磁盘或进程内存）。
//!
//! # key 的纪律
//!
//! **不能只按路径**：同一路径在编辑栈变化后是另一份输出。调用方必须把
//! 文件身份、尺寸档、编辑栈指纹、源文件签名与管线版本纳入 key（与各自缓存键同源）。
//!
//! # 失败与 panic
//!
//! leader 返回错误时，等待者收到同样的错误文本，不会永远等；leader panic
//! 时 [`FlightLeader`] 的 `Drop` 也会结算并唤醒等待者。这个组件不可重入：
//! 同一线程在 leader 的任务里再次等待同一个 key 会自锁。
//!
//! # 等待有上限（2026-10-27 评审补）
//!
//! 合并之前，每个请求各自等自己那一份：卡住也只卡它一个。合并之后等待者把
//! 自己挂在 leader 上 —— leader 万一永久卡死（RAW worker 无响应、阻塞文件系统），
//! 它们会一直占着 blocking 线程与库任务许可。所以 [`FlightWait::wait`] 有上限：
//! 超时**只放弃等待**，绝不在超时后自己重渲（那会把重复渲染重新引回来）。

use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// 等待同一份渲染的上限。
///
/// 取值要**长于**已知的合法慢路径：RAW 解码自身超时是 45s，最重的整帧管线
/// （高质量降噪）十几秒。只为防「永久挂住」，不作为性能阈值。
pub const DEFAULT_WAIT: Duration = Duration::from_secs(120);

/// 单飞 key 里的路径：**显示形式**进 Debug，继续走日志脱敏。
/// 不用 `PathBuf` 自带的 Debug（带引号与转义），免得路径脱敏器认不出。
pub struct LogPath<'a>(pub &'a Path);

impl Debug for LogPath<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

/// 等待者达到这个数时记 WARN：正常预览/看图偶尔共享一次不算异常，
/// 一串相同请求持续压进来就必须能从日志反查调用源。
const DUPLICATE_WARN_AT: usize = 3;

struct Flight<T> {
    state: Mutex<FlightState<T>>,
    done: Condvar,
}

struct FlightState<T> {
    result: Option<Result<Arc<T>, String>>,
    /// 当前已经合并进来的等待者数（含已经结束等待的），只用于诊断。
    waiters: usize,
}

impl<T> Default for FlightState<T> {
    fn default() -> Self {
        Self {
            result: None,
            waiters: 0,
        }
    }
}

/// 一份进程内单飞表。`K` 是完整请求身份，`T` 是等待者要共享的结果。
pub struct FlightGate<K, T> {
    flights: Arc<Mutex<HashMap<K, Arc<Flight<T>>>>>,
}

impl<K, T> Default for FlightGate<K, T> {
    fn default() -> Self {
        Self {
            flights: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl<K, T> Clone for FlightGate<K, T> {
    fn clone(&self) -> Self {
        Self {
            flights: Arc::clone(&self.flights),
        }
    }
}

/// `begin` 的结果：要么由你执行，要么等已经执行中的那一份。
pub enum FlightClaim<K, T>
where
    K: Eq + Hash + Clone + Debug,
{
    Leader(FlightLeader<K, T>),
    Wait(FlightWait<K, T>),
}

/// 拿到**自有**的值：没有其它持者时零拷贝，否则复制一次。
///
/// 合并之后结果在 `Arc` 里共享，但对外接口（IPC 回包、写缓存）要的是 `Vec<u8>`。
/// 最后一个持者可以直接把内部缓冲区拿走，避免“共享了还要人人复制一份”。
pub fn into_owned<T: Clone>(value: Arc<T>) -> T {
    Arc::try_unwrap(value).unwrap_or_else(|shared| (*shared).clone())
}

impl<K, T> FlightGate<K, T>
where
    K: Eq + Hash + Clone + Debug,
{
    /// 认领一个 key。重复调用不会创建第二个 leader。
    pub fn begin(&self, key: K) -> FlightClaim<K, T> {
        let mut flights = self
            .flights
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(flight) = flights.get(&key) {
            let flight = Arc::clone(flight);
            drop(flights);
            let ordinal = {
                let mut state = flight
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.waiters += 1;
                state.waiters
            };
            log::debug!(
                "[image] 相同渲染请求已在执行，合并第 {ordinal} 个等待者：{key:?}"
            );
            if ordinal == DUPLICATE_WARN_AT {
                log::warn!(
                    "[image] 相同渲染请求连续压入 {} 次，已合并；请检查上游重复触发：{key:?}",
                    ordinal + 1
                );
            }
            return FlightClaim::Wait(FlightWait {
                flight,
                ordinal,
                key,
            });
        }
        let flight = Arc::new(Flight {
            state: Mutex::new(FlightState::default()),
            done: Condvar::new(),
        });
        flights.insert(key.clone(), Arc::clone(&flight));
        FlightClaim::Leader(FlightLeader {
            gate: self.clone(),
            key,
            flight,
            settled: false,
        })
    }
}

/// 首个调用者的执行权。必须用 [`FlightLeader::complete`] 按任务结果结算。
pub struct FlightLeader<K, T>
where
    K: Eq + Hash + Clone + Debug,
{
    gate: FlightGate<K, T>,
    key: K,
    flight: Arc<Flight<T>>,
    settled: bool,
}

impl<K, T> FlightLeader<K, T>
where
    K: Eq + Hash + Clone + Debug,
{
    /// 发布**成功**结果并拿回共享句柄（leader 自己也用它，不再多复制一份）。
    pub fn publish(mut self, value: T) -> Arc<T> {
        let shared = Arc::new(value);
        self.settle(Ok(Arc::clone(&shared)));
        shared
    }

    /// 发布**失败**：等待者拿到错误文本，leader 自己保留原错误类型。
    pub fn publish_error<E: Display>(mut self, error: &E) {
        self.settle(Err(error.to_string()));
    }

    fn settle(&mut self, result: Result<Arc<T>, String>) {
        if self.settled {
            return;
        }
        self.settled = true;
        {
            let mut state = self
                .flight
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.result = Some(result);
        }
        self.flight.done.notify_all();
        let mut flights = self
            .gate
            .flights
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // 只移除自己那一份（防御性判断；同一个 key 此时不可能已经有新 leader）。
        if flights
            .get(&self.key)
            .is_some_and(|current| Arc::ptr_eq(current, &self.flight))
        {
            flights.remove(&self.key);
        }
    }
}

impl<K, T> Drop for FlightLeader<K, T>
where
    K: Eq + Hash + Clone + Debug,
{
    fn drop(&mut self) {
        // 正常路径已经 complete；提前 return / panic 走这里，保证等待者不被吊住。
        self.settle(Err("图像渲染任务未完成（调用方提前退出或 panic）".to_string()));
    }
}

/// 后来同 key 的调用者。
pub struct FlightWait<K, T>
where
    K: Eq + Hash + Clone + Debug,
{
    flight: Arc<Flight<T>>,
    ordinal: usize,
    /// 只用于超时日志（调用方那边也用它反查是哪一份）。
    key: K,
}

impl<K, T> FlightWait<K, T>
where
    K: Eq + Hash + Clone + Debug,
{
    /// 这是第几个等待者（1 = 第一次重复；日志与测试共用这个事实）。
    #[must_use]
    pub const fn ordinal(&self) -> usize {
        self.ordinal
    }

    /// 等 leader 结算并拿共享结果（上限 [`DEFAULT_WAIT`]）。
    ///
    /// 等待者拿到的错误是字符串，因为 leader 的原始错误类型可能不可克隆；
    /// leader 自己的错误类型不变。
    pub fn wait(self) -> Result<Arc<T>, String> {
        self.wait_timeout(DEFAULT_WAIT)
    }

    /// 同 [`Self::wait`]，但可指定上限。
    ///
    /// 超时**只放弃等待**：leader 及其它等待者不受影响，后续请求仍先查真缓存。
    /// 绝不在这里自己重渲 —— 那会把「同一份渲染做两次」重新引回来。
    pub fn wait_timeout(self, limit: Duration) -> Result<Arc<T>, String> {
        // 上限只用来防「永久等」；夹到一天以内，也避开 `Instant + Duration` 的溢出 panic 面。
        let limit = limit.min(Duration::from_secs(24 * 60 * 60));
        let deadline = Instant::now()
            .checked_add(limit)
            .unwrap_or_else(Instant::now);
        let mut state = self
            .flight
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if let Some(result) = state.result.as_ref() {
                return result.clone();
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                log::warn!(
                    "[image] 等待同一份渲染超过 {}ms，放弃等待（不重复渲染）：{:?}",
                    limit.as_millis(),
                    self.key
                );
                return Err(format!("等待同一份图像渲染超时（{}ms）", limit.as_millis()));
            }
            let (next, _) = self
                .flight
                .done
                .wait_timeout(state, left)
                .unwrap_or_else(|error| error.into_inner());
            state = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn same_key_has_one_leader_and_waiters_share_the_result() {
        let gate = Arc::new(FlightGate::<String, Vec<u8>>::default());
        let runs = Arc::new(AtomicUsize::new(0));
        let (started_tx, started_rx) = mpsc::channel();
        let (waiting_tx, waiting_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release_rx = Arc::new(Mutex::new(release_rx));

        let leader = {
            let gate = Arc::clone(&gate);
            let runs = Arc::clone(&runs);
            let release_rx = Arc::clone(&release_rx);
            thread::spawn(move || {
                let FlightClaim::Leader(leader) = gate.begin("raw:1:full".into()) else {
                    panic!("第一个请求必须是 leader");
                };
                runs.fetch_add(1, Ordering::SeqCst);
                started_tx.send(()).unwrap();
                release_rx
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(3))
                    .unwrap();
                let shared = leader.publish(vec![1, 2, 3]);
                super::into_owned(shared)
            })
        };
        started_rx.recv_timeout(Duration::from_secs(3)).unwrap();

        let waiter = {
            let gate = Arc::clone(&gate);
            let runs = Arc::clone(&runs);
            thread::spawn(move || {
                let FlightClaim::Wait(wait) = gate.begin("raw:1:full".into()) else {
                    panic!("并发同 key 必须等待");
                };
                assert_eq!(wait.ordinal(), 1);
                // 先确认已经以 waiter 身份认领，再让主线程放行 leader；
                // 否则全量测试的线程调度可能让这里变成第二个 leader（假失败）。
                waiting_tx.send(()).unwrap();
                let value = wait.wait().unwrap();
                // 等待者不再执行任务体。
                runs.fetch_add(0, Ordering::SeqCst);
                (*value).clone()
            })
        };
        waiting_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        release_tx.send(()).unwrap();
        assert_eq!(leader.join().unwrap(), vec![1, 2, 3]);
        assert_eq!(waiter.join().unwrap(), vec![1, 2, 3]);
        assert_eq!(runs.load(Ordering::SeqCst), 1);

        // 单飞不是永久缓存：结算后允许新 leader（调用方先查真缓存）。
        assert!(matches!(
            gate.begin("raw:1:full".into()),
            FlightClaim::Leader(_)
        ));
    }

    #[test]
    fn different_keys_do_not_block_each_other() {
        let gate = FlightGate::<String, usize>::default();
        let first = gate.begin("a".into());
        let second = gate.begin("b".into());
        assert!(matches!(first, FlightClaim::Leader(_)));
        assert!(matches!(second, FlightClaim::Leader(_)));
    }

    #[test]
    fn failure_wakes_waiters_and_releases_the_key() {
        let gate = Arc::new(FlightGate::<String, usize>::default());
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let leader = {
            let gate = Arc::clone(&gate);
            thread::spawn(move || {
                let FlightClaim::Leader(leader) = gate.begin("bad".into()) else {
                    panic!("leader");
                };
                started_tx.send(()).unwrap();
                // 等主线程确实以 waiter 身份认领后再失败，消除调度竞态。
                release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
                leader.publish_error(&"decode failed");
                Err::<usize, String>("decode failed".into())
            })
        };
        started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let wait = match gate.begin("bad".into()) {
            FlightClaim::Wait(wait) => wait,
            FlightClaim::Leader(_) => panic!("应等待失败中的 leader"),
        };
        let error = {
            release_tx.send(()).unwrap();
            wait.wait().unwrap_err()
        };
        assert!(error.contains("decode failed"));
        assert_eq!(leader.join().unwrap().unwrap_err(), "decode failed");
        assert!(matches!(gate.begin("bad".into()), FlightClaim::Leader(_)));
    }

    #[test]
    fn leader_drop_does_not_leave_waiters_stuck() {
        let gate = FlightGate::<String, usize>::default();
        let wait = {
            let leader = match gate.begin("panic".into()) {
                FlightClaim::Leader(leader) => leader,
                FlightClaim::Wait(_) => panic!("leader"),
            };
            let wait = match gate.begin("panic".into()) {
                FlightClaim::Wait(wait) => wait,
                FlightClaim::Leader(_) => panic!("waiter"),
            };
            drop(leader); // 模拟没有 complete 的提前退出 / unwind
            wait
        };
        assert!(wait.wait().unwrap_err().contains("未完成"));
        assert!(matches!(gate.begin("panic".into()), FlightClaim::Leader(_)));
    }

    #[test]
    fn waiter_gives_up_after_the_limit_and_leaves_the_leader_alone() {
        let gate = FlightGate::<String, usize>::default();
        let leader = match gate.begin("slow".into()) {
            FlightClaim::Leader(leader) => leader,
            FlightClaim::Wait(_) => panic!("leader"),
        };
        let wait = match gate.begin("slow".into()) {
            FlightClaim::Wait(wait) => wait,
            FlightClaim::Leader(_) => panic!("waiter"),
        };
        let started = Instant::now();
        let error = wait
            .wait_timeout(Duration::from_millis(60))
            .expect_err("超时应当报错而不是无限等");
        let waited = started.elapsed();
        assert!(error.contains("超时"), "{error}");
        assert!(waited >= Duration::from_millis(50), "不能提前返回：{waited:?}");
        assert!(waited < Duration::from_secs(5), "必须真的在限额附近返回：{waited:?}");

        // 超时只放弃等待：leader 仍可以结算，后面的请求仍会命中新结果。
        let shared = leader.publish(7);
        assert_eq!(*shared, 7);
        assert!(matches!(gate.begin("slow".into()), FlightClaim::Leader(_)));
        assert_eq!(super::into_owned(shared), 7);
    }
}
