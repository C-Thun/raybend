//! 编辑视口：**洞口事实**（M3-W1 的契约层）+ **渲染线程**（M3-W2）。
//!
//! ```text
//!   前端（CSS 矩形 + 运行时 DPR + CSS 视口尺寸 + 洞口底色）
//!            │  editor_set_viewport          ┌── editor_bind_renderer（懒启动）
//!            ▼                               ▼
//!   EditorState（进程级）  ──转发──▶  渲染线程（监督器 + 会话循环）
//!            │                               │  意图：zoomAt / pan / fit / reset / hitTest
//!            │  editor_viewport_state         │  照片：editor_set_photo → 解码线程 → 纹理
//!            ▼                               ▼
//!   回读（诊断：CSS 与物理两份都带回去）    RenderState（快照，前端 250ms 轮询一次）
//! ```
//!
//! # 这一层只做三件事
//!
//! 1. **收下前端报的原始事实**（不猜 DPR、不补偏移、只乘一次 DPR）—— §7.9 的全部教训；
//! 2. **把意图转给渲染线程**（前端只发意图，坐标数学在 `raybend::render::viewport`）；
//! 3. **把渲染线程的状态如实报回去**（含重启次数与当前错误 —— 「静默冻住」是禁止的）。
//!
//! # 为什么要有渲染线程（而不是在命令里画一帧就完事）
//!
//! * 每帧的 `submit/present` 不能被 IPC 线程的节奏绑住；
//! * 空闲时线程阻塞在通道上，**一帧都不画**（CPU 0，笔记本不掉电）；
//! * 设备丢失 / panic 的重建需要「一个长期存在的循环」才能兜住（监督器）。
//!
//! # 三条硬规矩（都是从真机事故里来的）
//!
//! * **渲染线程零窗口查询**：窗口尺寸只由主线程的窗口事件推过来（`AGENTS.md` §7.9；
//!   拖动时的窗口查询会让渲染线程永久阻塞）；
//! * **layout / buffer / texture 都记着自己属于哪个设备**：设备重建必须整套重建
//!   （`GpuContext::recover` 是唯一入口）；
//! * **panic 必须被 `catch_unwind` 接住**并重启 + 上报（spike 那次「界面照旧响应、图永远冻住」
//!   就是没接住）。

use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};

use raybend::display::{self, PixelSize};
use raybend::render::{
    FitMode, GpuContext, ImageTier, RenderImage, RenderOutcome, RestartPolicy, Verdict, tier_for,
};

use crate::MAIN_WINDOW_LABEL;
use crate::render_window::{client_size, raw_handles};

/* ══════════════════════════════════════════════════════════════
 * 一、洞口契约（M3-W1；前端**只报原始事实**）
 * ══════════════════════════════════════════════════════════════ */

/// 一个矩形（**单位由字段名标明**：`hole_css` 是 CSS 像素，`hole_physical` 是物理像素）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct RectDto {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl RectDto {
    /// 每个数都是有限数、且宽高非负。
    ///
    /// 不做「取绝对值」「夹到 0」这类补救：非法值说明上游算错了，
    /// 静默修好只会把错误带到下一层（§7.9：静默回退比报错可怕）。
    fn is_valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width >= 0.0
            && self.height >= 0.0
    }
}

/// 一个尺寸（CSS 像素）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct SizeDto {
    pub width: f64,
    pub height: f64,
}

impl SizeDto {
    fn is_valid(self) -> bool {
        self.width.is_finite() && self.height.is_finite() && self.width >= 0.0 && self.height >= 0.0
    }
}

/// 一个点（**图像像素**；命中测试的结果）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PointDto {
    pub x: f32,
    pub y: f32,
}

/// 洞口底色（**sRGB 8bit** 的三个通道；线上是数字，不是字符串）。
///
/// 为什么由前端报：它是**主题相关**的（深色/浅色两套令牌），Rust 不该认识主题；
/// 而「洞口里照片没盖住的地方」必须与界面底色一致，否则会出现一条突兀的色带。
/// 前端报的是**事实**（浏览器算出来的颜色），不是意图 —— 符合 §6.1 红线 2。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorDto {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl From<raybend::render::Srgb8> for ColorDto {
    fn from(color: raybend::render::Srgb8) -> Self {
        Self {
            r: f64::from(color.r),
            g: f64::from(color.g),
            b: f64::from(color.b),
        }
    }
}

impl Default for ColorDto {
    fn default() -> Self {
        Self::from(raybend::render::Srgb8::DARK_SURFACE_BAR)
    }
}

impl ColorDto {
    /// → 洞口底色（`Srgb8`）。
    ///
    /// 换算在渲染 crate 里（`Srgb8::to_clear_color`）—— 见那里的说明：
    /// 「wgpu 在 sRGB 目标上把清屏值当已编码值」这条约定只该有一个答案。
    #[must_use]
    pub fn to_srgb8(self) -> raybend::render::Srgb8 {
        let clamp = |value: f64| value.round().clamp(0.0, 255.0) as u8;
        raybend::render::Srgb8 {
            r: clamp(self.r),
            g: clamp(self.g),
            b: clamp(self.b),
        }
    }
}

/// 前端 `editor_set_viewport` 的一次载荷（**原始事实**，见模块文档）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetViewportArgs {
    pub hole: RectDto,
    /// WebView 的 `devicePixelRatio`：**含显示器 DPI + 系统文字缩放 + 页面缩放**。
    /// 不许拿 Tauri 的 `scale_factor()` 顶替（它不含文字缩放，§7.9 铁律 3）。
    pub dpr: f64,
    pub viewport: SizeDto,
    /// 洞口底色 —— **前端把 `getComputedStyle` 的字符串原样报上来**，解析在 Rust。
    ///
    /// 为什么不叫前端解析成三个数：解析是「解释一段 CSS」的活（两套主题、简写形式、
    /// 透明度……），放在前端就会多出一份「颜色是什么」的知识；而颜色怎么到 GPU 上
    /// （sRGB 清屏约定）本来就在 Rust。缺省 / 解不开时用深色主题的 `--surface-bar` 兜底，
    /// 并在状态里把「用的是报上来的还是兜底的」如实标出。
    #[serde(default)]
    pub backdrop: Option<String>,
}

/// 校验后的事实（**只有合法的值才进得来**）。
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedViewport {
    pub hole: RectDto,
    pub dpr: f64,
    pub viewport: SizeDto,
    /// 实际使用的洞口底色（已解析；解不开就是兜底色）
    pub backdrop: raybend::render::Srgb8,
    /// 底色是不是**前端报上来的**（`false` = 用了兜底色 —— 状态里要能看出来）
    pub backdrop_reported: bool,
}

/// 校验。纯函数，边界都在测试里钉住。
///
/// # Errors
/// 任一数字不是有限数、宽高为负、`dpr <= 0`（它是所有 CSS→物理换算的分母）、
/// 或颜色不在 0..=255。
pub fn validate(args: &SetViewportArgs) -> Result<ValidatedViewport, String> {
    if !args.hole.is_valid() {
        return Err(format!("洞口矩形非法：{:?}", args.hole));
    }
    if !args.viewport.is_valid() {
        return Err(format!("视口尺寸非法：{:?}", args.viewport));
    }
    if !args.dpr.is_finite() || args.dpr <= 0.0 {
        return Err(format!("DPR 必须是有限正数，收到 {}", args.dpr));
    }
    // 底色：**解不开不是错误**（不因此把整条上报打回），但也不静默 ——
    // 用兜底色，并在状态里把「这次用的是兜底的」如实标出来
    let parsed = args
        .backdrop
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .and_then(raybend::render::Srgb8::parse_css);
    Ok(ValidatedViewport {
        hole: args.hole,
        dpr: args.dpr,
        viewport: args.viewport,
        backdrop: parsed.unwrap_or(raybend::render::Srgb8::DARK_SURFACE_BAR),
        backdrop_reported: parsed.is_some(),
    })
}

/// CSS 矩形 → 物理像素矩形（只有这一步乘 DPR，别处都不许再乘）。
#[must_use]
pub fn physical_rect(hole: RectDto, dpr: f64) -> RectDto {
    RectDto {
        x: hole.x * dpr,
        y: hole.y * dpr,
        width: hole.width * dpr,
        height: hole.height * dpr,
    }
}

/// 收到的事实（存起来的那一份）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StoredViewport {
    pub hole_css: Option<RectDto>,
    pub dpr: f64,
    pub viewport: SizeDto,
    /// 实际使用的洞口底色
    pub backdrop: raybend::render::Srgb8,
    /// 底色是不是前端报上来的（`false` = 兜底色）
    pub backdrop_reported: bool,
    /// 收到过几次（诊断：一直不涨 = 前端根本没报）
    pub updates: u64,
}

impl Default for StoredViewport {
    fn default() -> Self {
        Self {
            hole_css: None,
            dpr: 0.0,
            viewport: SizeDto::default(),
            backdrop: raybend::render::Srgb8::DARK_SURFACE_BAR,
            backdrop_reported: false,
            updates: 0,
        }
    }
}

impl StoredViewport {
    /// 物理像素洞口（还没收到过就是 `None`）。
    #[must_use]
    pub fn hole_physical(&self) -> Option<RectDto> {
        self.hole_css.map(|hole| physical_rect(hole, self.dpr))
    }

    /// 存下一次上报。
    pub fn apply(&mut self, next: ValidatedViewport) {
        self.hole_css = Some(next.hole);
        self.dpr = next.dpr;
        self.viewport = next.viewport;
        self.backdrop = next.backdrop;
        self.backdrop_reported = next.backdrop_reported;
        self.updates = self.updates.saturating_add(1);
    }
}

/// 回给前端的整块**洞口**状态（W1 的契约 + M3-W2 的底色）。
#[derive(Debug, Clone, Copy, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ViewportStateDto {
    pub hole_css: Option<RectDto>,
    pub hole_physical: Option<RectDto>,
    pub dpr: f64,
    pub viewport_css: SizeDto,
    /// 实际使用的洞口底色
    pub backdrop: ColorDto,
    /// 底色是不是前端报上来的（`false` = 兜底色 —— 诊断时一眼看出「主题色没报上来」）
    pub backdrop_reported: bool,
    pub updates: u64,
}

impl StoredViewport {
    /// 快照（回读命令用）。
    #[must_use]
    pub fn snapshot(&self) -> ViewportStateDto {
        ViewportStateDto {
            hole_css: self.hole_css,
            hole_physical: self.hole_physical(),
            dpr: self.dpr,
            viewport_css: self.viewport,
            backdrop: self.backdrop.into(),
            backdrop_reported: self.backdrop_reported,
            updates: self.updates,
        }
    }
}

/* ══════════════════════════════════════════════════════════════
 * 二、视口意图（前端只发意图，坐标数学在渲染 crate）
 * ══════════════════════════════════════════════════════════════ */

/// 前端发来的视口意图。
///
/// ⚠️ **字段名是契约**：`rename_all` 只管变体名，字段要 `rename_all_fields`
/// （`docs/native-viewport-coordinate-guide.md` §6 第 2 条：缺 DPR 必须报错，
/// 字段名不许靠猜）。这一条有单测钉着。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ViewportIntent {
    /// 以某个 **CSS 窗口坐标**（`clientX/clientY`）为锚点缩放。
    ZoomAt { x: f32, y: f32, factor: f32 },
    /// 以**洞口中心**为锚点缩放（按钮 / 快捷键 —— 与「滚轮以光标为锚点」区分开）。
    ///
    /// 前端不自己算中心：中心在哪只有 Rust 知道（洞口是它存的），
    /// 让前端去减洞口原点再加一半，就是又把坐标数学搬回了前端。
    ZoomBy { factor: f32 },
    /// 平移（CSS 像素位移）。
    Pan { dx: f32, dy: f32 },
    /// 档位：`fit` / `fill` / `oneToOne` / `free`。
    Fit { mode: String },
    /// **适合窗口 ↔ 1:1**（双击 / `0` 键；与看图件同一条语义）。
    ///
    /// 为什么做成一个意图而不是前端先读状态再决定：状态是 250ms 轮询来的，
    /// 而「现在是哪一档」的事实住在 Rust —— 让它自己判，不存在「双击后反而跳远」的窗口期。
    ToggleFit,
    /// 复位：适合窗口 + 零旋转 + 零平移。
    Reset,
    /// 命中测试：把 CSS 坐标换算成图像像素（**不动视口**；W5 的三个工具要用）。
    HitTest { x: f32, y: f32 },
}

/// 档位名 → [`FitMode`]（大小写不敏感；不认识的**报错**，不静默回退）。
///
/// # Errors
/// 名字不在四个之内。
pub fn parse_fit_mode(mode: &str) -> Result<FitMode, String> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "fit" => Ok(FitMode::Fit),
        "fill" => Ok(FitMode::Fill),
        "onetoone" => Ok(FitMode::OneToOne),
        "free" => Ok(FitMode::Free),
        other => Err(format!("未知的档位名：{other}")),
    }
}

/// 档位 → 线上一词（诊断显示）。
#[must_use]
pub fn fit_mode_name(mode: FitMode) -> &'static str {
    match mode {
        FitMode::Fit => "fit",
        FitMode::Fill => "fill",
        FitMode::OneToOne => "oneToOne",
        FitMode::Free => "free",
    }
}

/* ══════════════════════════════════════════════════════════════
 * 三、渲染线程
 * ══════════════════════════════════════════════════════════════ */

/// 会话稳定的门槛：活过这么久才算「一次长跑」，重启预算清零（见 `RestartPolicy`）。
const STABLE_AFTER: Duration = Duration::from_secs(5);

/// 连续渲染错误到几次就交给监督器重建（真机事故里那些错误是「一套坏状态」，
/// 重建整套资源才会好 —— 干等着只会一直报同一句）。
const MAX_CONSECUTIVE_RENDER_ERRORS: u32 = 5;

/// 渲染线程的命令（**一个通道**：前端意图 + 窗口事件 + 解码结果）。
///
/// 为什么合成一个通道：渲染线程只需要一个「醒来」的理由，醒来之后把攒下的命令
/// 全部吃掉再画一帧（不做「一条命令一帧」）。多通道就要 `select`，那是白送的复杂度。
enum RenderCommand {
    /// 洞口事实（含 DPR 与底色）
    Viewport(SetViewportArgs),
    /// 窗口客户区尺寸变化（物理像素；**主线程推来**，渲染线程不查窗口）
    Resize { width: u32, height: u32 },
    /// 换照片 / 清空照片
    SetPhoto { path: Option<String> },
    /// 视口意图
    Intent(ViewportIntent),
    /// 解码完了一张
    Decoded(DecodeOutcome),
    /// 结束会话（离开编辑器）
    Stop,
}

/// 一次解码任务（渲染线程 → 解码线程）。
struct DecodeJob {
    /// 单调递增的任务号：**只有最新那个的结果有效**（换照片/换档位会把旧的作废）
    id: u64,
    path: String,
    tier: ImageTier,
}

/// 解码线程 → 渲染线程。
struct DecodeOutcome {
    id: u64,
    path: String,
    tier: ImageTier,
    /// 像素从哪来（`bitmap` / `raw-embedded-preview` / `raw-decoded`）
    origin: String,
    result: Result<RenderImage, String>,
}

/// 渲染线程的共享状态 —— **前端轮询读的就是它**（序列化后直接回给前端）。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderState {
    /// 会话在（线程活着）
    pub bound: bool,
    /// 渲染器建起来了（surface + 设备齐了）
    pub ready: bool,
    /// 适配器与后端（诊断：真机上一眼看出跑的是 DX12 还是 Vulkan）
    pub adapter: String,
    /// surface 的物理像素尺寸
    pub surface: SizeDto,
    /// 渲染器正在用的 DPR（WebView 的，不是 native scale）
    pub dpr: f64,
    /// 洞口（CSS 与物理两份，一比就知道单位对不对）
    pub hole_css: Option<RectDto>,
    pub hole_physical: Option<RectDto>,
    /// 洞口底色
    pub backdrop: ColorDto,
    /// 当前**装上纹理**的那张照片
    pub photo_path: Option<String>,
    /// **真的画出来过**的那张 —— 前端据此把洞口那条 DOM 链切成透明
    pub painted_path: Option<String>,
    /// 图像尺寸（图像像素）
    pub image: Option<SizeDto>,
    /// 当前档位（`preview` / `full`）
    pub tier: Option<ImageTier>,
    /// 正在取的档位（不一样就说明解码还没回来）
    pub wanted_tier: Option<ImageTier>,
    /// 像素从哪来（RAW 内嵌预览 / 完整解码 / 位图）
    pub origin: Option<String>,
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub fit_mode: String,
    pub rotation: f32,
    /// 画过多少帧（0 = 一帧都没出）
    pub drawn_frames: u64,
    /// 监督器重启过几次（**>0 就是「崩过但爬起来了」**）
    pub restarts: u32,
    /// 解码状态：`idle` / `loading` / `ready` / `error`
    pub decode: String,
    /// 解码失败的说明
    pub decode_error: Option<String>,
    /// 最近一次命中测试的结果（图像像素）
    pub last_hit: Option<PointDto>,
    /// **当前**错误（恢复出图后清掉 —— spike 的「红字永远挂着」教训）
    pub last_error: Option<String>,
    /// 近期事件（重启 / 设备丢失 / 恢复；只留最近几条）
    pub history: Vec<String>,
}

/// 事件历史的上限（界面上只看最近几条，别把内存当日志用）。
const HISTORY_LIMIT: usize = 12;

impl RenderState {
    fn note(&mut self, message: impl Into<String>) {
        let message = message.into();
        eprintln!("[editor] {message}");
        self.history.push(message);
        if self.history.len() > HISTORY_LIMIT {
            let overflow = self.history.len() - HISTORY_LIMIT;
            self.history.drain(0..overflow);
        }
    }
}

/// 渲染会话（挂在 `EditorState` 上）。
struct Session {
    commands: Sender<RenderCommand>,
    shared: Arc<Mutex<RenderState>>,
    thread: std::thread::JoinHandle<()>,
}

/// 挂在 Tauri 状态上的编辑器状态：洞口事实 + 渲染会话 + 窗口事件转发槽。
#[derive(Default)]
pub struct EditorState {
    viewport: Mutex<StoredViewport>,
    session: Mutex<Option<Session>>,
    /// 窗口事件 → 渲染线程的转发槽。
    ///
    /// 为什么单独一个 `Arc`：窗口事件回调在**主线程**上跑，它只许做「塞一个纯数据」；
    /// 渲染线程**不碰**这个槽 ⇒ 两边永远不互相等（spike 的 `pending_facts` 那条教训）。
    /// 窗口事件的注册在 Tauri 里**没有反注册 API**，所以进程里只装一次，
    /// 靠这个槽的「当前会话在不在」来决定事件往哪去。
    sink: Arc<Mutex<Option<Sender<RenderCommand>>>>,
    /// 窗口事件是否已经装过（装过就不再装 —— 否则进出编辑器一次就多一份回调）
    events_installed: Arc<AtomicBool>,
}

impl EditorState {
    /// 取一份洞口快照（**唯一读法**）。
    ///
    /// # Errors
    /// 锁中毒（有线程在持锁时 panic）时返回错误字符串，而不是 `unwrap` 让命令层 panic。
    pub fn snapshot(&self) -> Result<ViewportStateDto, String> {
        let guard = self
            .viewport
            .lock()
            .map_err(|_| "编辑器视口状态锁中毒（有线程持锁时崩过）".to_string())?;
        Ok(guard.snapshot())
    }
}

/// 锁共享状态；**中毒也照读**。
///
/// 渲染线程 panic 时可能正握着这把锁 —— 但里面装的是「给界面看的展示状态」，
/// 宁可读到半旧的一份，也不能让整个编辑器因为锁中毒而彻底不可用
/// （真机事故里「图冻住但界面照旧响应」已经够糟了，不该再叠一层「命令全报错」）。
fn lock_state(shared: &Arc<Mutex<RenderState>>) -> MutexGuard<'_, RenderState> {
    match shared.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn session_sender(state: &EditorState) -> Option<Sender<RenderCommand>> {
    state.sink.lock().ok().and_then(|slot| slot.clone())
}

/* ── 命令层 ─────────────────────────────────────────────── */

/// 起渲染线程（幂等：已经起着就直接回状态）。
///
/// 前端在编辑器**挂载**时调它；离开时调 [`editor_unbind_renderer`]。
/// 懒启动的好处：没进编辑器就一份 GPU 资源都不占，而且渲染线程起不来也**只影响编辑器**
/// （其余工作流照样跑）。
#[tauri::command]
pub async fn editor_bind_renderer<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, EditorState>,
) -> Result<RenderState, String> {
    // 已经起着就用现有的（**除非那条线程已经退出了** —— 监督器放弃之后线程就没了，
    // 这时必须允许重新起一个，否则「重试」会永远回同一份死状态）
    {
        let mut guard = state
            .session
            .lock()
            .map_err(|_| "编辑器会话锁中毒".to_string())?;
        if let Some(session) = guard.as_ref()
            && !session.thread.is_finished()
        {
            drop(guard);
            return current_render_state(&state);
        }
        if let Some(dead) = guard.take() {
            let _ = dead.thread.join();
        }
    }

    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| format!("找不到主窗口（label = {MAIN_WINDOW_LABEL}）"))?;
    let size = client_size(&window);

    let (commands, receiver) = channel::<RenderCommand>();
    let shared = Arc::new(Mutex::new(RenderState {
        bound: true,
        surface: SizeDto {
            width: size.0 as f64,
            height: size.1 as f64,
        },
        decode: "idle".to_string(),
        fit_mode: fit_mode_name(FitMode::Fit).to_string(),
        ..Default::default()
    }));

    install_window_events(&state, &window);
    *state
        .sink
        .lock()
        .map_err(|_| "渲染通道槽锁中毒".to_string())? = Some(commands.clone());

    // 初值：主线程上取一次窗口事实（**渲染线程零窗口查询**，所以初值在这里取好交给它）
    let initial = InitialFacts {
        width: size.0,
        height: size.1,
    };
    let initial_dpr = {
        let stored = state
            .viewport
            .lock()
            .map_err(|_| "编辑器视口状态锁中毒".to_string())?;
        if stored.dpr > 0.0 { stored.dpr } else { 1.0 }
    };
    let thread_commands = commands.clone();
    let thread_shared = shared.clone();
    let handle = std::thread::Builder::new()
        .name("editor-render".to_string())
        .spawn(move || {
            supervisor_loop(
                window,
                receiver,
                thread_commands,
                thread_shared,
                initial,
                initial_dpr,
            )
        })
        .map_err(|error| format!("起渲染线程失败：{error}"))?;

    *state
        .session
        .lock()
        .map_err(|_| "编辑器会话锁中毒".to_string())? = Some(Session {
        commands,
        shared: shared.clone(),
        thread: handle,
    });

    Ok(lock_state(&shared).clone())
}

/// 停渲染线程（幂等）。离开编辑器时调；线程退出前会把最后的状态写回去。
#[tauri::command]
pub async fn editor_unbind_renderer(state: State<'_, EditorState>) -> Result<(), String> {
    if let Ok(mut slot) = state.sink.lock() {
        *slot = None;
    }
    let session = state
        .session
        .lock()
        .map_err(|_| "编辑器会话锁中毒".to_string())?
        .take();
    if let Some(session) = session {
        let _ = session.commands.send(RenderCommand::Stop);
        // 线程退出很快（最多等一次 render 的时间）；join 失败不该让命令报错
        let _ = session.thread.join();
    }
    Ok(())
}

/// 上报洞口事实（`EDITOR.md` 的洞口契约；前端**只报原始值**）。
///
/// 同时转给渲染线程（起着的话）—— **两个消费方一份事实**，不存在「读回的是这个、
/// 渲染用的是那个」的漂移。
#[tauri::command]
pub fn editor_set_viewport(
    state: State<'_, EditorState>,
    hole: RectDto,
    dpr: f64,
    viewport: SizeDto,
    backdrop: Option<String>,
) -> Result<ViewportStateDto, String> {
    let args = SetViewportArgs {
        hole,
        dpr,
        viewport,
        backdrop,
    };
    let validated = validate(&args)?;
    let snapshot = {
        let mut guard = state
            .viewport
            .lock()
            .map_err(|_| "编辑器视口状态锁中毒".to_string())?;
        guard.apply(validated);
        guard.snapshot()
    };
    if let Some(sender) = session_sender(&state) {
        let _ = sender.send(RenderCommand::Viewport(args));
    }
    Ok(snapshot)
}

/// 回读 Rust 手里的洞口事实（诊断 / 冒烟；W1 契约不变）。
#[tauri::command]
pub fn editor_viewport_state(state: State<'_, EditorState>) -> Result<ViewportStateDto, String> {
    state.snapshot()
}

/// 换照片（`None` = 清空）。前端在锚点变化时调。
#[tauri::command]
pub fn editor_set_photo(
    state: State<'_, EditorState>,
    path: Option<String>,
) -> Result<RenderState, String> {
    let sender = session_sender(&state).ok_or_else(|| "渲染线程还没起来".to_string())?;
    let _ = sender.send(RenderCommand::SetPhoto { path });
    current_render_state(&state)
}

/// 发一条视口意图（前端**不做坐标数学**，只把看到的原始值报过来）。
#[tauri::command]
pub fn editor_viewport_intent(
    state: State<'_, EditorState>,
    intent: ViewportIntent,
) -> Result<RenderState, String> {
    let sender = session_sender(&state).ok_or_else(|| "渲染线程还没起来".to_string())?;
    let _ = sender.send(RenderCommand::Intent(intent));
    current_render_state(&state)
}

/// 读渲染线程的状态（前端每 250ms 一次：既是握手也是**上报通道**）。
#[tauri::command]
pub fn editor_render_state(state: State<'_, EditorState>) -> Result<RenderState, String> {
    current_render_state(&state)
}

fn current_render_state(state: &EditorState) -> Result<RenderState, String> {
    let guard = state
        .session
        .lock()
        .map_err(|_| "编辑器会话锁中毒".to_string())?;
    match guard.as_ref() {
        Some(session) => Ok(lock_state(&session.shared).clone()),
        None => Ok(RenderState {
            decode: "idle".to_string(),
            fit_mode: fit_mode_name(FitMode::Fit).to_string(),
            ..Default::default()
        }),
    }
}

/* ── 窗口事件（主线程 → 渲染线程；装一次）────────────────── */

fn install_window_events<R: Runtime>(state: &EditorState, window: &WebviewWindow<R>) {
    if state.events_installed.swap(true, Ordering::SeqCst) {
        return;
    }
    let sink = state.sink.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Resized(size) = event {
            // 回调在主线程上：只做「塞一个纯数据」，绝不渲染、绝不锁共享状态
            let sender = sink.lock().ok().and_then(|slot| slot.clone());
            if let Some(sender) = sender {
                let _ = sender.send(RenderCommand::Resize {
                    width: size.width.max(1),
                    height: size.height.max(1),
                });
            }
        }
    });
}

/* ── 渲染线程本体 ───────────────────────────────────────── */

#[derive(Debug, Clone, Copy)]
struct InitialFacts {
    width: u32,
    height: u32,
}

/// 一轮会话的结局。
enum SessionExit {
    /// 收到 `Stop`（离开编辑器）
    Stopped,
    /// 失败（交给监督器决定重不重来）
    Failed(String),
}

/// **监督器**：一轮失败（建不起来 / panic / 恢复失败）就按策略再来一轮。
///
/// 三件事共用一套策略 —— 启动失败、线程 panic、设备恢复失败。
/// `catch_unwind` 是这里的重点：spike 那次「`create_bind_group` panic 打死渲染线程、
/// 界面照旧响应、图永远冻住」就是缺了它。
fn supervisor_loop<R: Runtime>(
    window: WebviewWindow<R>,
    receiver: Receiver<RenderCommand>,
    commands: Sender<RenderCommand>,
    shared: Arc<Mutex<RenderState>>,
    initial: InitialFacts,
    dpr: f64,
) {
    let mut policy = RestartPolicy::default();
    loop {
        let started = Instant::now();
        let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
            run_session(&window, &receiver, &commands, &shared, initial, dpr)
        }));

        let failure = match outcome {
            Ok(SessionExit::Stopped) => return,
            Ok(SessionExit::Failed(message)) => message,
            Err(payload) => format!("渲染线程 panic：{}", panic_message(&payload)),
        };

        let was_stable = started.elapsed() >= STABLE_AFTER;
        if was_stable {
            policy.on_stable();
        }

        let verdict = policy.on_failure();
        let attempt = policy.attempts();
        if let Ok(mut state) = shared.lock() {
            state.restarts = attempt;
            state.ready = false;
            state.painted_path = None;
            state.last_error = Some(failure.clone());
            state.note(format!("第 {attempt} 次重启：{failure}"));
        }
        match verdict {
            Verdict::Retry { delay, .. } => {
                std::thread::sleep(delay);
            }
            Verdict::GiveUp => {
                if let Ok(mut state) = shared.lock() {
                    state.last_error = Some(format!(
                        "渲染线程连续失败 {attempt} 次，已放弃：{failure}（界面退回 DOM 显示）"
                    ));
                    state.note("监督器放弃：渲染线程不再重启");
                }
                return;
            }
        }
    }
}

/// 从 panic 载荷里抠出一句人话。
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        return (*text).to_string();
    }
    if let Some(text) = payload.downcast_ref::<String>() {
        return text.clone();
    }
    "（没有文本信息的 panic）".to_string()
}

/// 一轮会话：建上下文 → 跑循环，直到 `Stop` / 失败。
fn run_session<R: Runtime>(
    window: &WebviewWindow<R>,
    receiver: &Receiver<RenderCommand>,
    commands: &Sender<RenderCommand>,
    shared: &Arc<Mutex<RenderState>>,
    initial: InitialFacts,
    dpr: f64,
) -> SessionExit {
    let handles = match raw_handles(window) {
        Ok(handles) => handles,
        Err(error) => return SessionExit::Failed(error),
    };
    // 还没有照片：先只出洞口底色（`has_image = false`）
    let mut context = match GpuContext::new(
        handles,
        (initial.width, initial.height),
        dpr as f32,
        None,
        "editor",
    ) {
        Ok(context) => context,
        Err(error) => return SessionExit::Failed(error.to_string()),
    };

    let adapter = context.adapter_info();
    let details = context.surface_details();
    {
        let mut state = lock_state(shared);
        state.ready = true;
        state.adapter = format!("{} · {}", adapter.name, adapter.backend);
        state.surface = SizeDto {
            width: details.size.0 as f64,
            height: details.size.1 as f64,
        };
        state.last_error = None;
        state.note(format!("渲染器就绪（{} / {}）", adapter.backend, details.format));
    }

    // 解码线程：与这一轮会话同生共死（Stop 之后由通道断开自然退出）
    let (decode_sender, decode_receiver) = channel::<DecodeJob>();
    let decode_commands = commands.clone();
    let max_texture = context.max_texture_dimension_2d();
    let decoder = std::thread::Builder::new()
        .name("editor-decode".to_string())
        .spawn(move || decode_loop(decode_receiver, decode_commands, max_texture));

    let exit = session_loop(&mut context, receiver, shared, &decode_sender);
    drop(decode_sender); // 让解码线程看到通道关闭
    if let Ok(handle) = decoder {
        let _ = handle.join();
    }
    exit
}

/// 会话主循环：吃命令 → 画一帧 → 报状态。
fn session_loop(
    context: &mut GpuContext,
    receiver: &Receiver<RenderCommand>,
    shared: &Arc<Mutex<RenderState>>,
    decoder: &Sender<DecodeJob>,
) -> SessionExit {
    let mut dirty = true;
    let mut latest_job: u64 = 0;
    let mut consecutive_errors: u32 = 0;

    loop {
        match receiver.recv_timeout(Duration::from_millis(200)) {
            Ok(command) => {
                if let Err(error) = apply_command(
                    command,
                    context,
                    shared,
                    decoder,
                    &mut latest_job,
                    &mut dirty,
                )
                    && let Ok(mut state) = shared.lock() {
                        state.last_error = Some(error.clone());
                        state.note(format!("命令被拒：{error}"));
                    }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return SessionExit::Stopped,
        }

        // 把攒下的命令全部吃掉（**一帧最多画一次**）
        loop {
            match receiver.try_recv() {
                Ok(RenderCommand::Stop) | Err(TryRecvError::Disconnected) => {
                    return SessionExit::Stopped;
                }
                Ok(command) => {
                    if let Err(error) = apply_command(
                        command,
                        context,
                        shared,
                        decoder,
                        &mut latest_job,
                        &mut dirty,
                    ) && let Ok(mut state) = shared.lock()
                    {
                        state.last_error = Some(error.clone());
                    }
                }
                Err(TryRecvError::Empty) => break,
            }
        }

        if !dirty {
            continue;
        }
        dirty = false;

        match context.render() {
            Ok(RenderOutcome::Drawn) => {
                consecutive_errors = 0;
                let mut state = lock_state(shared);
                if state.photo_path.is_some() {
                    state.painted_path = state.photo_path.clone();
                }
                // 出图正常 ⇒ 清掉当前错误（spike 的「红字永远挂着」教训）
                if state.last_error.take().is_some() {
                    state.note("已恢复：重新出图");
                }
                publish(&mut state, context);
            }
            Ok(RenderOutcome::Skipped) => {
                let mut state = lock_state(shared);
                publish(&mut state, context);
            }
            Ok(RenderOutcome::Reconfigured(reason)) => {
                if let Ok(mut state) = shared.lock() {
                    state.note(format!("surface：{reason}"));
                }
                dirty = true; // 下一轮再画一帧
            }
            Err(error) => {
                consecutive_errors += 1;
                let lost = context.drain_device_lost();
                if let Ok(mut state) = shared.lock() {
                    state.last_error = Some(error.to_string());
                    state.painted_path = None;
                    state.note(format!("渲染错误（第 {consecutive_errors} 次）：{error}"));
                    for entry in &lost {
                        state.note(format!("设备丢失记录：{entry}"));
                    }
                }
                if !lost.is_empty() {
                    // 设备真的丢了：整套重建（layout / buffer / texture 都与设备绑定）
                    match context.recover() {
                        Ok(()) => {
                            if let Ok(mut state) = shared.lock() {
                                state.note("设备已重建（纹理重新上传）");
                            }
                            dirty = true;
                        }
                        Err(error) => {
                            return SessionExit::Failed(format!("设备恢复失败：{error}"));
                        }
                    }
                } else if consecutive_errors >= MAX_CONSECUTIVE_RENDER_ERRORS {
                    // 同一套坏状态一直报错：交回监督器重建整套资源
                    return SessionExit::Failed(format!(
                        "连续 {consecutive_errors} 次渲染错误：{error}"
                    ));
                } else {
                    dirty = true; // 再试一帧
                }
            }
        }
    }
}

/// 一条命令落到上下文上。返回 `Err` = 拒绝（前端能看见原因）。
fn apply_command(
    command: RenderCommand,
    context: &mut GpuContext,
    shared: &Arc<Mutex<RenderState>>,
    decoder: &Sender<DecodeJob>,
    latest_job: &mut u64,
    dirty: &mut bool,
) -> Result<(), String> {
    match command {
        RenderCommand::Stop => Ok(()),
        RenderCommand::Resize { width, height } => {
            context.resize_surface(width, height);
            if let Ok(mut state) = shared.lock() {
                state.surface = SizeDto {
                    width: width as f64,
                    height: height as f64,
                };
            }
            *dirty = true;
            Ok(())
        }
        RenderCommand::Viewport(args) => {
            let validated = validate(&args)?;
            let previous = context.viewport().clip_rect.map(|rect| (rect.width, rect.height));
            let hole = physical_rect(validated.hole, validated.dpr);
            let next = raybend::render::ClipRect {
                x: hole.x as f32,
                y: hole.y as f32,
                width: hole.width as f32,
                height: hole.height as f32,
            };
            let size_changed = previous != Some((next.width, next.height));
            context.set_dpr(validated.dpr as f32);
            context.set_backdrop(validated.backdrop.to_clear_color());
            context.viewport_mut().clip_rect = Some(next);
            // 洞口尺寸一变就要重新适配（`refit` 会清 pan）—— 与 spike 的 `sync_hole` 同款。
            // 只按尺寸比：主题变化（底色变了）不该把用户拖好的 pan 清掉。
            if size_changed && context.viewport().fit_mode != FitMode::Free {
                context.viewport_mut().refit();
            }
            if let Ok(mut state) = shared.lock() {
                state.hole_css = Some(validated.hole);
                state.hole_physical = Some(hole);
                state.backdrop = validated.backdrop.into();
                state.dpr = validated.dpr;
            }
            ensure_tier(context, shared, decoder, latest_job);
            *dirty = true;
            Ok(())
        }
        RenderCommand::SetPhoto { path } => {
            let mut state = lock_state(shared);
            match path {
                Some(path) => {
                    state.photo_path = Some(path.clone());
                    state.decode = "loading".to_string();
                    state.decode_error = None;
                    state.wanted_tier = Some(ImageTier::Preview);
                    state.history_photo(&path);
                    *latest_job += 1;
                    let job = DecodeJob {
                        id: *latest_job,
                        path,
                        tier: ImageTier::Preview,
                    };
                    drop(state);
                    if decoder.send(job).is_err() {
                        return Err("解码线程不在了".to_string());
                    }
                }
                None => {
                    context.clear_image();
                    state.photo_path = None;
                    state.painted_path = None;
                    state.image = None;
                    state.tier = None;
                    state.wanted_tier = None;
                    state.origin = None;
                    state.decode = "idle".to_string();
                    state.decode_error = None;
                    drop(state);
                }
            }
            *dirty = true;
            Ok(())
        }
        RenderCommand::Decoded(outcome) => {
            if outcome.id != *latest_job {
                return Ok(()); // 过期结果：丢掉（换照片/换档位之后的旧任务）
            }
            let mut state = lock_state(shared);
            state.wanted_tier = None;
            match outcome.result {
                Ok(image) => {
                    let size = (image.width, image.height);
                    context.set_image(image);
                    let mut viewport = *context.viewport();
                    if viewport.fit_mode != FitMode::Free {
                        viewport.refit();
                    }
                    *context.viewport_mut() = viewport;
                    state.photo_path = Some(outcome.path);
                    state.image = Some(SizeDto {
                        width: size.0 as f64,
                        height: size.1 as f64,
                    });
                    state.tier = Some(outcome.tier);
                    state.origin = Some(outcome.origin);
                    state.decode = "ready".to_string();
                    state.decode_error = None;
                    drop(state);
                    ensure_tier(context, shared, decoder, latest_job);
                }
                Err(error) => {
                    state.decode = "error".to_string();
                    state.decode_error = Some(error);
                    drop(state);
                }
            }
            *dirty = true;
            Ok(())
        }
        RenderCommand::Intent(intent) => {
            let dpr = context.viewport().dpr;
            match intent {
                ViewportIntent::ZoomAt { x, y, factor } => {
                    // CSS 窗口坐标 → 物理（**只乘一次 DPR**，数学在 crate 里）
                    let anchor = (x * dpr, y * dpr);
                    context.viewport_mut().zoom_at(anchor, factor);
                }
                ViewportIntent::ZoomBy { factor } => {
                    let rect = context.viewport().effective_rect();
                    let anchor = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
                    context.viewport_mut().zoom_at(anchor, factor);
                }
                ViewportIntent::Pan { dx, dy } => {
                    context.viewport_mut().pan_by((dx * dpr, dy * dpr));
                }
                ViewportIntent::Fit { ref mode } => {
                    context.viewport_mut().fit_mode = parse_fit_mode(mode)?;
                    context.viewport_mut().refit();
                }
                ViewportIntent::ToggleFit => {
                    // 「适合窗口 ↔ 1:1」：当前是 1:1 就回适合窗口，否则去 1:1
                    let viewport = context.viewport_mut();
                    viewport.fit_mode = if viewport.fit_mode == FitMode::OneToOne {
                        FitMode::Fit
                    } else {
                        FitMode::OneToOne
                    };
                    viewport.refit();
                }
                ViewportIntent::Reset => {
                    let viewport = context.viewport_mut();
                    viewport.rotation = 0.0;
                    viewport.fit_mode = FitMode::Fit;
                    viewport.refit();
                }
                ViewportIntent::HitTest { x, y } => {
                    // 负数坐标 = 「没有指针」（前端在指针离开洞口时这么发）
                    let hit = if x < 0.0 || y < 0.0 {
                        None
                    } else {
                        context.viewport().hit_test_css((x, y)).map(|(px, py)| PointDto {
                            x: px,
                            y: py,
                        })
                    };
                    if let Ok(mut state) = shared.lock() {
                        state.last_hit = hit;
                    }
                    return Ok(());
                }
            }
            ensure_tier(context, shared, decoder, latest_job);
            *dirty = true;
            Ok(())
        }
    }
}

/// 需要换档位吗（缩放跨过 `1:1` 就要真解码）—— 需要就发一个解码任务。
fn ensure_tier(
    context: &GpuContext,
    shared: &Arc<Mutex<RenderState>>,
    decoder: &Sender<DecodeJob>,
    latest_job: &mut u64,
) {
    let mut state = lock_state(shared);
    let Some(path) = state.photo_path.clone() else {
        return;
    };
    let viewport = context.viewport();
    let wanted = tier_for(viewport.zoom, state.tier.unwrap_or(ImageTier::Preview));
    if state.tier == Some(wanted) || state.wanted_tier == Some(wanted) {
        return;
    }
    state.wanted_tier = Some(wanted);
    state.decode = "loading".to_string();
    *latest_job += 1;
    let job = DecodeJob {
        id: *latest_job,
        path,
        tier: wanted,
    };
    drop(state);
    let _ = decoder.send(job);
}

/// 把渲染器的状态如实地搬进快照。
fn publish(state: &mut RenderState, context: &GpuContext) {
    let viewport = context.viewport();
    let details = context.surface_details();
    state.ready = true;
    state.zoom = viewport.zoom;
    state.pan_x = viewport.pan_px.0;
    state.pan_y = viewport.pan_px.1;
    state.rotation = viewport.rotation;
    state.fit_mode = fit_mode_name(viewport.fit_mode).to_string();
    state.dpr = f64::from(viewport.dpr);
    state.drawn_frames = context.frames_drawn();
    state.surface = SizeDto {
        width: details.size.0 as f64,
        height: details.size.1 as f64,
    };
    if let Some(rect) = viewport.clip_rect {
        state.hole_physical = Some(RectDto {
            x: f64::from(rect.x),
            y: f64::from(rect.y),
            width: f64::from(rect.width),
            height: f64::from(rect.height),
        });
    }
}

impl RenderState {
    /// 换照片时记一行（只在**换了**才记，避免同一条留一堆）。
    fn history_photo(&mut self, path: &str) {
        let name = Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());
        if self.history.last().is_some_and(|last| last.contains(&name)) {
            return;
        }
        self.note(format!("装载照片：{name}"));
    }
}

/// 解码线程：一次一张、**最新者优先**（过期的任务连做都不做）。
fn decode_loop(
    receiver: Receiver<DecodeJob>,
    commands: Sender<RenderCommand>,
    max_texture: u32,
) {
    while let Ok(job) = receiver.recv() {
        // 队列里躺着的时候就可能已经过期了：先看通道里还有没有更新的（有就跳过这一个）
        let mut job = job;
        loop {
            match receiver.try_recv() {
                Ok(newer) => job = newer,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }

        let size = match job.tier {
            ImageTier::Preview => PixelSize::Screen,
            ImageTier::Full => PixelSize::Full,
        };
        let (origin, result) = match display::pixels(Path::new(&job.path), size) {
            Ok(Some(pixels)) => {
                let origin = match pixels.origin {
                    raybend::thumbnail::render::PixelOrigin::Bitmap => "bitmap",
                    raybend::thumbnail::render::PixelOrigin::RawEmbeddedPreview => {
                        "raw-embedded-preview"
                    }
                    raybend::thumbnail::render::PixelOrigin::RawDecoded => "raw-decoded",
                };
                match RenderImage::from_rgb8(pixels.width, pixels.height, &pixels.rgb) {
                    Some(image) => {
                        // 夹到设备上限：超过 `max_texture_dimension_2d` 的图
                        // `create_texture` 会直接报错（不是静默降级）
                        let image = image.clamped_to_long_edge(max_texture);
                        (origin.to_string(), Ok(image))
                    }
                    None => (
                        origin.to_string(),
                        Err(format!(
                            "解码结果的尺寸与字节数对不上：{}×{}",
                            pixels.width, pixels.height
                        )),
                    ),
                }
            }
            Ok(None) => ("unknown".to_string(), Err(format!("解不开这张照片：{}", job.path))),
            Err(error) => ("unknown".to_string(), Err(format!("{error}"))),
        };

        let outcome = DecodeOutcome {
            id: job.id,
            path: job.path,
            tier: job.tier,
            origin,
            result,
        };
        if commands.send(RenderCommand::Decoded(outcome)).is_err() {
            return; // 渲染线程走了
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(dpr: f64) -> SetViewportArgs {
        SetViewportArgs {
            hole: RectDto {
                x: 300.0,
                y: 120.0,
                width: 1280.0,
                height: 720.0,
            },
            dpr,
            viewport: SizeDto {
                width: 1920.0,
                height: 1080.0,
            },
            // 前端报的是 `getComputedStyle` 的字符串，解析在 Rust
            backdrop: Some("rgb(42, 45, 51)".to_string()),
        }
    }

    #[test]
    fn physical_rect_multiplies_dpr_once() {
        // 1.375 = 系统 125% × 页面 110%（§7.9 那次事故的真实比例）
        let hole = RectDto {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 50.0,
        };
        let physical = physical_rect(hole, 1.375);
        assert!((physical.x - 13.75).abs() < 1e-9);
        assert!((physical.y - 27.5).abs() < 1e-9);
        assert!((physical.width - 137.5).abs() < 1e-9);
        assert!((physical.height - 68.75).abs() < 1e-9);
    }

    #[test]
    fn validate_rejects_nonsense() {
        assert!(validate(&args(1.0)).is_ok());
        assert!(validate(&args(0.0)).is_err(), "dpr 不能是 0：它是换算的分母");
        assert!(validate(&args(-1.0)).is_err());
        assert!(validate(&args(f64::NAN)).is_err());
        assert!(validate(&args(f64::INFINITY)).is_err());

        let mut bad = args(1.0);
        bad.hole.width = -1.0;
        assert!(bad.hole.is_valid().eq(&false), "负宽度非法");
        assert!(validate(&bad).is_err());

        let mut nan = args(1.0);
        nan.viewport.height = f64::NAN;
        assert!(validate(&nan).is_err());
    }

    #[test]
    fn zero_sized_hole_is_legal_but_kept_as_zero() {
        // 最小化 / 布局中间态会给 0 宽：**合法**（不是垃圾值），照实存下来
        let mut zero = args(1.0);
        zero.hole.width = 0.0;
        zero.hole.height = 0.0;
        let ok = validate(&zero).expect("0 尺寸是合法输入");
        let mut stored = StoredViewport::default();
        stored.apply(ok);
        assert_eq!(stored.hole_physical().expect("有值").width, 0.0);
    }

    #[test]
    fn stored_keeps_last_value_and_counts_updates() {
        let mut stored = StoredViewport::default();
        assert!(stored.snapshot().hole_css.is_none());
        assert_eq!(stored.snapshot().updates, 0);

        stored.apply(validate(&args(1.25)).expect("合法"));
        stored.apply(validate(&args(1.375)).expect("合法"));
        let snapshot = stored.snapshot();
        assert_eq!(snapshot.updates, 2, "每一次上报都要计数");
        assert!((snapshot.dpr - 1.375).abs() < 1e-9);
        let physical = snapshot.hole_physical.expect("有洞口");
        assert!((physical.width - 1280.0 * 1.375).abs() < 1e-6);
        assert!(
            (physical.x - 300.0 * 1.375).abs() < 1e-6,
            "物理洞口必须带偏移，不能只算宽高"
        );
    }

    #[test]
    fn snapshot_serializes_with_camel_case_keys() {
        // 前端 `src/api/editor.ts` 读的是 camelCase —— 与 `src/api/dto-contract.json` 对齐
        let mut stored = StoredViewport::default();
        stored.apply(validate(&args(2.0)).expect("合法"));
        let value = serde_json::to_value(stored.snapshot()).expect("能序列化");
        let object = value.as_object().expect("是个对象");
        let mut keys: Vec<&String> = object.keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "backdrop",
                "backdropReported",
                "dpr",
                "holeCss",
                "holePhysical",
                "updates",
                "viewportCss"
            ]
        );
    }

    #[test]
    fn fit_mode_names_round_trip_and_reject_junk() {
        for mode in [FitMode::Fit, FitMode::Fill, FitMode::OneToOne, FitMode::Free] {
            let name = fit_mode_name(mode);
            assert_eq!(parse_fit_mode(name).expect("能解回来"), mode);
        }
        // 大小写不敏感（前端可能给 camelCase）
        assert_eq!(parse_fit_mode("OneToOne").expect("大小写不敏感"), FitMode::OneToOne);
        assert_eq!(parse_fit_mode(" onetoone ").expect("两侧空白容忍"), FitMode::OneToOne);
        assert!(parse_fit_mode("zoom").is_err(), "不认识的名字必须报错");
        assert!(parse_fit_mode("").is_err());
    }

    #[test]
    fn intents_deserialize_with_the_real_frontend_field_names() {
        // ⚠️ 这一条是 §7.9 §6 第 2 条要求的：**用真实字段名反序列化**。
        // `rename_all` 只管变体名，字段要靠 `rename_all_fields` —— 少写一个就全变 `undefined`
        let zoom: ViewportIntent = serde_json::from_value(serde_json::json!({
            "kind": "zoomAt",
            "x": 640.0,
            "y": 360.0,
            "factor": 1.25
        }))
        .expect("zoomAt 能反序列化");
        match zoom {
            ViewportIntent::ZoomAt { x, y, factor } => {
                assert_eq!((x, y), (640.0, 360.0));
                assert!((factor - 1.25).abs() < 1e-6);
            }
            other => panic!("解成了别的变体：{other:?}"),
        }

        let pan: ViewportIntent = serde_json::from_value(serde_json::json!({
            "kind": "pan",
            "dx": -12.5,
            "dy": 3.0
        }))
        .expect("pan 能反序列化");
        assert!(matches!(pan, ViewportIntent::Pan { .. }));

        let fit: ViewportIntent = serde_json::from_value(serde_json::json!({
            "kind": "fit",
            "mode": "oneToOne"
        }))
        .expect("fit 能反序列化");
        assert!(matches!(fit, ViewportIntent::Fit { .. }));

        let hit: ViewportIntent = serde_json::from_value(serde_json::json!({
            "kind": "hitTest",
            "x": 1.0,
            "y": 2.0
        }))
        .expect("hitTest 能反序列化");
        assert!(matches!(hit, ViewportIntent::HitTest { .. }));

        let reset: ViewportIntent =
            serde_json::from_value(serde_json::json!({ "kind": "reset" })).expect("reset 能反序列化");
        assert!(matches!(reset, ViewportIntent::Reset));

        // 缺字段必须报错（**不许静默回退** —— §7.9 的教训是「缺 DPR 悄悄用错坐标系」）
        assert!(
            serde_json::from_value::<ViewportIntent>(serde_json::json!({
                "kind": "zoomAt",
                "x": 1.0,
                "factor": 2.0
            }))
            .is_err(),
            "缺 y 必须报错"
        );
    }

    #[test]
    fn set_viewport_args_accept_an_omitted_backdrop() {
        // 老前端（W1 的载荷）不带 backdrop：**不把整条上报打回**，用兜底色，
        // 并在状态里把「这次用的是兜底的」如实标出来
        let args: SetViewportArgs = serde_json::from_value(serde_json::json!({
            "hole": { "x": 1.0, "y": 2.0, "width": 3.0, "height": 4.0 },
            "dpr": 1.375,
            "viewport": { "width": 1600.0, "height": 1000.0 }
        }))
        .expect("能反序列化");
        assert!(args.backdrop.is_none());
        let validated = validate(&args).expect("缺底色照样合法");
        assert_eq!(validated.backdrop, raybend::render::Srgb8::DARK_SURFACE_BAR);
        assert!(!validated.backdrop_reported, "用的是兜底色，状态里要看得出来");

        // 报了但解不开：也是兜底 + 标记，而不是报错
        let mut broken = args;
        broken.backdrop = Some("hsl(210, 10%, 18%)".to_string());
        let validated = validate(&broken).expect("解不开不报错");
        assert!(!validated.backdrop_reported);

        // 报了且能解开：就是报上来的那个色，并且标着「是报上来的」
        let mut reported = broken;
        reported.backdrop = Some("rgb(18, 20, 24)".to_string());
        let validated = validate(&reported).expect("能解开");
        assert_eq!((validated.backdrop.r, validated.backdrop.g, validated.backdrop.b), (18, 20, 24));
        assert!(validated.backdrop_reported);
    }

    #[test]
    fn reported_backdrop_ends_up_as_the_opaque_clear_color() {
        let validated = validate(&args(1.0)).expect("合法");
        let color = validated.backdrop.to_clear_color();
        /*
         * ⚠️ 期望值是**线性值**，不是 42/255：
         * wgpu 在 sRGB 目标上把清屏值当线性值，再由硬件编码回 sRGB
         * （实测：直接传 42/255 时回读到 110；见 `Srgb8::to_clear_color` 的说明）。
         * 下面三个常数是 42/45/51 的 sRGB→线性结果（外部给定，由标准传输函数算得）。
         */
        assert!((color.r - 0.0231).abs() < 1e-3, "42 的线性值：{}", color.r);
        assert!((color.g - 0.0263).abs() < 1e-3, "45 的线性值：{}", color.g);
        assert!((color.b - 0.0331).abs() < 1e-3, "51 的线性值：{}", color.b);
        assert!((color.a - 1.0).abs() < 1e-9, "洞口底色必须不透明（否则透出桌面）");
    }

    #[test]
    fn render_state_serializes_with_camel_case_keys() {
        let state = RenderState {
            bound: true,
            ready: true,
            adapter: "fake".to_string(),
            painted_path: Some("a.jpg".to_string()),
            tier: Some(ImageTier::Preview),
            ..Default::default()
        };
        let value = serde_json::to_value(&state).expect("能序列化");
        let object = value.as_object().expect("是个对象");
        assert!(object.contains_key("paintedPath"), "前端读的是 camelCase");
        assert!(object.contains_key("drawnFrames"));
        assert!(object.contains_key("lastHit"));
        assert_eq!(
            object.get("tier").and_then(|tier| tier.as_str()),
            Some("preview"),
            "档位名是线上一词"
        );
    }
}
