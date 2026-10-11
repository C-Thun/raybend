//! 崩溃助手进程的**接入**（主进程侧）：拉起 `raybend-crashpad.exe`、建管道、
//! 把 panic 与硬崩溃都变成一份 dump（`specs/logging-and-crash-report.md` §5.2）。
//!
//! 分工：
//!
//! | 谁来 | 干什么 |
//! | --- | --- |
//! | `raybend::diagnostics::crash`（主进程内） | panic 记录（消息 / 位置 / 日志尾）→ `report.json` |
//! | `raybend-crashpad`（独立进程） | 从主进程身上取一份 minidump → `dump.dmp` |
//!
//! 为什么要独立进程：`panic = "abort"` 在 Windows 上是 `int 0x29`（fastfail），内核直接结束
//! 进程，**同进程的 handler 接不住**；而且进程内写自己的 dump 在栈溢出/堆损坏时本就不可靠。
//!
//! ❗降级原则：助手起不来只是「少一份 dump」，日志与 panic 记录照旧 —— 绝不让它拖住启动。

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use minidumper::Client;
use raybend::diagnostics::crash::CrashRecorder;

/// 消息：本次崩溃的目标目录（让 dump 与 report.json 落在同一个 `RB-*` 里）。
const MSG_CRASH_DIR: u32 = 1;
/// 等助手把管道建好的上限（超时就走降级分支）。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// 正常退出时留给助手自行退出的时间。
const EXIT_GRACE: Duration = Duration::from_millis(500);

/// 助手进程句柄（进程级只此一份）。
pub struct Crashpad {
    child: Mutex<Option<Child>>,
    /// 保住 crash-handler 的注册（Drop 会撤掉它）。
    _handler: Mutex<Option<crash_handler::CrashHandler>>,
    assistant: PathBuf,
}

impl Crashpad {
    /// 正常退出：先把助手请走（它靠「client 断开」自行退出），兜底再 kill。
    pub fn shutdown(&self) {
        std::thread::sleep(EXIT_GRACE);
        if let Ok(mut slot) = self.child.lock()
            && let Some(mut child) = slot.take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// 助手可执行文件的路径（诊断时用）。
    #[must_use]
    pub fn assistant_path(&self) -> &Path {
        &self.assistant
    }
}

/// 拉起助手并接上（幂等：重复调用返回第一次的结果）。
pub fn attach(recorder: &Arc<CrashRecorder>, crashes_dir: &Path) -> Option<&'static Crashpad> {
    PAD.get_or_init(|| match try_attach(recorder, crashes_dir) {
        Ok(crashpad) => {
            log::info!(
                target: "crashpad",
                "崩溃助手已就位 assistant={}",
                crashpad.assistant_path().display()
            );
            Some(crashpad)
        }
        Err(error) => {
            // 不是致命错误：没有 dump，还有日志与 panic 记录
            log::warn!(target: "crashpad", "崩溃助手不可用：{error}（继续运行，崩溃时没有 dump）");
            None
        }
    })
    .as_ref()
}

/// 已接上的助手（没接上返回 `None`）。
#[must_use]
pub fn current() -> Option<&'static Crashpad> {
    PAD.get().and_then(Option::as_ref)
}

static PAD: OnceLock<Option<Crashpad>> = OnceLock::new();

fn try_attach(recorder: &Arc<CrashRecorder>, crashes_dir: &Path) -> Result<Crashpad, String> {
    let assistant = assistant_path()?;
    let pipe = format!("raybend-crashpad-{}", std::process::id());
    let child = spawn_assistant(&assistant, &pipe, crashes_dir)?;
    let client = connect(&pipe)?;
    let client = Arc::new(Mutex::new(client));

    // ① 硬崩溃（访问违例 / 栈溢出 / invalid parameter…）：crash-handler 接得住
    let handler = {
        let client = Arc::clone(&client);
        let event = unsafe {
            crash_handler::make_crash_event(move |context: &crash_handler::CrashContext| {
                let handled = client
                    .lock()
                    .map(|client| client.request_dump(context).is_ok())
                    .unwrap_or(false);
                crash_handler::CrashEventResult::Handled(handled)
            })
        };
        crash_handler::CrashHandler::attach(event)
            .map_err(|error| format!("装崩溃处理器失败：{error}"))?
    };

    // ② panic（fastfail 之前的那一瞬间）：由 panic hook 调这个钩子去要 dump
    {
        let client = Arc::clone(&client);
        recorder.set_dump_hook(Arc::new(move |dir: Option<String>| {
            let Ok(client) = client.lock() else {
                return;
            };
            if let Some(dir) = dir {
                let _ = client.send_message(MSG_CRASH_DIR, dir.as_bytes());
            }
            if let Err(error) = client.ping() {
                log::warn!(target: "crashpad", "与助手失联：{error}");
                return;
            }
            /*
             * panic 时**没有** EXCEPTION_POINTERS 可给（不是异常，是主动 abort）。
             * 但 pid / tid **必须是真的** —— 曾经试过全零的 context，Windows 上
             * `MiniDumpWriteDump` 直接回 `os error 87（参数错误）`，dump.dmp 是个 0 字节空文件
             * （2026-10-12 真机验出来的）。
             */
            let context = panic_crash_context();
            if let Err(error) = client.request_dump(&context) {
                log::warn!(target: "crashpad", "请求 dump 失败：{error}");
            }
        }));
    }

    Ok(Crashpad {
        child: Mutex::new(Some(child)),
        _handler: Mutex::new(Some(handler)),
        assistant,
    })
}

/**
 * panic 路径用的崩溃上下文：**空异常指针 + 真实的 pid / tid**。
 *
 * * Windows：`MiniDumpWriteDump` 要求 pid / tid 合法 —— 全零会直接报 `os error 87`，
 *   dump.dmp 变成 0 字节空文件（2026-10-12 真机验出来的）；
 * * 非 Windows：`CrashContext` 是另一套字段（signal 等），返回零值 ——
 *   规格 §2 只对 Windows 承诺 dump。
 */
#[cfg(windows)]
fn panic_crash_context() -> crash_handler::CrashContext {
    // SAFETY: 无参数、无副作用，只读当前线程 id。
    let thread_id = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
    crash_handler::CrashContext {
        exception_pointers: std::ptr::null(),
        exception_code: 0,
        process_id: std::process::id(),
        thread_id,
    }
}

#[cfg(not(windows))]
fn panic_crash_context() -> crash_handler::CrashContext {
    // SAFETY: 全零对非 Windows 的 CrashContext 合法（字段均为整数/指针）。
    unsafe { std::mem::zeroed() }
}

/// 助手可执行文件的位置：与主程序**同目录**（dev 构建与安装包都是这个布局）。
///
/// 打包侧由发版配置的 `externalBin` + `scripts/lib/release-windows.mjs` 的拷贝步骤保证
/// 它和主程序在同一层（`AGENTS.md` §5.3）。
fn assistant_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|error| format!("拿不到当前可执行文件路径：{error}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "当前可执行文件没有父目录".to_string())?;
    let name = format!("raybend-crashpad{}", std::env::consts::EXE_SUFFIX);
    // ① 同目录；② 上一级（`target/debug/deps/…` 跑测试时的布局）
    let direct = dir.join(&name);
    if direct.is_file() {
        return Ok(direct);
    }
    if let Some(parent) = dir.parent() {
        let upper = parent.join(&name);
        if upper.is_file() {
            return Ok(upper);
        }
    }
    Err(format!("找不到 {}（应位于 {}）", name, dir.display()))
}

fn spawn_assistant(assistant: &Path, pipe: &str, crashes_dir: &Path) -> Result<Child, String> {
    let mut command = Command::new(assistant);
    command
        .arg("--pipe")
        .arg(pipe)
        .arg("--crash-root")
        .arg(crashes_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    suppress_console(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("启动助手失败：{error}"))?;
    // 助手的 stderr 也要接进日志（它自己崩了就靠这一行）
    if let Some(stderr) = child.stderr.take() {
        let _ = std::thread::Builder::new()
            .name("crashpad-stderr".to_string())
            .spawn(move || {
                use std::io::BufRead;
                for line in std::io::BufReader::new(stderr).lines() {
                    let Ok(line) = line else { break };
                    if !line.trim().is_empty() {
                        log::info!(target: "crashpad", "{line}");
                    }
                }
            });
    }
    Ok(child)
}

/// 连上助手的命名管道（助手启动需要一点时间，重试到超时为止）。
fn connect(pipe: &str) -> Result<Client, String> {
    let started = std::time::Instant::now();
    let mut last = String::new();
    while started.elapsed() < CONNECT_TIMEOUT {
        match Client::with_name(minidumper::SocketName::path(pipe)) {
            Ok(client) => return Ok(client),
            Err(error) => last = error.to_string(),
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(format!("连不上助手管道（{last}）"))
}

/// 不让助手弹控制台窗口（它是控制台子系统二进制）。
#[cfg(windows)]
fn suppress_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn suppress_console(_command: &mut Command) {}
