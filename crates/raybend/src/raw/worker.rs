//! RAW worker 进程：**崩溃隔离**的实际承担者（`AGENTS.md` §6.3 的第 2 条）。
//!
//! # 为什么必须隔离
//!
//! rawler 的显影路径里有 `todo!()` / `panic!()` / `unimplemented!()` 分支（CFA 形状意外、
//! 色彩矩阵长度不是 9、富士旋转与活跃区冲突……），上游还明确声明**不要拿它处理不可信文件**。
//! 一张格式古怪的照片就能让解码器崩掉 —— 这件事必须发生在**别的进程**里。
//!
//! # 形态：常驻子进程 + 长度前缀协议
//!
//! ```text
//! 请求：  [u32 LE 头长][JSON 头]                 （JSON 头里带 payloadLen=0）
//! 响应：  [u32 LE 头长][JSON 头][payload 字节]   （payloadLen 由 JSON 头声明）
//! ```
//!
//! * **常驻**：不用每张图开一次进程（Windows 上 spawn 一次 5–15ms，5000 张就是几十秒）；
//! * **一次一个请求**：协议是同步的，客户端一次只发一个请求，不需要请求 id；
//! * **超时看门狗**：单次解码超过 [`DEFAULT_TIMEOUT`] 就杀掉子进程 ——
//!   看门狗从另一线程 `kill`，管道随之关闭，阻塞在读上的客户端拿到 EOF，不会永久挂住 UI；
//! * **崩溃后自动重建**：子进程死了就把句柄丢掉，下次请求重新 spawn（一次失败不影响后续）。
//!
//! # 进程从哪来
//!
//! 解析顺序（[`resolve_worker`]）：
//!
//! 1. 环境变量 `RAYBEND_RAW_WORKER` 指定（诊断用）；
//! 2. 与当前可执行文件同级的 `raybend-raw-worker`（cargo 的 `target/debug`、`deps/` 布局）；
//! 3. 兜底：**自己**带上标记参数 `--raybend-raw-worker` 重启 —— 打包后的应用只有主程序
//!    一个可执行文件，这条路径才是发布形态（`src-tauri/src/main.rs` 里有对应的入口）。

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::backend::{PixelSource, RawBackend, RawImage8, RawImage16};
use super::rawler_backend::RawlerBackend;

/// worker 进程的启动标记：主程序看到它就走 worker 循环（`main` 里判断）。
pub const WORKER_ARG: &str = "--raybend-raw-worker";
/// 覆盖 worker 可执行文件路径的环境变量。
pub const WORKER_ENV: &str = "RAYBEND_RAW_WORKER";
/// 单次解码超时。60MP 的完整解码在慢机器上也就几秒，45 秒足够宽松。
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(45);
/// 握手超时（只验证对面会不会说协议，不该久）。
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// 看门狗的轮询间隔。
const WATCHDOG_TICK: Duration = Duration::from_millis(50);
/// 头长度上限（防御：不能让对端用一个巨大的数字把我们撑爆）。
const MAX_HEADER_BYTES: u32 = 64 * 1024;

/// 进程内**共享**的 worker（主进程只有一条解码管道）。
///
/// 为什么共享而不是各自新建：worker 进程里缓存了 rawler 的相机数据库（建一次 ~130ms，
/// 见 `rawler_backend` 里的 `loader()`），每张图新起一个进程就要白付这笔钱。
/// 共享一条管道把「进程启动」与「建库」两笔开销都省了。
///
/// 代价：解码是串行的（同一时刻只有一个调用者拿得到锁）。RAW 解码本来就是重活，
/// 缩略图队列也是单线程在跑，不亏。
pub fn shared() -> &'static Mutex<RawWorker> {
    static SHARED: std::sync::OnceLock<Mutex<RawWorker>> = std::sync::OnceLock::new();
    SHARED.get_or_init(|| Mutex::new(RawWorker::new()))
}

// ─────────────────────────── 协议 ───────────────────────────

/// 线上协议版本。**加/改字段就抬它**（两边同源，重建时自动一致）。
///
/// ⚠️ 为什么要有这个（2026-09-24 真机事故）：Windows 侧那个 `raybend-raw-worker.exe`
/// 是 9 月 19 日建的，而 `linear16` 是 9 月 23 日加的 —— **主程序新、worker 旧**，
/// 于是编辑器的线性解码在真机上一直失败（旧 worker 只认 `srgb8`）。
/// 更糟的是这个失败当时被「过期结果」那条路吞掉了，界面表现为**永远卡在「正在载入照片」**。
/// 光靠「记得重建」不够 —— 所以现在版本对不上就**当面报错**，并且错误里写清怎么修。
///
/// 版本史：v1 = 只有 `srgb8`；v2 = 加 `linear16` + `as_shot_temperature`。
pub const PROTOCOL_VERSION: u32 = 3;

/// 版本标签（**机器可读**，给「产物是不是这一份源码建的」用）。
///
/// `scripts/check-win-artifact.mjs` 会在 Windows 的 worker 可执行文件里找这个串 ——
/// 找不到就说明**主程序新、worker 旧**（2026-09-24 那次真机事故的样子），当场报错。
/// 它由 worker 启动时打到 stderr，所以一定在二进制里。**改协议就改它**（连同版本号）。
pub const PROTOCOL_TAG: &str = "raybend-worker-proto-v3";

/// 版本对不上时给人的那句话（客户端与测试共用一份文案）。
fn protocol_mismatch(theirs: u32) -> String {
    format!(
        "RAW worker 协议版本对不上：它报 v{theirs}，本程序要 v{PROTOCOL_VERSION}。\
         多半是 `raybend-raw-worker` 没跟着一起重建 —— \
         重建它：`cargo build -p raybend --bin raybend-raw-worker`\
         （Windows 侧见 AGENTS.md §5.3；`pnpm debug:win` / 发布脚本已带上这一步）。"
    )
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Request {
    op: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    max_edge: Option<u32>,
    #[serde(default)]
    allow_preview: bool,
    /// 像素形态：缺省 / `"srgb8"` = 8bit sRGB（浏览用）；`"linear16"` = 线性 sRGB u16（显影用）。
    #[serde(default)]
    format: Option<String>,
    /// 仅 `op = "sleep"` 用（隔离演练）。
    #[serde(default)]
    secs: u64,
    /// 协议版本（见 [`PROTOCOL_VERSION`]；旧客户端不发 = 0）。
    #[serde(default)]
    protocol: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Response {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    orientation: Option<u16>,
    /// 像素形态回执（与请求对得上；旧客户端不认它就当 `srgb8`）。
    #[serde(default)]
    format: Option<String>,
    /// **拍摄色温估计**（K，只有 `linear16` 会给）——色温拉杆的基线。
    #[serde(default)]
    as_shot_temperature: Option<f32>,
    #[serde(default)]
    lens_name: Option<String>,
    #[serde(default)]
    payload_len: u64,
    /// 协议版本回执（旧 worker 不发这个字段 = 0 ⇒ 客户端据此判定「它过期了」）。
    #[serde(default)]
    protocol: u32,
}

/// 像素形态（协议层的字符串是稳定接口，**不要改**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PixelFormat {
    /// 8bit sRGB（浏览 / 缩略图）。
    Srgb8,
    /// 线性 sRGB u16（显影管线）。
    Linear16,
}

impl PixelFormat {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Srgb8 => "srgb8",
            Self::Linear16 => "linear16",
        }
    }
}

/// 响应里的像素来源字符串 → 枚举（认不出就当内嵌预览 —— 旧协议没有这个字段）。
fn source_of(head: &Response) -> PixelSource {
    match head.source.as_deref() {
        Some("decoded") => PixelSource::Decoded,
        _ => PixelSource::EmbeddedPreview,
    }
}

/// 响应里的像素形态（认不出 / 没给 = `srgb8`）。
fn format_of(head: &Response) -> PixelFormat {
    match head.format.as_deref() {
        Some("linear16") => PixelFormat::Linear16,
        _ => PixelFormat::Srgb8,
    }
}

/// worker 相关的一切失败（进程 / 协议 / 超时 / 解码）。
#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    /// 找不到或起不来 worker 进程。
    #[error("起不了 RAW 解码进程：{0}")]
    Spawn(String),
    /// 单次解码超时（子进程已被杀掉）。
    #[error("RAW 解码超过 {0} 秒没有响应，已终止该解码进程")]
    Timeout(u64),
    /// 子进程非正常退出 —— 大概率就是它把解码器崩掉了。
    #[error("RAW 解码进程异常退出（{0}）")]
    Crashed(String),
    /// 协议层面的错误（头太长、JSON 不合法、payload 长度对不上）。
    #[error("RAW 解码进程通信异常：{0}")]
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

// ─────────────────────────── 客户端 ───────────────────────────

struct Proc {
    child: Child,
    stdin: ChildStdin,
    /// 读响应时**先把它取出去**（见 [`exchange`]）—— 否则阻塞读会一直持锁，
    /// 看门狗拿不到锁就杀不掉进程，超时形同虚设。
    stdout: Option<BufReader<ChildStdout>>,
}

/// 常驻 worker 的客户端。**不要求 `Sync`** —— 由缩略图后台线程独占使用。
pub struct RawWorker {
    proc: Option<Arc<Mutex<Proc>>>,
    timeout: Duration,
    /// 指定的 worker 可执行文件（`None` = 走 [`resolve_worker`] 的解析顺序）。
    bin: Option<PathBuf>,
}

impl Default for RawWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for RawWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawWorker")
            .field("running", &self.proc.is_some())
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl RawWorker {
    #[must_use]
    pub fn new() -> Self {
        Self {
            proc: None,
            timeout: DEFAULT_TIMEOUT,
            bin: None,
        }
    }

    #[must_use]
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            proc: None,
            timeout,
            bin: None,
        }
    }

    /// 指定 worker 可执行文件（集成测试与诊断用；生产代码走 [`resolve_worker`]）。
    #[must_use]
    pub fn with_bin(bin: impl Into<PathBuf>) -> Self {
        Self {
            proc: None,
            timeout: DEFAULT_TIMEOUT,
            bin: Some(bin.into()),
        }
    }

    /// 改超时（链式，给集成测试与调试用）。
    #[must_use]
    pub fn with_timeout_of(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// 子进程是否已经在跑（诊断/测试用）。
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.proc.is_some()
    }

    /// 不解码像素，只从 RAW 厂商元数据读取镜头名称。
    pub fn lens_name(&mut self, path: &std::path::Path) -> Result<Option<String>, WorkerError> {
        let wire = Request {
            op: "metadata".to_string(),
            path: path.to_string_lossy().into_owned(),
            ..Request::default()
        };
        let (head, payload) = self.round_trip_raw(&wire)?;
        if !head.ok {
            return Err(WorkerError::Decode(head.error.unwrap_or_else(|| "RAW 元数据读取失败".to_string())));
        }
        if !payload.is_empty() {
            return Err(WorkerError::Protocol("元数据响应不应包含像素".to_string()));
        }
        Ok(head.lens_name)
    }

    /// 主动结束子进程（应用退出时调用；`Drop` 里也会做一次）。
    pub fn shutdown(&mut self) {
        if let Some(proc) = self.proc.take() {
            let mut guard = match proc.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            let _ = guard.child.kill();
            let _ = guard.child.wait();
        }
    }

    /// 解码一张 RAW（8bit sRGB）。失败时**不需要**调用方做清理（下次请求会自动重建进程）。
    ///
    /// # Errors
    /// 进程故障（起不来 / 崩了 / 超时）、协议错、或解码本身失败。
    pub fn decode(
        &mut self,
        req: &super::backend::DecodeRequest,
    ) -> Result<RawImage8, WorkerError> {
        let (head, payload) = self.round_trip(req, PixelFormat::Srgb8)?;
        let width = head.width;
        let height = head.height;
        let expected = u64::from(width) * u64::from(height) * 3;
        if payload.len() as u64 != expected {
            return Err(WorkerError::Protocol(format!(
                "像素长度对不上：期望 {expected}，收到 {}",
                payload.len()
            )));
        }
        let image = RawImage8 {
            width,
            height,
            rgb: payload,
            source: source_of(&head),
            orientation: head.orientation,
        };
        if !image.is_consistent() {
            return Err(WorkerError::Protocol("解出来的图尺寸不合法".to_string()));
        }
        Ok(image)
    }

    /// 解码一张 RAW 成**线性 16 位**（M3-W3 的显影管线输入）。
    ///
    /// 载荷是 u16 小端（每像素 3 个值），与 [`RawImage8`] 的协议只差格式。
    ///
    /// # Errors
    /// 同 [`Self::decode`]。
    pub fn decode_linear(
        &mut self,
        req: &super::backend::DecodeRequest,
    ) -> Result<RawImage16, WorkerError> {
        let (head, payload) = self.round_trip(req, PixelFormat::Linear16)?;
        if format_of(&head) != PixelFormat::Linear16 {
            return Err(WorkerError::Protocol(format!(
                "worker 回的像素形态不对：期望 linear16，收到 {:?}",
                head.format
            )));
        }
        let width = head.width;
        let height = head.height;
        let expected = u64::from(width) * u64::from(height) * 3 * 2;
        if payload.len() as u64 != expected {
            return Err(WorkerError::Protocol(format!(
                "线性像素长度对不上：期望 {expected} 字节，收到 {}",
                payload.len()
            )));
        }
        let mut rgb = Vec::with_capacity(payload.len() / 2);
        for chunk in payload.as_chunks::<2>().0 {
            rgb.push(u16::from_le_bytes([chunk[0], chunk[1]]));
        }
        let image = RawImage16 {
            width,
            height,
            rgb,
            source: source_of(&head),
            orientation: head.orientation,
            as_shot_temperature: head.as_shot_temperature,
        };
        if !image.is_consistent() {
            return Err(WorkerError::Protocol("线性解码结果尺寸不合法".to_string()));
        }
        Ok(image)
    }

    /// 探活（不起解码，只验证进程与协议）。
    pub fn ping(&mut self) -> Result<(), WorkerError> {
        let req = Request {
            op: "ping".to_string(),
            path: String::new(),
            max_edge: None,
            allow_preview: false,
            format: None,
            secs: 0,
            ..Request::default()
        };
        let (resp, _) = self.round_trip_raw(&req)?;
        if resp.ok {
            Ok(())
        } else {
            Err(WorkerError::Decode(
                resp.error.unwrap_or_else(|| "ping 失败".to_string()),
            ))
        }
    }

    /// **隔离演练用**：让 worker 自己 panic（验证「它崩了、主进程活着」）。
    pub fn crash_for_test(&mut self) -> Result<(), WorkerError> {
        let req = Request {
            op: "panic".to_string(),
            path: String::new(),
            max_edge: None,
            allow_preview: false,
            format: None,
            secs: 0,
            ..Request::default()
        };
        let (resp, _) = self.round_trip_raw(&req)?;
        if resp.ok {
            Ok(())
        } else {
            Err(WorkerError::Decode(
                resp.error.unwrap_or_else(|| "panic 演练失败".to_string()),
            ))
        }
    }

    /// **隔离演练用**：让 worker 卡住 `secs` 秒（验证看门狗真的会杀它）。
    pub fn sleep_for_test(&mut self, secs: u64) -> Result<(), WorkerError> {
        let req = Request {
            op: "sleep".to_string(),
            path: String::new(),
            max_edge: None,
            allow_preview: false,
            format: None,
            secs,
            ..Request::default()
        };
        let (resp, _) = self.round_trip_raw(&req)?;
        if resp.ok {
            Ok(())
        } else {
            Err(WorkerError::Decode(
                resp.error.unwrap_or_else(|| "sleep 演练失败".to_string()),
            ))
        }
    }

    fn round_trip(
        &mut self,
        req: &super::backend::DecodeRequest,
        format: PixelFormat,
    ) -> Result<(Response, Vec<u8>), WorkerError> {
        let wire = Request {
            op: "decode".to_string(),
            path: req.path.to_string_lossy().into_owned(),
            max_edge: req.max_edge,
            allow_preview: req.allow_preview,
            format: Some(format.as_str().to_string()),
            secs: 0,
            ..Request::default()
        };
        let pair = match self.round_trip_raw(&wire) {
            Ok(pair) => pair,
            Err(e) => {
                if e.needs_respawn() {
                    self.shutdown();
                }
                return Err(e);
            }
        };
        let (head, payload) = pair;
        if !head.ok {
            return Err(WorkerError::Decode(
                head.error.unwrap_or_else(|| "未知错误".to_string()),
            ));
        }
        Ok((head, payload))
    }

    fn round_trip_raw(&mut self, wire: &Request) -> Result<(Response, Vec<u8>), WorkerError> {
        let proc = self.ensure_proc()?;
        exchange(&proc, wire, self.timeout)
    }

    fn ensure_proc(&mut self) -> Result<Arc<Mutex<Proc>>, WorkerError> {
        if let Some(existing) = &self.proc {
            // 子进程可能已经死了（上次崩溃后我们没来得及清理，或外部杀掉了）
            let mut guard = existing.lock().unwrap_or_else(|p| p.into_inner());
            match guard.child.try_wait() {
                Ok(None) => return Ok(existing.clone()),
                Ok(Some(_)) | Err(_) => {
                    drop(guard);
                    self.proc = None;
                }
            }
        }
        let (path, _) = match self.bin.as_deref() {
            Some(p) => (p.to_path_buf(), false),
            None => resolve_worker().map_err(WorkerError::Spawn)?,
        };
        let proc = spawn_proc(&path)?;
        let shared = Arc::new(Mutex::new(proc));

        /*
         * 握手：确认对面真的在说我们的协议。
         *
         * 没有它，一个「解析到错误可执行文件」的情况（比如回退成了自己、结果对方根本不认
         * `--raybend-raw-worker`）会在第一次解码时表现为「响应头过长」这种莫名其妙的协议错误，
         * 甚至让父进程把子进程打印的一堆文字当成帧头。握手一次就把这种错误变成一句人话。
         */
        let handshake = Request {
            op: "ping".to_string(),
            path: String::new(),
            max_edge: None,
            allow_preview: false,
            format: None,
            secs: 0,
            ..Request::default()
        };
        match exchange(&shared, &handshake, HANDSHAKE_TIMEOUT) {
            Ok((resp, _)) if resp.ok => {}
            Ok(_) => {
                kill_proc(&shared);
                return Err(WorkerError::Spawn(format!(
                    "{} 不会说我们的协议（握手被拒）",
                    path.display()
                )));
            }
            Err(e) => {
                kill_proc(&shared);
                return Err(WorkerError::Spawn(format!(
                    "{} 握手失败：{e}",
                    path.display()
                )));
            }
        }

        self.proc = Some(shared.clone());
        Ok(shared)
    }
}

impl Drop for RawWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// 给请求盖上协议版本（**唯一一处** —— 构造点不必各自记得填）。
fn stamp(wire: &Request) -> Request {
    Request {
        protocol: PROTOCOL_VERSION,
        ..wire.clone()
    }
}

/// 一次同步往返：发请求 → 装看门狗 → 读响应。
///
/// ⚠️ **两个坑都在这里，别改回去**：
///
/// 1. 阻塞读期间**不能持锁** —— `ChildStdout` 先 `take()` 出去，读完再还；
///    否则看门狗拿不到锁、杀不掉卡住的子进程，超时机制完全失效
///    （实测：`sleep 60` 会把父进程真的卡 60 秒）；
/// 2. 「看门狗开了刀」与「正常完成」必须**两个不同的标志**区分 ——
///    否则子进程被杀后读会返回 EOF，被当成普通崩溃而不是超时。
fn exchange(
    proc: &Arc<Mutex<Proc>>,
    wire: &Request,
    timeout: Duration,
) -> Result<(Response, Vec<u8>), WorkerError> {
    // ① 发请求（只在写这一段持锁）
    let mut reader = {
        let mut guard = proc.lock().unwrap_or_else(|p| p.into_inner());
        let body = serde_json::to_vec(&stamp(wire))
            .map_err(|e| WorkerError::Protocol(format!("请求序列化失败：{e}")))?;
        write_frame(&mut guard.stdin, &body)
            .map_err(|e| WorkerError::Crashed(format!("写请求失败：{e}")))?;
        guard
            .stdout
            .take()
            .ok_or_else(|| WorkerError::Protocol("子进程 stdout 已被取走".to_string()))?
    };

    // ② 看门狗（超时就 kill —— 它要能拿到锁，所以上面必须已经把 stdout 拿出来了）
    let done = Arc::new(AtomicBool::new(false));
    let timed_out = Arc::new(AtomicBool::new(false));
    let watchdog = spawn_watchdog(proc.clone(), done.clone(), timed_out.clone(), timeout);

    // ③ 读响应
    let result = read_response(&mut reader);

    done.store(true, Ordering::Relaxed);
    let _ = watchdog.join();

    // ④ 归还 stdout（进程还在的话），让下一次请求能复用
    if let Ok(mut guard) = proc.lock()
        && guard.stdout.is_none()
    {
        guard.stdout = Some(reader);
    }

    match result {
        // 版本核对：**过期 worker 不许静默工作**（它就是「卡在正在载入照片」那次的原因）
        Ok((resp, _)) if resp.protocol != PROTOCOL_VERSION => {
            Err(WorkerError::Protocol(protocol_mismatch(resp.protocol)))
        }
        Ok(v) => Ok(v),
        // 超时：错误归档成 Timeout（真实的读取错误对用户没意义，超时才是）
        Err(_) if timed_out.load(Ordering::Relaxed) => Err(WorkerError::Timeout(timeout.as_secs())),
        Err(e) => {
            // 读失败时顺手看一眼子进程死没死 —— “退出码 101”比
            // “failed to fill whole buffer”有用得多
            let status = proc
                .lock()
                .ok()
                .and_then(|mut g| g.child.try_wait().ok().flatten())
                .map(|s| describe_exit(&s));
            Err(match status {
                Some(what) => WorkerError::Crashed(format!("子进程已退出（{what}）：{e}")),
                None => e,
            })
        }
    }
}

/// 读一个响应（头 + 可选 payload）。**不碰锁**。
fn read_response<R: BufRead>(reader: &mut R) -> Result<(Response, Vec<u8>), WorkerError> {
    let head = read_frame(reader)?;
    let resp: Response = serde_json::from_slice(&head)
        .map_err(|e| WorkerError::Protocol(format!("响应头不是合法 JSON：{e}")))?;
    let payload = if resp.payload_len > 0 {
        let mut buf = vec![0u8; resp.payload_len as usize];
        reader
            .read_exact(&mut buf)
            .map_err(|e| WorkerError::Crashed(format!("读像素失败：{e}")))?;
        buf
    } else {
        Vec::new()
    };
    Ok((resp, payload))
}

/// 无条件杀掉子进程（换掉一个坏掉的 / 不会说协议的 worker）。
fn kill_proc(proc: &Arc<Mutex<Proc>>) {
    if let Ok(mut guard) = proc.lock() {
        let _ = guard.child.kill();
        let _ = guard.child.wait();
    }
}

/// 看门狗：到点还没完成就杀掉子进程。返回的 `JoinHandle` 由调用方 `join`。
fn spawn_watchdog(
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

fn write_frame<W: Write>(w: &mut W, body: &[u8]) -> std::io::Result<()> {
    let len = u32::try_from(body.len()).map_err(|_| std::io::Error::other("请求体过大"))?;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(body)?;
    w.flush()
}

fn read_frame<R: BufRead>(r: &mut R) -> Result<Vec<u8>, WorkerError> {
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

// ─────────────────────────── 进程启动 ───────────────────────────

/// 解析 worker 可执行文件。返回 `(路径, 是否需要加标记参数)`。
///
/// ⚠️ **不允许自我递归**：若当前进程自己就是 worker（argv 里有 [`WORKER_ARG`]），
/// 说明父进程把我们当成了 worker 入口、而请求又打到了这个进程上 —— 直接报错，
/// 而不是再 spawn 一次自己（那样会无限套娃，而且会把非协议输出当成帧头）。
pub fn resolve_worker() -> Result<(PathBuf, bool), String> {
    if std::env::args().any(|a| a == WORKER_ARG) {
        return Err(format!(
            "当前进程本身就是 RAW worker（argv 含 {WORKER_ARG}），不能再拿它当 worker 入口"
        ));
    }

    if let Ok(custom) = std::env::var(WORKER_ENV)
        && !custom.trim().is_empty()
    {
        let path = PathBuf::from(custom);
        if path.is_file() {
            return Ok((path, false));
        }
        return Err(format!("{WORKER_ENV} 指向的文件不存在：{}", path.display()));
    }

    let exe = std::env::current_exe().map_err(|e| format!("拿不到当前可执行文件路径：{e}"))?;
    let name = format!("raybend-raw-worker{}", std::env::consts::EXE_SUFFIX);
    // 先看自己所在目录，再看上一层：
    //   * `target/debug/raybend-desktop` → 同目录
    //   * `target/debug/deps/…`（测试）与 `target/debug/examples/…`（示例）→ 上一层
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(dir) = exe.parent() {
        dirs.push(dir.to_path_buf());
        if let Some(parent) = dir.parent() {
            dirs.push(parent.to_path_buf());
        }
    }
    for dir in dirs {
        let candidate = dir.join(&name);
        if candidate.is_file() {
            return Ok((candidate, false));
        }
    }

    // 兜底：自己是那个入口（打包后的应用只有主程序这一个可执行文件）。
    // 是否真的能当 worker 由**主程序**决定（它在 main 里检查标记参数）。
    Ok((exe, true))
}

fn spawn_proc(path: &std::path::Path) -> Result<Proc, WorkerError> {
    let needs_marker = path
        .file_name()
        .is_none_or(|n| !n.to_string_lossy().starts_with("raybend-raw-worker"));
    let mut cmd = Command::new(path);
    if needs_marker {
        cmd.arg(WORKER_ARG);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // stderr 继承：解码器自己的日志/panic 直接进我们这边的日志，不吞掉
        .stderr(Stdio::inherit())
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

    Ok(Proc {
        child,
        stdin,
        stdout: Some(BufReader::new(stdout)),
    })
}

fn describe_exit(status: &std::process::ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("退出码 {code}"),
        None => "被信号终止".to_string(),
    }
}

// ─────────────────────────── worker 端 ───────────────────────────

/// worker 进程的主循环。**在主程序 `main` 最前面调用**（标记参数命中时）。
///
/// 返回值就是进程退出码。协议错误返回 2，正常收到 EOF 返回 0。
pub fn run_worker_main() -> i32 {
    // 启动就报一次版本：既方便人看日志，也让 [`PROTOCOL_TAG`] 一定在二进制里
    // （`check-win-artifact.mjs` 靠这个串判断「这个 worker 是不是当前源码建的」）。
    eprintln!("[worker] raybend-raw-worker {PROTOCOL_TAG}（协议 v{PROTOCOL_VERSION}）就绪");
    let stdin = std::io::stdin();
    let mut input = BufReader::new(stdin.lock());
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    loop {
        let body = match read_frame(&mut input) {
            Ok(body) => body,
            // EOF：父进程关了管道（正常收工）
            Err(WorkerError::Crashed(_)) => return 0,
            Err(_) => return 2,
        };
        let request: Request = match serde_json::from_slice(&body) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response {
                    ok: false,
                    error: Some(format!("请求不是合法 JSON：{e}")),
                    ..Response::default()
                };
                if write_response(&mut out, &resp, &[]).is_err() {
                    return 2;
                }
                continue;
            }
        };

        let (resp, payload) = handle(request);
        if write_response(&mut out, &resp, &payload).is_err() {
            return 2;
        }
    }
}

fn handle(request: Request) -> (Response, Vec<u8>) {
    match request.op.as_str() {
        "ping" => (
            Response {
                ok: true,
                ..Response::default()
            },
            Vec::new(),
        ),
        /*
         * 下面两条是**隔离演练**用的：`--crash` / `--timeout` 会走它们。
         * 它们故意不捕获 panic —— 要验证的正是「worker 崩掉之后主进程还活着」。
         */
        "panic" => panic!("隔离演练：worker 主动 panic（这条消息应当只出现在日志里）"),
        "sleep" => {
            std::thread::sleep(std::time::Duration::from_secs(request.secs));
            (
                Response {
                    ok: true,
                    ..Response::default()
                },
                Vec::new(),
            )
        }
        "metadata" => match RawlerBackend::lens_name(std::path::Path::new(&request.path)) {
            Ok(lens_name) => (Response { ok: true, lens_name, ..Response::default() }, Vec::new()),
            Err(error) => (Response { ok: false, error: Some(error.to_string()), ..Response::default() }, Vec::new()),
        },
        "decode" => {
            let req = super::backend::DecodeRequest {
                path: PathBuf::from(&request.path),
                max_edge: request.max_edge,
                allow_preview: request.allow_preview,
            };
            let backend = RawlerBackend::new();
            let linear = request.format.as_deref() == Some("linear16");
            if linear {
                match backend.decode_linear(&req) {
                    Ok(image) => {
                        // u16 → 小端字节（协议层只搬字节，形态由 format 字段说明）
                        let mut payload = Vec::with_capacity(image.rgb.len() * 2);
                        for value in &image.rgb {
                            payload.extend_from_slice(&value.to_le_bytes());
                        }
                        let payload_len = payload.len() as u64;
                        (
                            Response {
                                ok: true,
                                error: None,
                                width: image.width,
                                height: image.height,
                                source: Some(image.source.as_str().to_string()),
                                orientation: image.orientation,
                                format: Some("linear16".to_string()),
                                as_shot_temperature: image.as_shot_temperature,
                                payload_len,
                                ..Response::default()
                            },
                            payload,
                        )
                    }
                    Err(e) => (
                        Response {
                            ok: false,
                            error: Some(e.to_string()),
                            ..Response::default()
                        },
                        Vec::new(),
                    ),
                }
            } else {
                match backend.decode(&req) {
                    Ok(image) => {
                        let payload_len = image.rgb.len() as u64;
                        (
                            Response {
                                ok: true,
                                error: None,
                                width: image.width,
                                height: image.height,
                                source: Some(image.source.as_str().to_string()),
                                orientation: image.orientation,
                                format: Some("srgb8".to_string()),
                                as_shot_temperature: None,
                                payload_len,
                                ..Response::default()
                            },
                            image.rgb,
                        )
                    }
                    Err(e) => (
                        Response {
                            ok: false,
                            error: Some(e.to_string()),
                            ..Response::default()
                        },
                        Vec::new(),
                    ),
                }
            }
        }
        other => (
            Response {
                ok: false,
                error: Some(format!("未知操作：{other}")),
                ..Response::default()
            },
            Vec::new(),
        ),
    }
}

fn write_response<W: Write>(out: &mut W, resp: &Response, payload: &[u8]) -> std::io::Result<()> {
    let stamped = Response {
        protocol: PROTOCOL_VERSION,
        ..resp.clone()
    };
    let body = serde_json::to_vec(&stamped).unwrap_or_else(|_| b"{\"ok\":false}".to_vec());
    write_frame(out, &body)?;
    if !payload.is_empty() {
        out.write_all(payload)?;
        out.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stale_worker_is_rejected_loudly_instead_of_silently_wrong() {
        /*
         * 2026-09-24 真机事故：主程序是新的、Windows 那个 worker 是旧的，
         * 于是编辑器的线性解码一直失败 —— 而界面只表现为「卡在正在载入照片」。
         * 现在旧 worker 的响应（没有 protocol 字段 = 0）必须**当面报错**，
         * 而且错误里要写清怎么修。
         */
        let old: Response =
            serde_json::from_slice(br#"{"ok":true,"width":4,"height":4,"payload_len":48}"#)
                .expect("旧协议的头能解出来");
        assert_eq!(old.protocol, 0, "旧 worker 不发这个字段，解出来就是 0");
        assert_ne!(old.protocol, PROTOCOL_VERSION, "0 必须被判成过期");

        let message = protocol_mismatch(old.protocol);
        assert!(message.contains("raybend-raw-worker"), "要说清是哪个文件过期：{message}");
        assert!(
            message.contains("cargo build -p raybend --bin raybend-raw-worker"),
            "要给一条能照抄的命令：{message}"
        );

        // 同一版本的响应要能过
        let fresh: Response = serde_json::from_slice(
            format!(r#"{{"ok":true,"protocol":{PROTOCOL_VERSION}}}"#).as_bytes(),
        )
        .expect("新协议的头能解出来");
        assert_eq!(fresh.protocol, PROTOCOL_VERSION);
    }

    #[test]
    fn the_version_is_on_the_wire_in_both_directions() {
        // 请求：客户端盖章（`exchange` 那一处调的就是这个函数）
        let raw = Request {
            op: "decode".to_string(),
            path: "a.rw2".to_string(),
            format: Some("linear16".to_string()),
            ..Request::default()
        };
        assert_eq!(raw.protocol, 0, "构造点不管版本，盖章是统一那一步的事");
        let wire = stamp(&raw);
        let json = serde_json::to_value(&wire).expect("能序列化");
        assert_eq!(json["protocol"], PROTOCOL_VERSION);
        assert_eq!(json["format"], "linear16", "像素形态照旧在线上");
        assert_eq!(raw.protocol, 0, "盖章不许改动原来那一份");

        // 响应：worker 盖章（`write_response` 里那一处）—— 盖过之后必须能被解回来
        let mut buf = Vec::new();
        write_response(
            &mut buf,
            &Response {
                ok: true,
                width: 2,
                height: 2,
                payload_len: 8, // 头里说有多少字节，读的那边才会去读
                ..Response::default()
            },
            &[0u8; 8],
        )
        .expect("写得出去");
        let mut reader = BufReader::new(std::io::Cursor::new(buf));
        let (resp, payload) = read_response(&mut reader).expect("读得回来");
        assert_eq!(resp.protocol, PROTOCOL_VERSION, "worker 的响应必须带版本");
        assert_eq!(payload.len(), 8);
    }

    #[test]
    fn resolve_worker_never_fails_in_a_dev_tree() {
        // 解析必须成功（要么找到专用 bin，要么兜底成「自己 + 标记」）。
        //
        // ⚠️ **真正的 spawn 测试在 `tests/raw_worker.rs`**（集成测试）——
        // 那里 cargo 保证 `raybend-raw-worker` 已经被构建出来，
        // 而 `cargo test --lib` 并不保证 bin 已存在（bin 是按需构建的）。
        let (path, needs_marker) = resolve_worker().expect("必须能解析到 worker 入口");
        assert!(path.is_absolute(), "路径应当是绝对的：{}", path.display());
        if needs_marker {
            assert_eq!(path, std::env::current_exe().unwrap());
        }
    }

    #[test]
    fn shutdown_is_idempotent() {
        let mut worker = RawWorker::new();
        worker.shutdown();
        worker.shutdown();
        assert!(!worker.is_running());
    }

    #[test]
    fn frame_round_trip_handles_the_wire_format() {
        let mut buf: Vec<u8> = Vec::new();
        write_frame(&mut buf, b"{\"a\":1}").unwrap();
        let mut cursor = std::io::Cursor::new(buf);
        let mut reader = BufReader::new(&mut cursor);
        assert_eq!(read_frame(&mut reader).unwrap(), b"{\"a\":1}".to_vec());
    }

    #[test]
    fn oversized_header_is_rejected_not_allocated() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&(MAX_HEADER_BYTES + 1).to_le_bytes());
        let mut reader = BufReader::new(std::io::Cursor::new(buf));
        match read_frame(&mut reader) {
            Err(WorkerError::Protocol(msg)) => assert!(msg.contains("过长"), "{msg}"),
            other => panic!("应当拒绝过长的头：{other:?}"),
        }
    }

    #[test]
    fn truncated_frame_is_a_crash_not_a_hang() {
        // 声明 100 字节但只给 10 字节 —— 必须报错而不是死等
        let mut buf = Vec::new();
        buf.extend_from_slice(&100u32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 10]);
        let mut reader = BufReader::new(std::io::Cursor::new(buf));
        assert!(matches!(
            read_frame(&mut reader),
            Err(WorkerError::Crashed(_))
        ));
    }

    #[test]
    fn needs_respawn_classification() {
        assert!(WorkerError::Timeout(1).needs_respawn());
        assert!(WorkerError::Crashed("x".into()).needs_respawn());
        assert!(WorkerError::Protocol("x".into()).needs_respawn());
        assert!(!WorkerError::Decode("x".into()).needs_respawn());
        assert!(!WorkerError::Spawn("x".into()).needs_respawn());
    }
}
