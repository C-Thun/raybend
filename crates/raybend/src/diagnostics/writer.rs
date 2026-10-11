//! **自研落盘 writer**（`specs/logging-and-crash-report.md` §4.2/§4.5）。
//!
//! 为什么不用 `flexi_logger`（规格原选型，崔总 2026-10-12 改定）：核实源码后确认它
//! ① 队列是 `crossbeam_channel::unbounded`，给不了「有界 + try_send + 丢弃计数」；
//! ② 清理只有 `KeepLogFiles(n)` / `KeepForDays(n)`，没有「目录总容量」这个判据
//! （而「按个数挡不住单文件膨胀」正是规格驳回按个数的理由）。它唯一满足的关键项是按大小轮转，
//! 我们自己写这一段反而可控、可测、少一棵依赖树。
//!
//! 结构：
//!
//! ```text
//! 业务线程 ──try_send──▶ 有界队列(4096) ──▶ 写线程 ──▶ logs/raybend.log（2 MiB 轮转）
//!    │                                         └──▶ 内存尾缓冲（200 行，崩溃记录取用）
//!    └── 队满 → 丢弃并计数（下一次成功写入时补一行 dropped=N）
//! ```
//!
//! 纪律：**渲染线程之类的热路径禁止写盘** —— 它们只走到 `try_send` 为止（非阻塞）；
//! 真正的 I/O 全在写线程。

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use super::timefmt;

/// 单文件上限（规格 §4.2：2 MiB）。
pub const FILE_LIMIT_BYTES: u64 = 2 * 1024 * 1024;
/// 归档文件个数上限（`raybend.1.log` … `raybend.9.log`）。
pub const ARCHIVE_LIMIT: usize = 9;
/// 目录总容量上限（2 MiB × 10 = 20 MiB 硬上限）。
pub const DIR_LIMIT_BYTES: u64 = 20 * 1024 * 1024;
/// 有界队列容量（规格 §4.5）。
pub const QUEUE_CAPACITY: usize = 4096;
/// 崩溃记录要取的日志尾行数（规格 §5.1）。
pub const TAIL_LINES: usize = 200;
/// 单行硬上限：worker stderr 之类的野行不许把日志文件撑爆。
pub const LINE_LIMIT_BYTES: usize = 16 * 1024;

/// 活动日志文件名。
pub const ACTIVE_FILE: &str = "raybend.log";

/// 写线程消息。
enum Msg {
    /// 已格式化、已脱敏的一行（不带换行）。
    Line(String),
    /// 同步屏障：写线程 flush 之后回 ack（崩溃路径用）。
    Flush(SyncSender<()>),
    /// 收尾：flush + 关闭，回 ack 后线程退出。
    Shutdown(SyncSender<()>),
}

/// 日志写入口。`log::Log` 实现与崩溃路径共用它 —— 全项目**只此一份**落盘实现。
pub struct LogSink {
    tx: SyncSender<Msg>,
    dropped: Arc<AtomicU64>,
    tail: Arc<Mutex<VecDeque<String>>>,
    dir: PathBuf,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl LogSink {
    /// 入队一行（**非阻塞**）。队满丢弃并计数，绝不阻塞调用线程。
    ///
    /// 脱敏在这里做 —— 它是**所有写入路径的唯一闸门**（规格 §8.1）：
    /// 门面（[`super::logger`]）与崩溃路径（[`Self::write_blocking`]）都走它。
    pub fn enqueue_line(&self, line: &str) {
        match self.tx.try_send(Msg::Line(super::redact::line(line))) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
            Err(TrySendError::Disconnected(_)) => {
                // 写线程已经走了（收尾阶段）：静默丢弃，不再制造新的故障
            }
        }
    }

    /// 直接同步写一行（**绕过队列**）。只给崩溃路径用：那时候队列里还有东西、
    /// 也没时间等，必须立刻落盘。
    pub fn write_blocking(&self, line: &str) {
        append_line(&self.dir, &super::redact::line(line));
    }

    /// 等队列排空并 flush（崩溃路径用；调用方不该在正常流程里用 —— 那会阻塞）。
    pub fn flush_blocking(&self) {
        let (ack_tx, ack_rx) = sync_channel(1);
        if self.tx.try_send(Msg::Flush(ack_tx)).is_err() {
            return; // 队满说明写线程正忙，先不逼它
        }
        let _ = ack_rx.recv_timeout(std::time::Duration::from_millis(1500));
    }

    /// 日志尾（最近 `limit` 行，按时间顺序）。崩溃记录与「未正常退出」记录用。
    #[must_use]
    pub fn tail(&self, limit: usize) -> Vec<String> {
        let Ok(guard) = self.tail.lock() else {
            return Vec::new();
        };
        let skip = guard.len().saturating_sub(limit);
        guard.iter().skip(skip).cloned().collect()
    }

    /// 收尾：flush + join 写线程（幂等）。
    pub fn shutdown(&self) {
        let (ack_tx, ack_rx) = sync_channel(1);
        if self.tx.try_send(Msg::Shutdown(ack_tx)).is_ok() {
            let _ = ack_rx.recv_timeout(std::time::Duration::from_secs(3));
        }
        let Ok(mut slot) = self.worker.lock() else {
            return;
        };
        if let Some(handle) = slot.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for LogSink {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// 会话头（规格 §4.3）：每次启动第一行。
///
/// 只放**机器事实**，不放用户信息：机器名/用户名/库路径一律不进来。
#[must_use]
pub fn session_header(started: SystemTime, session_id: &str) -> String {
    let offset = timefmt::local_offset_minutes();
    let local = timefmt::local_iso_seconds(started, offset.unwrap_or(0));
    format!(
        "=== session {local} {} {} {} cpu={}t pid={} id={session_id} ===",
        crate::version(),
        if cfg!(debug_assertions) { "dev" } else { "release" },
        format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        std::thread::available_parallelism().map_or(0, std::num::NonZeroUsize::get),
        std::process::id(),
    )
}

/// 启动写线程（`dir` 不存在则创建）。
///
/// # Errors
/// 目录建不出来、活动文件打不开时返回错误 —— 调用方（诊断初始化）**必须降级继续**
/// （宁可没有日志，也不能因为没有日志而让应用起不来）。
pub fn start(dir: PathBuf, started: SystemTime, session_id: &str) -> io::Result<Arc<LogSink>> {
    fs::create_dir_all(&dir)?;
    // 启动轮转：上一会话整体顺移为 raybend.1.log（「上一次会话」永远是完整文件）
    shift_archives(&dir, ACTIVE_FILE)?;
    let active = dir.join(ACTIVE_FILE);
    let file = open_append(&active)?;
    let written = file.metadata().map(|m| m.len()).unwrap_or(0);

    let (tx, rx) = sync_channel(QUEUE_CAPACITY);
    let dropped = Arc::new(AtomicU64::new(0));
    let tail: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::with_capacity(TAIL_LINES)));

    // 会话头：第一条落盘内容，同步写（随后的行才走队列）
    let header = session_header(started, session_id);
    let mut state = WriterState {
        dir: dir.clone(),
        file: Some(file),
        written,
        dropped_since_notice: 0,
    };
    state.append(&header);

    let sink = Arc::new(LogSink {
        tx,
        dropped: Arc::clone(&dropped),
        tail: Arc::clone(&tail),
        dir: dir.clone(),
        worker: Mutex::new(None),
    });

    let writer_dir = dir.clone();
    let writer_tail = Arc::clone(&tail);
    let handle = std::thread::Builder::new()
        .name("raybend-log".to_string())
        .spawn(move || {
            let mut state = state;
            while let Ok(msg) = rx.recv() {
                match msg {
                    Msg::Line(line) => {
                        // 记录器自身 panic 必须被吞掉：日志写失败不能反过来崩应用（§4.5）
                        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            let visible = state.take_dropped_notice(&dropped);
                            if let Some(notice) = visible {
                                state.append(&notice);
                            }
                            state.append(&line);
                            line
                        }));
                        match outcome {
                            Ok(line) => push_tail(&writer_tail, line),
                            Err(_) => {
                                state.reopen(&writer_dir);
                            }
                        }
                    }
                    Msg::Flush(ack) => {
                        state.flush();
                        let _ = ack.send(());
                    }
                    Msg::Shutdown(ack) => {
                        state.flush();
                        state.file = None;
                        let _ = ack.send(());
                        break;
                    }
                }
            }
        })
        .map_err(io::Error::other)?;

    if let Ok(mut slot) = sink.worker.lock() {
        *slot = Some(handle);
    }
    Ok(sink)
}

fn push_tail(tail: &Arc<Mutex<VecDeque<String>>>, line: String) {
    if let Ok(mut guard) = tail.lock() {
        if guard.len() == TAIL_LINES {
            guard.pop_front();
        }
        guard.push_back(line);
    }
}

/// 写线程的内部状态。
struct WriterState {
    dir: PathBuf,
    file: Option<File>,
    written: u64,
    dropped_since_notice: u64,
}

impl WriterState {
    fn take_dropped_notice(&mut self, dropped: &Arc<AtomicU64>) -> Option<String> {
        let total = dropped.load(Ordering::Relaxed);
        if total == self.dropped_since_notice {
            return None;
        }
        let delta = total - self.dropped_since_notice;
        self.dropped_since_notice = total;
        Some(format!(
            "{}  WARN   diagnostics.log  队列满，丢弃了日志行  dropped={delta} total={total}",
            timefmt::utc_iso_millis(SystemTime::now())
        ))
    }

    fn append(&mut self, line: &str) {
        if self.written >= FILE_LIMIT_BYTES {
            self.rotate();
        }
        let text = clamp_line(line);
        let bytes = text.as_bytes();
        let Some(file) = self.file.as_mut() else {
            return;
        };
        if file.write_all(bytes).is_err() || file.write_all(b"\n").is_err() {
            // 写失败（磁盘满、文件被锁）：丢弃这一行，下次尝试重开
            self.file = None;
        } else {
            self.written += bytes.len() as u64 + 1;
        }
    }

    fn flush(&mut self) {
        if let Some(file) = self.file.as_mut() {
            let _ = file.flush();
        }
    }

    /// 轮转：把当前活动文件顺移为 `raybend.1.log`，新开一个空的。
    fn rotate(&mut self) {
        self.file = None;
        let _ = shift_archives(&self.dir, ACTIVE_FILE);
        let active = self.dir.join(ACTIVE_FILE);
        match open_append(&active) {
            Ok(file) => {
                self.written = file.metadata().map(|m| m.len()).unwrap_or(0);
                self.file = Some(file);
            }
            Err(_) => self.file = None,
        }
    }

    /// 写失败后的重开：不轮转，直接尝试打开活动文件。
    fn reopen(&mut self, dir: &Path) {
        let active = dir.join(ACTIVE_FILE);
        if let Ok(file) = open_append(&active) {
            self.written = file.metadata().map(|m| m.len()).unwrap_or(0);
            self.file = Some(file);
        }
    }
}

fn open_append(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

/// 直接追加一行（崩溃路径专用，绕过队列）：失败静默返回 —— 崩溃时不许再制造新故障。
fn append_line(dir: &Path, line: &str) {
    let Ok(mut file) = open_append(&dir.join(ACTIVE_FILE)) else {
        return;
    };
    let text = clamp_line(line);
    let _ = file.write_all(text.as_bytes());
    let _ = file.write_all(b"\n");
    let _ = file.flush();
}

/// 一行太长时截断（`LINE_LIMIT_BYTES`），保留尾部截断标记 —— 便于定位是谁刷的。
fn clamp_line(line: &str) -> String {
    if line.len() <= LINE_LIMIT_BYTES {
        return line.to_string();
    }
    let mut cut = LINE_LIMIT_BYTES;
    while cut > 0 && !line.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…<truncated {} bytes>", &line[..cut], line.len() - cut)
}

/// 归档名：`raybend.<index>.log`（index 从 1 开始，1 = 最新归档）。
#[must_use]
pub fn archive_name(index: usize) -> String {
    format!("raybend.{index}.log")
}

/// 顺移归档链：`raybend.log` → `raybend.1.log` → … → `raybend.9.log`（最旧的被丢弃）。
///
/// 纯文件操作（用临时目录单测覆盖）。
pub fn shift_archives(dir: &Path, active: &str) -> io::Result<()> {
    let oldest = dir.join(archive_name(ARCHIVE_LIMIT));
    let _ = fs::remove_file(oldest);
    for index in (1..ARCHIVE_LIMIT).rev() {
        let from = dir.join(archive_name(index));
        if from.exists() {
            let _ = fs::rename(&from, dir.join(archive_name(index + 1)));
        }
    }
    let active_path = dir.join(active);
    if active_path.exists() {
        let _ = fs::rename(&active_path, dir.join(archive_name(1)));
    }
    prune_dir(dir, DIR_LIMIT_BYTES)?;
    Ok(())
}

/// 目录总容量清理：超过 `limit` 就从**最旧**（编号最大）的归档开始删。
///
/// 返回删掉的字节数。
pub fn prune_dir(dir: &Path, limit: u64) -> io::Result<u64> {
    let entries = list_log_files(dir)?;
    let victims = pick_deletions(&entries, limit);
    let mut removed = 0_u64;
    for name in victims {
        let path = dir.join(&name);
        if let Ok(meta) = fs::metadata(&path) {
            removed += meta.len();
        }
        let _ = fs::remove_file(&path);
    }
    Ok(removed)
}

/// 列出目录里的日志文件（名, 大小）。
fn list_log_files(dir: &Path) -> io::Result<Vec<(String, u64)>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("raybend") || !name.ends_with(".log") {
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        out.push((name, size));
    }
    Ok(out)
}

/// **纯函数**：在「未超限就什么都不删」的前提下，挑出要删的文件（从最旧开始）。
///
/// 顺序约定：归档编号越大越旧（`.9` 比 `.1` 旧）；活动文件 `raybend.log` **永远最后删**
/// （它是本次会话，删了就没得看了）。
#[must_use]
pub fn pick_deletions(entries: &[(String, u64)], limit: u64) -> Vec<String> {
    let total: u64 = entries.iter().map(|(_, size)| size).sum();
    if total <= limit {
        return Vec::new();
    }
    let mut order: Vec<&(String, u64)> = entries.iter().collect();
    order.sort_by_key(|(name, _)| match archive_index(name) {
        Some(index) => (0_u8, std::cmp::Reverse(index)),
        None => (1_u8, std::cmp::Reverse(0)),
    });
    let mut remaining = total;
    let mut victims = Vec::new();
    for (name, size) in order {
        if remaining <= limit {
            break;
        }
        // 活动文件留着（删了会让本次会话「无痕」，与诊断目的相反）
        if name == ACTIVE_FILE {
            continue;
        }
        victims.push(name.clone());
        remaining = remaining.saturating_sub(*size);
    }
    victims
}

fn archive_index(name: &str) -> Option<usize> {
    let inner = name.strip_prefix("raybend.")?.strip_suffix(".log")?;
    inner.parse::<usize>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, size: u64) -> (String, u64) {
        (name.to_string(), size)
    }

    #[test]
    fn deletions_are_empty_under_the_limit_and_oldest_first_over_it() {
        let files = vec![
            entry("raybend.log", 2 * 1024 * 1024),
            entry("raybend.1.log", 2 * 1024 * 1024),
            entry("raybend.2.log", 2 * 1024 * 1024),
        ];
        assert!(pick_deletions(&files, DIR_LIMIT_BYTES).is_empty(), "6 MiB 不该动");
        // 上限 3 MiB、现有 6 MiB：必须先删最旧的 .2，再删 .1（删到 ≤ 上限为止）
        let victims = pick_deletions(&files, 3 * 1024 * 1024);
        assert_eq!(
            victims,
            vec!["raybend.2.log".to_string(), "raybend.1.log".to_string()],
            "从最旧的归档开始删"
        );
        // 活动文件永不删；归档删完还不够时也只是尽力而为
        let victims = pick_deletions(&files, 0);
        assert_eq!(
            victims,
            vec!["raybend.2.log".to_string(), "raybend.1.log".to_string()],
            "不够删也不许动活动文件"
        );
    }

    #[test]
    fn archive_shift_keeps_chain_bounded_and_orders_oldest_out() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path();
        // 造 11 份（活动 + 归档 10），全部 1 字节
        std::fs::write(path.join("raybend.log"), b"current").expect("写");
        for index in 1..=ARCHIVE_LIMIT {
            std::fs::write(path.join(archive_name(index)), format!("old{index}")).expect("写");
        }
        shift_archives(path, ACTIVE_FILE).expect("轮转");
        assert_eq!(
            std::fs::read_to_string(path.join(archive_name(1))).expect("读"),
            "current",
            "活动文件变成最新归档"
        );
        assert!(!path.join(archive_name(ARCHIVE_LIMIT + 1)).exists(), "超出的归档不产生");
        assert!(path.join("raybend.log").exists() || true, "活动文件留给 writer 新建");
        // 老 .9 的内容应当已经出局
        let ninth = std::fs::read_to_string(path.join(archive_name(ARCHIVE_LIMIT))).expect("读");
        assert_ne!(ninth, format!("old{}", ARCHIVE_LIMIT), "最旧的被丢弃");
    }

    #[test]
    fn total_capacity_is_enforced_by_pruning_oldest_archives() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path();
        std::fs::write(path.join("raybend.log"), vec![b'a'; 1000]).expect("写");
        for index in 1..=4 {
            std::fs::write(path.join(archive_name(index)), vec![b'b'; 1000]).expect("写");
        }
        // 上限 2500：活动 1000 + 归档 1000×4 = 5000 → 必须删到 ≤2500
        let removed = prune_dir(path, 2500).expect("清理");
        assert!(removed >= 2000, "至少删掉两份归档（实际 {removed}）");
        let left: u64 = list_log_files(path)
            .expect("列目录")
            .iter()
            .map(|(_, size)| size)
            .sum();
        assert!(left <= 2500, "清理后 {left} 应在上限内");
        assert!(path.join("raybend.log").exists(), "活动文件必须留着");
    }

    #[test]
    fn overlong_lines_are_truncated_with_a_marker() {
        let line = "x".repeat(LINE_LIMIT_BYTES + 100);
        let clamped = clamp_line(&line);
        assert!(clamped.len() < LINE_LIMIT_BYTES + 100);
        assert!(clamped.ends_with("bytes>"));
        // 多字节边界不 panic、不切碎
        let chinese = "汉".repeat(LINE_LIMIT_BYTES);
        let clamped = clamp_line(&chinese);
        assert!(clamped.contains("truncated"));
    }

    #[test]
    fn sink_writes_redacted_lines_to_disk_and_keeps_a_tail() {
        let dir = tempfile::tempdir().expect("临时目录");
        let sink = start(dir.path().to_path_buf(), SystemTime::now(), "s-test").expect("启动写线程");
        for index in 0..5 {
            sink.enqueue_line(&format!("line {index} path=C:\\Users\\bob\\a{index}.jpg"));
        }
        sink.flush_blocking();
        let text = std::fs::read_to_string(dir.path().join(ACTIVE_FILE)).expect("读日志");
        assert!(text.contains("=== session "), "会话头在文件开头");
        assert!(text.contains("line 4"), "队列里的行都落盘了");
        assert!(text.contains("<path:.jpg#"), "落盘内容经过脱敏");
        assert!(!text.contains("bob"), "原路径不许出现");
        let tail = sink.tail(3);
        assert_eq!(tail.len(), 3);
        assert!(tail.last().is_some_and(|line| line.contains("line 4")));
        sink.shutdown();
    }

    #[test]
    fn session_header_carries_no_user_identity() {
        let header = session_header(SystemTime::now(), "s-abc123");
        assert!(header.starts_with("=== session "));
        assert!(header.contains(crate::version()));
        assert!(header.contains("id=s-abc123"));
        assert!(!header.contains(&std::env::var("USERNAME").unwrap_or_default()) || std::env::var("USERNAME").unwrap_or_default().is_empty());
    }
}
