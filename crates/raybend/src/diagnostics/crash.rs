//! 第二层：异常采集（`specs/logging-and-crash-report.md` §5.1 / §5.7）。
//!
//! 本文件负责两类**进程内**能看到的异常：
//!
//! * **panic**（§5.1）：`std::panic::set_hook` —— 发布版 `panic = "abort"` 下 hook 仍会先跑，
//!   所以这里**同步写**（绕过队列）是可行的，写完再让进程继续 abort；
//! * **未正常退出**（§5.7）：启动写 `session.lock`、正常退出删除；下次启动发现残留
//!   就补一条记录 —— 硬崩溃（段错误/驱动崩）连 panic hook 都不跑，这条是最后的兜底。
//!
//! 落盘结构（W3 的报告包会原样打包这一份）：
//!
//! ```text
//! crashes/RB-20261012-3F9A21/
//! ├── report.json   机器读（kind / panic / backtrace / logTail）
//! └── report.txt    人读一页（同样内容，但排版过）
//! ```
//!
//! W4 的独立助手进程会把 `dump.dmp` 落进**同一个**目录（它通过命令行拿到目录路径）。

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use serde::Serialize;

use super::redact;
use super::timefmt;
use super::writer::{LogSink, TAIL_LINES};

/// `session.lock` 文件名（放在 crashes/ 下：它只服务异常判定的。
pub const SESSION_LOCK: &str = "session.lock";
/// 未正常退出时，从上一会话日志里取的行数。
pub const PREVIOUS_TAIL_LINES: usize = TAIL_LINES;

/// 崩溃记录器：panic hook 与启动兜底共用它。
pub struct CrashRecorder {
    crashes_dir: PathBuf,
    sink: Option<Arc<LogSink>>,
    session_id: String,
    /// 同一次会话里重复 panic 去重（指纹 → 出现次数）。
    seen: Mutex<HashMap<String, u32>>,
    /// 崩溃时的 dump 请求者（W4 的助手进程接入后才有；没接就是 `None`，静默跳过）。
    dump_hook: Mutex<Option<Arc<dyn Fn(Option<String>) + Send + Sync>>>,
}

impl CrashRecorder {
    #[must_use]
    pub fn new(crashes_dir: PathBuf, sink: Option<Arc<LogSink>>, session_id: String) -> Self {
        Self {
            crashes_dir,
            sink,
            session_id,
            seen: Mutex::new(HashMap::new()),
            dump_hook: Mutex::new(None),
        }
    }

    /// 接入 dump 请求者（W4）：传入的是「崩溃目录路径」（`Some`）时的回调，
    /// 由 `src-tauri` 的 crashpad 模块装进来 —— 核心库不知道助手进程的存在。
    pub fn set_dump_hook(&self, hook: Arc<dyn Fn(Option<String>) + Send + Sync>) {
        if let Ok(mut slot) = self.dump_hook.lock() {
            *slot = Some(hook);
        }
    }

    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    #[must_use]
    pub fn crashes_dir(&self) -> &Path {
        &self.crashes_dir
    }

    /// 写一条**panic** 记录（hook 内调用，必须同步、不许 panic）。
    pub fn record_panic(&self, message: &str, location: Option<&str>, thread: Option<&str>) {
        // 防御性再脱敏一次（幂等）：**记录器是 sink 的另一道闸门**，不信调用方。
        let message = redact::line(message);
        let location = location.map(redact::line);
        let fingerprint = format!("{}|{}", location.as_deref().unwrap_or("?"), message);
        let count = {
            let Ok(mut guard) = self.seen.lock() else {
                return;
            };
            let entry = guard.entry(fingerprint.clone()).or_insert(0);
            *entry += 1;
            *entry
        };
        let dir = self.report_dir_for(&fingerprint);
        let report = CrashReport {
            schema: 1,
            kind: "panic".to_string(),
            report_id: dir_name(dir.file_name()),
            session: self.session_id.clone(),
            created_at: timefmt::utc_iso_millis(SystemTime::now()),
            app: AppInfo::current(),
            panic: Some(PanicInfo {
                message: message.clone(),
                location: location.clone(),
                thread: thread.map(str::to_string),
                count,
            }),
            backtrace: capture_backtrace(),
            log_tail: self.log_tail(),
        };
        write_report(&dir, &report);
        request_dump(&self.dump_hook, &dir);
    }

    /// 写一条**未正常退出**记录（下次启动时调用；上一会话的日志已经轮转成 `raybend.1.log`）。
    pub fn record_unclean_exit(&self, previous: &PreviousSession, log_tail: Vec<String>) {
        let dir = self.fresh_dir();
        let report = CrashReport {
            schema: 1,
            kind: "unclean-exit".to_string(),
            report_id: dir_name(dir.file_name()),
            session: previous.session_id.clone(),
            created_at: timefmt::utc_iso_millis(SystemTime::now()),
            app: AppInfo::current(),
            panic: None,
            backtrace: Vec::new(),
            log_tail,
        };
        write_report(&dir, &report);
    }

    fn log_tail(&self) -> Vec<String> {
        self.sink
            .as_ref()
            .map_or_else(Vec::new, |sink| sink.tail(TAIL_LINES))
    }

    /// 同一指纹复用同一个目录（去重）：目录名由指纹派生，稳定且不可逆。
    fn report_dir_for(&self, fingerprint: &str) -> PathBuf {
        let suffix = short_hash(fingerprint, &self.session_id);
        let name = format!("RB-{}-{suffix}", yymmdd(SystemTime::now()));
        self.crashes_dir.join(name)
    }

    fn fresh_dir(&self) -> PathBuf {
        let mut bytes = [0_u8; 3];
        if getrandom::fill(&mut bytes).is_err() {
            bytes = [0, 0, 1];
        }
        let suffix = format!("{:02X}{:02X}{:02X}", bytes[0], bytes[1], bytes[2]);
        self.crashes_dir.join(format!("RB-{}-{suffix}", yymmdd(SystemTime::now())))
    }
}

/// 上一会话的信息（`session.lock` 的内容）。
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct PreviousSession {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub pid: u32,
    #[serde(rename = "startedAt")]
    pub started_at: String,
}

/// 崩溃记录（JSON 结构，W3 会把它嵌进报告包；这里已经按「机器可读」组织）。
#[derive(Debug, Serialize, serde::Deserialize)]
pub struct CrashReport {
    pub schema: u32,
    pub kind: String,
    #[serde(rename = "reportId")]
    pub report_id: String,
    pub session: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    pub app: AppInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub panic: Option<PanicInfo>,
    pub backtrace: Vec<String>,
    #[serde(rename = "logTail")]
    pub log_tail: Vec<String>,
}

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct AppInfo {
    pub version: String,
    pub profile: String,
    pub os: String,
    pub arch: String,
}

impl AppInfo {
    #[must_use]
    pub fn current() -> Self {
        Self {
            version: crate::version().to_string(),
            profile: if cfg!(debug_assertions) { "dev" } else { "release" }.to_string(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        }
    }
}

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct PanicInfo {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread: Option<String>,
    pub count: u32,
}

/// 装进程级 panic hook（重复调用只装一次）。
pub fn install_panic_hook(recorder: Arc<CrashRecorder>) {
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // 重入保护：记录器自己不许再触发记录（§4.5 的递归禁令）
        thread_local! {
            static IN_HOOK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        }
        let already = IN_HOOK.with(|flag| flag.replace(true));
        if already {
            return;
        }
        let message = panic_message(info);
        let location = info.location().map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()));
        let thread = std::thread::current().name().map(str::to_string);
        recorder.record_panic(&message, location.as_deref(), thread.as_deref());
        if let Some(sink) = recorder.sink.as_ref() {
            // 日志里也留一行（同步写：abort 之前必须落盘）
            let line = format!(
                "{}  ERROR crash  panic  message={} location={}",
                timefmt::utc_iso_millis(SystemTime::now()),
                redact::line(&message),
                redact::line(location.as_deref().unwrap_or("?")),
            );
            sink.write_blocking(&line);
            sink.flush_blocking();
        }
        // 交还给上一个 hook（默认 hook 会往 stderr 打印 —— dev 下要看得见）
        previous(info);
    }));
}

fn panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    if let Some(text) = payload.downcast_ref::<&str>() {
        return redact::line(text);
    }
    if let Some(text) = payload.downcast_ref::<String>() {
        return redact::line(text);
    }
    "panic（payload 不是字符串）".to_string()
}

fn capture_backtrace() -> Vec<String> {
    // 发布版 strip=true 时只有地址（崔总 2026-10-12 拍：符号策略发版前再定）；
    // dev 构建有行号，直接可读。
    let backtrace = std::backtrace::Backtrace::force_capture();
    backtrace
        .to_string()
        .lines()
        .map(str::to_string)
        .collect()
}

fn dir_name(name: Option<&std::ffi::OsStr>) -> String {
    name.map_or_else(String::new, |value| value.to_string_lossy().into_owned())
}

/// `yyyymmdd`（本地时区，拿不到偏移就按 UTC —— 只是目录名，不参与排序）。
fn yymmdd(at: SystemTime) -> String {
    let offset = timefmt::local_offset_minutes().unwrap_or(0);
    let local = timefmt::local_iso_seconds(at, offset);
    let date = local.get(0..10).unwrap_or("0000-00-00");
    date.replace('-', "")
}

/// 6 位大写十六进制：指纹 → 目录后缀，稳定且不可逆。
fn short_hash(value: &str, salt: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in salt.as_bytes().iter().chain(value.as_bytes()) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{:06X}", hash & 0xff_ffff)
}

/// 请助手进程落一份 dump（W4）。没接助手时静默跳过 —— dump 是加分项，不是必需品。
///
/// 为什么要在这里调：`panic = "abort"` 下 Rust 走的是 Windows `int 0x29`（fastfail），
/// 内核直接结束进程，**crash-handler 是接不住的** —— 只有 panic hook 这一瞬间还活着，
/// 所以 dump 请求必须在这一刻发出（助手在另一个进程里对我们自己的 pid 落 dump）。
fn request_dump(
    hook: &Mutex<Option<Arc<dyn Fn(Option<String>) + Send + Sync>>>,
    dir: &Path,
) {
    let Ok(guard) = hook.lock() else {
        return;
    };
    if let Some(hook) = guard.as_ref() {
        hook(Some(dir.to_string_lossy().into_owned()));
    }
}

/// 写报告（JSON + 人读文本）。失败静默 —— 崩溃路径不许再制造新故障。
fn write_report(dir: &Path, report: &CrashReport) {
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let json = serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string());
    let _ = std::fs::write(dir.join("report.json"), json);
    let _ = std::fs::write(dir.join("report.txt"), human_text(report));
}

/// 人读一页（用户/崔总能直接看懂大概发生了什么）。
#[must_use]
pub fn human_text(report: &CrashReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "RayBend 崩溃记录\n报告 ID : {}\n类型    : {}\n时间    : {}\n版本    : {} ({}-{})\n会话    : {}\n",
        report.report_id,
        report.kind,
        report.created_at,
        report.app.version,
        report.app.profile,
        report.app.os,
        report.session,
    ));
    if let Some(panic) = report.panic.as_ref() {
        out.push_str(&format!(
            "\npanic   : {}\n位置    : {}\n线程    : {}\n",
            panic.message,
            panic.location.as_deref().unwrap_or("?"),
            panic.thread.as_deref().unwrap_or("?"),
        ));
        if panic.count > 1 {
            out.push_str(&format!("重复    : 同一次会话内第 {} 次\n", panic.count));
        }
    }
    if report.kind == "unclean-exit" {
        out.push_str("\n上次会话没有正常退出（没有收到退出流程，日志在此中断）。\n");
    }
    out.push_str("\n──── 日志尾部 ────\n");
    for line in &report.log_tail {
        out.push_str(line);
        out.push('\n');
    }
    if !report.backtrace.is_empty() {
        out.push_str("\n──── backtrace ────\n");
        for line in &report.backtrace {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// 启动时写 `session.lock`（原子写：先写临时文件再改名）。
pub fn write_session_lock(crashes_dir: &Path, session_id: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(crashes_dir)?;
    let payload = PreviousSession {
        session_id: session_id.to_string(),
        pid: std::process::id(),
        started_at: timefmt::utc_iso_millis(SystemTime::now()),
    };
    let body = serde_json::to_vec_pretty(&payload).unwrap_or_default();
    let path = crashes_dir.join(SESSION_LOCK);
    let temp = crashes_dir.join(format!("{SESSION_LOCK}.tmp"));
    {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(&body)?;
        file.sync_all()?;
    }
    std::fs::rename(&temp, &path)
}

/// 正常情况下删除 `session.lock`（进程退出流程调用）。
pub fn clear_session_lock(crashes_dir: &Path) {
    let _ = std::fs::remove_file(crashes_dir.join(SESSION_LOCK));
}

/// 读上一会话的 `session.lock`（不存在或坏了都返回 `None`）。
#[must_use]
pub fn read_previous_session(crashes_dir: &Path) -> Option<PreviousSession> {
    let body = std::fs::read(crashes_dir.join(SESSION_LOCK)).ok()?;
    serde_json::from_slice(&body).ok()
}

/// 取一个日志文件的尾部 `limit` 行（上一会话日志 = `raybend.1.log`）。
#[must_use]
pub fn tail_of_file(path: &Path, limit: usize) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    let skip = lines.len().saturating_sub(limit);
    lines[skip..].iter().map(|line| (*line).to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_report_lands_with_tail_and_dedups_by_fingerprint() {
        let dir = tempfile::tempdir().expect("临时目录");
        let recorder = CrashRecorder::new(dir.path().to_path_buf(), None, "s-test".to_string());
        recorder.record_panic("boom at C:\\Users\\bob\\x.jpg", Some("crates/raybend/src/a.rs:9:1"), Some("render"));
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .expect("列目录")
            .filter_map(Result::ok)
            .collect();
        assert_eq!(entries.len(), 1, "一次 panic 一个目录");
        let report_path = entries[0].path().join("report.json");
        let body = std::fs::read_to_string(&report_path).expect("读报告");
        assert!(body.contains("\"kind\": \"panic\""));
        assert!(!body.contains("bob"), "panic 消息也要脱敏：{body}");
        assert!(body.contains("crates/raybend/src/a.rs:9:1"), "源码位置保留");

        // 同一指纹第二次：仍是一个目录，count 累加
        recorder.record_panic("boom at C:\\Users\\bob\\x.jpg", Some("crates/raybend/src/a.rs:9:1"), Some("render"));
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .expect("列目录")
            .filter_map(Result::ok)
            .collect();
        assert_eq!(entries.len(), 1, "重复 panic 不该开第二个目录");
        let body = std::fs::read_to_string(&report_path).expect("读报告");
        assert!(body.contains("\"count\": 2"));
    }

    #[test]
    fn human_page_is_readable_and_mentions_the_kind() {
        let report = CrashReport {
            schema: 1,
            kind: "unclean-exit".to_string(),
            report_id: "RB-20261012-000001".to_string(),
            session: "s-1".to_string(),
            created_at: "2026-10-12T01:39:51.123Z".to_string(),
            app: AppInfo::current(),
            panic: None,
            backtrace: Vec::new(),
            log_tail: vec!["line a".to_string(), "line b".to_string()],
        };
        let text = human_text(&report);
        assert!(text.contains("RB-20261012-000001"));
        assert!(text.contains("没有正常退出"));
        assert!(text.contains("line b"));
    }

    #[test]
    fn session_lock_roundtrips_and_clears() {
        let dir = tempfile::tempdir().expect("临时目录");
        write_session_lock(dir.path(), "s-xyz").expect("写锁");
        let previous = read_previous_session(dir.path()).expect("读锁");
        assert_eq!(previous.session_id, "s-xyz");
        assert_eq!(previous.pid, std::process::id());
        clear_session_lock(dir.path());
        assert!(read_previous_session(dir.path()).is_none());
    }

    #[test]
    fn tail_of_file_takes_the_last_lines_only() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("raybend.1.log");
        std::fs::write(&path, "a\nb\nc\nd\n").expect("写");
        assert_eq!(tail_of_file(&path, 2), vec!["c".to_string(), "d".to_string()]);
        assert_eq!(tail_of_file(&path, 99).len(), 4);
        assert!(tail_of_file(&dir.path().join("nope.log"), 5).is_empty());
    }

    #[test]
    fn report_id_shape_matches_the_spec() {
        let dir = tempfile::tempdir().expect("临时目录");
        let recorder = CrashRecorder::new(dir.path().to_path_buf(), None, "s-1".to_string());
        let name = recorder
            .fresh_dir()
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        assert!(name.starts_with("RB-"), "{name}");
        let parts: Vec<&str> = name.split('-').collect();
        assert_eq!(parts.len(), 3, "{name}");
        assert_eq!(parts[1].len(), 8, "yymmdd");
        assert_eq!(parts[2].len(), 6, "6 位随机");
    }
}
