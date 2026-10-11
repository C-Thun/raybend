//! **诊断层**：运行日志 + 异常采集（`specs/logging-and-crash-report.md` 的第一、二层）。
//!
//! ```text
//! <应用数据目录>/
//! ├── logs/                      运行日志（常驻 · 2 MiB × 10 · 20 MiB 硬上限 · 出口脱敏）
//! └── crashes/RB-<yymmdd>-<hex>/ 异常记录（panic / 未正常退出；W4 的 dump 也落这里）
//! ```
//!
//! 依赖方向（规格 §3）：异常采集**只读**运行日志的产物；运行日志不知道异常采集的存在。
//!
//! 初始化在 `src-tauri`（有 Tauri path API 的那一层）——本模块只接收一个根目录参数，
//! 所以核心库不依赖 tauri（`AGENTS.md` §4）。
//!
//! **降级原则**：任何一步失败都不许把应用拖崩 —— 没有日志是好过没有应用。

pub mod bundle;
pub mod crash;
pub mod logger;
pub mod paths;
pub mod redact;
pub mod timefmt;
pub mod writer;

/// 把 `log` 门面转出（调用方只依赖本模块，不各自声明 `log` 依赖）。
pub use log;

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

use writer::LogSink;

/// 一次进程生命周期的诊断句柄。
pub struct Diagnostics {
    root: PathBuf,
    sink: Option<Arc<LogSink>>,
    recorder: Arc<crash::CrashRecorder>,
    session_id: String,
    /// 启动时发现「上一次没正常退出」（界面层 W3 会据此提示；这里只如实记录）。
    previous_unclean: bool,
}

impl Diagnostics {
    /// 诊断根目录（`logs/`、`crashes/` 的父目录）。
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 会话 id（日志行与会话头里那个 `s-xxxxxx`）。
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 日志写入口（拿不到时为 `None` —— 日志层起不来时的降级形态）。
    #[must_use]
    pub fn sink(&self) -> Option<&Arc<LogSink>> {
        self.sink.as_ref()
    }

    /// 崩溃记录器。
    #[must_use]
    pub fn recorder(&self) -> &Arc<crash::CrashRecorder> {
        &self.recorder
    }

    /// 上次是否未正常退出。
    #[must_use]
    pub fn previous_unclean(&self) -> bool {
        self.previous_unclean
    }

    /// 正常退出：flush + 关写线程 + 删 `session.lock`（幂等）。
    pub fn shutdown(&self) {
        if let Some(sink) = self.sink.as_ref() {
            sink.shutdown();
        }
        crash::clear_session_lock(&paths::crashes_dir(&self.root));
    }
}

/// 初始化诊断层并装 panic hook。**幂等**：重复调用返回第一次的那个句柄。
///
/// 不会失败：各层各自降级（日志起不来就只剩崩溃记录，崩溃记录写不进去就只剩 stderr）。
pub fn init(root: &Path) -> &'static Diagnostics {
    CURRENT.get_or_init(|| init_once(root))
}

/// 已初始化的诊断句柄（没初始化过返回 `None`）。
#[must_use]
pub fn current() -> Option<&'static Diagnostics> {
    CURRENT.get()
}

static CURRENT: OnceLock<Diagnostics> = OnceLock::new();

fn init_once(root: &Path) -> Diagnostics {
    let session_id = random_session_id();
    let crashes = paths::crashes_dir(root);

    // ① 先看上一会话有没有留下锁 —— 必须在日志轮转之前记下这件事（轮转后旧日志变 .1）
    let previous = crash::read_previous_session(&crashes);

    // ② 启日志（内部会把上一会话的 raybend.log 顺移为 raybend.1.log）
    let sink = match logger::init(paths::logs_dir(root), &session_id) {
        Ok(sink) => Some(sink),
        Err(error) => {
            // 日志起不来不是致命错误；把原因交给 stderr（dev 构建看得见）
            eprintln!("[diagnostics] 日志初始化失败：{error}（继续运行，仅失去落盘日志）");
            None
        }
    };

    // ③ 本会话的锁 + panic hook
    if let Err(error) = crash::write_session_lock(&crashes, &session_id) {
        eprintln!("[diagnostics] 写 session.lock 失败：{error}");
    }
    let recorder = Arc::new(crash::CrashRecorder::new(
        crashes.clone(),
        sink.clone(),
        session_id.clone(),
    ));
    crash::install_panic_hook(Arc::clone(&recorder));

    // ④ 上次没正常退出 → 补一条记录（日志尾从上一会话的 .1 文件取）
    let previous_unclean = previous.is_some();
    if let Some(previous) = previous {
        let tail = crash::tail_of_file(
            &paths::logs_dir(root).join(writer::archive_name(1)),
            crash::PREVIOUS_TAIL_LINES,
        );
        recorder.record_unclean_exit(&previous, tail);
        if let Some(sink) = sink.as_ref() {
            sink.write_blocking(&format!(
                "{}  WARN   diagnostics 上次会话未正常退出  previous={} pid={}",
                timefmt::utc_iso_millis(SystemTime::now()),
                previous.session_id,
                previous.pid,
            ));
        }
    }

    // ⑤ 现在才允许告一段落：记一条「本次启动」
    log::info!(
        target: "diagnostics",
        "会话开始 session={session_id} version={} profile={} previous_unclean={previous_unclean}",
        crate::version(),
        if cfg!(debug_assertions) { "dev" } else { "release" },
    );

    
    Diagnostics {
        root: root.to_path_buf(),
        sink,
        recorder,
        session_id,
        previous_unclean,
    }
}

/// 会话 id：3 字节随机（`s-xxxxxx`）。随机源拿不到时用时间兜底。
fn random_session_id() -> String {
    let mut bytes = [0_u8; 3];
    if getrandom::fill(&mut bytes).is_err() {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        bytes.copy_from_slice(&nanos.to_le_bytes()[..3]);
    }
    format!("s-{:02x}{:02x}{:02x}", bytes[0], bytes[1], bytes[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_layout_and_marks_unclean_exit_on_second_boot() {
        let dir = tempfile::tempdir().expect("临时目录");
        let root = dir.path();

        // 第一次启动：建目录、写锁、起日志
        {
            let diagnostics = init_once(root);
            assert!(paths::logs_dir(root).join(writer::ACTIVE_FILE).exists(), "活动日志在");
            assert!(!diagnostics.previous_unclean());
            if let Some(sink) = diagnostics.sink() {
                sink.write_blocking("boot-1 结束前的最后一行");
                sink.flush_blocking();
            }
            // 不调用 shutdown：模拟「没正常退出」
        }

        // 第二次启动：应当发现残留锁并补一条 unclean-exit 记录
        let diagnostics = init_once(root);
        assert!(diagnostics.previous_unclean(), "残留 session.lock ⇒ 上次未正常退出");
        let crashes = paths::crashes_dir(root);
        let entries: Vec<_> = std::fs::read_dir(&crashes)
            .expect("列目录")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("RB-"))
            .collect();
        assert_eq!(entries.len(), 1, "恰好一条未正常退出记录");
        let report = std::fs::read_to_string(entries[0].path().join("report.json")).expect("读");
        assert!(report.contains("\"kind\": \"unclean-exit\""));
        assert!(report.contains("boot-1 结束前的最后一行"), "带上一会话日志尾：{report}");

        // 正常退出流程：删锁
        diagnostics.shutdown();
        assert!(crash::read_previous_session(&crashes).is_none());
    }
}
