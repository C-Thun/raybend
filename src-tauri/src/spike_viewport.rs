//! 渲染 spike 的**调试窗口**与命令层（`PLAN.md` A.2 的窗口策略）。
//!
//! 三条立场：
//!
//! 1. **不动主窗口**。另开一个 `label = "spike-viewport"` 的窗口（`transparent: true`）——
//!    即使透明挖洞彻底失败，主窗口、设计工作与别的开发都不受影响。
//! 2. **渲染在独立线程**。窗口线程不能被每帧的 `submit/present` 占住；
//!    命令走通道进线程，线程把可读状态写回 [`Shared`]。
//! 3. **能自测的自测**。适配器/后端/表面/alpha 模式/dpr/坐标往返偏差/设备丢失恢复成败
//!    全部由程序算出并落盘，人只填「透明区是不是真的透」这类**只有人眼能判**的项
//!    （见 `plans/M2-W1-windows-gpu.md`）。
//!
//! # 空闲时一帧都不画
//!
//! 线程在空闲时阻塞在 `recv()` 上（不空转）：窗口内容留在屏幕上，CPU 占用为 0。
//! 只有「脚本化运动」或「正在采集」时才按 `~60Hz` 循环。这既省电，
//! 也让「静止帧成本」与「运动帧成本」两个数分开量 —— 否则量到的是我们的空转。

use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use raybend::render::stats::{
    AdapterInfo, FrameStats, HumanNotes, ScenarioStats, SpikeReport, SurfaceInfo,
};
use raybend::render::viewport::FitMode;
use raybend::render::{GpuContext, RenderOutcome, Viewport};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use crate::render_window::raw_handles;

/// 调试窗口的 label（`tauri.conf.json` 里没有它 —— 它是按需建的）。
pub const SPIKE_LABEL: &str = "spike-viewport";

/// 脚本化缩放的折返上下限（不跑到视口的极端值上，免得量到「只剩一格像素」那种情况）。
const MIN_SCRIPT_ZOOM: f32 = 0.05;
const MAX_SCRIPT_ZOOM: f32 = 4.0;

/* ══════════════════════════════════════════════════════════════
 * 命令
 * ══════════════════════════════════════════════════════════════ */

/// 前端发来的交互意图（**只有意图，没有坐标数学** —— `AGENTS.md` §6.1 红线 #1）。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SpikeCommand {
    /// 以某个 CSS 像素点为锚点缩放
    Zoom { x: f32, y: f32, factor: f32, dpr: f32 },
    /// 命中测试：把鼠标位置交给 Rust 换算成图像像素（**不动视口**，坐标同步的判据）
    HitTest { x: f32, y: f32, dpr: f32 },
    /// 平移（CSS 像素位移）
    Pan { dx: f32, dy: f32, dpr: f32 },
    /// 档位：`fit` / `fill` / `oneToOne` / `free`
    Fit { mode: String },
    /// 旋转（度）
    Rotate { degrees: f32 },
    /// 洞口开关（关掉就是整窗出图，用来对照）
    SetHole { on: bool },
    /// 界面把洞口矩形的当前布局报上来（CSS 像素）—— DOM 是布局权威，Rust 是变换权威
    HoleRect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        dpr: f32,
        viewport_width: u32,
        viewport_height: u32,
    },
    /// 复位（适配 + 零旋转）
    Reset,
    /// 脚本化运动：**不经过 IPC**，在渲染线程里自己推进 N 秒
    Scripted { name: String, seconds: f32 },
    /// 场景采集：`Some(name)` 开始（名字即报告里的场景名），`None` 结束
    Scenario { name: Option<String> },
    /// 演练设备丢失
    DeviceLoss,
    /// 从设备丢失里恢复
    Recover,
    /// 窗口尺寸/DPR 变化（由窗口事件转发）
    Resize {
        width: u32,
        height: u32,
        dpr: f32,
    },
    Shutdown,
}

/// 渲染线程里的脚本化运动。
///
/// 带**方向**与折返：脚本要来回扫（走到上下限就反向），否则量到的只是单程，
/// 而且缩放方向不折返的话几帧就跑出可视范围了。
#[derive(Debug, Clone, Copy)]
struct Script {
    kind: ScriptKind,
    /// +1 / −1：当前推进方向
    direction: f32,
    deadline: Instant,
}

#[derive(Debug, Clone, Copy)]
enum ScriptKind {
    /// 每帧按每秒多少 CSS 像素平移
    Pan { speed: f32 },
    /// 每秒多少倍的连续缩放（围绕洞口中心）
    Zoom { per_second: f32 },
}

/* ══════════════════════════════════════════════════════════════
 * 共享状态（命令读、渲染线程写）
 * ══════════════════════════════════════════════════════════════ */

/// 报告里要的视口快照。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewportView {
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub rotation: f32,
    pub fit_mode: String,
    pub hole_css: Option<(f32, f32, f32, f32)>,
    pub hole_physical: Option<(f32, f32, f32, f32)>,
    pub dpr: f32,
    pub image_width: u32,
    pub image_height: u32,
}

/// 一个场景的可读摘要（前端上表用）。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioView {
    pub name: String,
    pub how: String,
    pub frames: usize,
    pub p50_ms: Option<f32>,
    pub p95_ms: Option<f32>,
    pub fps: Option<f64>,
    pub cpu_p50_ms: Option<f32>,
    pub over_budget: f32,
}

/// 命中测试的结果（坐标同步靠它检）。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HitView {
    pub css_x: f32,
    pub css_y: f32,
    pub center_css: (f32, f32),
    pub image_x: f32,
    pub image_y: f32,
    pub inside: bool,
    /// 这个点是不是图像中心（±2 图像像素内）—— 人把鼠标放到白块上时这里应当为真
    pub at_center: bool,
}

/// 命令层读得到的全部状态。
#[derive(Default)]
pub struct Shared {
    pub open: bool,
    pub adapter: AdapterInfo,
    pub format: String,
    pub alpha_mode: String,
    pub surface_size: (u32, u32),
    pub css_size: (u32, u32),
    pub monitor_scale: f32,
    pub monitor_name: String,
    pub maximized: bool,
    pub fullscreen: bool,
    pub decorated: bool,
    pub transparent: bool,
    pub upload_ms: f32,
    pub rendered: u64,
    pub viewport: Viewport,
    pub hole_css: Option<(f32, f32, f32, f32)>,
    /// 保存布局真相；关闭裁剪时也保留，以便恢复与 DPI 重算。
    pub layout_hole_css: Option<(f32, f32, f32, f32)>,
    pub hole_disabled: bool,
    pub last_pointer_css: Option<(f32, f32)>,
    /// devicePixelRatio 包含系统文字/页面缩放，不能用 OS scale_factor 替代。
    pub webview_dpr: Option<f32>,
    pub scenarios: Vec<(String, String, FrameStats, FrameStats)>,
    pub active_scenario: Option<usize>,
    pub device_lost: Vec<String>,
    pub coord_max_error: f32,
    pub last_hit: Option<HitView>,
    pub last_error: Option<String>,
    /// 洞口矩形的物理像素版本（前端显示用，见 `ViewportView::hole_physical`）
    pub hole_physical: Option<(f32, f32, f32, f32)>,
    /// 客户区原点（物理屏幕像素）
    pub client_origin: (i32, i32),
    /// 窗口原点（物理屏幕像素，含边框）
    pub window_origin: (i32, i32),
    /// 原生 WebView bounds 换算的屏幕原点（物理像素）
    pub webview_origin: Option<(i32, i32)>,
    /// 最近一次的帧间隔与 CPU 时长（界面上实时看）
    pub last_frame_ms: f32,
    pub last_cpu_ms: f32,
}

impl Shared {
    /// 把主线程推过来的窗口事实搬进共享状态（见 [`WindowFacts`] 上的说明）。
    ///
    /// ⚠️ **不碰 `viewport`**：尺寸/DPR 对渲染数学的影响由渲染线程自己按 context 走
    /// （`SpikeCommand::Resize` 的处理里已经做了），这里只搬「窗口是什么样」。
    fn apply_window_facts(&mut self, facts: &WindowFacts) {
        let dpr = facts.dpr.max(0.01);
        self.surface_size = facts.surface_size;
        if self.webview_dpr.is_none() {
            self.css_size = (
                (facts.surface_size.0 as f32 / dpr) as u32,
                (facts.surface_size.1 as f32 / dpr) as u32,
            );
        }
        self.monitor_scale = facts.monitor_scale;
        self.maximized = facts.maximized;
        self.fullscreen = facts.fullscreen;
        self.decorated = facts.decorated;
        self.client_origin = facts.client_origin;
        self.window_origin = facts.window_origin;
        self.webview_origin = facts.webview_origin;
    }
    fn sync_webview_dpr(&mut self, viewport: &mut Viewport, dpr: f32) -> Result<(), String> {
        if !dpr.is_finite() || dpr <= 0.0 {
            return Err("WebView DPR 必须是有限正数".into());
        }
        self.webview_dpr = Some(dpr);
        if viewport.dpr != dpr {
            viewport.dpr = dpr;
            self.sync_hole(viewport);
        }
        Ok(())
    }

    /// CSS 洞口与 DPI 在同一处换算；布局消息和 DPI 消息无论谁先到，最终一致。
    fn sync_hole(&mut self, viewport: &mut Viewport) {
        self.hole_css = if self.hole_disabled { None } else { self.layout_hole_css };
        viewport.clip_rect = self.hole_css.map(|(x, y, width, height)| raybend::render::ClipRect {
            x: x * viewport.dpr,
            y: y * viewport.dpr,
            width: width * viewport.dpr,
            height: height * viewport.dpr,
        });
        self.hole_physical = viewport.clip_rect.map(|r| (r.x, r.y, r.width, r.height));
        if viewport.fit_mode != FitMode::Free {
            viewport.refit();
        }
    }

    fn publish_viewport(&mut self, viewport: &Viewport) {
        self.viewport = *viewport;
        self.coord_max_error = coord_roundtrip_error(viewport);
        self.last_hit = self.last_pointer_css.map(|css| hit_view(viewport, css));
    }

    fn snapshot(&self) -> SpikeSnapshot {
        SpikeSnapshot {
            open: self.open,
            adapter: self.adapter.clone(),
            format: self.format.clone(),
            alpha_mode: self.alpha_mode.clone(),
            surface_size: self.surface_size,
            css_size: self.css_size,
            monitor_scale: self.monitor_scale,
            monitor_name: self.monitor_name.clone(),
            maximized: self.maximized,
            fullscreen: self.fullscreen,
            decorated: self.decorated,
            transparent: self.transparent,
            upload_ms: self.upload_ms,
            rendered: self.rendered,
            viewport: ViewportView {
                zoom: self.viewport.zoom,
                pan_x: self.viewport.pan_px.0,
                pan_y: self.viewport.pan_px.1,
                rotation: self.viewport.rotation,
                fit_mode: format!("{:?}", self.viewport.fit_mode),
                hole_css: self.hole_css,
                hole_physical: self.hole_physical,
                dpr: self.viewport.dpr,
                image_width: self.viewport.image_size.0,
                image_height: self.viewport.image_size.1,
            },
            scenarios: self
                .scenarios
                .iter()
                .map(|(name, how, frames, cpu)| ScenarioView {
                    name: name.clone(),
                    how: how.clone(),
                    frames: frames.count(),
                    p50_ms: frames.percentile(0.5),
                    p95_ms: frames.percentile(0.95),
                    fps: frames.fps(),
                    cpu_p50_ms: cpu.percentile(0.5),
                    over_budget: frames.over_budget_ratio(16.7),
                })
                .collect(),
            device_lost: self.device_lost.clone(),
            coord_max_error: self.coord_max_error,
            last_hit: self.last_hit.clone(),
            last_frame_ms: self.last_frame_ms,
            last_cpu_ms: self.last_cpu_ms,
            last_error: self.last_error.clone(),
            client_origin: self.client_origin,
            window_origin: self.window_origin,
            webview_origin: self.webview_origin,
            input_offset: self
                .webview_origin
                .map(|w| (w.0 - self.client_origin.0, w.1 - self.client_origin.1)),
            viewport_problems: self.viewport.sanity_problems(),
        }
    }

    fn frame_stats(&mut self) -> (Option<&mut FrameStats>, Option<&mut FrameStats>) {
        match self.active_scenario {
            Some(index) if index < self.scenarios.len() => {
                let (_, _, frames, cpu) = &mut self.scenarios[index];
                (Some(frames), Some(cpu))
            }
            _ => (None, None),
        }
    }

    /// 建一次场景（若同名已存在就复用 —— 人可以反复点）。
    fn ensure_scenario(&mut self, name: &str, how: &str) -> usize {
        if let Some(index) = self.scenarios.iter().position(|(n, _, _, _)| n == name) {
            self.scenarios[index].2.clear();
            self.scenarios[index].3.clear();
            return index;
        }
        self.scenarios.push((
            name.to_string(),
            how.to_string(),
            FrameStats::new(),
            FrameStats::new(),
        ));
        self.scenarios.len() - 1
    }
}

/// 前端读的整块状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpikeSnapshot {
    pub open: bool,
    pub adapter: AdapterInfo,
    pub format: String,
    pub alpha_mode: String,
    pub surface_size: (u32, u32),
    pub css_size: (u32, u32),
    pub monitor_scale: f32,
    pub monitor_name: String,
    pub maximized: bool,
    pub fullscreen: bool,
    pub decorated: bool,
    pub transparent: bool,
    pub upload_ms: f32,
    pub rendered: u64,
    pub viewport: ViewportView,
    pub scenarios: Vec<ScenarioView>,
    pub device_lost: Vec<String>,
    pub coord_max_error: f32,
    pub last_hit: Option<HitView>,
    pub last_frame_ms: f32,
    pub last_cpu_ms: f32,
    pub last_error: Option<String>,
    /// 客户区 / 窗口 / webview 三个原点（物理屏幕像素）与容器偏移
    pub client_origin: (i32, i32),
    pub window_origin: (i32, i32),
    pub webview_origin: Option<(i32, i32)>,
    /// `webview 原点 − 客户区原点`；只用于容器诊断，不据此宣称呈现已经对齐
    pub input_offset: Option<(i32, i32)>,
    /// 视口自查（`Viewport::sanity_problems`）—— 界面上要显眼
    pub viewport_problems: Vec<String>,
}

/// 会话：命令通道 + 共享状态。
struct Session {
    commands: Sender<SpikeCommand>,
    shared: Arc<Mutex<Shared>>,
}

/// 挂在 Tauri 状态上的 spike 会话。
#[derive(Default)]
pub struct SpikeState {
    session: Mutex<Option<Session>>,
}

/* ══════════════════════════════════════════════════════════════
 * 命令层
 * ══════════════════════════════════════════════════════════════ */

/// 打开调试窗口（已开着就直接返回当前状态）。
#[tauri::command]
pub async fn spike_open<R: Runtime>(app: AppHandle<R>) -> Result<SpikeSnapshot, String> {
    open_window(&app)
}

/// 开窗的**同步**实现：Tauri 的 setup 钩子里（启动参数）也要用，
/// 而那会儿不方便 await 一个命令。
pub fn open_window<R: Runtime>(app: &AppHandle<R>) -> Result<SpikeSnapshot, String> {
    if let Some(window) = app.get_webview_window(SPIKE_LABEL) {
        let state = app.state::<SpikeState>();
        let shared = state
            .session
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|s| s.shared.clone()));
        if let Some(shared) = shared {
            let _ = window.set_focus();
            return Ok(shared.lock().map_err(|e| e.to_string())?.snapshot());
        }
    }

    let window = WebviewWindowBuilder::new(
        app,
        SPIKE_LABEL,
        // `?spike=1` 让前端渲染 spike 页而不是应用外壳（dev 与打包版都走这一条）
        WebviewUrl::App("index.html?spike=1".into()),
    )
    .title("raybend 渲染 spike（M2-W1）")
    .inner_size(1280.0, 820.0)
    /*
     * ❗ **保留系统标题栏**（2026-09-18 回退）：去边框那版把窗口变成了「拿不动的盒子」——
     * `data-tauri-drag-region` 只在**挂了该属性的元素本身**上生效（子元素不继承），
     * 顶栏几乎被文字与按钮铺满，人抓不到能拖的地方；窗口又默认开在屏幕边缘之外，
     * 于是连测试都没法进行。**测试工具首先要能用**，其次才是与被测对象同形。
     *
     * 2026-09-19 根因：WebView 有效比例包含系统文字缩放，不能用 native scale 代替。
     * 保留系统标题栏，按测得的 WebView DPR 换算；不再猜测或补偿标题栏偏移。
     */
    .center()
    // 透明是**这一条 spike 的主角**：webview 中间挖洞，wgpu 在洞里出图
    .transparent(true)
    .resizable(true)
    .build()
    .map_err(|e| format!("建 spike 窗口失败：{e}"))?;

    let (tx, rx) = channel::<SpikeCommand>();
    let shared = Arc::new(Mutex::new(Shared::default()));
    {
        let mut guard = shared.lock().map_err(|e| e.to_string())?;
        guard.open = true;
        guard.viewport = Viewport {
            image_size: (6000, 4000),
            viewport_size: (1280.0, 820.0),
            dpr: window.scale_factor().unwrap_or(1.0) as f32,
            ..Default::default()
        };
        guard.viewport.refit();
    }

    /*
     * 窗口事实的**唯一**来源（见 [`WindowFacts`] 与 `read_window_facts` 的说明）：
     * 回调在**主线程**上跑，只有在这里问窗口才安全；问到的纯数据塞进暂存区，
     * 渲染线程自己来取 —— 它一个窗口查询都不做。
     *
     * ❗ 回调里**不许锁 `shared`**：渲染线程一旦僵住，主线程锁它就会跟着僵，
     * 那正是 2026-09-18 那次「拖动跨屏 → 未响应、松手也不恢复」的链条。
     * 暂存区只存纯数据（`Mutex<Option<WindowFacts>>`），让两个线程永远不互相等。
     */
    let pending_facts = Arc::new(Mutex::new(None::<WindowFacts>));
    {
        let tx = tx.clone();
        let pending_for_events = pending_facts.clone();
        let window_for_events = window.clone();
        window.on_window_event(move |event| match event {
            tauri::WindowEvent::Resized(size) => {
                note_window_event("Resized", format!("{}×{}", size.width, size.height));
                let facts = read_window_facts(&window_for_events);
                let _ = tx.send(SpikeCommand::Resize {
                    width: facts.surface_size.0,
                    height: facts.surface_size.1,
                    dpr: facts.dpr,
                });
                if let Ok(mut staged) = pending_for_events.lock() {
                    *staged = Some(facts);
                }
            }
            tauri::WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                note_window_event("ScaleFactorChanged", format!("dpr={scale_factor}"));
                let facts = read_window_facts(&window_for_events);
                let _ = tx.send(SpikeCommand::Resize {
                    width: facts.surface_size.0,
                    height: facts.surface_size.1,
                    dpr: facts.dpr,
                });
                if let Ok(mut staged) = pending_for_events.lock() {
                    *staged = Some(facts);
                }
            }
            tauri::WindowEvent::Moved(_) => {
                let facts = read_window_facts(&window_for_events);
                if let Ok(mut staged) = pending_for_events.lock() { *staged = Some(facts); }
            }
            other => {
                note_window_event("other", format!("{other:?}"));
            }
        });
    }

    /*
     * 主线程回显心跳（**诊断用**，见 `ASSISTANCE.md` A′）。
     *
     * 跨屏拖动会把主线程卡在原生调用里（现象：「未响应」、CPU 不涨、日志安静）。
     * 但日志安静本身分不清「渲染线程也停了」还是「只有主线程停了」——
     * 这里用一个**独立小线程**每 5 秒往主线程投一次回显：投递不等待
     * （`run_on_main_thread` 只是把闭包塞进事件队列就返回），所以卡住时
     * 日志会停在「派发…」而永远等不到「回显」。
     */
    {
        let echo_app = app.clone();
        std::thread::Builder::new()
            .name("spike-heartbeat".into())
            .spawn(move || {
                let mut beat = 0u32;
                loop {
                    std::thread::sleep(Duration::from_secs(5));
                    beat += 1;
                    eprintln!("[spike] 心跳 #{beat}：派发主线程回显…");
                    let posted = echo_app.run_on_main_thread(move || {
                        eprintln!("[spike] 心跳 #{beat}：主线程回显到了");
                    });
                    if let Err(error) = posted {
                        eprintln!("[spike] 心跳 #{beat}：派发失败（{error}）");
                    }
                }
            })
            .map_err(|e| format!("起心跳线程失败：{e}"))?;
    }

    let thread_window = window.clone();
    let thread_shared = shared.clone();
    // 建窗之后、渲染线程启动之前，在主线程上拿一次初值（安全时刻，见 `read_window_facts`）
    let initial_facts = read_window_facts(&window);
    let pending_facts_for_thread = pending_facts.clone();
    std::thread::Builder::new()
        .name("spike-render".into())
        .spawn(move || {
            if let Err(error) = render_loop(
                thread_window,
                rx,
                thread_shared.clone(),
                initial_facts,
                pending_facts_for_thread,
            )
                && let Ok(mut guard) = thread_shared.lock() {
                    guard.last_error = Some(error);
                }
        })
        .map_err(|e| format!("起渲染线程失败：{e}"))?;

    let state = app.state::<SpikeState>();
    {
        let mut guard = state.session.lock().map_err(|e| e.to_string())?;
        *guard = Some(Session {
            commands: tx,
            shared: shared.clone(),
        });
    }
    Ok(shared.lock().map_err(|e| e.to_string())?.snapshot())
}

/// 关掉调试窗口（渲染线程随之退出）。
#[tauri::command]
pub async fn spike_close<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let state = app.state::<SpikeState>();
    let session = state
        .session
        .lock()
        .map_err(|e| e.to_string())?
        .take();
    if let Some(session) = session {
        let _ = session.commands.send(SpikeCommand::Shutdown);
    }
    if let Some(window) = app.get_webview_window(SPIKE_LABEL) {
        window.destroy().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 发一条交互意图。
#[tauri::command]
pub async fn spike_command<R: Runtime>(
    app: AppHandle<R>,
    command: SpikeCommand,
) -> Result<SpikeSnapshot, String> {
    let shared = with_session(&app, |session| {
        let _ = session.commands.send(command);
        session.shared.clone()
    })?;
    // 命令是异步进线程的：这里**等一下再读**，免得界面上看到上一帧的值
    std::thread::sleep(Duration::from_millis(8));
    Ok(shared.lock().map_err(|e| e.to_string())?.snapshot())
}

/// 只读一次状态（界面轮询用）。
#[tauri::command]
pub async fn spike_snapshot<R: Runtime>(app: AppHandle<R>) -> Result<SpikeSnapshot, String> {
    let shared = with_session(&app, |session| session.shared.clone())?;
    Ok(shared.lock().map_err(|e| e.to_string())?.snapshot())
}

/// 命中测试：把鼠标位置（CSS 像素）交给 Rust 换算（坐标同步的判据）。
#[tauri::command]
pub async fn spike_hit_test<R: Runtime>(
    app: AppHandle<R>,
    x: f32,
    y: f32,
    dpr: f32,
) -> Result<SpikeSnapshot, String> {
    let shared = with_session(&app, |session| {
        let _ = session.commands.send(SpikeCommand::HitTest { x, y, dpr });
        session.shared.clone()
    })?;
    Ok(shared.lock().map_err(|e| e.to_string())?.snapshot())
}

/// 落盘报告（JSON + Markdown）。人填的那几项由前端传进来。
#[tauri::command]
pub async fn spike_write_report<R: Runtime>(
    app: AppHandle<R>,
    dir: String,
    notes: HumanNotes,
) -> Result<Vec<String>, String> {
    let shared = with_session(&app, |session| session.shared.clone())?;
    let window = app.get_webview_window(SPIKE_LABEL);
    let guard = shared.lock().map_err(|e| e.to_string())?;

    let scenario_stats: Vec<ScenarioStats> = guard
        .scenarios
        .iter()
        .map(|(name, how, frames, cpu)| ScenarioStats {
            name: name.clone(),
            how: how.clone(),
            frames: frames.clone(),
            cpu: cpu.clone(),
        })
        .collect();

    let report = SpikeReport {
        generated_at: local_now(),
        platform: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        app_version: app.package_info().version.to_string(),
        wgpu_backend_env: std::env::var("WGPU_BACKEND").unwrap_or_default(),
        adapter: guard.adapter.clone(),
        surface: SurfaceInfo {
            format: guard.format.clone(),
            alpha_mode: guard.alpha_mode.clone(),
            size_physical: guard.surface_size,
            size_css: guard.css_size,
            dpr: guard.viewport.dpr,
            monitor_scale: guard.monitor_scale,
            monitor_name: guard.monitor_name.clone(),
            is_maximized: guard.maximized,
            is_fullscreen: guard.fullscreen,
            is_decorated: guard.decorated,
            is_transparent: guard.transparent,
        },
        image_size: guard.viewport.image_size,
        upload_ms: guard.upload_ms,
        scenarios: scenario_stats,
        device_lost: guard.device_lost.clone(),
        viewport_problems: guard.viewport.sanity_problems(),
        coord_roundtrip_max_error_px: guard.coord_max_error,
        human: notes,
    };
    let diagnostics = guard.snapshot();
    drop(guard);

    let path = std::path::PathBuf::from(&dir);
    let (json, md) = report
        .write_to(&path)
        .map_err(|e| format!("写报告失败（{}）：{e}", path.display()))?;
    let diagnostics_path = path.join("spike-coordinates.json");
    let diagnostics_json = serde_json::to_vec_pretty(&diagnostics).map_err(|e| e.to_string())?;
    std::fs::write(&diagnostics_path, diagnostics_json).map_err(|e| e.to_string())?;
    let _ = window;
    Ok(vec![
        diagnostics_path.to_string_lossy().into_owned(),
        json.to_string_lossy().into_owned(),
        md.to_string_lossy().into_owned(),
    ])
}

fn with_session<R: Runtime, T>(
    app: &AppHandle<R>,
    f: impl FnOnce(&Session) -> T,
) -> Result<T, String> {
    let state = app.state::<SpikeState>();
    let guard = state.session.lock().map_err(|e| e.to_string())?;
    match guard.as_ref() {
        Some(session) => Ok(f(session)),
        None => Err("spike 窗口还没打开".to_string()),
    }
}

fn local_now() -> String {
    // 不引时间库：报告只需要「人看得懂的时间戳」，格式交给本地时区换算不划算
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("unix+{}s（本地时间见文件系统时间戳）", now.as_secs())
}

/* ══════════════════════════════════════════════════════════════
 * 渲染线程
 * ══════════════════════════════════════════════════════════════ */

/// 从窗口上取裸句柄（rwh 版本一致性由 `Cargo.lock` 保证：tauri 与 wgpu 都是 rwh 0.6）。
fn render_loop<R: Runtime>(
    window: tauri::WebviewWindow<R>,
    rx: Receiver<SpikeCommand>,
    shared: Arc<Mutex<Shared>>,
    initial: WindowFacts,
    pending_facts: Arc<Mutex<Option<WindowFacts>>>,
) -> Result<(), String> {
    /*
     * ⚠️ `size` / `dpr` 用调用方（主线程）取好的 `initial`，**不在这里问窗口** ——
     * 见 [`WindowFacts`] 上面那段：渲染线程查窗口会在拖动时永久阻塞。
     * 这里唯一调的窗口 API 是 `raw_handles()`（一次性取原生 HWND），不查窗口状态。
     */
    let size = tauri::PhysicalSize::new(initial.surface_size.0, initial.surface_size.1);
    let dpr = initial.dpr;
    let handles = raw_handles(&window)?;

    let started = Instant::now();
    let mut context = GpuContext::new(
        handles,
        (size.width, size.height),
        dpr,
        // spike 的图像是**合成测试图**（与真实照片同一个 `RenderImage` 类型）
        Some(raybend::render::scene::make_test_image(6000, 4000)),
        raybend::render::SurfaceComposition::Transparent,
        raybend::render::PresentationAdapter::PlatformDefault,
        "spike",
    )
    .map_err(|e| e.to_string())?;
    let upload_ms = started.elapsed().as_secs_f32() * 1000.0;

    // 纯数学自查；不能用于证明 DOM 与 GPU 最终呈现对齐。
    let coord_max_error = coord_roundtrip_error(context.viewport());
    let mut last_present: Option<Instant> = None;
    let mut script: Option<Script> = None;
    let mut last_beat: Option<Instant> = None;
    let mut beats = 0u32;

    {
        let mut guard = shared.lock().map_err(|e| e.to_string())?;
        guard.adapter = context.adapter_info();
        let details = context.surface_details();
        guard.format = details.format.clone();
        guard.alpha_mode = details.alpha_mode.clone();
        guard.surface_size = details.size;
        guard.upload_ms = upload_ms;
        guard.coord_max_error = coord_max_error;
        guard.apply_window_facts(&initial);
        guard.viewport = *context.viewport();
    }

    loop {
        // 空闲时阻塞在这里（不空转）；有脚本/采集时按 ~60Hz 醒来
        let timeout = if script.is_some() || is_recording(&shared) {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(200)
        };
        match rx.recv_timeout(timeout) {
            Ok(command) => {
                if matches!(command, SpikeCommand::Shutdown) {
                    break;
                }
                let t_command = Instant::now();
                let failed = apply_command(command, &mut context, &mut script, &shared);
                log_slow("apply_command", t_command.elapsed());
                if let Some(error) = failed
                    && let Some(mut guard) = lock_shared(&shared, "apply_command 结果回写") {
                        // 命令自己的失败也进历史：横幅会在下一帧成功出图时被撤掉（见下面那个 match），
                        // 只写横幅的话，过一会儿就查无此事了。
                        guard.device_lost.push(format!("命令失败：{error}"));
                        guard.last_error = Some(error);
                    }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        /*
         * 心跳：只说明「渲染线程还在转」，**不碰窗口**（这条原则见本文件上面那段：
         * 渲染线程零窗口查询）。冻结时日志安静，只有这条能区分「渲染线程也停了」
         * 与「只有主线程停了」—— 配合下面那条主线程回显用。
         */
        if last_beat.is_none_or(|at| at.elapsed() >= Duration::from_secs(5)) {
            last_beat = Some(Instant::now());
            beats += 1;
            eprintln!("[spike] 心跳 #{beats}（渲染线程在转，已出 {} 帧）", context.frames_drawn());
        }

        // 脚本化运动推进（不经过 IPC —— 量的是渲染本身，不是 IPC 往返）
        if let Some(active) = script.as_mut() {
            if Instant::now() >= active.deadline {
                script = None;
                stop_recording(&shared);
            } else {
                advance_script(&mut context, active);
            }
        }

        let cpu_start = Instant::now();
        let outcome = context.render();
        let cpu_ms = cpu_start.elapsed().as_secs_f32() * 1000.0;
        log_slow("render", cpu_start.elapsed());
        let now = Instant::now();
        let interval_ms = last_present
            .map(|last| (now - last).as_secs_f32() * 1000.0)
            .unwrap_or(0.0);
        last_present = Some(now);

        if let Some(mut guard) = lock_shared(&shared, "每帧写统计数据") {
            guard.rendered = context.frames_drawn();
            guard.publish_viewport(context.viewport());
            guard.last_frame_ms = interval_ms;
            guard.last_cpu_ms = cpu_ms;
            // 采集只在「有间隔样本」时记（第一帧没有间隔）
            if interval_ms > 0.0 {
                let (frames, cpu) = guard.frame_stats();
                if let Some(frames) = frames {
                    frames.push(interval_ms);
                    frames.add_window(interval_ms as f64);
                }
                if let Some(cpu) = cpu {
                    cpu.push(cpu_ms);
                }
            }
        }

        /*
         * 窗口事实**完全由事件推过来**（渲染线程零窗口查询，见 [`WindowFacts`] 上面那段）。
         * 这里的职责只剩：把事件暂存区里攒下的最新事实搬进共享状态。
         *
         * 暂存区是 `Mutex<Option<WindowFacts>>` 而不是直接写 `shared` —— 事件回调在
         * **主线程**上跑，它只做「存一个纯数据快照」这种极短的操作，绝不碰 `shared`，
         * 这样即便渲染线程正拿着 `shared` 也不会互相等。
         */
        if let Ok(mut staged) = pending_facts.lock()
            && let Some(facts) = staged.take()
            && let Some(mut guard) = lock_shared(&shared, "应用窗口事实")
        {
            guard.apply_window_facts(&facts);
        }

        match outcome {
            Ok(RenderOutcome::Drawn) | Ok(RenderOutcome::Skipped) => {
                /*
                 * 出图正常就把横幅撤掉：`last_error` 描述的是**当前**状态，不是历史。
                 *
                 * 2026-09-19 真机留的教训：`last_error` 只写不清，于是「演练丢失」留下的那句红字
                 * 永远挂在顶上 —— 人类点「恢复」时看到**一模一样的一句**，据此判断「恢复也失败了」。
                 * 实际上那一刻渲染线程已经被 `recover()` 里的 panic 打死（布局跨设备复用），
                 * 真正的错误只在 `/tmp/raybend-desktop.log` 的 panic 里。
                 * 撤横幅 + 把「第 N 帧重新出图」记进历史，这两个动作才让「恢复成没成」一眼可判。
                 */
                if let Some(mut guard) = lock_shared(&shared, "清除已恢复的错误")
                    && let Some(previous) = guard.last_error.take()
                {
                    guard.device_lost.push(format!(
                        "已恢复：第 {} 帧起重新出图（清掉横幅：{previous}）",
                        context.frames_drawn()
                    ));
                }
            }
            Ok(RenderOutcome::Reconfigured(reason)) => {
                if let Some(mut guard) = lock_shared(&shared, "记录 surface 重配") {
                    guard.device_lost.push(format!("{reason}（第 {} 帧）", context.frames_drawn()));
                }
            }
            Err(error) => {
                if let Some(mut guard) = lock_shared(&shared, "记录渲染错误") {
                    let text = error.to_string();
                    /*
                     * 带帧号进历史：恢复之后到底是「继续出图」还是「同一句错一直刷」，靠它分辨。
                     * 每帧都会重试，所以同一个错误连续出现时**只记第一条**（否则历史被刷屏淹没）。
                     */
                    let repeated = guard
                        .device_lost
                        .last()
                        .is_some_and(|last| last.starts_with("渲染错误") && last.contains(&text));
                    if !repeated {
                        guard.device_lost.push(format!(
                            "渲染错误（第 {} 帧）：{text}",
                            context.frames_drawn()
                        ));
                    }
                    guard.last_error = Some(text);
                }
            }
        }
    }
    Ok(())
}

fn is_recording(shared: &Arc<Mutex<Shared>>) -> bool {
    shared
        .lock()
        .map(|guard| guard.active_scenario.is_some())
        .unwrap_or(false)
}

fn stop_recording(shared: &Arc<Mutex<Shared>>) {
    if let Ok(mut guard) = shared.lock() {
        guard.active_scenario = None;
    }
}

fn advance_script(context: &mut GpuContext, script: &mut Script) {
    // 每帧当作 1/60 秒推进
    const DT: f32 = 1.0 / 60.0;
    match script.kind {
        ScriptKind::Pan { speed } => {
            // 平移范围：图像宽度的一半（图像会从中间往两边扫）
            let (width, _) = context.viewport().image_size;
            let zoom = context.viewport().zoom;
            let half = (width as f32 * zoom * 0.5).max(1.0);
            let pan = context.viewport().pan_px;
            if pan.0.abs() >= half {
                script.direction = -script.direction;
                context.viewport_mut().pan_px.0 = pan.0.signum() * half;
            }
            context.viewport_mut().pan_px.0 += script.direction * speed * DT;
            context.viewport_mut().fit_mode = FitMode::Free;
        }
        ScriptKind::Zoom { per_second } => {
            let zoom = context.viewport().zoom;
            if zoom >= MAX_SCRIPT_ZOOM {
                script.direction = -1.0;
            } else if zoom <= MIN_SCRIPT_ZOOM {
                script.direction = 1.0;
            }
            let factor = if script.direction > 0.0 {
                (per_second * DT).exp()
            } else {
                (-per_second * DT).exp()
            };
            // 锚点取洞口中心：与「滚轮以光标为锚点」区分开，这一项量的是连续缩放本身
            let rect = context.viewport().effective_rect();
            let anchor = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
            context.viewport_mut().zoom_at(anchor, factor);
        }
    }
}

/// 把一条命令落到上下文（返回错误文本，交给界面显示）。
fn apply_command(
    command: SpikeCommand,
    context: &mut GpuContext,
    script: &mut Option<Script>,
    shared: &Arc<Mutex<Shared>>,
) -> Option<String> {
    let how = match command {
        SpikeCommand::Zoom { x, y, factor, dpr } => {
            if let Ok(mut guard) = shared.lock() {
                if let Err(error) = guard.sync_webview_dpr(context.viewport_mut(), dpr) { return Some(error); }
                guard.last_pointer_css = Some((x, y));
            }
            let anchor = context.viewport().css_to_physical((x, y));
            context.viewport_mut().zoom_at(anchor, factor);
            "以光标为锚点缩放"
        }
        SpikeCommand::HitTest { x, y, dpr } => {
            if !x.is_finite() || !y.is_finite() { return Some("指针坐标必须为有限数".into()); }
            if let Ok(mut guard) = shared.lock() {
                if let Err(error) = guard.sync_webview_dpr(context.viewport_mut(), dpr) { return Some(error); }
                guard.last_pointer_css = Some((x, y));
            }
            return None;
        }
        SpikeCommand::Pan { dx, dy, dpr } => {
            if let Ok(mut guard) = shared.lock()
                && let Err(error) = guard.sync_webview_dpr(context.viewport_mut(), dpr)
            {
                return Some(error);
            }
            // 前端给的是 CSS 像素位移 → 换算成物理像素（乘 dpr）
            let dpr = context.viewport().dpr;
            context.viewport_mut().pan_by((dx * dpr, dy * dpr));
            "拖动平移到鼠标处"
        }
        SpikeCommand::Fit { mode } => {
            let fit = match mode.as_str() {
                "fill" => FitMode::Fill,
                "oneToOne" => FitMode::OneToOne,
                "free" => FitMode::Free,
                _ => FitMode::Fit,
            };
            context.viewport_mut().fit_mode = fit;
            context.viewport_mut().refit();
            "切换档位"
        }
        SpikeCommand::Rotate { degrees } => {
            context.viewport_mut().rotation = degrees % 360.0;
            "旋转"
        }
        SpikeCommand::SetHole { on } => {
            if let Ok(mut guard) = shared.lock() {
                guard.hole_disabled = !on;
                guard.sync_hole(context.viewport_mut());
            }
            "洞口开关"
        }
        SpikeCommand::HoleRect { x, y, width, height, dpr, viewport_width, viewport_height } => {
            if ![x, y, width, height].iter().all(|n| n.is_finite()) || width < 0.0 || height < 0.0 {
                return Some("洞口矩形必须是有限数，宽高不得为负".into());
            }
            if let Ok(mut guard) = shared.lock() {
                if let Err(error) = guard.sync_webview_dpr(context.viewport_mut(), dpr) { return Some(error); }
                guard.css_size = (viewport_width, viewport_height);
                guard.layout_hole_css = Some((x, y, width, height));
                guard.sync_hole(context.viewport_mut());
            }
            "洞口跟随界面布局"
        }
        SpikeCommand::Reset => {
            context.viewport_mut().rotation = 0.0;
            context.viewport_mut().fit_mode = FitMode::Fit;
            context.viewport_mut().refit();
            "复位"
        }
        SpikeCommand::Scripted { name, seconds } => {
            let kind = match name.as_str() {
                "zoom" => ScriptKind::Zoom { per_second: 1.6 },
                _ => ScriptKind::Pan { speed: 900.0 },
            };
            *script = Some(Script {
                kind,
                direction: 1.0,
                deadline: Instant::now() + Duration::from_secs_f32(seconds.max(0.5)),
            });
            let how = match name.as_str() {
                "zoom" => "脚本化连续缩放 3 秒（不经过 IPC）",
                _ => "脚本化左右平移 3 秒（不经过 IPC）",
            };
            if let Ok(mut guard) = shared.lock() {
                let index = guard.ensure_scenario(&name, how);
                guard.active_scenario = Some(index);
            }
            "脚本化运动"
        }
        SpikeCommand::Scenario { name } => match name {
            Some(name) => {
                if let Ok(mut guard) = shared.lock() {
                    let how = "人手动操作（拖动/滚轮/缩放窗口）";
                    let index = guard.ensure_scenario(&name, how);
                    guard.active_scenario = Some(index);
                }
                "开始采集"
            }
            None => {
                stop_recording(shared);
                "结束采集"
            }
        },
        SpikeCommand::DeviceLoss => {
            context.simulate_device_loss();
            if let Ok(mut guard) = shared.lock() {
                guard.device_lost.push("演练：device.destroy()".to_string());
            }
            return None;
        }
        SpikeCommand::Recover => {
            let result = context.recover();
            if let Ok(mut guard) = shared.lock() {
                match &result {
                    Ok(()) => guard.device_lost.push("恢复成功（设备/管线/纹理已重建）".to_string()),
                    Err(error) => guard.device_lost.push(format!("恢复失败：{error}")),
                }
            }
            return result.err().map(|error| error.to_string());
        }
        SpikeCommand::Resize {
            width,
            height,
            dpr,
        } => {
            // 即使 DOM 的 CSS 尺寸不变、ResizeObserver 不触发，也按新 DPR 重算洞口。
            if !dpr.is_finite() || dpr <= 0.0 { return Some("DPR 必须为有限正数".into()); }
            let css_dpr = shared.lock().ok().and_then(|guard| guard.webview_dpr).unwrap_or(dpr);
            context.resize(width, height, css_dpr);
            if let Ok(mut guard) = shared.lock() {
                guard.sync_hole(context.viewport_mut());
                guard.surface_size = (width, height);
                guard.publish_viewport(context.viewport());
            }
            "窗口尺寸变化"
        }
        SpikeCommand::Shutdown => "退出",
    };
    let _ = how;
    None
}

fn hit_view(viewport: &Viewport, css: (f32, f32)) -> HitView {
    let image = viewport.pointer_to_image(css);
    let center = viewport.image_center();
    HitView {
        css_x: css.0,
        css_y: css.1,
        center_css: viewport.physical_to_css(viewport.image_to_physical(center)),
        image_x: image.0,
        image_y: image.1,
        inside: viewport.hit_test_css(css).is_some(),
        at_center: (image.0 - center.0).abs() <= 2.0 && (image.1 - center.1).abs() <= 2.0,
    }
}

/// 坐标往返最大偏差：CSS → 图像 → 物理 → CSS，绕一圈回来差多少。
///
/// 只验证数学互逆，按物理像素报告误差；不能据此排除 DOM 比例或呈现链路错误。
fn coord_roundtrip_error(viewport: &Viewport) -> f32 {
    let (w, h) = viewport.viewport_size;
    let mut worst = 0.0f32;
    for gx in 0..=8 {
        for gy in 0..=8 {
            let css = (w / 8.0 * gx as f32 / viewport.dpr, h / 8.0 * gy as f32 / viewport.dpr);
            let image = viewport.pointer_to_image(css);
            let back = viewport.physical_to_css(viewport.image_to_physical(image));
            worst = worst.max((back.0 - css.0).abs() * viewport.dpr).max((back.1 - css.1).abs() * viewport.dpr);
        }
    }
    worst
}

/// 每帧刷新那些只有窗口才知道的事实（最大化/全屏/显示器/DPR）。
/// 加锁并**计时**：锁等待超过 100ms 就记一行。
///
/// 为什么值得专门做：渲染线程与「窗口事件处理器」、「前端每 300ms 的状态轮询」都在碰这把锁。
/// 一旦渲染线程在持有它的时候被拖住（跨线程调用、swapchain 重建），别的地方就会排队 ——
/// 「窗口卡住几秒」这类现象，**等锁耗时是能直接看出真相的探针**。
fn lock_shared<'a>(shared: &'a Arc<Mutex<Shared>>, what: &str) -> Option<MutexGuard<'a, Shared>> {
    let started = Instant::now();
    let guard = shared.lock().ok()?;
    let waited = started.elapsed();
    if waited.as_millis() >= 100 {
        eprintln!("[spike] 等锁 `{what}` 用了 {waited:?} —— 有人在长时间持锁");
    }
    Some(guard)
}

/// 限速的慢操作日志：同一类操作每秒最多打一行（免得本身把日志刷爆、拖慢渲染）。
fn log_slow(kind: &str, elapsed: Duration) {
    const THRESHOLD: Duration = Duration::from_millis(50);
    if elapsed < THRESHOLD {
        return;
    }
    static LAST: OnceLock<Mutex<std::collections::HashMap<String, Instant>>> = OnceLock::new();
    let map = LAST.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let Ok(mut guard) = map.lock() else { return };
    let now = Instant::now();
    if let Some(last) = guard.get(kind)
        && now.duration_since(*last) < Duration::from_secs(1)
    {
        return;
    }
    guard.insert(kind.to_string(), now);
    eprintln!("[spike] `{kind}` 耗时 {elapsed:?}（超过 {THRESHOLD:?}）");
}

/// 窗口事件计数（拖动 / 跨屏时会爆量 —— 这是判断「事件风暴」的依据）。
static WINDOW_EVENT_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// 记一次窗口事件，并**按秒汇总**打印（具体事件只在低频时逐个打，免得刷屏）。
///
/// 人类拖动窗口跨屏时会触发大量 `Resized` / `ScaleFactorChanged`，一旦卡死，
/// 这张时间线就是「哪个阶段、哪个事件量级」的第一手证据。
fn note_window_event(kind: &str, detail: String) {
    static LAST: OnceLock<Mutex<(Instant, usize)>> = OnceLock::new();
    let count = WINDOW_EVENT_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    let slot = LAST.get_or_init(|| Mutex::new((Instant::now(), 0)));
    let Ok(mut guard) = slot.lock() else { return };
    let (since, last_count) = *guard;
    let burst = count.saturating_sub(last_count);
    if since.elapsed() >= Duration::from_secs(1) {
        eprintln!("[spike] 窗口事件：过去 {:?} 共 {burst} 次（累计 {count}）；最近一次 {kind} {detail}", since.elapsed());
        *guard = (Instant::now(), count);
    } else if burst < 4 {
        // 低频时逐条打，便于对照时间线
        eprintln!("[spike] 窗口事件 {kind} {detail}（累计 {count}）");
    }
}

/// 从窗口问出来的「事实」快照。
///
/// 存在的理由（2026-09-18 卡死事故的修法）：这些查询**都要跨线程回主线程**，
/// 而 Windows 拖动窗口/跨屏时主线程跑的是**模态消息循环**，查询会被卡住不返回。
/// 所以规矩是两条：
///
/// 1. **问窗口时绝不持有 `shared` 锁** —— 先把事实读进这个纯数据快照，再拿锁写回；
/// 2. **只在事件驱动 + 兜底时问**（见渲染循环），不是每帧。
///
/// 持锁去问窗口的后果是死锁：渲染线程持锁等主线程，主线程等锁（抢 `shared` 的地方
/// 在主线程上也有）→ 窗口 `Not Responding`。
#[derive(Debug, Clone, Copy)]
struct WindowFacts {
    surface_size: (u32, u32),
    dpr: f32,
    maximized: bool,
    fullscreen: bool,
    decorated: bool,
    monitor_scale: f32,
    /// 客户区在屏幕上的原点（物理像素）
    client_origin: (i32, i32),
    /// 窗口（含边框）在屏幕上的原点（物理像素）
    window_origin: (i32, i32),
    webview_origin: Option<(i32, i32)>,
}

/*
 * ── 为什么事实必须由「事件」推给渲染线程，而不是渲染线程自己去问 ──────────
 *
 * 2026-09-18 第二次事故（人类把窗口从笔记本屏拖到外接屏 → 卡死、`Not Responding`，
 * 且**松手后也不恢复**）复盘：
 *
 * · Tao/Win32 的 `inner_size()` / `scale_factor()` / `current_monitor()` / `is_maximized()`
 *   都要**跨线程回主线程**取窗口状态；
 * · 拖动窗口时主线程跑的是 Win32 的**模态移动循环**（`WM_ENTERSIZEMOVE` 之后由系统接管）；
 * · 在这个循环里从**别的线程**发起的窗口查询不会返回 —— 它不是「慢」，是**永久阻塞**；
 * · 渲染线程于是僵在那里，`shared` 的统计也不再更新，主线程任何要拿 `shared` 的路径
 *   （命令、事件回调）跟着一起僵 → 窗口 `Not Responding`。
 *
 * 所以纪律升级为：**渲染线程对这四类查询实行零调用**。它需要的窗口事实全部由
 * 主线程的窗口事件推过来（`Resized` / `ScaleFactorChanged` 的 payload 里就带着值，
 * 其余开关量在事件回调里于主线程上问一次）。
 */

/// 问窗口当前事实。**只允许在「不会卡住」的时刻调用**（见 [`WindowFacts`]）。
///
/// ❗ 2026-09-18 第二次卡死事故后的结论：**渲染线程一律不再调这个函数**。
/// 它现在只有两个调用点，都在主线程、且都发生在窗口不处于模态移动/缩放循环时：
///   1. 建窗之后、渲染线程启动之前（拿一次初值）；
///   2. 窗口事件回调里（此时主线程正跑消息循环，这些查询是安全的 —— 而**渲染线程**
///      在那个时刻去查就会永久阻塞，`Not Responding` 就是这么来的）。
fn read_window_facts<R: Runtime>(window: &tauri::WebviewWindow<R>) -> WindowFacts {
    let surface_size = window
        .inner_size()
        .map(|size| (size.width, size.height))
        .unwrap_or((1280, 820));
    let monitor = window.current_monitor().ok().flatten();
    let client_origin = window.inner_position().ok();
    // WebView bounds 是相对父客户区的位置，screenX/Y 是浏览器窗口的屏幕坐标，不能混用。
    let webview: &tauri::Webview<R> = window.as_ref();
    let dpr = window.scale_factor().unwrap_or(1.0);
    let webview_origin = client_origin.and_then(|client| {
        webview.bounds().ok().map(|bounds| {
            let offset = bounds.position.to_physical::<i32>(dpr);
            (client.x + offset.x, client.y + offset.y)
        })
    });
    WindowFacts {
        webview_origin,
        surface_size,
        dpr: dpr as f32,
        maximized: window.is_maximized().unwrap_or(false),
        fullscreen: window.is_fullscreen().unwrap_or(false),
        decorated: window.is_decorated().unwrap_or(false),
        monitor_scale: monitor.as_ref().map_or(1.0, |m| m.scale_factor() as f32),
        client_origin: client_origin.map(|p| (p.x, p.y)).unwrap_or((0, 0)),
        window_origin: window
            .outer_position()
            .map(|p| (p.x, p.y))
            .unwrap_or((0, 0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_error_is_effectively_zero() {
        let mut viewport = Viewport {
            image_size: (6000, 4000),
            viewport_size: (1280.0, 820.0),
            dpr: 1.5,
            ..Default::default()
        };
        viewport.fit_mode = FitMode::Fit;
        viewport.refit();
        assert!(coord_roundtrip_error(&viewport) < 0.01);
    }

    #[test]
    fn hit_view_marks_the_image_center() {
        let mut viewport = Viewport {
            image_size: (6000, 4000),
            viewport_size: (1280.0, 820.0),
            dpr: 1.0,
            ..Default::default()
        };
        viewport.fit_mode = FitMode::Fit;
        viewport.refit();
        // 洞口整窗时中心就是视口中心
        let hit = hit_view(&viewport, (640.0, 410.0));
        assert!(hit.inside);
        assert!(hit.at_center, "视口中心应当命中图像中心：{hit:?}");
        // 视口外面（负坐标）不命中
        let outside = hit_view(&viewport, (-10.0, -10.0));
        assert!(!outside.inside);
    }

    #[test]
    fn screenshot_regression_os_125_percent_and_webview_110_percent() {
        // QQ_1789714051780.png：原代码洞口右边为 1030，DOM 实际约为 1133（均相对客户区）。
        let mut shared = Shared {
            layout_hole_css: Some((300.0, 37.0, 524.0, 707.0)),
            ..Default::default()
        };
        let mut viewport = Viewport {
            image_size: (6000, 4000), viewport_size: (1598.0, 1022.0),
            dpr: 1.25, fit_mode: FitMode::OneToOne, ..Default::default()
        };
        shared.sync_hole(&mut viewport);
        let old = viewport.clip_rect.unwrap();
        assert_eq!(old.x + old.width, 1030.0);
        shared.sync_webview_dpr(&mut viewport, 1.375).unwrap();
        let rect = viewport.clip_rect.unwrap();
        assert_eq!((rect.x, rect.y, rect.width, rect.height), (412.5, 50.875, 720.5, 972.125));
        assert_eq!(rect.x + rect.width, 1133.0);
        // 灰块中心、鼠标命中和矩阵投影要用同一个有效 DPR。
        let css_center = (562.0, 390.5);
        assert!(hit_view(&viewport, css_center).at_center);
        let anchor = viewport.css_to_physical(css_center);
        viewport.zoom_at(anchor, 1.12);
        let actual = viewport.image_to_physical(viewport.image_center());
        assert!((actual.0 - anchor.0).abs() < 0.001);
        assert!((actual.1 - anchor.1).abs() < 0.001);
        let before = viewport;
        for invalid in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(shared.sync_webview_dpr(&mut viewport, invalid).is_err());
            assert_eq!(viewport, before);
        }
    }

    #[test]
    fn layout_ipc_uses_camel_case_fields_and_requires_actual_dpr() {
        let command: SpikeCommand = serde_json::from_value(serde_json::json!({
            "kind": "holeRect", "x": 300, "y": 37, "width": 524, "height": 707,
            "dpr": 1.375, "viewportWidth": 1162, "viewportHeight": 743
        })).unwrap();
        assert!(matches!(command, SpikeCommand::HoleRect { dpr: 1.375, viewport_width: 1162, .. }));
        assert!(serde_json::from_value::<SpikeCommand>(serde_json::json!({
            "kind": "zoom", "x": 1, "y": 2, "factor": 1.12
        })).is_err());
    }

    #[test]
    fn css_hole_reprojects_on_dpi_change_and_stays_disabled() {
        let mut shared = Shared {
            layout_hole_css: Some((300.0, 37.0, 524.0, 707.0)),
            ..Default::default()
        };
        let mut viewport = Viewport {
            image_size: (6000, 4000),
            viewport_size: (1598.0, 1022.0),
            dpr: 1.0,
            fit_mode: FitMode::OneToOne,
            ..Default::default()
        };
        for dpr in [1.0, 1.25, 1.5, 2.0, 1.0] {
            viewport.dpr = dpr;
            shared.sync_hole(&mut viewport);
            let rect = viewport.clip_rect.unwrap();
            assert_eq!((rect.x, rect.y), (300.0 * dpr, 37.0 * dpr));
            assert_eq!((rect.width, rect.height), (524.0 * dpr, 707.0 * dpr));
            assert!(hit_view(&viewport, (562.0, 390.5)).at_center);
        }
        shared.hole_disabled = true;
        shared.sync_hole(&mut viewport);
        viewport.dpr = 1.5;
        shared.layout_hole_css = Some((310.0, 40.0, 500.0, 700.0));
        shared.sync_hole(&mut viewport);
        assert!(viewport.clip_rect.is_none());
        assert!(shared.hole_css.is_none());
        shared.hole_disabled = false;
        shared.sync_hole(&mut viewport);
        assert_eq!(viewport.clip_rect.unwrap().x, 465.0);
    }

    #[test]
    fn stationary_pointer_is_recomputed_after_view_changes() {
        let mut shared = Shared { last_pointer_css: Some((640.0, 410.0)), ..Default::default() };
        let mut viewport = Viewport {
            image_size: (6000, 4000), viewport_size: (1280.0, 820.0),
            fit_mode: FitMode::OneToOne, ..Default::default()
        };
        viewport.refit();
        shared.publish_viewport(&viewport);
        assert!(shared.last_hit.as_ref().unwrap().at_center);
        viewport.pan_by((40.0, 30.0));
        shared.publish_viewport(&viewport);
        let hit = shared.last_hit.as_ref().unwrap();
        assert_eq!((hit.image_x, hit.image_y), (2960.0, 1970.0));
        assert_eq!(hit.center_css, (680.0, 440.0));
        assert_eq!((hit.css_x, hit.css_y), (640.0, 410.0));
    }

    #[test]
    fn layout_change_keeps_free_pan_and_zero_hole_does_not_draw() {
        let mut shared = Shared { layout_hole_css: Some((300.0, 37.0, 0.0, 0.0)), ..Default::default() };
        let mut viewport = Viewport { fit_mode: FitMode::Free, pan_px: (33.8, 44.0), ..Default::default() };
        shared.sync_hole(&mut viewport);
        assert_eq!(viewport.pan_px, (33.8, 44.0));
        assert!(viewport.scissor().is_none());
    }

    #[test]
    fn scenario_bookkeeping_reuses_the_same_name() {
        let mut shared = Shared::default();
        let first = shared.ensure_scenario("pan", "拖动");
        assert_eq!(shared.scenarios.len(), 1);
        let second = shared.ensure_scenario("pan", "拖动");
        assert_eq!(first, second, "同名场景应当复用（人可以反复点）");
        assert_eq!(shared.scenarios.len(), 1);
        let other = shared.ensure_scenario("zoom", "滚轮");
        assert_ne!(first, other);
        assert_eq!(shared.scenarios.len(), 2);
    }

    #[test]
    fn frame_stats_only_record_for_the_active_scenario() {
        let mut shared = Shared::default();
        assert!(shared.frame_stats().0.is_none(), "没在采集时不该记录");
        let index = shared.ensure_scenario("pan", "拖动");
        shared.active_scenario = Some(index);
        let (frames, cpu) = shared.frame_stats();
        frames.expect("应当有帧统计").push(16.0);
        cpu.expect("应当有 CPU 统计").push(2.0);
        assert_eq!(shared.scenarios[index].2.count(), 1);
        assert_eq!(shared.scenarios[index].3.count(), 1);
    }
}
