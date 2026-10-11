//! RAW/AI 共用的有界帧、子进程与超时监督。阻塞读取不得持有进程锁。
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
const WATCHDOG_TICK: Duration = Duration::from_millis(50);
pub(crate) const MAX_HEADER_BYTES: u32 = 64 * 1024;
/// worker 相关的一切失败（进程 / 协议 / 超时 / 解码）。
#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    /// 找不到或起不来 worker 进程。
    #[error("起不了 后台处理进程：{0}")]
    Spawn(String),
    /// 单次解码超时（子进程已被杀掉）。
    #[error("后台处理超过 {0} 秒没有响应，已终止该后台进程")]
    Timeout(u64),
    /// 子进程非正常退出 —— 大概率就是它把解码器崩掉了。
    #[error("后台处理进程异常退出（{0}）")]
    Crashed(String),
    /// 协议层面的错误（头太长、JSON 不合法、payload 长度对不上）。
    #[error("后台处理进程通信异常：{0}")]
    Protocol(String),
    /// 解码本身失败（格式不支持、文件损坏…），**进程是健康的**。
    #[error("{0}")]
    Decode(String),
}

impl WorkerError {
    /// 这次失败之后能不能立刻重试下一张？（进程崩溃/超时要重建，解码失败不用）
    #[must_use]
    pub const fn needs_respawn(&self) -> bool {
        matches!(
            self,
            Self::Timeout(_) | Self::Crashed(_) | Self::Protocol(_)
        )
    }
}

pub(crate) struct Proc {
    pub(crate) child: Child,
    pub(crate) stdin: ChildStdin,
    /// 读响应时**先把它取出去**（见 [`exchange`]）—— 否则阻塞读会一直持锁，
    /// 看门狗拿不到锁就杀不掉进程，超时形同虚设。
    pub(crate) stdout: Option<BufReader<ChildStdout>>,
}

/// 无条件杀掉子进程（换掉一个坏掉的 / 不会说协议的 worker）。
pub(crate) fn kill_proc(proc: &Arc<Mutex<Proc>>) {
    if let Ok(mut guard) = proc.lock() {
        let _ = guard.child.kill();
        let _ = guard.child.wait();
    }
}

/// 看门狗：到点还没完成就杀掉子进程。返回的 `JoinHandle` 由调用方 `join`。
pub(crate) fn spawn_watchdog(
    proc: Arc<Mutex<Proc>>,
    done: Arc<AtomicBool>,
    timed_out: Arc<AtomicBool>,
    timeout: Duration,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if done.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(WATCHDOG_TICK);
        }
        if done.load(Ordering::Relaxed) {
            return;
        }
        // 超时：先立旗（调用方据此把错误归类成 Timeout），再杀掉子进程。
        // **这是唯一能打断阻塞读的手段**（进程一死，管道就 EOF）。
        timed_out.store(true, Ordering::Relaxed);
        if let Ok(mut guard) = proc.lock() {
            let _ = guard.child.kill();
            let _ = guard.child.wait();
        }
    })
}

// ─────────────────────────── 帧读写 ───────────────────────────

pub(crate) fn write_frame<W: Write>(w: &mut W, body: &[u8]) -> std::io::Result<()> {
    let len = u32::try_from(body.len()).map_err(|_| std::io::Error::other("请求体过大"))?;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(body)?;
    w.flush()
}

pub(crate) fn read_frame<R: BufRead>(r: &mut R) -> Result<Vec<u8>, WorkerError> {
    let mut len_bytes = [0u8; 4];
    r.read_exact(&mut len_bytes)
        .map_err(|e| WorkerError::Crashed(format!("读响应头失败：{e}")))?;
    let len = u32::from_le_bytes(len_bytes);
    if len > MAX_HEADER_BYTES {
        return Err(WorkerError::Protocol(format!("响应头过长（{len} 字节）")));
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body)
        .map_err(|e| WorkerError::Crashed(format!("读响应头失败：{e}")))?;
    Ok(body)
}

pub(crate) fn spawn_proc(
    path: &std::path::Path,
    args: &[String],
    tag: &'static str,
) -> Result<Proc, WorkerError> {
    let mut cmd = Command::new(path);
    cmd.args(args);
    suppress_console_window(&mut cmd);
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        /*
         * stderr **必须管道接回**（`specs/logging-and-crash-report.md` §4.6）：
         * 以前用 `Stdio::inherit()`，而发布版/双击启动的父进程没有控制台 ——
         * 继承来的句柄无效，worker 自己的 panic 与解码器报错**一条都留不下**
         * （本文件的 `suppress_console_window` 注释早就承认了这一点）。
         * 现在逐行转进统一日志（带 `[raw-worker]` / `[ai-worker]` 标签）。
         */
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| WorkerError::Spawn(format!("{}：{e}", path.display())))?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| WorkerError::Spawn("拿不到子进程 stdin".to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| WorkerError::Spawn("拿不到子进程 stdout".to_string()))?;
    if let Some(stderr) = child.stderr.take() {
        forward_stderr(stderr, tag);
    }

    Ok(Proc {
        child,
        stdin,
        stdout: Some(BufReader::new(stdout)),
    })
}

/// 把 worker 的 stderr 逐行转进统一日志（后台线程；父进程不读管道会把它阻塞）。
///
/// worker 侧不需要知道日志框架的存在：»stderr 是它唯一的日志出口» 这条约定照旧。
fn forward_stderr(stderr: std::process::ChildStderr, tag: &'static str) {
    let _ = std::thread::Builder::new()
        .name(format!("{tag}-stderr"))
        .spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                if line.trim().is_empty() {
                    continue;
                }
                // panic 块的行升到 ERROR：诊断时一眼能扫到
                if line.contains("panicked at") || line.starts_with("thread '") {
                    log::error!(target: tag, "{line}");
                } else {
                    log::info!(target: tag, "{line}");
                }
            }
        });
}

/// 不让 worker 弹出一个**新的控制台窗口**（Windows）。
///
/// 2026-09-28 真机反馈：从构建目录直接跑 release 的 `raybend-desktop.exe`，一启动就多出一块
/// 黑框。查下来是 worker —— `raybend-raw-worker.exe` 是**控制台子系统**的可执行文件
/// （`windows_subsystem` 只加在主程序 `src-tauri/src/main.rs` 上），而父进程是 GUI 子系统、
/// 自己**没有**控制台，Windows 于是给子进程新开一个（实测：worker 的子进程里出现
/// `conhost.exe 0x4`）。
///
/// 修法不是把 worker 改成 GUI 子系统（那样它就不能单独跑给人看日志了），而是**按需**加
/// `CREATE_NO_WINDOW`：
///
/// * 父进程**没有**控制台（双击 / 资源管理器 / 打包后的应用）→ 加标志，子进程不再弹窗；
/// * 父进程**有**控制台（开发期从终端跑 debug）→ 什么都不加，worker 照旧拿得到那个终端。
///
/// ❗注意 stderr 的去向**不靠这个标志**：worker 的 stderr 一律走管道接回统一日志
/// （见 [`spawn_proc`]），不再依赖「继承父进程的控制台句柄」——双击启动也能留下记录。
///
/// 见 <https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags>。
#[cfg(windows)]
fn suppress_console_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // SAFETY: 无参数、无副作用，只读当前进程的控制台窗口句柄。
    let has_console = !unsafe { windows_sys::Win32::System::Console::GetConsoleWindow() }.is_null();
    if !has_console {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
}

#[cfg(not(windows))]
fn suppress_console_window(_cmd: &mut Command) {}

/// 共用同步往返：只在写入/归还 stdout 时持锁，超时线程可随时终止阻塞的 worker。
pub(crate) fn exchange<T>(
    proc: &Arc<Mutex<Proc>>,
    body: &[u8],
    timeout: Duration,
    read: impl FnOnce(&mut BufReader<ChildStdout>) -> Result<T, WorkerError>,
) -> Result<T, WorkerError> {
    let mut reader = {
        let mut guard = proc.lock().unwrap_or_else(|p| p.into_inner());
        write_frame(&mut guard.stdin, body)
            .map_err(|e| WorkerError::Crashed(format!("写请求失败：{e}")))?;
        guard
            .stdout
            .take()
            .ok_or_else(|| WorkerError::Protocol("子进程 stdout 已被取走".into()))?
    };
    let done = Arc::new(AtomicBool::new(false));
    let timed_out = Arc::new(AtomicBool::new(false));
    let watcher = spawn_watchdog(proc.clone(), done.clone(), timed_out.clone(), timeout);
    let result = read(&mut reader);
    done.store(true, Ordering::Relaxed);
    let _ = watcher.join();
    if let Ok(mut guard) = proc.lock()
        && guard.stdout.is_none()
    {
        guard.stdout = Some(reader);
    }
    if timed_out.load(Ordering::Relaxed) {
        return Err(WorkerError::Timeout(timeout.as_secs()));
    }
    result
}
