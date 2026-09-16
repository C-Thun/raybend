//! RAW worker 的进程级测试（**集成测试**，会真的 spawn 子进程）。
//!
//! 为什么放在这里而不是 `src/raw/worker.rs` 的单测里：worker 可执行文件是**另一个
//! target**，`cargo test --lib` 不保证它已经被构建；而集成测试由 cargo 保证
//! `CARGO_BIN_EXE_<name>` 对应的 bin 一定先构建好。所以凡是「真的要起进程」的
//! 用例都住在这里。
//!
//! 这些用例特意**不碰真实 RAW 文件**（那需要有样本，放在 `examples/raw-smoke.rs`）——
//! 它们验证的是进程与协议本身：能起来、能复用、出错不崩、错误分类正确。

use std::time::Duration;

use raybend::raw::backend::DecodeRequest;
use raybend::raw::worker::{RawWorker, WorkerError};

/// cargo 保证这个 bin 已构建，并把它放进环境变量里。
const WORKER_BIN: &str = env!("CARGO_BIN_EXE_raybend-raw-worker");

fn worker() -> RawWorker {
    // 给足时间：CI/沙箱上第一次 spawn 可能慢
    RawWorker::with_bin(WORKER_BIN).with_timeout_of(Duration::from_secs(30))
}

#[test]
fn ping_proves_process_and_protocol() {
    let mut w = worker();
    w.ping().expect("worker 应当能回应 ping");
    assert!(w.is_running(), "ping 之后子进程应当常驻");
    w.ping().expect("同一个子进程应当可复用");
}

#[test]
fn a_non_raw_file_is_a_decode_error_and_the_process_survives() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.txt");
    std::fs::write(&path, b"definitely not a photograph").unwrap();

    let mut w = worker();
    let err = w
        .decode(&DecodeRequest::thumb(&path, 128))
        .expect_err("文本文件不该解出图");
    assert!(matches!(err, WorkerError::Decode(_)), "{err:?}");
    assert!(!err.needs_respawn(), "数据问题不该重建进程");

    w.ping().expect("解码失败之后进程应当还活着");
}

#[test]
fn a_missing_file_is_a_clean_error() {
    let mut w = worker();
    let err = w
        .decode(&DecodeRequest::thumb("/definitely/not/here.RW2", 128))
        .expect_err("不存在的文件必须报错");
    assert!(matches!(err, WorkerError::Decode(_)), "{err:?}");
}

#[test]
fn a_fake_raw_does_not_take_the_worker_down_permanently() {
    // TIFF 头 + 垃圾：预检能过，解码器必然失败
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fake.RW2");
    let mut bytes = b"II\x2a\x00".to_vec();
    bytes.resize(8192, 0);
    std::fs::write(&path, &bytes).unwrap();

    let mut w = worker();
    let err = w
        .decode(&DecodeRequest::thumb(&path, 128))
        .expect_err("假 RAW 不该解出图");
    // 可能是解码失败（进程健康），也可能是解码器直接崩了（进程死掉）——
    // **两种都必须是我们能处理的结果**，不能挂住、不能 panic 到主进程
    match err {
        WorkerError::Decode(_) => {
            w.ping().expect("解码失败时进程应当还活着");
        }
        WorkerError::Crashed(_) | WorkerError::Timeout(_) => {
            // 崩了就重建：下一次请求必须能重新起一个进程
            w.ping().expect("崩溃之后应当能自动重建进程");
            assert!(w.is_running());
        }
        other => panic!("不该出现这种失败：{other:?}"),
    }
}

#[test]
fn shutdown_makes_the_next_request_start_a_new_process() {
    let mut w = worker();
    w.ping().unwrap();
    assert!(w.is_running());
    w.shutdown();
    assert!(!w.is_running());
    w.ping().expect("关掉之后应当能重新起来");
}

/// 回归：阻塞读期间**不能持锁**，否则看门狗拿不到锁、杀不掉卡住的子进程。
///
/// 这条曾经真实存在：`sleep 60` 把父进程卡了真的 60 秒（超时机制形同虚设）。
/// 所以除了看结果，还**卡时间**：必须大幅早于子进程自己要睡的时长。
#[test]
fn a_stuck_worker_is_killed_by_the_watchdog() {
    let mut w = RawWorker::with_bin(WORKER_BIN).with_timeout_of(Duration::from_secs(2));
    let started = std::time::Instant::now();
    let err = w
        .sleep_for_test(60)
        .expect_err("卡住的请求不该成功返回");
    let elapsed = started.elapsed();

    assert!(matches!(err, WorkerError::Timeout(_)), "{err:?}");
    assert!(
        elapsed < Duration::from_secs(15),
        "看门狗没起作用：卡了 {elapsed:?}（子进程自己要睡 60 秒，超时是 2 秒）"
    );

    // 杀干净之后必须能重新起来
    w.ping().expect("超时之后应当能重建进程");
}
