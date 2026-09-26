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

use raybend::develop::{
    denoise::NrMethod,
    denoise_job::{DenoiseKey, HighDenoise},
    geometry::{CropHandle, CropRect, EditGeometry, drag_crop, hit_crop, largest_centered_rect},
    reference::{ReferenceCache, ReferenceFrame},
};
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};

use raybend::develop::lens::{LensCorrection, LensMap};
use raybend::develop::local_tone::{LocalToneOpts, LocalToneState};
use raybend::develop::{
    Curve, CurveChannel, CurveSet, DevelopParams, DevelopPlans, DevelopStages, LinearImage,
    Resolved, chain_image, render_develop,
};
use raybend::display::{self, FullCache, PixelSize};
use raybend::render::{
    FitMode, GpuContext, ImageTier, PresentationAdapter, RenderImage, RenderOutcome, RestartPolicy,
    SurfaceComposition, Verdict, tier_for, tier_for_params,
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
#[derive(Debug, Clone, PartialEq, Deserialize)]
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
    /// 线、描边、裁切网格、旋转网格的 CSS 令牌，前端只报字符串。
    #[serde(default)]
    pub overlay_colors: Option<[String; 4]>,
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
#[derive(Debug, Clone, PartialEq)]
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
    /// 最近一次上报的**原始载荷**（补发给晚起的会话用 —— 见 [`Self::replay`]）。
    /// 存原始串而不是重拼：底色那一项 `backdrop_reported` 的语义依赖它。
    raw: Option<SetViewportArgs>,
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
            raw: None,
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

    /// 记住这次的**原始**载荷（与 [`Self::apply`] 分开，是因为校验后的结构
    /// 丢失了「底色原本报的是什么串」）。
    pub fn remember_raw(&mut self, args: SetViewportArgs) {
        self.raw = Some(args);
    }

    /// 补发给新会话的原始载荷。
    ///
    /// 为什么需要：前端的上报器**会去重**（同一份载荷不重复发），而视口上报
    /// 可能早于会话建立 —— 那时 `sink` 还没有，事实只存进了 `EditorState`，
    /// 新会话的 `clip_rect` 就一直是 `None`（症状：照片按整窗适配、不在洞口里）。
    /// 会话起来时补发一次，两个方向的时序都成立。
    #[must_use]
    pub fn replay(&self) -> Option<SetViewportArgs> {
        self.raw.clone()
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
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ViewportIntent {
    /// 以某个 **CSS 窗口坐标**（`clientX/clientY`）为锚点缩放。
    ZoomAt {
        x: f32,
        y: f32,
        factor: f32,
    },
    /// 以**洞口中心**为锚点缩放（按钮 / 快捷键 —— 与「滚轮以光标为锚点」区分开）。
    ///
    /// 前端不自己算中心：中心在哪只有 Rust 知道（洞口是它存的），
    /// 让前端去减洞口原点再加一半，就是又把坐标数学搬回了前端。
    ZoomBy {
        factor: f32,
    },
    /// 平移（CSS 像素位移）。
    Pan {
        dx: f32,
        dy: f32,
    },
    /// 档位：`fit` / `fill` / `oneToOne` / `free`。
    Fit {
        mode: String,
    },
    /// **适合窗口 ↔ 1:1**（双击 / `0` 键；与看图件同一条语义）。
    ///
    /// 为什么做成一个意图而不是前端先读状态再决定：状态是 250ms 轮询来的，
    /// 而「现在是哪一档」的事实住在 Rust —— 让它自己判，不存在「双击后反而跳远」的窗口期。
    ToggleFit,
    /// 复位：适合窗口 + 零旋转 + 零平移。
    Reset,
    /// 命中测试：把 CSS 坐标换算成图像像素（**不动视口**；W5 的三个工具要用）。
    HitTest {
        x: f32,
        y: f32,
    },
    /// 对比开关；每次重新打开从中点开始。参考帧未就绪时先记住状态。
    SetCompare {
        enabled: bool,
    },
    /// 对比分线的原始 CSS 窗口指针事实；命中和移动由 Rust 决定。
    ComparePointer {
        phase: String,
        x: f32,
        y: f32,
    },
    SetTool {
        tool: Option<String>,
        initial_ratio: Option<f32>,
    },
    SetReferenceBase {
        base: String,
        sequence: u64,
    },
    SetCropRatio {
        ratio: Option<f32>,
    },
    SetRotation {
        degrees: f32,
    },
    ToolPointer {
        phase: String,
        x: f32,
        y: f32,
    },
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

/// 每轮最多处理这么多条命令，然后把最新状态画出去。拖动窗口或拉杆时通道可以
/// 持续有新消息；无限 try_recv 会让绘制一直等不到「队列变空」。
const MAX_COMMANDS_PER_FRAME: usize = 32;

/// 渲染线程的命令（**一个通道**：前端意图 + 窗口事件 + 解码结果）。
///
/// 为什么合成一个通道：渲染线程只需要一个「醒来」的理由，醒来之后把攒下的命令
/// 全部吃掉再画一帧（不做「一条命令一帧」）。多通道就要 `select`，那是白送的复杂度。
enum RenderCommand {
    /// 洞口事实（含 DPR 与底色）
    Viewport(SetViewportArgs),
    /// 窗口客户区尺寸变化（物理像素；**主线程推来**，渲染线程不查窗口）
    Resize {
        width: u32,
        height: u32,
    },
    /// 窗口移动可能只让 WebView 重绘；同尺寸的 GPU surface 也必须重新呈现一帧。
    Redraw,
    /// 换照片 / 清空照片
    ///
    /// `transition`：**过渡帧计划**（进编辑先出图，handoff §3）——
    /// 命令层算好（那里有库、有 catalog），显影线程只管试。`None` = 不发过渡帧。
    SetPhoto {
        path: Option<String>,
        transition: Option<TransitionPlan>,
    },
    /// 视口意图
    Intent(ViewportIntent),
    /// 已缓存的命名定稿 1920 预览作为对比参照；序号与照片路径挡住迟到结果。
    ReferenceIssue {
        frame: Arc<ReferenceFrame>,
        issue_id: i64,
        photo_path: String,
        sequence: u64,
    },
    /// 显影参数（拉杆 / 曲线）变了 —— 只重算像素，不重新解码
    ///
    /// 带的是**已经校验过的**解析结果（命令层负责校验，线程里不再解析一遍）。
    ///
    /// `interactive` = 手指还按着（人类 2026-09-24）：拖动中只算预览档，
    /// 松手那一下才按缩放补全尺寸（`tier_for_params`）。
    SetParams {
        nr_method: NrMethod,
        params: DevelopParams,
        curves: CurveSet,
        interactive: bool,
        /// 这次要用的**镜头校正**（已解析；`None` = 不用配置文件 —— 手动三根拉杆
        /// 在显影线程里从参数合并，所以拖动时它们是实时的）。
        /// `Box` 是因为它比其它变体大得多（clippy 的 `large_enum_variant`）。
        lens: Option<Box<raybend::develop::lens::LensCorrection>>,
        lut: Option<Arc<raybend::develop::lut::Lut>>,
        geometry: Option<EditGeometry>,
    },
    /// 显影完了一张（新照片或新参数）
    Developed(DevelopOutcome),
    /// 高质量缓存已更新：用当前参数重出帧，结果仍经过任务号闸门。
    DenoiseReady,
    ConfirmTool(Sender<Result<EditGeometry, String>>),
    /// 结束会话（离开编辑器）
    Stop,
}

/// 前端送来的显影参数（**只装非默认项**；色温基线随照片走）。
///
/// 校验在这里（`into_parts`）：未知 id / 非法值 / 坏曲线一律**报错**，
/// 不静默夹取 —— 一个 NaN 悄悄过去，整张图就变黑了，而界面上看不出原因。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopParamsDto {
    /// 参数 id → 值（只装与基线不同的项）
    #[serde(default)]
    pub values: std::collections::BTreeMap<String, f64>,
    /// 这张照片的 as-shot 色温（K）；`None` = 读不到
    #[serde(default)]
    pub as_shot_temperature: Option<f32>,
    /// 通道（`rgb` / `r` / `g` / `b`）→ 控制点 `[[x, y], …]`（归一化 0..1）
    #[serde(default)]
    pub curves: std::collections::BTreeMap<String, Vec<[f32; 2]>>,
    /// RAW 专用的机型基础曲线快照，位于用户曲线之前。
    #[serde(default)]
    pub base_curve_points: Option<Vec<[f32; 2]>>,
    #[serde(default)]
    pub lut_id: Option<String>,
    #[serde(default)]
    pub lut_enabled: Option<bool>,
    /// **手指还按在滑杆 / 曲线上**（人类 2026-09-24）：
    /// 拖动中只算屏幕那一档，松手那一下才按缩放补全尺寸（`tier_for_params`）。
    #[serde(default)]
    pub interactive: bool,
    /// 镜头配置文件（`None` = 未选择；`"none"` = 显式关掉；否则是 `maker|model`）
    #[serde(default)]
    pub lens_profile: Option<String>,
    /// 配置文件那一半的开关（`None` = 默认开）
    #[serde(default)]
    pub lens_enabled: Option<bool>,
    /// 降噪方式（`None` = 快速档；高质量档由后台 BM3D 计算）
    #[serde(default)]
    pub nr_method: Option<String>,
    #[serde(default)]
    pub geometry: Option<EditGeometry>,
}

impl DevelopParamsDto {
    /// 转成管线要的两件东西（参数 + 曲线）。
    ///
    /// # Errors
    /// 未知参数 id / 值非法 / 未知曲线通道 / 控制点不合法。
    pub fn into_parts(self) -> Result<(DevelopParams, CurveSet), String> {
        let params = DevelopParams::from_values(self.values, self.as_shot_temperature)?;
        let mut curves = CurveSet::identity();
        if let Some(points) = self.base_curve_points {
            curves.base = Curve::from_points(points)?;
        }
        for (channel, points) in self.curves {
            let Some(channel) = CurveChannel::parse(&channel) else {
                return Err(format!("未知的曲线通道：{channel}"));
            };
            let curve = Curve::from_points(points)?;
            curves.set_channel(channel, curve);
        }
        Ok((params, curves))
    }
}

/// 过渡帧的一个候选来源（按顺序试，第一个能解出来的上屏）。
#[derive(Debug, Clone, PartialEq, Eq)]
enum TransitionKind {
    /// 我们自己的大图缓存（AVIF，`cache/full/<id>/latest-<base>-v<pipeline>.avif`）。
    AvifCache,
    /// 照片文件本身（RAW → 内嵌预览；位图 → 直接解）。
    PhotoFile,
}

/// 过渡帧的一个候选：文件 + 它是什么 + 上屏后怎么报「像素从哪来」。
#[derive(Debug, Clone, PartialEq, Eq)]
struct TransitionSource {
    path: std::path::PathBuf,
    kind: TransitionKind,
    /// 写进 `RenderState.origin` 的字符串（诊断与界面提示要看）。
    origin: &'static str,
}

/// **过渡帧的取图计划**（handoff §3「进来先读 preview」+ §2.5「切图优先」）。
///
/// 进编辑 / 在胶片带上换照片时，先拿一张**已经存在的图**画上去（毫秒级），
/// 真帧（RAW 全解码 + 管线）随后替换。候选顺序即优先级：
/// `latest`（缓存里有就读）→ `SOOC`（相机直出位图）→ `RAW`（内嵌预览）。
///
/// 计划在**命令层**算好（那里有库、有 catalog），显影线程只管试 ——
/// 它是个普通线程，够不着数据库。
#[derive(Debug, Clone, PartialEq, Eq)]
struct TransitionPlan {
    sources: Vec<TransitionSource>,
    /// **原图尺寸**（已按方向摆正）—— 过渡帧的逻辑尺寸。
    ///
    /// 为什么不报过渡帧自己的尺寸（例如 1920）：`set_image` 在逻辑尺寸变了时会 `refit`，
    /// 真帧回来那一刻几何会重摆（`Free` 模式下用户拖过的视角会被拉回中心）。
    /// 报原图尺寸就**根本不会发生那次 refit**。
    /// 查不到时退回用过渡帧自己的尺寸 —— 两者都已摆正、宽高比一致，
    /// `Fit` 档下 refit 的结果逐像素相同（`zoom_for_fit` 只取决于宽高比）。
    source_size: Option<(u32, u32)>,
    sooc: Option<std::path::PathBuf>,
}

/// 一次**显影任务**（渲染线程 → 显影线程）。
///
/// 显影线程同时管两件事，因为它们是同一份数据的两个阶段：
/// **解码**（文件 → 线性源，贵）与**按参数算像素**（线性源 → 显示像素，便宜）。
/// 把源缓存在线程里，参数一变就只跑第二段 —— 这就是「拖拉杆要跟得上手」的全部秘密。
#[derive(Clone)]
struct DevelopJob {
    nr_method: NrMethod,
    /// 单调递增的任务号：**只有最新那个的结果有效**（换照片 / 换档位 / 又拖了一下）
    id: u64,
    /// 这一次任务用的是第几版参数（回传给前端看「算完没有」）
    rev: u64,
    /// `Some(path)` = 先换照片（解码线性源）；`None` = 用缓存里的源重算
    photo: Option<String>,
    /// 输出档位（预览 = 缩到屏幕档；全尺寸 = 原尺寸）
    tier: ImageTier,
    interactive: bool,
    params: DevelopParams,
    /// 已解析的镜头校正（配置文件那一半；`None` = 不用配置文件）。
    /// 手动三根拉杆不在这里 —— 显影线程从 `params` 里合并（那样拖动才是实时的）。
    lens: Option<raybend::develop::lens::LensCorrection>,
    lut: Option<Arc<raybend::develop::lut::Lut>>,
    curves: CurveSet,
    geometry: Option<EditGeometry>,
    prefer_sooc: bool,
    /// **过渡帧计划**（进编辑先出图）：只在真的要解码那一下用一次；
    /// `None` = 不发过渡帧（换档位 / 改参数那类任务）
    transition: Option<TransitionPlan>,
}

/// 显影线程手里缓存的**线性源**（一张照片一份，含按档位缩好的预览副本）。
struct CachedSource {
    reference: ReferenceCache,
    sooc: Option<std::path::PathBuf>,
    source_revision: u64,
    source_signature: Option<String>,
    path: String,
    /// 全尺寸线性源（显影的唯一输入）
    full: Arc<LinearImage>,
    /// 屏幕档副本（第一次要预览档时缩一次，之后一直用）
    preview: Option<Arc<LinearImage>>,
    /// **分析图**（长边 1/4，链之前的线性源）—— 只依赖照片，参数变了不用重缩
    analysis_source: Option<LinearImage>,
    /// **动态反差的分析结果** + 它是用哪组链参数算出来的（见 [`Resolved::chain_key`]）。
    ///
    /// 分两段缓存是拖得动拉杆的关键：分解与分位数只依赖「照片 + 线性链参数」，
    /// **与强度无关**；拖动动态反差杆本身根本不需要重算它。
    local_tone: Option<([f32; 6], LocalToneState)>,
    /// 拍摄色温估计（K）——前端拿它当色温拉杆的基线
    as_shot_temperature: Option<f32>,
    /// 像素从哪来（`raw-linear` / `bitmap-linear`）
    origin: String,
}

/// 一次显影的结果（显影线程 → 渲染线程）。
struct DevelopOutcome {
    reference: Option<Arc<ReferenceFrame>>,
    reference_base: Option<&'static str>,
    nr_pending: bool,
    nr_error: Option<String>,
    id: u64,
    rev: u64,
    path: String,
    tier: ImageTier,
    /// 像素从哪来（`bitmap` / `raw-linear` / `preview-latest`…）——诊断与界面提示要看
    origin: String,
    /// 这是**过渡帧**吗（进编辑先出图）：真帧还在路上 ——
    /// 所以它**不清 `wanted_tier`、不置 `ready`、不动 `applied_params_rev`**。
    transition: bool,
    /// 拍摄色温估计（K）——前端拿它当色温拉杆的基线
    as_shot_temperature: Option<f32>,
    /// 解码耗时（毫秒；只换照片那一次有值）
    decode_ms: Option<f64>,
    /// 管线耗时（毫秒）——**实施记录里的数字就是它**
    develop_ms: f64,
    /// 松手后的直方图，与本帧像素来自同一次显影；拖动帧为 None。
    histogram: Option<crate::thumbs::HistogramDto>,
    result: Result<DevelopedImage, String>,
}

/// 显影好的像素（RGBA8 由渲染线程扩；这里给 RGB8）。
struct DevelopedImage {
    /// **这一档纹理**的尺寸（预览档可能是 1920）。
    width: u32,
    height: u32,
    rgb: Vec<u8>,
    /// **逻辑图像尺寸**（原图 / 解码尺寸）—— 视口摆图与「1:1」按它算，
    /// 与当前是哪一档无关（M3-W4 修「双击 1:1 变成预览图的 1:1」）。
    source_width: u32,
    source_height: u32,
    original_width: u32,
    original_height: u32,
}

/// 渲染线程的共享状态 —— **前端轮询读的就是它**（序列化后直接回给前端）。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderState {
    pub reference_ready: bool,
    pub reference_base: Option<String>,
    pub comparing: bool,
    /// 对比分线在洞口内的 CSS x（Rust 已处理 DPR/洞口原点）。
    pub compare_line_css: Option<f32>,
    pub nr_pending: bool,
    pub nr_error: Option<String>,
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
    pub original_image: Option<SizeDto>,
    pub tool_box_css: Option<RectDto>,
    pub tool_revision: u64,
    /// 当前档位（`preview` / `full`）
    pub tier: Option<ImageTier>,
    /// 正在取的档位（不一样就说明解码还没回来）
    pub wanted_tier: Option<ImageTier>,
    /// 像素从哪来（RAW 线性解码 / 位图线性化）
    pub origin: Option<String>,
    /// 当前照片最近一次非交互显影的直方图；拖动中保持上一份。
    pub histogram: Option<crate::thumbs::HistogramDto>,
    /// **拍摄色温估计**（K）——前端拿它当色温拉杆的基线（`AGENTS.md` §11.5）
    pub as_shot_temperature: Option<f32>,
    /// 收到过几次显影参数（前端据此判断「我发的那次算完没有」：
    /// `params_rev > applied_params_rev` = 还在算）
    pub params_rev: u64,
    /// 已经画到屏幕上的那一次参数（`applied_params_rev`）
    pub applied_params_rev: u64,
    /// 最近一次**管线**耗时（毫秒）——诊断与性能基线用
    pub develop_ms: Option<f64>,
    /// 最近一次**解码**耗时（毫秒；只有换照片那一次有值）
    pub decode_ms: Option<f64>,
    /// **图像像素 → 洞口内 CSS 像素**的仿射矩阵 `[a, b, c, d, e, f]`（覆盖层专用）。
    ///
    /// 覆盖层（裁切柄 / 旋转框 / 对比线 / 将来的蒙版）把子元素写成**图像像素坐标**，
    /// 整层套这个矩阵 —— 数学只有 Rust 一份（`AGENTS.md` §6.1 红线 2）。
    pub overlay_transform: Option<[f32; 6]>,
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
    /// editor_set_photo 的异步过渡帧计划可能乱序完成；只有最后一次请求可发命令。
    photo_request: RequestGate,
    /// 高频参数命令的旧解析结果不能在新值后面入队。
    params_request: RequestGate,
    /// 同一照片/镜头选择的解析只做一次（实际工作仍在阻塞线程里）。
    lens_cache: Arc<Mutex<Option<(LensCacheKey, Option<LensCorrection>)>>>,
    lut_cache: Arc<Mutex<Option<(String, Arc<raybend::develop::lut::Lut>)>>>,
}

type LensCacheKey = (String, i64, Option<String>, Option<bool>);

/// 序号检查与送命令必须在同一把锁内，否则旧请求可能在通过检查后
/// 被新请求超车，最后仍把旧照片送进队列。
#[derive(Default)]
struct RequestGate(Mutex<u64>);

impl RequestGate {
    fn begin(&self) -> Result<u64, String> {
        let mut request = self
            .0
            .lock()
            .map_err(|_| "编辑请求序号锁中毒".to_string())?;
        *request = request.wrapping_add(1);
        Ok(*request)
    }

    fn send_if_latest(
        &self,
        request: u64,
        send: impl FnOnce() -> Result<(), String>,
    ) -> Result<bool, String> {
        let latest = self
            .0
            .lock()
            .map_err(|_| "编辑请求序号锁中毒".to_string())?;
        if *latest != request {
            return Ok(false);
        }
        send()?;
        Ok(true)
    }
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

    /*
     * 把**已经收到的**洞口事实补发给新会话（见 `StoredViewport::replay`）。
     * 这条通道在 `sink` 设好之前是空的，所以补发要赶在把 sender 存进 `Session` 之前。
     */
    if let Ok(guard) = state.viewport.lock()
        && let Some(args) = guard.replay()
    {
        let _ = commands.send(RenderCommand::Viewport(args));
    }

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
    overlay_colors: Option<[String; 4]>,
) -> Result<ViewportStateDto, String> {
    let args = SetViewportArgs {
        hole,
        dpr,
        viewport,
        backdrop,
        overlay_colors,
    };
    let validated = validate(&args)?;
    let snapshot = {
        let mut guard = state
            .viewport
            .lock()
            .map_err(|_| "编辑器视口状态锁中毒".to_string())?;
        guard.apply(validated);
        guard.remember_raw(args.clone());
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
///
/// 同时算一份**过渡帧计划**（handoff §3「进来先读 preview」+ §2.5「切图优先」）：
/// 进编辑 / 在胶片带上换照片时，先拿一张**已经存在的图**顶上（毫秒级），
/// 真帧（RAW 全解码 + 管线）随后替换。
///
/// 为什么计划在命令层算：候选里要查库（这张属于哪个资产、位图在哪一侧）——
/// 显影线程是个普通线程，够不着数据库（`AGENTS.md` §4：业务不依赖 Tauri）。
#[tauri::command]
pub async fn editor_set_photo<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, EditorState>,
    path: Option<String>,
) -> Result<RenderState, String> {
    let request = state.photo_request.begin()?;
    // 旧照片还在后台解析的参数绝不能套到新照片上。
    state.params_request.begin()?;
    let plan = match path.as_deref() {
        Some(path) => {
            let handle = app.clone();
            let path = path.to_string();
            crate::source::blocking(move || Ok(transition_plan(&handle, &path))).await?
        }
        None => None,
    };
    // 查库/读元数据在后台完成：若期间用户已经选了另一张，这条旧请求
    // 不能再进渲染队列，否则会在新照片之后把旧照片装回来。
    state.photo_request.send_if_latest(request, || {
        let sender = session_sender(&state).ok_or_else(|| "渲染线程还没起来".to_string())?;
        sender
            .send(RenderCommand::SetPhoto {
                path,
                transition: plan,
            })
            .map_err(|_| "渲染线程不在了".to_string())
    })?;
    current_render_state(&state)
}

/// **候选顺序**（纯逻辑，可单测）：`latest`（同基准）→ `SOOC` 位图 → 要编的文件本身。
///
/// 入参是已经查好的事实（缓存路径、位图路径），调用方负责查库；
/// 这里只回答「先试哪个、后试哪个」。**缺文件的一律不进列表** ——
/// 显影线程里试一个不存在的文件只是白跑一趟。
fn transition_sources(
    path: &Path,
    base: raybend::store::develop::EditBase,
    latest_cache: Option<std::path::PathBuf>,
    sooc: Option<std::path::PathBuf>,
) -> Vec<TransitionSource> {
    let mut sources = Vec::new();

    // ① latest：编辑结果（与当前基准同一侧）—— 最接近用户最终会看到的那张
    if let Some(cached) = latest_cache.filter(|cached| cached.is_file()) {
        sources.push(TransitionSource {
            path: cached,
            kind: TransitionKind::AvifCache,
            origin: "preview-latest",
        });
    }

    // ② SOOC：相机直出的位图。
    //    只在「要编的是 RAW」时单独推 —— 要编的就是位图时，它就是下面第三步那张，别重复。
    if base == raybend::store::develop::EditBase::Raw
        && let Some(sooc) = sooc.filter(|sooc| sooc.is_file())
    {
        sources.push(TransitionSource {
            path: sooc,
            kind: TransitionKind::PhotoFile,
            origin: "preview-sooc",
        });
    }

    // ③ 要编的这张文件本身：RAW 走 worker 的内嵌预览（比全解码快一两个数量级），位图直接解
    if path.is_file() {
        sources.push(TransitionSource {
            path: path.to_path_buf(),
            kind: TransitionKind::PhotoFile,
            origin: match base {
                raybend::store::develop::EditBase::Raw => "preview-raw",
                raybend::store::develop::EditBase::Sooc => "preview-bitmap",
            },
        });
    }

    sources
}

/// **过渡帧的取图计划**（能找到哪个用哪个，顺序即优先级，见 [`transition_sources`]）：
///
/// 1. `latest` —— 大图缓存里**与当前编辑基准同一侧**的那一份（编辑过的照片才有；
///    基准进文件名，所以这里拿到的不会另一侧的图，见 `IMAGING.md` §4.3）；
/// 2. `SOOC` —— 相机直出的位图（同名 JPG）；
/// 3. 要编的这张文件本身 —— RAW 走 worker 的内嵌预览，位图直接解。
///
/// 不在任何库里（源目录直接进编辑这种）或一个候选都没有 → `None`：
/// 不发过渡帧，走老路（视口等真帧）。
fn transition_plan<R: Runtime>(app: &AppHandle<R>, path: &str) -> Option<TransitionPlan> {
    let path = Path::new(path);
    let asset = crate::develop::resolve_asset(app, path)?;
    let base = raybend::store::develop::EditBase::of_file(path);

    let signature = raybend::media::source::source_signature(path).ok();
    let latest_cache =
        FullCache::open(&asset.root)
            .ok()
            .zip(signature)
            .map(|(cache, signature)| {
                cache.path_for(
                    asset.asset_id,
                    &FullCache::source_name("latest", &signature),
                    base,
                    raybend::thumbnail::render::PIPELINE_VERSION,
                )
            });
    let sooc = crate::develop::source_path_of(app, &asset, raybend::store::develop::EditBase::Sooc);
    let sources = transition_sources(path, base, latest_cache, sooc.clone());
    if sources.is_empty() {
        return None;
    }

    // 原图尺寸（**已按方向摆正**）：过渡帧的逻辑尺寸用它，真帧回来时不会 refit。
    // 读的是文件头（`read_photo_meta` 只读头，不整图解码），毫秒级。
    let source_size = raybend::media::meta::read_photo_meta(path)
        .ok()
        .filter(raybend::media::meta::PhotoMeta::has_size)
        .map(|meta| (meta.width, meta.height));

    Some(TransitionPlan {
        sources,
        source_size,
        sooc: sooc.filter(|path| path.is_file()),
    })
}

/// 发一条视口意图（前端**不做坐标数学**，只把看到的原始值报过来）。
/// **显影参数**（拉杆 / 曲线）变了。
///
/// 前端在**每一帧合并后**发（拖动中每帧一条完全没问题：渲染线程按最新者优先丢弃过期任务，
/// 见 `lib/editor-intent.ts` 那套「合并 + 尾样本」的口径）。
///
/// 非法值在这里**当面报错**（未知 id / NaN / 超范围 / 坏曲线），不静默夹取 ——
/// 一个 NaN 悄悄过去整张图就变黑了，而界面上看不出原因。
///
/// # Errors
/// 参数不合法时返回原因；渲染器还没起来时**不算错误**（返回当前状态，前端不必特判）。
#[tauri::command]
pub async fn editor_set_params<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, EditorState>,
    repository_id: Option<String>,
    asset_id: Option<i64>,
    params: DevelopParamsDto,
) -> Result<RenderState, String> {
    let request = state.params_request.begin()?;
    let interactive = params.interactive;
    let geometry = params.geometry.filter(|value| !value.is_identity());
    if let Some(value) = geometry
        && (!value.rotation.is_finite()
            || value.rotation.abs() > 360.0
            || value.crop.is_some_and(|crop| !crop.valid())
            || value.crop_ratio.is_some_and(|setting| !setting.valid()))
    {
        return Err("成片几何参数无效".into());
    }
    let nr_method = params
        .nr_method
        .as_deref()
        .map(NrMethod::parse)
        .unwrap_or(Some(NrMethod::Fast))
        .ok_or_else(|| "未知的降噪方式".to_string())?;
    let lut_id = if params.lut_enabled == Some(true) {
        Some(params.lut_id.clone().ok_or("启用 LUT 时必须选择一支 LUT")?)
    } else {
        None
    };
    let lut = if let Some(id) = lut_id {
        let cache = Arc::clone(&state.lut_cache);
        let handle = app.clone();
        Some(
            crate::source::blocking(move || {
                let mut cached = cache.lock().map_err(|_| "LUT 缓存锁中毒".to_string())?;
                if let Some((stored_id, value)) = cached.as_ref()
                    && *stored_id == id
                {
                    return Ok(Arc::clone(value));
                }
                let value = crate::lut::resolve(&handle, &id)?;
                *cached = Some((id, Arc::clone(&value)));
                Ok(value)
            })
            .await?,
        )
    } else {
        None
    };
    let key = repository_id
        .zip(asset_id)
        .map(|(repository_id, asset_id)| {
            (
                repository_id,
                asset_id,
                params.lens_profile.clone(),
                params.lens_enabled,
            )
        });
    let (parsed, curves) = params.into_parts()?;
    // catalog 读取和 lensfun 解析不能占住 Tauri 的同步命令线程。拖动期间
    // 相同的镜头选择命中缓存；等待解析的旧参数由请求序号丢弃。
    let lens = if let Some(key) = key {
        let cache = Arc::clone(&state.lens_cache);
        let handle = app.clone();
        crate::source::blocking(move || {
            let mut cached = cache.lock().map_err(|_| "镜头缓存锁中毒".to_string())?;
            if let Some((stored_key, value)) = cached.as_ref()
                && *stored_key == key
            {
                return Ok(value.clone());
            }
            let value =
                crate::lens::render_correction(&handle, &key.0, key.1, key.2.as_deref(), key.3);
            *cached = Some((key, value.clone()));
            Ok(value)
        })
        .await?
    } else {
        None
    };
    state.params_request.send_if_latest(request, || {
        if let Some(sender) = session_sender(&state) {
            sender
                .send(RenderCommand::SetParams {
                    nr_method,
                    params: parsed,
                    curves,
                    interactive,
                    lens: lens.map(Box::new),
                    lut,
                    geometry,
                })
                .map_err(|_| "渲染线程不在了".to_string())?;
        }
        Ok(())
    })?;
    current_render_state(&state)
}

#[tauri::command]
pub async fn editor_confirm_tool(state: State<'_, EditorState>) -> Result<EditGeometry, String> {
    let sender = session_sender(&state).ok_or_else(|| "渲染线程还没起来".to_string())?;
    let (reply, receiver) = channel();
    sender
        .send(RenderCommand::ConfirmTool(reply))
        .map_err(|_| "渲染线程不在了".to_string())?;
    crate::source::blocking(move || receiver.recv().map_err(|_| "渲染线程不在了".to_string())?)
        .await
}

#[tauri::command]
pub fn editor_viewport_intent(
    state: State<'_, EditorState>,
    intent: ViewportIntent,
) -> Result<(), String> {
    let sender = session_sender(&state).ok_or_else(|| "渲染线程还没起来".to_string())?;
    // 高频指针只入队；此刻复制的快照还没应用该意图，返回它既费时又会误导调用方。
    sender
        .send(RenderCommand::Intent(intent))
        .map_err(|_| "渲染线程不在了".to_string())
}

/// 按命名定稿自身的 1920 快照设置对比左侧；在后台读缓存/解 AVIF。
#[tauri::command]
pub async fn editor_set_reference_issue<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, EditorState>,
    repository_id: String,
    asset_id: i64,
    issue_id: i64,
    photo_path: String,
    sequence: u64,
) -> Result<(), String> {
    let sender = session_sender(&state).ok_or_else(|| "渲染线程还没起来".to_string())?;
    let handle = app.clone();
    crate::source::blocking(move || {
        let bytes = crate::issues::preview_bytes(&handle, &repository_id, asset_id, issue_id)?;
        let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Avif)
            .map_err(|error| format!("定稿对比预览解码失败：{error}"))?
            .to_rgb8();
        let frame = ReferenceFrame::from_rgb(image.width(), image.height(), image.into_raw())
            .ok_or("定稿对比预览尺寸无效")?;
        sender
            .send(RenderCommand::ReferenceIssue {
                frame: Arc::new(frame),
                issue_id,
                photo_path,
                sequence,
            })
            .map_err(|_| "渲染线程不在了".to_string())
    })
    .await
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
        // 同尺寸移动仅请求重呈现，不能重配 swapchain。防止透出桌面的根本约束
        // 是产品 surface 使用 Opaque；这里的补帧只负责唤醒被遮挡后恢复的视图。
        let command = match event {
            tauri::WindowEvent::Resized(size) => Some(RenderCommand::Resize {
                width: size.width,
                height: size.height,
            }),
            tauri::WindowEvent::Moved(_) => Some(RenderCommand::Redraw),
            _ => None,
        };
        if let Some(command) = command {
            let sender = sink.lock().ok().and_then(|slot| slot.clone());
            if let Some(sender) = sender {
                let _ = sender.send(command);
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
        SurfaceComposition::Opaque,
        PresentationAdapter::for_editor(),
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
        state.note(format!(
            "渲染器就绪（{} / {} / {} / {}）",
            adapter.backend, details.format, details.alpha_mode, details.presentation
        ));
    }

    // 显影线程：与这一轮会话同生共死（Stop 之后由通道断开自然退出）
    let (develop_sender, develop_receiver) = channel::<DevelopJob>();
    let develop_commands = commands.clone();
    let max_texture = context.max_texture_dimension_2d();
    let developer = std::thread::Builder::new()
        .name("editor-develop".to_string())
        .spawn(move || develop_loop(develop_receiver, develop_commands, max_texture));

    let exit = session_loop(&mut context, receiver, shared, &develop_sender);
    drop(develop_sender); // 让显影线程看到通道关闭
    if let Ok(handle) = developer {
        let _ = handle.join();
    }
    exit
}

/// 会话主线程手里那份「当前参数」（渲染线程算 job 时要用）。
#[derive(Debug, Clone)]
struct SessionParams {
    nr_method: NrMethod,
    params: DevelopParams,
    curves: CurveSet,
    /// 已解析的镜头校正（配置文件那一半；`None` = 不用配置文件）。
    /// **手动微调不在这里** —— 它每次从 `params` 里合并（那样拖动才是实时的）。
    lens: Option<raybend::develop::lens::LensCorrection>,
    lut: Option<Arc<raybend::develop::lut::Lut>>,
    interactive: bool,
    geometry: Option<EditGeometry>,
    tool: Option<ToolDraft>,
    prefer_sooc: bool,
    reference_issue: Option<(Arc<ReferenceFrame>, i64)>,
    reference_sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolKind {
    Crop,
    Rotate,
}

fn crop_after_ratio_change(
    source: (u32, u32),
    rotation: f32,
    current: CropRect,
    ratio: f32,
) -> CropRect {
    let current_ratio = current.width * source.0 as f32 / (current.height * source.1.max(1) as f32);
    if (current_ratio - ratio).abs() <= ratio * 0.001 {
        current
    } else {
        largest_centered_rect(source, rotation, ratio)
    }
}

#[derive(Debug, Clone, Copy)]
struct CropDrag {
    handle: CropHandle,
    start: CropRect,
    pointer: (f32, f32),
}

#[derive(Debug, Clone, Copy)]
struct ToolDraft {
    kind: ToolKind,
    geometry: EditGeometry,
    crop: CropRect,
    ratio: Option<f32>,
    drag: Option<CropDrag>,
    line_start: Option<(f32, f32)>,
    line_end: Option<(f32, f32)>,
}

impl Default for SessionParams {
    fn default() -> Self {
        Self {
            nr_method: NrMethod::Fast,
            params: DevelopParams::new(None),
            curves: CurveSet::identity(),
            lens: None,
            lut: None,
            interactive: false,
            geometry: None,
            tool: None,
            prefer_sooc: true,
            reference_issue: None,
            reference_sequence: 0,
        }
    }
}

/// 一批窗口事件只保留最后一个尺寸；其它命令原样继续处理。
fn defer_resize(command: RenderCommand, pending: &mut Option<(u32, u32)>) -> Option<RenderCommand> {
    match command {
        RenderCommand::Resize { width, height } => {
            *pending = Some((width, height));
            None
        }
        other => Some(other),
    }
}

/// 没有成功呈现就保留补帧需求。surface 重配/设备恢复下一帧重试；
/// 被遮挡/最小化/取帧超时退到 5Hz，既不忙转，也不因漏掉窗口事件永久空白。
/// 成功后完全休眠；新命令会立刻唤醒任何一档等待。
fn frame_retry_delay(
    outcome: &Result<RenderOutcome, raybend::render::GpuError>,
) -> Option<Duration> {
    match outcome {
        Ok(RenderOutcome::Drawn) => None,
        Ok(RenderOutcome::Skipped) => Some(Duration::from_millis(200)),
        Ok(RenderOutcome::Reconfigured(_)) | Err(_) => Some(Duration::from_millis(16)),
    }
}

/// 会话主循环：吃命令 → 画一帧 → 报状态。
fn session_loop(
    context: &mut GpuContext,
    receiver: &Receiver<RenderCommand>,
    shared: &Arc<Mutex<RenderState>>,
    developer: &Sender<DevelopJob>,
) -> SessionExit {
    let mut dirty = true;
    let mut retry_delay = Duration::ZERO;
    let mut latest_job: u64 = 0;
    let mut consecutive_errors: u32 = 0;
    let mut session = SessionParams::default();

    loop {
        let mut pending_resize = None;
        let received = if dirty {
            receiver.recv_timeout(retry_delay)
        } else {
            receiver.recv().map_err(|_| RecvTimeoutError::Disconnected)
        };
        match received {
            Ok(RenderCommand::Stop) => return SessionExit::Stopped,
            Ok(command) => {
                if let Some(command) = defer_resize(command, &mut pending_resize)
                    && let Err(error) = apply_command(
                        command,
                        context,
                        shared,
                        developer,
                        &mut latest_job,
                        &mut dirty,
                        &mut session,
                    )
                    && let Ok(mut state) = shared.lock()
                {
                    state.last_error = Some(error.clone());
                    state.note(format!("命令被拒：{error}"));
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return SessionExit::Stopped,
        }

        // 连续拖动时队列可能一直不空；定期画一帧，不以排空队列为前提。
        for _ in 0..MAX_COMMANDS_PER_FRAME {
            match receiver.try_recv() {
                Ok(RenderCommand::Stop) | Err(TryRecvError::Disconnected) => {
                    return SessionExit::Stopped;
                }
                Ok(command) => {
                    if let Some(command) = defer_resize(command, &mut pending_resize)
                        && let Err(error) = apply_command(
                            command,
                            context,
                            shared,
                            developer,
                            &mut latest_job,
                            &mut dirty,
                            &mut session,
                        )
                        && let Ok(mut state) = shared.lock()
                    {
                        state.last_error = Some(error.clone());
                    }
                }
                Err(TryRecvError::Empty) => break,
            }
        }

        // 窗口拖动会连续发来几十个尺寸。只按这一批的最终尺寸重配
        // swapchain：逐条 configure 会把 Windows 移窗循环和 GPU 互相卡住。
        if let Some((width, height)) = pending_resize
            && let Err(error) = apply_command(
                RenderCommand::Resize { width, height },
                context,
                shared,
                developer,
                &mut latest_job,
                &mut dirty,
                &mut session,
            )
        {
            let mut state = lock_state(shared);
            state.last_error = Some(error);
        }

        if !dirty {
            continue;
        }
        // 照片与工具顶点取同一次会话状态，在同一 pass 中呈现。
        let tool_overlay = session.tool.and_then(|tool| {
            let state = lock_state(shared);
            let size = state.original_image?;
            if state.image != Some(size) {
                return None;
            }
            Some(raybend::render::overlay::ToolOverlay {
                crop: tool.crop,
                source: (size.width as u32, size.height as u32),
                handles: tool.kind == ToolKind::Crop,
                straighten: tool.line_start.zip(tool.line_end).map(|(a, b)| [a, b]),
            })
        });
        context.set_tool_overlay(tool_overlay);
        let outcome = context.render();
        let retry = frame_retry_delay(&outcome);
        dirty = retry.is_some();
        retry_delay = retry.unwrap_or(Duration::ZERO);

        match outcome {
            Ok(RenderOutcome::Drawn) => {
                consecutive_errors = 0;
                let mut state = lock_state(shared);
                /*
                 * 「画出来了」= **真的有一张图在纹理里**（`has_image`）且当前确实选了照片。
                 *
                 * ❗ 不能只看 `photo_path.is_some()`：`SetPhoto` 之后、解码回来之前
                 * 也会画帧（只有洞口底色），那时标上 `painted_path` 会让前端立刻把洞口切透明，
                 * 而且 `editorViewportNotice` 的「paintedPath 有值就不提示」会**压掉载入提示** ——
                 * 用户看到的就是一个空洞口、什么信息都没有（2026-09-23 人类报的
                 * 「视口空、点图片不显示」有这一份）。
                 */
                if state.photo_path.is_some() && context.has_image() {
                    state.painted_path = state.photo_path.clone();
                }
                // 出图正常 ⇒ 清掉当前错误（spike 的「红字永远挂着」教训）
                if state.last_error.take().is_some() {
                    state.note("已恢复：重新出图");
                }
                publish(&mut state, context, &session);
            }
            Ok(RenderOutcome::Skipped) => {
                let mut state = lock_state(shared);
                publish(&mut state, context, &session);
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
#[allow(clippy::too_many_arguments)]
fn apply_command(
    command: RenderCommand,
    context: &mut GpuContext,
    shared: &Arc<Mutex<RenderState>>,
    developer: &Sender<DevelopJob>,
    latest_job: &mut u64,
    dirty: &mut bool,
    session: &mut SessionParams,
) -> Result<(), String> {
    match command {
        RenderCommand::Stop => Ok(()),
        RenderCommand::Redraw => {
            *dirty = true;
            Ok(())
        }
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
            if let Some(colors) = args.overlay_colors {
                let [Some(line), Some(halo), Some(crop), Some(rotate)] =
                    colors.map(|color| raybend::render::Srgb8::parse_css(&color))
                else {
                    return Err("工具覆盖层颜色无效".into());
                };
                context.set_overlay_palette(raybend::render::overlay::OverlayPalette {
                    line,
                    halo,
                    crop,
                    rotate,
                });
            }
            let previous = context
                .viewport()
                .clip_rect
                .map(|rect| (rect.width, rect.height));
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
            ensure_output(context, shared, developer, latest_job, session);
            *dirty = true;
            Ok(())
        }
        RenderCommand::SetPhoto { path, transition } => {
            let mut state = lock_state(shared);
            if state.photo_path != path {
                session.reference_issue = None;
            }
            match path {
                Some(path) => {
                    // 旧纹理可能是整张 RAW 的 RGBA。换图时马上释放，避免把旧图
                    // 误报成新图，也压低新解码与旧 GPU 纹理叠加时的内存峰值。
                    if state.photo_path.as_deref() != Some(path.as_str()) {
                        context.clear_image();
                        state.painted_path = None;
                        state.image = None;
                        state.original_image = None;
                        state.origin = None;
                        state.histogram = None;
                        state.reference_ready = false;
                        state.reference_base = None;
                        state.nr_pending = false;
                        state.nr_error = None;
                    }
                    state.photo_path = Some(path.clone());
                    state.decode = "loading".to_string();
                    state.decode_error = None;
                    state.wanted_tier = Some(ImageTier::Preview);
                    state.as_shot_temperature = None;
                    state.tier = None;
                    state.history_photo(&path);
                    *latest_job += 1;
                    let job = DevelopJob {
                        nr_method: session.nr_method,
                        id: *latest_job,
                        rev: state.params_rev,
                        photo: Some(path),
                        tier: ImageTier::Preview,
                        interactive: session.interactive,
                        params: session.params.clone(),
                        lens: session.lens.clone(),
                        lut: session.lut.clone(),
                        curves: session.curves.clone(),
                        geometry: if session.tool.is_some() {
                            None
                        } else {
                            session.geometry
                        },
                        prefer_sooc: session.prefer_sooc,
                        transition,
                    };
                    drop(state);
                    if developer.send(job).is_err() {
                        return Err("显影线程不在了".to_string());
                    }
                }
                None => {
                    context.clear_image();
                    // 抬任务号：在途的结果必须作废（否则它回来时又把这幅画上去）
                    *latest_job += 1;
                    state.photo_path = None;
                    state.painted_path = None;
                    state.image = None;
                    state.original_image = None;
                    state.tier = None;
                    state.wanted_tier = None;
                    state.origin = None;
                    state.histogram = None;
                    state.reference_ready = false;
                    state.reference_base = None;
                    state.nr_pending = false;
                    state.nr_error = None;
                    state.decode = "idle".to_string();
                    state.decode_error = None;
                    state.as_shot_temperature = None;
                    drop(state);
                }
            }
            *dirty = true;
            Ok(())
        }
        RenderCommand::ReferenceIssue {
            frame,
            issue_id,
            photo_path,
            sequence,
        } => {
            let current_path = lock_state(shared).photo_path.clone();
            if !accept_issue_reference(
                sequence,
                session.reference_sequence,
                &photo_path,
                current_path.as_deref(),
            ) {
                return Ok(());
            }
            session.reference_sequence = sequence;
            session.reference_issue = Some((Arc::clone(&frame), issue_id));
            if let Some(image) = RenderImage::from_rgb8(frame.width, frame.height, &frame.rgb) {
                context.set_reference_image(frame.id, image);
                let mut state = lock_state(shared);
                state.reference_ready = true;
                state.reference_base = Some(format!("issue:{issue_id}"));
                *dirty = true;
            }
            Ok(())
        }
        RenderCommand::SetParams {
            nr_method,
            params,
            curves,
            interactive,
            lens,
            lut,
            geometry,
        } => {
            session.nr_method = nr_method;
            session.params = params;
            session.curves = curves;
            session.lens = lens.map(|boxed| *boxed);
            session.lut = lut;
            session.interactive = interactive;
            session.geometry = geometry;
            let mut state = lock_state(shared);
            state.params_rev += 1;
            let rev = state.params_rev;
            let path = state.photo_path.clone();
            if path.is_some() {
                // 档位（人类 2026-09-24）：**拖动中一律预览档** —— 1:1 下也不做全尺寸；
                // 松手那一下按缩放重新算（要全尺寸就给全尺寸）。见 `tier_for_params`。
                let wanted = tier_for_params(
                    interactive,
                    context.viewport().zoom,
                    state.tier.unwrap_or(ImageTier::Preview),
                );
                state.wanted_tier = Some(wanted);
                state.decode = "loading".to_string();
                *latest_job += 1;
                let job = DevelopJob {
                    nr_method: session.nr_method,
                    id: *latest_job,
                    rev,
                    photo: None,
                    tier: wanted,
                    interactive,
                    params: session.params.clone(),
                    lens: session.lens.clone(),
                    lut: session.lut.clone(),
                    curves: session.curves.clone(),
                    geometry: if session.tool.is_some() {
                        None
                    } else {
                        session.geometry
                    },
                    prefer_sooc: session.prefer_sooc,
                    // 参数任务不发过渡帧（图已经在屏幕上，只是要重算）
                    transition: None,
                };
                drop(state);
                if developer.send(job).is_err() {
                    return Err("显影线程不在了".to_string());
                }
            }
            *dirty = true;
            Ok(())
        }
        RenderCommand::DenoiseReady => {
            if session.nr_method != NrMethod::High || session.interactive {
                return Ok(());
            }
            let state = lock_state(shared);
            let Some(path) = state.photo_path.clone() else {
                return Ok(());
            };
            *latest_job += 1;
            let job = DevelopJob {
                nr_method: session.nr_method,
                id: *latest_job,
                rev: state.params_rev,
                photo: Some(path),
                tier: state
                    .wanted_tier
                    .or(state.tier)
                    .unwrap_or(ImageTier::Preview),
                interactive: false,
                params: session.params.clone(),
                lens: session.lens.clone(),
                lut: session.lut.clone(),
                curves: session.curves.clone(),
                geometry: if session.tool.is_some() {
                    None
                } else {
                    session.geometry
                },
                prefer_sooc: session.prefer_sooc,
                transition: None,
            };
            drop(state);
            developer
                .send(job)
                .map_err(|_| "显影线程不在了".to_string())
        }
        RenderCommand::ConfirmTool(reply) => {
            let result = match session.tool {
                Some(tool) => {
                    let mut geometry = tool.geometry;
                    geometry.crop = Some(tool.crop);
                    let size = lock_state(shared)
                        .original_image
                        .map(|s| (s.width as u32, s.height as u32));
                    match size {
                        Some(size) => geometry.validate(size).map(|()| geometry),
                        None => Err("照片还未就绪".into()),
                    }
                }
                None => Err("没有正在操作的裁切或旋转工具".into()),
            };
            if let Ok(geometry) = result {
                session.geometry = Some(geometry);
                session.tool = None;
                queue_geometry_job(context, shared, developer, latest_job, session)?;
                *dirty = true;
            }
            let _ = reply.send(result);
            Ok(())
        }
        RenderCommand::Developed(outcome) => {
            if outcome.id != *latest_job {
                return Ok(()); // 过期结果：丢掉（换照片/换参数之后的旧任务）
            }
            /*
             * **过渡帧**（进编辑先出图，handoff §3）：把图先画上去，但**状态留在「载入中」**——
             * 真帧还在路上，不许把它当「算完了」：
             *
             * * 不清 `wanted_tier`（真帧还要按它出）；
             * * 不置 `decode = ready`（否则载入提示提前撤掉、界面骗人）；
             * * 不动 `applied_params_rev`（这张不是按当前参数算出来的）；
             * * 不调 `ensure_output`（它不是一帧「已就绪」的结果，别拿它去触发档位切换）。
             *
             * 逻辑尺寸照旧用**原图尺寸**（`source_width/height`）——
             * 与真帧同一个值，真帧回来时 `set_image` 直接早退，**连 refit 都不会发生**。
             */
            if outcome.transition {
                let mut state = lock_state(shared);
                if let Ok(image) = outcome.result {
                    let source_size = (image.source_width, image.source_height);
                    if let Some(render_image) =
                        RenderImage::from_rgb8(image.width, image.height, &image.rgb)
                    {
                        context.set_image(render_image, source_size);
                        state.image = Some(SizeDto {
                            width: source_size.0 as f64,
                            height: source_size.1 as f64,
                        });
                        state.original_image = Some(SizeDto {
                            width: image.original_width as f64,
                            height: image.original_height as f64,
                        });
                        state.origin = Some(outcome.origin);
                    }
                }
                *dirty = true;
                return Ok(());
            }
            let mut state = lock_state(shared);
            let (reference, reference_base) = match &session.reference_issue {
                Some((frame, issue_id)) => {
                    (Some(Arc::clone(frame)), Some(format!("issue:{issue_id}")))
                }
                None => (
                    outcome.reference,
                    outcome.reference_base.map(str::to_string),
                ),
            };
            if reference.is_some() {
                state.reference_ready = true;
                state.reference_base = reference_base;
            }
            if let Some(reference) = reference
                && context.reference_id() != Some(reference.id)
                && let Some(image) =
                    RenderImage::from_rgb8(reference.width, reference.height, &reference.rgb)
            {
                context.set_reference_image(reference.id, image);
            }
            state.nr_pending = outcome.nr_pending;
            state.nr_error = outcome.nr_error;
            state.wanted_tier = None;
            state.decode_ms = outcome.decode_ms;
            state.develop_ms = Some(outcome.develop_ms);
            match outcome.result {
                Ok(image) => {
                    let source_size = (image.source_width, image.source_height);
                    let Some(render_image) =
                        RenderImage::from_rgb8(image.width, image.height, &image.rgb)
                    else {
                        state.decode = "error".to_string();
                        state.decode_error = Some(format!(
                            "显影结果的尺寸与字节数对不上：{}×{}",
                            image.width, image.height
                        ));
                        return Ok(());
                    };
                    /*
                     * 只换纹理，**不重摆视口**：`set_image` 只在逻辑尺寸真的变了
                     * （换照片）时才 `refit`；预览档 ↔ 全尺寸档之间逻辑尺寸不变，
                     * 几何必须原地不动 —— 否则「切到 1:1 要等全图算完」那段会跳一下。
                     */
                    let previous_size = context.viewport().image_size;
                    context.set_image(render_image, source_size);
                    if let Some(tool) = session.tool {
                        context.viewport_mut().rotation = tool.geometry.rotation;
                        if previous_size != source_size {
                            context.viewport_mut().refit_rotated();
                        }
                    } else {
                        context.viewport_mut().rotation = 0.0;
                    }
                    state.photo_path = Some(outcome.path);
                    // 「尺寸」是**原图**尺寸，不是当前渲染档位的尺寸
                    state.image = Some(SizeDto {
                        width: source_size.0 as f64,
                        height: source_size.1 as f64,
                    });
                    state.original_image = Some(SizeDto {
                        width: image.original_width as f64,
                        height: image.original_height as f64,
                    });
                    state.tier = Some(outcome.tier);
                    state.origin = Some(outcome.origin);
                    if let Some(histogram) = outcome.histogram {
                        state.histogram = Some(histogram);
                    }
                    state.decode = "ready".to_string();
                    state.decode_error = None;
                    state.applied_params_rev = outcome.rev;
                    if outcome.as_shot_temperature.is_some() {
                        state.as_shot_temperature = outcome.as_shot_temperature;
                    }
                    drop(state);
                    ensure_output(context, shared, developer, latest_job, session);
                }
                Err(error) => {
                    state.decode = "error".to_string();
                    state.decode_error = Some(error);
                    state.applied_params_rev = outcome.rev;
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
                ViewportIntent::SetCompare { enabled } => {
                    context.set_compare_fraction(enabled.then_some(0.5));
                }
                ViewportIntent::ComparePointer { phase, x, y } => {
                    if !matches!(phase.as_str(), "down" | "move" | "up" | "cancel") {
                        return Err(format!("未知的对比指针阶段：{phase}"));
                    }
                    if !x.is_finite() || !y.is_finite() {
                        return Err("对比指针坐标无效".into());
                    }
                    if !context.compare_pointer(&phase, (x, y)) {
                        return Ok(());
                    }
                }
                ViewportIntent::SetReferenceBase { base, sequence } => {
                    let prefer = match base.as_str() {
                        "sooc" => true,
                        "raw" => false,
                        _ => return Err("未知的对比参照来源".into()),
                    };
                    if sequence >= session.reference_sequence {
                        session.reference_sequence = sequence;
                        let changed =
                            session.prefer_sooc != prefer || session.reference_issue.is_some();
                        session.prefer_sooc = prefer;
                        session.reference_issue = None;
                        if changed {
                            queue_geometry_job(context, shared, developer, latest_job, session)?;
                        }
                    }
                }
                ViewportIntent::SetTool {
                    tool,
                    initial_ratio,
                } => {
                    if initial_ratio
                        .is_some_and(|value| !value.is_finite() || value <= 0.0 || value > 100.0)
                    {
                        return Err("裁切比例无效".into());
                    }
                    let kind = match tool.as_deref() {
                        Some("crop") => Some(ToolKind::Crop),
                        Some("rotate") => Some(ToolKind::Rotate),
                        None => None,
                        _ => return Err("未知的编辑工具".into()),
                    };
                    if kind != session.tool.map(|draft| draft.kind) {
                        session.tool = kind.map(|kind| {
                            let geometry = session.geometry.unwrap_or_default();
                            let size = lock_state(shared)
                                .original_image
                                .map(|s| (s.width as u32, s.height as u32))
                                .unwrap_or((1, 1));
                            ToolDraft {
                                kind,
                                geometry,
                                crop: geometry.output_rect(size),
                                ratio: if kind == ToolKind::Crop {
                                    initial_ratio
                                } else {
                                    None
                                },
                                drag: None,
                                line_start: None,
                                line_end: None,
                            }
                        });
                        queue_geometry_job(context, shared, developer, latest_job, session)?;
                    }
                }
                ViewportIntent::SetCropRatio { ratio } => {
                    if ratio
                        .is_some_and(|value| !value.is_finite() || value <= 0.0 || value > 100.0)
                    {
                        return Err("裁切比例无效".into());
                    }
                    if let Some(tool) = session
                        .tool
                        .as_mut()
                        .filter(|tool| tool.kind == ToolKind::Crop)
                    {
                        tool.ratio = ratio;
                        if let Some(ratio) = ratio {
                            let size = lock_state(shared)
                                .original_image
                                .map(|s| (s.width as u32, s.height as u32))
                                .unwrap_or((1, 1));
                            // 重进已确认的裁切时，面板会同步一次比例；同一比例不能
                            // 把原位置的框重新居中，否则框外内容也无法向外扩展。
                            tool.crop = crop_after_ratio_change(
                                size,
                                tool.geometry.rotation,
                                tool.crop,
                                ratio,
                            );
                        }
                    }
                }
                ViewportIntent::SetRotation { degrees } => {
                    if !degrees.is_finite() || degrees.abs() > 360.0 {
                        return Err("旋转角度无效".into());
                    }
                    if let Some(tool) = session
                        .tool
                        .as_mut()
                        .filter(|tool| tool.kind == ToolKind::Rotate)
                    {
                        let size = lock_state(shared)
                            .original_image
                            .map(|s| (s.width as u32, s.height as u32))
                            .unwrap_or((1, 1));
                        let aspect =
                            tool.crop.width * size.0 as f32 / (tool.crop.height * size.1 as f32);
                        tool.geometry.rotation = degrees;
                        tool.crop = largest_centered_rect(size, degrees, aspect);
                        context.viewport_mut().rotation = degrees;
                        context.viewport_mut().refit_rotated();
                    }
                }
                ViewportIntent::ToolPointer { phase, x, y } => {
                    if !matches!(phase.as_str(), "down" | "move" | "up" | "cancel")
                        || !x.is_finite()
                        || !y.is_finite()
                    {
                        return Err("工具指针事实无效".into());
                    }
                    if let Some(tool) = session.tool.as_mut() {
                        let size = lock_state(shared)
                            .original_image
                            .map(|s| (s.width as u32, s.height as u32))
                            .unwrap_or((1, 1));
                        let point = context.viewport().frame_pointer_normalized((x, y), size);
                        match (tool.kind, phase.as_str()) {
                            (ToolKind::Crop, "down") => {
                                tool.drag = point.and_then(|p| {
                                    hit_crop(
                                        tool.crop,
                                        p,
                                        context.viewport().frame_hit_tolerance(10.0, size),
                                    )
                                    .map(|handle| CropDrag {
                                        handle,
                                        start: tool.crop,
                                        pointer: p,
                                    })
                                });
                            }
                            (ToolKind::Crop, "move" | "up") => {
                                if let (Some(drag), Some(p)) = (tool.drag, point) {
                                    tool.crop = drag_crop(
                                        size,
                                        tool.geometry.rotation,
                                        drag.start,
                                        drag.handle,
                                        (p.0 - drag.pointer.0, p.1 - drag.pointer.1),
                                        tool.ratio,
                                    );
                                }
                                if phase == "up" {
                                    tool.drag = None;
                                }
                            }
                            (ToolKind::Crop, "cancel") => {
                                tool.drag = None;
                            }
                            (ToolKind::Rotate, "down") => {
                                tool.line_start = Some((x, y));
                                tool.line_end = Some((x, y));
                            }
                            (ToolKind::Rotate, "move") => {
                                if tool.line_start.is_some() {
                                    tool.line_end = Some((x, y));
                                }
                            }
                            (ToolKind::Rotate, "up") => {
                                tool.line_end = None;
                                if let Some(start) = tool.line_start.take() {
                                    let dx = x - start.0;
                                    let dy = y - start.1;
                                    if dx.hypot(dy) >= 8.0 {
                                        let angle = (tool.geometry.rotation
                                            - dy.atan2(dx).to_degrees())
                                        .clamp(-360.0, 360.0);
                                        let aspect = tool.crop.width * size.0 as f32
                                            / (tool.crop.height * size.1 as f32);
                                        tool.geometry.rotation = angle;
                                        tool.crop = largest_centered_rect(size, angle, aspect);
                                        context.viewport_mut().rotation = angle;
                                        context.viewport_mut().refit_rotated();
                                        lock_state(shared).tool_revision += 1;
                                    }
                                }
                            }
                            (ToolKind::Rotate, "cancel") => {
                                tool.line_start = None;
                                tool.line_end = None;
                            }
                            _ => {}
                        }
                    }
                }
                ViewportIntent::HitTest { x, y } => {
                    // 负数坐标 = 「没有指针」（前端在指针离开洞口时这么发）
                    let hit = if x < 0.0 || y < 0.0 {
                        None
                    } else {
                        context
                            .viewport()
                            .hit_test_css((x, y))
                            .map(|(px, py)| PointDto { x: px, y: py })
                    };
                    if let Ok(mut state) = shared.lock() {
                        state.last_hit = hit;
                    }
                    return Ok(());
                }
            }
            ensure_output(context, shared, developer, latest_job, session);
            *dirty = true;
            Ok(())
        }
    }
}

fn queue_geometry_job(
    context: &GpuContext,
    shared: &Arc<Mutex<RenderState>>,
    developer: &Sender<DevelopJob>,
    latest_job: &mut u64,
    session: &SessionParams,
) -> Result<(), String> {
    let mut state = lock_state(shared);
    let Some(path) = state.photo_path.clone() else {
        return Ok(());
    };
    *latest_job += 1;
    let tier = tier_for(
        context.viewport().zoom,
        state.tier.unwrap_or(ImageTier::Preview),
    );
    state.wanted_tier = Some(tier);
    state.decode = "loading".into();
    let job = DevelopJob {
        nr_method: session.nr_method,
        id: *latest_job,
        rev: state.params_rev,
        photo: Some(path),
        tier,
        interactive: false,
        params: session.params.clone(),
        lens: session.lens.clone(),
        lut: session.lut.clone(),
        curves: session.curves.clone(),
        geometry: if session.tool.is_some() {
            None
        } else {
            session.geometry
        },
        prefer_sooc: session.prefer_sooc,
        transition: None,
    };
    drop(state);
    developer
        .send(job)
        .map_err(|_| "显影线程不在了".to_string())
}

/// 需要换档位吗（缩放跨过 `1:1` 就要全尺寸输出）—— 需要就让显影线程重出一张。
///
/// 注意：**这里不重新解码**（线性源缓存在显影线程里），只换输出尺寸。
fn ensure_output(
    context: &GpuContext,
    shared: &Arc<Mutex<RenderState>>,
    developer: &Sender<DevelopJob>,
    latest_job: &mut u64,
    session: &SessionParams,
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
    let job = DevelopJob {
        nr_method: session.nr_method,
        id: *latest_job,
        rev: state.params_rev,
        photo: Some(path),
        tier: wanted,
        interactive: session.interactive,
        params: session.params.clone(),
        lens: session.lens.clone(),
        lut: session.lut.clone(),
        curves: session.curves.clone(),
        geometry: if session.tool.is_some() {
            None
        } else {
            session.geometry
        },
        prefer_sooc: session.prefer_sooc,
        // 换档位不发过渡帧：源已经在显影线程手里（这一步只是换输出尺寸）
        transition: None,
    };
    drop(state);
    let _ = developer.send(job);
}

/// 把渲染器的状态如实地搬进快照。
fn publish(state: &mut RenderState, context: &GpuContext, session: &SessionParams) {
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
    // 覆盖层矩阵：每次发布都算一遍（它只依赖视口状态，很便宜）
    state.overlay_transform = viewport.css_overlay_transform();
    state.comparing = context.compare_fraction().is_some();
    state.compare_line_css = context
        .compare_fraction()
        .and_then(|fraction| viewport.compare_css_x(fraction));
    state.tool_box_css = session.tool.and_then(|tool| {
        let size = state.original_image?;
        if state.image != Some(size) {
            return None;
        }
        viewport
            .frame_rect_css(tool.crop, (size.width as u32, size.height as u32))
            .map(|rect| RectDto {
                x: rect.x as f64,
                y: rect.y as f64,
                width: rect.width as f64,
                height: rect.height as f64,
            })
    });
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

/// **显影线程**：一次一张、**最新者优先**（过期的任务连做都不做）。
///
/// 两件事都在这条线程上：**解码线性源**（贵，换照片时才做）与**按参数算像素**
/// （便宜，拖拉杆时每帧一次）。源缓存在线程里 ⇒ 参数变化不必重新解码。
///
/// 为什么不在渲染线程上算：管线在 1:1 档要几十到一百多毫秒（实测见实施记录），
/// 放渲染线程会让**拖动/缩放一起卡**。放这里，渲染线程只管画上一张，永远跟手。
/// 两个排队中的任务合成一个：**取新的，但照片不能被旧的带走**。
///
/// ⚠️ 真栽过一次（2026-09-24「照片装载不出来，卡在正在载入照片」）：
/// 换照片时前端是**先后脚**发两条 —— `SetPhoto`（带路径）之后 `SetParams`
/// （`loadDevelop` 一回来就会推参数，`photo: None`）。而这里原先「只留最新的」，
/// 于是那条 `SetParams` 把 `SetPhoto` 整个顶掉 → 显影线程手里没有源 →
/// 下面那个 `continue` → **永远不发结果** → 界面卡在「正在载入照片」。
///
/// `photo: None` 的语义是「用缓存里的源重算」，不是「不要照片了」——
/// 清空照片走的是另一条路（`RenderCommand::SetPhoto { path: None }`，它不发任务）。
fn merge_jobs(older: DevelopJob, newer: DevelopJob) -> DevelopJob {
    DevelopJob {
        photo: newer.photo.or(older.photo),
        // 过渡帧计划跟着**照片**走：参数任务不带它（`None`），合并时不许把它丢了 ——
        // 丢了就变成「换照片的那条任务没过渡帧」，而那正是需要它的那一条。
        transition: newer.transition.or(older.transition),
        ..newer
    }
}

/// **显影线程**：吃任务 → 解码/管线 → 把结果交回渲染线程。
///
/// 每条任务都套 `catch_unwind`：管线里一次 panic 不许带走线程（`AGENTS.md` §7.9 同族教训 ——
/// 「线程静默死掉 → 拉什么杆都没反应」正是这么来的）。捕获之后：
///
/// * **丢掉缓存的线性源**（panic 可能发生在解码/分析中途，那份状态不可信，下一次任务重新解）；
/// * 把错误当作**这条任务的结果**交回渲染线程 → 界面上是可见的错误，而不是一张永远不动的旧帧。
fn develop_loop(receiver: Receiver<DevelopJob>, commands: Sender<RenderCommand>, max_texture: u32) {
    let notify = commands.clone();
    let mut high = HighDenoise::new(move || {
        let _ = notify.send(RenderCommand::DenoiseReady);
    });
    let mut cached: Option<CachedSource> = None;
    // 最近一次被告知要显示的照片（**粘住**：参数任务不该把「要看哪张」弄丢）
    let mut wanted_photo: Option<String> = None;

    while let Ok(job) = receiver.recv() {
        // 队列里躺着的时候就可能已经过期了：先看通道里还有没有更新的（合成，不是替换）
        let mut job = job;
        loop {
            match receiver.try_recv() {
                Ok(newer) => job = merge_jobs(job, newer),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        if let Some(path) = job.photo.clone() {
            wanted_photo = Some(path);
        }

        let outcome = match std::panic::catch_unwind(AssertUnwindSafe(|| {
            run_develop_job(
                &mut cached,
                &wanted_photo,
                &job,
                max_texture,
                &commands,
                &mut high,
            )
        })) {
            Ok(outcome) => outcome,
            Err(payload) => {
                cached = None; // 半路的状态不可信：下一次任务重新解码
                Some(DevelopOutcome {
                    nr_pending: false,
                    nr_error: None,
                    reference: None,
                    reference_base: None,
                    id: job.id,
                    rev: job.rev,
                    path: job
                        .photo
                        .clone()
                        .or_else(|| wanted_photo.clone())
                        .unwrap_or_default(),
                    tier: job.tier,
                    origin: "panic".to_string(),
                    transition: false,
                    as_shot_temperature: None,
                    decode_ms: None,
                    develop_ms: 0.0,
                    histogram: None,
                    result: Err(format!(
                        "显影线程内部错误（已捕获，线程继续）：{}",
                        panic_message(&payload)
                    )),
                })
            }
        };
        if let Some(outcome) = outcome
            && commands.send(RenderCommand::Developed(outcome)).is_err()
        {
            return; // 渲染线程走了
        }
    }
}

/// 一条显影任务（[`develop_loop`] 的实体）；`None` = 没有照片可算，跳过这条。
///
/// `commands`：**过渡帧直接从这儿发**（不等这条任务算完）——
/// 「先出图、再解码」的关键就在这一行的位置上：它必须在 `decode_linear_source` **之前**。
fn should_reload_source(
    cached_path: Option<&str>,
    cached_signature: Option<&str>,
    wanted_path: &str,
    observed_signature: Option<&str>,
    explicit_photo: bool,
) -> bool {
    cached_path != Some(wanted_path)
        || explicit_photo
            && (observed_signature.is_none() || observed_signature != cached_signature)
}
fn source_dependency_signature(path: &str, sooc: Option<&Path>) -> Option<String> {
    let mut signature = raybend::media::source::source_signature(Path::new(path)).ok()?;
    if let Some(sooc) = sooc.filter(|source| *source != Path::new(path))
        && let Ok(secondary) = raybend::media::source::source_signature(sooc) {
            signature.push('/');
            signature.push_str(&secondary);
        }
    Some(signature)
}

fn run_develop_job(
    cached: &mut Option<CachedSource>,
    wanted_photo: &Option<String>,
    job: &DevelopJob,
    max_texture: u32,
    commands: &Sender<RenderCommand>,
    high: &mut HighDenoise,
) -> Option<DevelopOutcome> {
    // ① 需要的话先解码线性源（换照片 / 上一张解失败过）
    let mut decode_ms = None;
    let mut origin = "bitmap".to_string();
    let signature = wanted_photo.as_deref().and_then(|path| {
        if job.photo.is_none() && cached.as_ref().is_some_and(|entry| entry.path == path) {
            return cached
                .as_ref()
                .and_then(|entry| entry.source_signature.clone());
        }
        let sooc = job
            .transition
            .as_ref()
            .and_then(|plan| plan.sooc.as_deref())
            .or_else(|| {
                cached
                    .as_ref()
                    .filter(|entry| entry.path == path)
                    .and_then(|entry| entry.sooc.as_deref())
            });
        source_dependency_signature(path, sooc)
    });
    if let Some(path) = wanted_photo.clone()
        && should_reload_source(
            cached.as_ref().map(|entry| entry.path.as_str()),
            cached
                .as_ref()
                .and_then(|entry| entry.source_signature.as_deref()),
            &path,
            signature.as_deref(),
            job.photo.is_some(),
        )
    {
        // 旧线性源在换图时已经无用；先释放，再解新 RAW。否则旧源、
        // worker 回传缓冲和新源会在解码瞬间同时驻留。
        high.clear();
        *cached = None;
        // ①′ **过渡帧**：真解码之前先发一张已经存在的图（handoff §3）——
        //     毫秒级 vs RAW 全解码的秒级，用户看到的是「先进先出图」。
        //     解不出来就什么都不发（不是错：真帧照样会来）。
        if let Some(plan) = job.transition.as_ref() {
            if let Some(frame) = transition_outcome(plan, job, &path) {
                if commands.send(RenderCommand::Developed(frame)).is_err() {
                    return None; // 渲染线程走了
                }
            } else {
                eprintln!(
                    "[editor] 过渡帧：{} 个候选一个都用不了（{path}）——直接解真帧",
                    plan.sources.len()
                );
            }
        }
        let started = std::time::Instant::now();
        match decode_linear_source(Path::new(&path), max_texture) {
            Ok(source) => {
                decode_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
                let as_shot_temperature = source.as_shot_temperature;
                origin = source.origin.clone();
                *cached = Some(CachedSource {
                    reference: ReferenceCache::default(),
                    sooc: job
                        .transition
                        .as_ref()
                        .and_then(|plan| plan.sooc.clone())
                        .or_else(|| {
                            (!matches!(
                                raybend::media::kind::kind_of_file(&path),
                                raybend::media::kind::MediaKind::Raw
                            ))
                            .then(|| std::path::PathBuf::from(&path))
                        }),
                    source_revision: job.id,
                    source_signature: signature,
                    path,
                    full: Arc::new(source.image),
                    preview: None,
                    analysis_source: None,
                    local_tone: None,
                    as_shot_temperature,
                    origin: origin.clone(),
                });
            }
            Err(error) => {
                return Some(DevelopOutcome {
                    nr_pending: false,
                    nr_error: None,
                    reference: None,
                    reference_base: None,
                    id: job.id,
                    rev: job.rev,
                    path,
                    tier: job.tier,
                    origin,
                    transition: false,
                    as_shot_temperature: None,
                    decode_ms: Some(started.elapsed().as_secs_f64() * 1000.0),
                    develop_ms: 0.0,
                    histogram: None,
                    result: Err(error),
                });
            }
        }
    }

    let Some(entry) = cached.as_mut() else {
        // 只有「还没让显示过任何照片」才会走到这里（开局那一条参数任务）。
        // 一旦要过照片，`wanted_photo` 就粘住了 —— 见 `merge_jobs` 的注释。
        eprintln!(
            "[editor] 参数任务先到、还没有照片可算（job #{}）：跳过",
            job.id
        );
        return None;
    };
    let as_shot_temperature = entry.as_shot_temperature;
    origin = entry.origin.clone();
    if let Some(geometry) = job.geometry
        && let Err(error) = geometry.validate((entry.full.width, entry.full.height))
    {
        return Some(DevelopOutcome {
            nr_pending: false,
            nr_error: None,
            reference: None,
            reference_base: None,
            id: job.id,
            rev: job.rev,
            path: entry.path.clone(),
            tier: job.tier,
            origin,
            transition: false,
            as_shot_temperature,
            decode_ms,
            develop_ms: 0.0,
            histogram: None,
            result: Err(error),
        });
    }
    let develop_started = Instant::now();

    // ②′ 可选阶段（镜头手动微调 / 降噪 / 锐化 / 动态反差强度）——
    //     用**完整线性源**的尺寸建（归一化坐标与档位无关，见 `develop/lens.rs` 模块头）。
    let plans = DevelopPlans::from_params(entry.full.width, entry.full.height, &job.params);

    // ② 动态反差：分析图只缩一次；分析结果只在**线性链参数**变了才重算。
    //    这一段必须放在取 source 之前（它要可变借 entry）。
    let local_strength = plans.local_tone;
    if local_strength > 0.0 {
        let key = Resolved::new(&job.params).chain_key();
        if entry
            .local_tone
            .as_ref()
            .is_none_or(|(cached_key, _)| *cached_key != key)
        {
            if entry.analysis_source.is_none() {
                let long = entry.full.width.max(entry.full.height);
                entry.analysis_source = Some(entry.full.downscaled_to((long / 4).max(256)));
            }
            if let Some(analysis) = entry.analysis_source.as_ref() {
                let chained = chain_image(analysis, &job.params);
                let state = LocalToneState::analyze(&chained, &LocalToneOpts::default());
                entry.local_tone = Some((key, state));
            }
        }
    }

    // ③ 按档位取源（预览档要缩一次，缩完缓存住）
    let (source, width, height) = match job.tier {
        ImageTier::Full => (&entry.full, entry.full.width, entry.full.height),
        ImageTier::Preview => {
            if entry.preview.is_none() {
                entry.preview = Some(Arc::new(entry.full.downscaled_to(PixelSize::SCREEN_EDGE)));
            }
            let preview = entry.preview.as_ref().expect("刚补上的");
            (preview, preview.width, preview.height)
        }
    };

    // ④ 跑管线（这一段就是「拖一下要多久」的全部）
    let local = if local_strength > 0.0 {
        entry
            .local_tone
            .as_ref()
            .map(|(_, state)| (state, local_strength))
    } else {
        None
    };
    let correction = {
        // 镜头：配置文件来自命令层（`job.lens`），**手动三根拉杆永远叠加在上面**
        // （人类 2026-09-25 拍板：「启用校正」开关只管配置文件那一半）。
        let manual = plans.lens.manual;
        let mut correction = job.lens.clone().unwrap_or_else(|| {
            LensCorrection::manual_only(entry.full.width, entry.full.height, manual)
        });
        correction.manual = manual;
        correction
    };
    let (reference, reference_base) = if job.interactive {
        (None, None)
    } else if let Some(frame) = job
        .prefer_sooc
        .then_some(entry.sooc.as_deref())
        .flatten()
        .and_then(|path| entry.reference.get_sooc(path, job.geometry))
    {
        (Some(frame), Some("sooc"))
    } else {
        (
            Some(entry.reference.get(source, &correction, job.geometry)),
            Some(
                if raybend::media::kind::kind_of_file(&entry.path)
                    == raybend::media::kind::MediaKind::Raw
                {
                    "raw"
                } else {
                    "sooc"
                },
            ),
        )
    };
    let lens_map = LensMap::new(&correction);
    let (mut nr_pending, mut nr_error) = (false, None);
    let high_image = if job.nr_method == NrMethod::High && !plans.denoise.is_identity() {
        let key = DenoiseKey {
            source: entry.source_revision,
            width,
            height,
            plan: plans.denoise,
            lens: correction,
        };
        match high.get_or_request(key, source, !job.interactive) {
            Some(Ok(image)) => Some(image),
            Some(Err(error)) => {
                nr_error = Some(error);
                None
            }
            None => {
                nr_pending = true;
                None
            }
        }
    } else {
        high.cancel();
        None
    };
    let stages = DevelopStages {
        nr_method: Default::default(),
        lens: high_image.is_none().then_some(&lens_map),
        denoise: high_image.is_none().then_some(&plans.denoise),
        local_tone: local,
        sharpen: Some(&plans.sharpen),
        lut: job.lut.as_deref(),
    };
    let started = std::time::Instant::now();
    let input = high_image.as_deref().unwrap_or(source);
    let rgb = render_develop(input, &job.params, &job.curves, &stages);
    let (width, height, rgb) = match job.geometry {
        Some(geometry) => raybend::develop::geometry::apply_rgb8((width, height), &rgb, geometry)
            .expect("已按源尺寸校验成片几何"),
        None => (width, height, rgb),
    };
    let (source_width, source_height) = job
        .geometry
        .map_or((entry.full.width, entry.full.height), |geometry| {
            geometry.output_size((entry.full.width, entry.full.height))
        });
    let develop_ms = started.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "[editor] 真帧：job #{id} 档 {tier:?} {w}×{h} 解码 {decode} 管线 {develop:.0}ms（整条 {total:.0}ms）",
        id = job.id,
        tier = job.tier,
        w = width,
        h = height,
        decode = decode_ms.map_or("—".to_string(), |ms| format!("{ms:.0}ms")),
        develop = develop_ms,
        total = develop_started.elapsed().as_secs_f64() * 1000.0
    );

    Some(DevelopOutcome {
        nr_pending,
        nr_error,
        reference,
        reference_base,
        id: job.id,
        rev: job.rev,
        path: entry.path.clone(),
        tier: job.tier,
        origin,
        transition: false,
        as_shot_temperature,
        decode_ms,
        develop_ms,
        histogram: settled_histogram(&rgb, job.interactive),
        result: Ok(DevelopedImage {
            width,
            height,
            rgb,
            // 逻辑尺寸 = 完整线性源（不是这一档的渲染尺寸）
            source_width,
            source_height,
            original_width: entry.full.width,
            original_height: entry.full.height,
        }),
    })
}

/// 拖动中不重算直方图；松手/换底图的真帧从同一份管线像素计算。
fn settled_histogram(rgb: &[u8], interactive: bool) -> Option<crate::thumbs::HistogramDto> {
    (!interactive).then(|| raybend::display::histogram_of_rgb8(rgb, display::DEFAULT_BINS).into())
}

/// **过渡帧**：按计划里的顺序找第一个能解出来的来源，做成一条显影结果（handoff §3）。
///
/// 与真帧走的是**同一条通道**（`RenderCommand::Developed` + **同一个任务号**）——
/// 任务号必须一致，否则渲染线程会把它当过期结果丢掉（`outcome.id != *latest_job`），
/// 过渡帧就永远不显示。
///
/// 一条都解不出来就返回 `None`（不是错：真帧照样会来）。
fn transition_outcome(
    plan: &TransitionPlan,
    job: &DevelopJob,
    path: &str,
) -> Option<DevelopOutcome> {
    let started = Instant::now();
    for source in &plan.sources {
        let attempt = Instant::now();
        let pixels = match source.kind {
            TransitionKind::AvifCache => {
                let Ok(bytes) = std::fs::read(&source.path) else {
                    eprintln!(
                        "[editor] 过渡帧候选 {origin}：读不到缓存 {path}（{ms:.0}ms）",
                        origin = source.origin,
                        path = source.path.display(),
                        ms = attempt.elapsed().as_secs_f64() * 1000.0
                    );
                    continue; // 缓存被删了 / 读不动：试下一个
                };
                display::pixels_from_avif(&bytes)
            }
            TransitionKind::PhotoFile => display::pixels(&source.path, PixelSize::Screen)
                .ok()
                .flatten(),
        };
        let Some(pixels) = pixels else {
            eprintln!(
                "[editor] 过渡帧候选 {origin}：解不出来 {path}（{ms:.0}ms）",
                origin = source.origin,
                path = source.path.display(),
                ms = attempt.elapsed().as_secs_f64() * 1000.0
            );
            continue;
        };
        let (source_width, source_height) =
            plan.source_size.unwrap_or((pixels.width, pixels.height));
        eprintln!(
            "[editor] 过渡帧：{origin} {w}×{h}（逻辑 {sw}×{sh}）耗时 {ms:.0}ms；候选 {count} 个",
            origin = source.origin,
            w = pixels.width,
            h = pixels.height,
            sw = source_width,
            sh = source_height,
            count = plan.sources.len(),
            ms = started.elapsed().as_secs_f64() * 1000.0,
        );
        return Some(DevelopOutcome {
            nr_pending: false,
            nr_error: None,
            reference: None,
            reference_base: None,
            id: job.id,
            rev: job.rev,
            path: path.to_string(),
            tier: job.tier,
            origin: source.origin.to_string(),
            transition: true,
            // 过渡帧不是「按参数算出来的」：色温基线还得等真帧（RAW 的 as-shot 在解码里）
            as_shot_temperature: None,
            decode_ms: None,
            develop_ms: 0.0,
            histogram: None,
            result: Ok(DevelopedImage {
                width: pixels.width,
                height: pixels.height,
                rgb: pixels.rgb,
                source_width,
                source_height,
                original_width: source_width,
                original_height: source_height,
            }),
        });
    }
    None
}

/// 解码一张照片的**线性源**（显影管线的唯一输入）。
///
/// * RAW → `raw::worker` 的线性解码（完整解码 + 去马赛克，scene-referred）；
/// * 位图 → 先解出 8bit sRGB 再线性化（**准线性**：8bit 里本来就没有更多信息）。
///
/// 长边超过设备的纹理上限时在这里缩掉（`create_texture` 会直接报错，不是静默降级）。
fn decode_linear_source(path: &Path, max_texture: u32) -> Result<LinearSource, String> {
    let started = std::time::Instant::now();
    let (mut image, origin, as_shot_temperature) = if display::pixels::is_raw_photo(path) {
        let request = raybend::raw::backend::DecodeRequest::full(path);
        let worker = raybend::raw::worker::shared();
        let mut guard = worker.lock().map_err(|_| "RAW worker 锁中毒".to_string())?;
        let mut decoded = guard.decode_linear(&request).map_err(|e| e.to_string())?;
        let (width, height) = (decoded.width, decoded.height);
        let as_shot_temperature = decoded.as_shot_temperature;
        /*
         * **方向与缩略图/看图那条路同一条规则**（`media::exif::raw_orientation`）：
         * 文件头优先（worker 对 TIFF 家族报 1 不可靠），读不到才用 worker 报的。
         * `from_raw16` 会按它把像素摆正（宽高随之互换）—— 显影管线、裁剪、覆盖层、
         * 1:1 看到的都该是「用户实际看到的那张图」。编辑侧曾经连这一步都没有。
         */
        decoded.orientation = raybend::media::exif::raw_orientation(path, decoded.orientation);
        let image = LinearImage::from_raw16(decoded)
            .ok_or_else(|| format!("线性解码结果的尺寸对不上：{width}×{height}"))?;
        (image, "raw-linear".to_string(), as_shot_temperature)
    } else {
        let Some(pixels) = display::pixels(path, PixelSize::Full).map_err(|e| e.to_string())?
        else {
            return Err(format!("解不开这张照片：{}", path.display()));
        };
        let image = LinearImage::from_srgb8(pixels.width, pixels.height, &pixels.rgb)
            .ok_or_else(|| format!("解码结果的尺寸对不上：{}×{}", pixels.width, pixels.height))?;
        (image, "bitmap-linear".to_string(), None)
    };
    let long = image.width.max(image.height);
    if max_texture > 0 && long > max_texture {
        image = image.downscaled_to(max_texture);
    }
    let _ = started;
    Ok(LinearSource {
        image,
        origin,
        as_shot_temperature,
    })
}

/// 解好的线性源（带出处与色温基线）。
struct LinearSource {
    image: LinearImage,
    origin: String,
    as_shot_temperature: Option<f32>,
}

/// 后台完成的旧快照不得覆盖更新的选择或另一张照片。
fn accept_issue_reference(
    sequence: u64,
    latest_sequence: u64,
    requested_path: &str,
    current_path: Option<&str>,
) -> bool {
    sequence >= latest_sequence && current_path == Some(requested_path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn named_issue_reference_rejects_stale_sequence_and_other_photo() {
        assert!(super::accept_issue_reference(3, 3, "a.raw", Some("a.raw")));
        assert!(super::accept_issue_reference(4, 3, "a.raw", Some("a.raw")));
        assert!(!super::accept_issue_reference(2, 3, "a.raw", Some("a.raw")));
        assert!(!super::accept_issue_reference(4, 3, "a.raw", Some("b.raw")));
        assert!(!super::accept_issue_reference(4, 3, "a.raw", None));
    }

    use super::*;

    #[test]
    fn tool_open_intent_carries_initial_ratio_with_camel_case_fields() {
        let intent: ViewportIntent =
            serde_json::from_str(r#"{"kind":"setTool","tool":"crop","initialRatio":1.25}"#)
                .expect("初始比例意图");
        assert!(matches!(intent, ViewportIntent::SetTool {
            tool: Some(tool), initial_ratio: Some(ratio)
        } if tool == "crop" && ratio == 1.25));
    }

    #[test]
    fn confirmed_crop_reopens_in_place_and_can_expand_into_original() {
        let size = (400, 300);
        let saved = CropRect {
            x: 0.2,
            y: 0.2,
            width: 0.6,
            height: 0.6,
        };
        let ratio = 4.0 / 3.0;
        assert_eq!(crop_after_ratio_change(size, 0.0, saved, ratio), saved);
        let expanded = drag_crop(size, 0.0, saved, CropHandle::Se, (0.1, 0.1), Some(ratio));
        assert!(expanded.width > saved.width && expanded.height > saved.height);
        assert!(raybend::develop::geometry::contains_rect(
            size, 0.0, expanded
        ));
        let changed = crop_after_ratio_change(size, 0.0, saved, 1.0);
        assert_ne!(changed, saved, "真正切换比例仍要更新裁切框");
        let outer = largest_centered_rect(size, 15.0, ratio);
        let rotated = CropRect {
            x: outer.x + outer.width * 0.2,
            y: outer.y + outer.height * 0.2,
            width: outer.width * 0.6,
            height: outer.height * 0.6,
        };
        assert_eq!(crop_after_ratio_change(size, 15.0, rotated, ratio), rotated);
        let expanded = drag_crop(
            size,
            15.0,
            rotated,
            CropHandle::Se,
            (0.02, 0.02),
            Some(ratio),
        );
        assert!(expanded.width > rotated.width && expanded.height > rotated.height);
        assert!(raybend::develop::geometry::contains_rect(
            size, 15.0, expanded
        ));
    }

    #[test]
    fn frame_retry_keeps_unpresented_frames_and_returns_to_idle_after_success() {
        // Lost → 重配 → 暂时取不到帧 → 恢复；中途都不能丢掉绘制请求。
        let outcomes = [
            Ok(RenderOutcome::Reconfigured("lost".into())),
            Ok(RenderOutcome::Skipped),
            Err(raybend::render::GpuError::Surface("device lost".into())),
            Ok(RenderOutcome::Drawn),
        ];
        let delays: Vec<_> = outcomes.iter().map(frame_retry_delay).collect();
        assert_eq!(
            delays,
            [
                Some(Duration::from_millis(16)),
                Some(Duration::from_millis(200)),
                Some(Duration::from_millis(16)),
                None,
            ]
        );
    }

    #[test]
    fn resize_batch_keeps_only_final_size_and_preserves_other_commands() {
        let mut pending = None;
        assert!(
            defer_resize(
                RenderCommand::Resize {
                    width: 800,
                    height: 600
                },
                &mut pending
            )
            .is_none()
        );
        assert!(
            defer_resize(
                RenderCommand::Resize {
                    width: 1200,
                    height: 900
                },
                &mut pending
            )
            .is_none()
        );
        let command = defer_resize(RenderCommand::Stop, &mut pending);
        assert!(matches!(command, Some(RenderCommand::Stop)));
        assert_eq!(pending, Some((1200, 900)));
    }

    #[test]
    fn photo_request_gate_drops_late_old_selection_and_accepts_latest() {
        let gate = RequestGate::default();
        let old = gate.begin().expect("第一张");
        let latest = gate.begin().expect("第二张");
        let mut sent = Vec::new();
        assert!(
            !gate
                .send_if_latest(old, || {
                    sent.push("old");
                    Ok(())
                })
                .expect("旧请求应被跳过")
        );
        assert!(
            gate.send_if_latest(latest, || {
                sent.push("latest");
                Ok(())
            })
            .expect("新请求应送出")
        );
        assert_eq!(sent, ["latest"]);
    }

    #[test]
    fn histogram_waits_for_slider_release_and_uses_rendered_rgb() {
        let rgb = [255, 0, 0, 0, 255, 0];
        assert!(settled_histogram(&rgb, true).is_none());
        let histogram = settled_histogram(&rgb, false).expect("松手要有直方图");
        assert_eq!(histogram.bins, display::DEFAULT_BINS);
        assert_eq!(histogram.r[0], 1.0);
        assert_eq!(histogram.g[0], 1.0);
        assert_eq!(histogram.b[0], 2.0);
        assert!(histogram.r[display::DEFAULT_BINS - 1] > 0.0);
        assert!(histogram.g[display::DEFAULT_BINS - 1] > 0.0);
    }

    fn job(id: u64, rev: u64, photo: Option<&str>) -> DevelopJob {
        DevelopJob {
            nr_method: NrMethod::Fast,
            id,
            rev,
            photo: photo.map(str::to_string),
            tier: ImageTier::Preview,
            interactive: false,
            params: DevelopParams::default(),
            curves: CurveSet::default(),
            lens: None,
            lut: None,
            geometry: None,
            prefer_sooc: true,
            transition: None,
        }
    }

    /// 一份只有一个候选的过渡帧计划（写测试用）。
    fn plan(path: &str, kind: TransitionKind, origin: &'static str) -> TransitionPlan {
        TransitionPlan {
            sources: vec![TransitionSource {
                path: std::path::PathBuf::from(path),
                kind,
                origin,
            }],
            source_size: Some((4000, 3000)),
            sooc: None,
        }
    }

    #[test]
    fn merging_keeps_the_transition_plan_with_the_photo() {
        // 换照片那条带计划、紧接着的参数任务不带 —— 合并时不许把计划弄丢
        // （丢了就变成「正是需要它的那一条没有过渡帧」）。
        let mut with_plan = job(1, 1, Some("a.rw2"));
        with_plan.transition = Some(plan("a.avif", TransitionKind::AvifCache, "preview-latest"));
        let merged = merge_jobs(with_plan, job(2, 2, None));
        assert!(merged.transition.is_some(), "参数任务不能把过渡帧计划顶掉");
        assert_eq!(merged.photo.as_deref(), Some("a.rw2"));

        // 换到另一张时，计划取新的那一份
        let mut other = job(3, 3, Some("b.rw2"));
        other.transition = Some(plan("b.avif", TransitionKind::AvifCache, "preview-sooc"));
        let merged = merge_jobs(job(1, 1, Some("a.rw2")), other);
        assert_eq!(
            merged.transition.expect("有计划").sources[0].origin,
            "preview-sooc"
        );
    }

    /// **过渡帧三条硬口径**（handoff §3.3）：任务号一致、`origin` 报得出来、
    /// 逻辑尺寸用**原图尺寸**（不是这张过渡图自己的 1920）。
    #[test]
    fn transition_frame_carries_job_id_origin_and_original_size() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("photo.png");
        let mut img = image::RgbImage::new(64, 48);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = image::Rgb([(x % 256) as u8, (y % 256) as u8, 7]);
        }
        img.save(&path).expect("写 PNG");

        let job = job(42, 7, Some(&path.to_string_lossy()));
        let plan = plan(
            &path.to_string_lossy(),
            TransitionKind::PhotoFile,
            "preview-photo",
        );
        let outcome = transition_outcome(&plan, &job, &path.to_string_lossy()).expect("有过渡帧");

        assert_eq!(
            outcome.id, 42,
            "任务号必须与当前任务一致，否则会被当过期结果丢掉"
        );
        assert_eq!(outcome.rev, 7);
        assert!(outcome.transition, "要标成过渡帧（渲染线程据此不置 ready）");
        assert_eq!(outcome.origin, "preview-photo");
        let image = outcome.result.expect("解得出这张 PNG");
        assert_eq!(
            (image.width, image.height),
            (64, 48),
            "纹理是这张图自己的尺寸"
        );
        assert_eq!(
            (image.source_width, image.source_height),
            (4000, 3000),
            "逻辑尺寸用原图尺寸 —— 报 64×48 的话真帧回来会 refit 跳一下"
        );
    }

    /// **真 RAW 的连接复现**（手跑）：过渡帧 + 真解码能不能顺利跑完、过渡帧是不是先发。
    ///
    /// ```bash
    /// cargo build -p raybend                      # 先把 worker 编出来
    /// cargo test -p raybend-desktop --lib -- --ignored real_raw
    /// ```
    ///
    /// 为什么进门就卡（"正在载入照片"）时先跑它：这一条走的就是显影线程那条路。
    #[test]
    #[ignore = "要真照片（/mnt/c/src/tmp/pic/P1000019.RW2）与 worker 二进制，手跑"]
    fn real_raw_transition_then_real_frame_terminates() {
        let raw = std::path::PathBuf::from("/mnt/c/src/tmp/pic/P1000019.RW2");
        if !raw.is_file() {
            eprintln!("跳过：没有样本 {}", raw.display());
            return;
        }
        let dir = tempfile::tempdir().expect("临时目录");
        // 假 SOOC（真代码里是同名 JPG，这里用一张 PNG 就够了：它只当候选帧）
        let sooc = dir.path().join("photo.jpg");
        image::RgbImage::new(64, 48).save(&sooc).expect("写假 SOOC");

        let plan = TransitionPlan {
            sources: vec![
                TransitionSource {
                    path: sooc,
                    kind: TransitionKind::PhotoFile,
                    origin: "preview-sooc",
                },
                TransitionSource {
                    path: raw.clone(),
                    kind: TransitionKind::PhotoFile,
                    origin: "preview-raw",
                },
            ],
            source_size: Some((3888, 5184)),
            sooc: None,
        };
        let mut job = job(1, 1, Some(&raw.to_string_lossy()));
        job.transition = Some(plan);

        let (tx, rx) = channel();
        let mut cached = None;
        let started = std::time::Instant::now();
        let outcome = run_develop_job(
            &mut cached,
            &Some(raw.to_string_lossy().into_owned()),
            &job,
            0,
            &tx,
            &mut HighDenoise::new(|| {}),
        );
        eprintln!("run_develop_job 返回耗时 {:?}", started.elapsed());

        let first = rx.try_recv().expect("过渡帧应当先发出来");
        match first {
            RenderCommand::Developed(outcome) => {
                assert!(outcome.transition, "第一条必须是过渡帧");
                eprintln!(
                    "过渡帧 origin={} （{:?}）",
                    outcome.origin,
                    started.elapsed()
                );
            }
            _ => panic!("第一条不是 Developed"),
        }
        let real = outcome.expect("真帧也该有");
        assert!(!real.transition);
        eprintln!(
            "真帧 origin={} 解码 {:?}ms 管线 {:.0}ms",
            real.origin, real.decode_ms, real.develop_ms
        );
    }

    /// **候选顺序**（§2.5 切图优先的规格）：`latest` → `SOOC` → 要编的文件本身。
    #[test]
    fn transition_sources_follow_the_latest_sooc_raw_order() {
        let dir = tempfile::tempdir().expect("临时目录");
        let raw = dir.path().join("photo.rw2");
        let jpg = dir.path().join("photo.jpg");
        // 版本号从常量取（不要写死 —— 拾 PIPELINE_VERSION 时会静默变成「找不到缓存」）
        let cached = dir.path().join(format!(
            "latest-raw-v{}.avif",
            raybend::thumbnail::render::PIPELINE_VERSION
        ));
        for file in [&raw, &jpg, &cached] {
            std::fs::write(file, b"x").expect("建文件");
        }

        let sources = transition_sources(
            &raw,
            raybend::store::develop::EditBase::Raw,
            Some(cached.clone()),
            Some(jpg.clone()),
        );
        let origins: Vec<&str> = sources.iter().map(|source| source.origin).collect();
        assert_eq!(
            origins,
            vec!["preview-latest", "preview-sooc", "preview-raw"],
            "顺序就是规格：latest → SOOC → RAW"
        );
        assert_eq!(sources[0].kind, TransitionKind::AvifCache);
        assert_eq!(sources[1].kind, TransitionKind::PhotoFile);
    }

    /// 没编辑过（没有 latest 缓存）、没有同名 JPG：只剩「要编的这张」—— 依然要出过渡帧。
    #[test]
    fn transition_sources_degrade_to_the_photo_itself() {
        let dir = tempfile::tempdir().expect("临时目录");
        let raw = dir.path().join("only.rw2");
        std::fs::write(&raw, b"x").expect("建文件");
        let missing_cache = dir.path().join("nope.avif");

        let sources = transition_sources(
            &raw,
            raybend::store::develop::EditBase::Raw,
            Some(missing_cache),
            None,
        );
        assert_eq!(sources.len(), 1, "不存在的缓存不许进候选：{sources:?}");
        assert_eq!(sources[0].origin, "preview-raw");

        // 文件不存在（路径是编出来的）→ 一个候选都没有
        assert!(
            transition_sources(
                &dir.path().join("gone.rw2"),
                raybend::store::develop::EditBase::Raw,
                None,
                None
            )
            .is_empty()
        );
    }

    /// 要编的就是位图（SOOC 基准）：不重复推同一张（否则会白解两遍）。
    #[test]
    fn transition_sources_do_not_duplicate_the_sooc_side() {
        let dir = tempfile::tempdir().expect("临时目录");
        let jpg = dir.path().join("photo.jpg");
        std::fs::write(&jpg, b"x").expect("建文件");

        let sources = transition_sources(
            &jpg,
            raybend::store::develop::EditBase::Sooc,
            None,
            Some(jpg.clone()),
        );
        assert_eq!(
            sources.len(),
            1,
            "位图基准下 SOOC 与「要编的这张」是同一个：{sources:?}"
        );
        assert_eq!(sources[0].origin, "preview-bitmap");
    }

    /// 没有原图尺寸时退回用过渡帧自己的尺寸（宽高比一致时 Fit 档下看不出区别）。
    #[test]
    fn transition_frame_falls_back_to_its_own_size() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("photo.png");
        image::RgbImage::new(32, 16).save(&path).expect("写 PNG");
        let mut plan = plan(
            &path.to_string_lossy(),
            TransitionKind::PhotoFile,
            "preview-photo",
        );
        plan.source_size = None;
        let job = job(1, 1, Some(&path.to_string_lossy()));
        let image = transition_outcome(&plan, &job, &path.to_string_lossy())
            .expect("有过渡帧")
            .result
            .expect("解得出");
        assert_eq!((image.source_width, image.source_height), (32, 16));
    }

    /// 候选一个都用不了（缓存文件不在、图解不开）→ `None`，**不是错**：真帧照样会来。
    #[test]
    fn transition_frame_skips_unusable_sources() {
        let dir = tempfile::tempdir().expect("临时目录");
        let junk = dir.path().join("junk.avif");
        std::fs::write(&junk, b"not an avif").expect("写垃圾");
        let missing = dir.path().join("missing.avif");
        let plan = TransitionPlan {
            sources: vec![
                TransitionSource {
                    path: missing,
                    kind: TransitionKind::AvifCache,
                    origin: "preview-latest",
                },
                TransitionSource {
                    path: junk,
                    kind: TransitionKind::AvifCache,
                    origin: "preview-sooc",
                },
            ],
            source_size: None,
            sooc: None,
        };
        let job = job(1, 1, Some("x.rw2"));
        assert!(transition_outcome(&plan, &job, "x.rw2").is_none());
    }

    /// 我们自己的 AVIF 缓存字节能变成过渡帧像素（缓存 → 纹理那条路）。
    #[test]
    fn transition_frame_decodes_our_avif_cache() {
        let dir = tempfile::tempdir().expect("临时目录");
        let cached = dir.path().join(format!(
            "latest-raw-v{}.avif",
            raybend::thumbnail::render::PIPELINE_VERSION
        ));
        let mut rgb = Vec::new();
        for y in 0..24u32 {
            for x in 0..32u32 {
                rgb.extend_from_slice(&[(x * 8) as u8, (y * 8) as u8, 100]);
            }
        }
        let bytes = raybend::thumbnail::render::encode_avif(&rgb, 32, 24).expect("编码");
        std::fs::write(&cached, &bytes).expect("写缓存");

        let plan = TransitionPlan {
            sources: vec![TransitionSource {
                path: cached,
                kind: TransitionKind::AvifCache,
                origin: "preview-latest",
            }],
            source_size: Some((6000, 4500)),
            sooc: None,
        };
        let job = job(9, 3, Some("x.rw2"));
        let outcome = transition_outcome(&plan, &job, "x.rw2").expect("缓存能解");
        assert_eq!(outcome.origin, "preview-latest");
        let image = outcome.result.expect("有像素");
        assert_eq!((image.width, image.height), (32, 24));
        assert_eq!((image.source_width, image.source_height), (6000, 4500));
        assert_eq!(image.rgb.len(), 32 * 24 * 3);
    }

    #[test]
    fn merging_keeps_the_newest_but_never_drops_the_photo() {
        // 换照片的真实顺序：SetPhoto(A) 之后紧跟一条 SetParams（photo: None）
        let merged = merge_jobs(job(1, 1, Some("a.rw2")), job(2, 2, None));
        assert_eq!(merged.id, 2, "任务号/参数取新的");
        assert_eq!(merged.rev, 2);
        assert_eq!(
            merged.photo.as_deref(),
            Some("a.rw2"),
            "参数任务不能把照片顶掉 —— 顶掉就是「卡在正在载入照片」那个 bug"
        );

        // 换到另一张：新照片赢
        let merged = merge_jobs(job(1, 1, Some("a.rw2")), job(2, 2, Some("b.rw2")));
        assert_eq!(merged.photo.as_deref(), Some("b.rw2"));

        // 开局那条参数任务：本来就没照片，合并后也没有
        assert!(merge_jobs(job(1, 1, None), job(2, 2, None)).photo.is_none());
        // 反过来（参数在前、照片在后）也要拿到照片
        assert_eq!(
            merge_jobs(job(1, 1, None), job(2, 2, Some("a.rw2")))
                .photo
                .as_deref(),
            Some("a.rw2")
        );
    }

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
            overlay_colors: None,
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
        assert!(
            validate(&args(0.0)).is_err(),
            "dpr 不能是 0：它是换算的分母"
        );
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
    fn replay_returns_the_raw_payload_for_late_sessions() {
        // 会话可能晚于第一次上报才建立；补发的那一份必须是**原始载荷**
        // （连底色串一起 —— 校验后的结构只留解析结果）。
        let mut stored = StoredViewport::default();
        assert!(stored.replay().is_none(), "还没收到过就没什么可补发");
        let raw = args(1.375);
        stored.remember_raw(raw.clone());
        let replay = stored.replay().expect("有原始载荷");
        assert_eq!(replay.hole.width, raw.hole.width);
        assert_eq!(replay.backdrop.as_deref(), Some("rgb(42, 45, 51)"));
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
        for mode in [
            FitMode::Fit,
            FitMode::Fill,
            FitMode::OneToOne,
            FitMode::Free,
        ] {
            let name = fit_mode_name(mode);
            assert_eq!(parse_fit_mode(name).expect("能解回来"), mode);
        }
        // 大小写不敏感（前端可能给 camelCase）
        assert_eq!(
            parse_fit_mode("OneToOne").expect("大小写不敏感"),
            FitMode::OneToOne
        );
        assert_eq!(
            parse_fit_mode(" onetoone ").expect("两侧空白容忍"),
            FitMode::OneToOne
        );
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

        let reset: ViewportIntent = serde_json::from_value(serde_json::json!({ "kind": "reset" }))
            .expect("reset 能反序列化");
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
        assert!(
            !validated.backdrop_reported,
            "用的是兜底色，状态里要看得出来"
        );

        // 报了但解不开：也是兜底 + 标记，而不是报错
        let mut broken = args;
        broken.backdrop = Some("hsl(210, 10%, 18%)".to_string());
        let validated = validate(&broken).expect("解不开不报错");
        assert!(!validated.backdrop_reported);

        // 报了且能解开：就是报上来的那个色，并且标着「是报上来的」
        let mut reported = broken;
        reported.backdrop = Some("rgb(18, 20, 24)".to_string());
        let validated = validate(&reported).expect("能解开");
        assert_eq!(
            (
                validated.backdrop.r,
                validated.backdrop.g,
                validated.backdrop.b
            ),
            (18, 20, 24)
        );
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
        assert!(
            (color.a - 1.0).abs() < 1e-9,
            "洞口底色必须不透明（否则透出桌面）"
        );
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
    #[test]
    fn high_quality_swaps_in_then_reuses_cache_for_tone_and_histogram() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("small.png");
        image::RgbImage::from_pixel(17, 13, image::Rgb([120, 130, 110]))
            .save(&path)
            .unwrap();
        let (tx, _rx) = channel();
        let (done, ready) = channel();
        let mut high = HighDenoise::new(move || {
            let _ = done.send(());
        });
        let photo = Some(path.to_string_lossy().into_owned());
        let mut request = job(1, 1, photo.as_deref());
        request.nr_method = NrMethod::High;
        request.params.set("lumaNr", 60.0).unwrap();
        let mut cached = None;
        let fast = run_develop_job(&mut cached, &photo, &request, 8192, &tx, &mut high).unwrap();
        assert!(fast.nr_pending);
        assert!(fast.nr_error.is_none());
        let reference = fast.reference.unwrap();
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        request.id = 2;
        request.rev = 2;
        request.params.set("exposure", 0.5).unwrap();
        let final_frame =
            run_develop_job(&mut cached, &photo, &request, 8192, &tx, &mut high).unwrap();
        assert!(!final_frame.nr_pending);
        assert!(final_frame.nr_error.is_none());
        assert_eq!(reference.id, final_frame.reference.unwrap().id);
        let image = final_frame.result.unwrap();
        let expected = settled_histogram(&image.rgb, false).unwrap();
        assert_eq!(
            serde_json::to_value(final_frame.histogram.unwrap()).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }
    #[test]
    fn changed_source_reloads_but_unchanged_and_parameter_jobs_reuse() {
        assert!(should_reload_source(
            Some("a.jpg"),
            Some("old"),
            "a.jpg",
            Some("new"),
            true
        ));
        assert!(!should_reload_source(
            Some("a.jpg"),
            Some("old"),
            "a.jpg",
            Some("old"),
            true
        ));
        assert!(!should_reload_source(
            Some("a.jpg"),
            Some("old"),
            "a.jpg",
            None,
            false
        ));
        assert!(should_reload_source(
            Some("a.jpg"),
            Some("old"),
            "b.jpg",
            Some("old"),
            false
        ));
        assert!(should_reload_source(
            None,
            None,
            "a.jpg",
            Some("new"),
            false
        ));
        let root = tempfile::tempdir().unwrap();
        let raw = root.path().join("x.raw");
        let sooc = root.path().join("x.jpg");
        std::fs::write(&raw, b"raw").unwrap();
        std::fs::write(&sooc, b"old").unwrap();
        let first = source_dependency_signature(&raw.to_string_lossy(), Some(&sooc));
        std::fs::write(&sooc, b"new sooc").unwrap();
        assert_ne!(
            first,
            source_dependency_signature(&raw.to_string_lossy(), Some(&sooc))
        );
        std::fs::remove_file(&sooc).unwrap();
        assert_ne!(
            first,
            source_dependency_signature(&raw.to_string_lossy(), Some(&sooc))
        );
    }
}
