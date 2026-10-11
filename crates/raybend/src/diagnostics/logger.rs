//! `log` 门面对接：格式化 → 脱敏 → 入队（`specs/logging-and-crash-report.md` §4.3–§4.5）。
//!
//! 行格式（定宽前缀 + `k=v` 尾巴，先人读再机读）：
//!
//! ```text
//! 2026-10-10T22:41:03.184Z  WARN   editor.render  drain 超时  waited=2500ms frames=812  thread=render
//! └─ 24 字符 UTC ─────────┘  └5宽┘  └─ target ─┘  └─ 消息（已脱敏）──────┘  └非主线程才追加┘
//! ```
//!
//! 消息里的 `k=v` 由**调用点**组织（这里不做结构化解析）；值同样要过脱敏 —— 所以
//! 脱敏发生在**整行**上，而不是逐字段。

use std::io;
use std::sync::Arc;
use std::time::SystemTime;

use log::{Level, LevelFilter, Log, Metadata, Record};

use super::timefmt;
use super::writer::{self, LogSink};

/// 诊断日志门面的实现。
struct DiagLogger {
    sink: Arc<LogSink>,
    level: LevelFilter,
}

impl Log for DiagLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        // 格式化 → 入队（脱敏在 sink 层做 —— 那是所有写入路径的唯一闸门）。
        let line = format_line(
            SystemTime::now(),
            record.level(),
            record.target(),
            &record.args().to_string(),
            thread_annotation().as_deref(),
        );
        self.sink.enqueue_line(&line);
    }

    fn flush(&self) {}
}

/// 格式化一行（纯函数，便于单测）。
#[must_use]
pub fn format_line(
    at: SystemTime,
    level: Level,
    target: &str,
    message: &str,
    thread: Option<&str>,
) -> String {
    let mut line = format!(
        "{}  {:<5}  {target}  {message}",
        timefmt::utc_iso_millis(at),
        level.as_str(),
    );
    if let Some(name) = thread {
        line.push_str("  thread=");
        line.push_str(name);
    }
    line
}

/// 非主线程才返回线程名（规格 §4.3：`thread=` 只在非主线程追加）。
#[must_use]
pub fn thread_annotation() -> Option<String> {
    let current = std::thread::current();
    let name = current.name()?;
    if name == "main" {
        return None;
    }
    Some(name.to_string())
}

/// 初始化日志：建目录、轮转、起写线程、装门面。
///
/// # Errors
/// 目录/文件层面失败时返回错误；**调用方必须降级继续** —— 没有日志是好过没有应用。
pub fn init(dir: std::path::PathBuf, session_id: &str) -> io::Result<Arc<LogSink>> {
    let sink = writer::start(dir, SystemTime::now(), session_id)?;
    let level = resolve_level();
    let logger = DiagLogger {
        sink: Arc::clone(&sink),
        level,
    };
    // set_boxed_logger 只在「已经设过」时失败（重复初始化）——那时沿用旧的即可
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(level);
    }
    Ok(sink)
}

/// 解析级别：`RAYBEND_LOG` 环境变量（仅开发用）→ 否则 dev 给 `DEBUG`、发布给 `INFO`。
#[must_use]
pub fn resolve_level() -> LevelFilter {
    if let Ok(spec) = std::env::var("RAYBEND_LOG")
        && let Some(level) = parse_level(&spec)
    {
        return level;
    }
    if cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    }
}

fn parse_level(spec: &str) -> Option<LevelFilter> {
    match spec.trim().to_ascii_lowercase().as_str() {
        "off" => Some(LevelFilter::Off),
        "error" => Some(LevelFilter::Error),
        "warn" | "warning" => Some(LevelFilter::Warn),
        "info" => Some(LevelFilter::Info),
        "debug" => Some(LevelFilter::Debug),
        "trace" => Some(LevelFilter::Trace),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn at(secs: u64, millis: u32) -> SystemTime {
        UNIX_EPOCH + Duration::new(secs, millis * 1_000_000)
    }

    #[test]
    fn line_layout_is_fixed_width_prefix_plus_message() {
        let line = format_line(
            at(1_791_672_063, 184),
            Level::Warn,
            "editor.render",
            "drain 超时  waited=2500ms frames=812",
            Some("render"),
        );
        assert_eq!(
            line,
            "2026-10-10T22:41:03.184Z  WARN   editor.render  drain 超时  waited=2500ms frames=812  thread=render"
        );
        // 前缀部分是定长的：24 字符时间 + 2 + 5 + 2
        assert_eq!(&line[..24], "2026-10-10T22:41:03.184Z");
        assert_eq!(&line[26..31], "WARN ");
        // 主线程不加 thread=（由 thread_annotation 决定，这里直接测无注解形态）
        let no_thread = format_line(at(0, 0), Level::Error, "sidecar.sync", "写出失败", None);
        assert!(no_thread.ends_with("写出失败"));
        assert!(!no_thread.contains("thread="));
    }

    #[test]
    fn level_spec_parsing_is_case_insensitive_and_rejects_junk() {
        assert_eq!(parse_level("debug"), Some(LevelFilter::Debug));
        assert_eq!(parse_level("WARN"), Some(LevelFilter::Warn));
        assert_eq!(parse_level("off"), Some(LevelFilter::Off));
        assert_eq!(parse_level("trace "), Some(LevelFilter::Trace));
        assert_eq!(parse_level("verbose"), None);
    }
}
