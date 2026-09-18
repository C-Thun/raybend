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
use raybend::render::{GpuContext, RawHandles, RenderOutcome, Viewport};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

/// 调试窗口的 label（`tauri.conf.json` 里没有它 —— 它是按需建的）。
pub const SPIKE_LABEL: &str = "spike-viewport";

/// 洞口的默认内边距（CSS 像素）：四周留出来给界面面板。
const HOLE_MARGIN_CSS: f32 = 190.0;

/// 脚本化缩放的折返上下限（不跑到视口的极端值上，免得量到「只剩一格像素」那种情况）。
const MIN_SCRIPT_ZOOM: f32 = 0.05;
const MAX_SCRIPT_ZOOM: f32 = 4.0;

/* ══════════════════════════════════════════════════════════════
 * 命令
 * ══════════════════════════════════════════════════════════════ */

/// 前端发来的交互意图（**只有意图，没有坐标数学** —— `AGENTS.md` §6.1 红线 #1）。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SpikeCommand {
    /// 以某个 CSS 像素点为锚点缩放
    Zoom { x: f32, y: f32, factor: f32 },
    /// 命中测试：把鼠标位置交给 Rust 换算成图像像素（**不动视口**，坐标同步的判据）
    HitTest { x: f32, y: f32 },
    /// 平移（CSS 像素位移）
    Pan { dx: f32, dy: f32 },
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
    pub scenarios: Vec<(String, String, FrameStats, FrameStats)>,
    pub active_scenario: Option<usize>,
    pub device_lost: Vec<String>,
    pub coord_max_error: f32,
    pub last_hit: Option<HitView>,
    pub last_error: Option<String>,
    /// 最近一次的帧间隔与 CPU 时长（界面上实时看）
    pub last_frame_ms: f32,
    pub last_cpu_ms: f32,
}

impl Shared {
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

    // 窗口事件 → 命令：尺寸/DPR 变化必须通知渲染线程（否则 surface 与坐标全错）
    //
    // 这里还挂两件事：① 标记「窗口事实脏了」（渲染线程据此刷新，见循环里的说明）；
    // ② 记一笔事件日志 —— 拖动跨屏时这里会爆量，卡死时的日志就是第一手证据。
    let facts_dirty = Arc::new(std::sync::atomic::AtomicBool::new(true));
    {
        let tx = tx.clone();
        let shared_for_events = shared.clone();
        let facts_dirty_for_events = facts_dirty.clone();
        window.on_window_event(move |event| match event {
            tauri::WindowEvent::Resized(size) => {
                facts_dirty_for_events.store(true, std::sync::atomic::Ordering::Relaxed);
                note_window_event("Resized", format!("{}×{}", size.width, size.height));
                let dpr = lock_shared(&shared_for_events, "事件·Resized 取 dpr")
                    .map(|guard| guard.viewport.dpr)
                    .unwrap_or(1.0);
                let _ = tx.send(SpikeCommand::Resize {
                    width: size.width,
                    height: size.height,
                    dpr,
                });
            }
            tauri::WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                facts_dirty_for_events.store(true, std::sync::atomic::Ordering::Relaxed);
                note_window_event("ScaleFactorChanged", format!("dpr={scale_factor}"));
                let size = lock_shared(&shared_for_events, "事件·ScaleFactorChanged 取尺寸")
                    .map(|guard| guard.surface_size)
                    .unwrap_or((1280, 820));
                let _ = tx.send(SpikeCommand::Resize {
                    width: size.0,
                    height: size.1,
                    dpr: *scale_factor as f32,
                });
            }
            other => {
                note_window_event("other", format!("{other:?}"));
            }
        });
    }

    let thread_window = window.clone();
    let thread_shared = shared.clone();
    // 事件处理器已经在用克隆的那份；这份原件移动进渲染线程（它负责刷新窗口事实）
    let facts_dirty_for_thread = facts_dirty.clone();
    std::thread::Builder::new()
        .name("spike-render".into())
        .spawn(move || {
            if let Err(error) = render_loop(
                thread_window,
                rx,
                thread_shared.clone(),
                facts_dirty_for_thread,
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
) -> Result<SpikeSnapshot, String> {
    let shared = with_session(&app, |session| {
        let _ = session.commands.send(SpikeCommand::HitTest { x, y });
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
    drop(guard);

    let path = std::path::PathBuf::from(&dir);
    let (json, md) = report
        .write_to(&path)
        .map_err(|e| format!("写报告失败（{}）：{e}", path.display()))?;
    let _ = window;
    Ok(vec![
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
fn raw_handles<R: Runtime>(window: &tauri::WebviewWindow<R>) -> Result<RawHandles, String> {
    use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
    let display = window
        .display_handle()
        .map_err(|e| format!("取 display handle 失败：{e}"))?
        .as_raw();
    let handle = window
        .window_handle()
        .map_err(|e| format!("取 window handle 失败：{e}"))?
        .as_raw();
    Ok(RawHandles {
        display,
        window: handle,
    })
}

fn render_loop<R: Runtime>(
    window: tauri::WebviewWindow<R>,
    rx: Receiver<SpikeCommand>,
    shared: Arc<Mutex<Shared>>,
    facts_dirty: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), String> {
    let size = window
        .inner_size()
        .map_err(|e| format!("拿窗口尺寸失败：{e}"))?;
    let dpr = window.scale_factor().unwrap_or(1.0) as f32;
    let handles = raw_handles(&window)?;

    let started = Instant::now();
    let mut context =
        GpuContext::new(handles, (size.width, size.height), dpr).map_err(|e| e.to_string())?;
    let upload_ms = started.elapsed().as_secs_f32() * 1000.0;

    // 坐标往返自查：拿一圈点走一遍 CSS → 图像 → CSS，取最大偏差
    let coord_max_error = coord_roundtrip_error(context.viewport());
    let mut last_present: Option<Instant> = None;
    let mut script: Option<Script> = None;
    // 上次刷新窗口事实的时刻（不再每帧去问窗口，见循环里的说明）
    let mut facts_last = Instant::now();

    {
        let mut guard = shared.lock().map_err(|e| e.to_string())?;
        guard.adapter = context.adapter_info();
        let details = context.surface_details();
        guard.format = details.format.clone();
        guard.alpha_mode = details.alpha_mode.clone();
        guard.surface_size = details.size;
        guard.upload_ms = upload_ms;
        guard.coord_max_error = coord_max_error;
        refresh_window_facts(&window, &mut guard);
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
                        guard.last_error = Some(error);
                    }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
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
            guard.viewport = *context.viewport();
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
            refresh_window_facts(&window, &mut guard);
        }

        /*
         * 窗口事实（尺寸 / dpr / 显示器 / 最大化 / 边框）**不再每帧去问窗口**。
         *
         * 为什么：`inner_size()` / `scale_factor()` / `current_monitor()` 这些都要跨线程回主线程 ——
         * 而 Windows **拖动窗口时跑的是模态消息循环**，这些调用会被拖住。每帧都问就等于
         * 每帧去碰一次那个循环（人类报的「拖到另一块屏就卡几秒」机理吻合）。
         * 现在改成**事件驱动**（`Resized` / `ScaleFactorChanged` 置脏）+ 1 秒兑底。
         */
        if facts_dirty.swap(false, std::sync::atomic::Ordering::Relaxed)
            || facts_last.elapsed() >= Duration::from_secs(1)
        {
            facts_last = Instant::now();
            if let Some(mut guard) = lock_shared(&shared, "刷新窗口事实") {
                refresh_window_facts(&window, &mut guard);
            }
        }

        match outcome {
            Ok(RenderOutcome::Drawn) | Ok(RenderOutcome::Skipped) => {}
            Ok(RenderOutcome::Reconfigured(reason)) => {
                if let Some(mut guard) = lock_shared(&shared, "记录 surface 重配") {
                    guard.device_lost.push(format!("{reason}（第 {} 帧）", context.frames_drawn()));
                }
            }
            Err(error) => {
                if let Some(mut guard) = lock_shared(&shared, "记录渲染错误") {
                    guard.last_error = Some(error.to_string());
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
        SpikeCommand::Zoom { x, y, factor } => {
            let anchor = context.viewport().css_to_physical((x, y));
            context.viewport_mut().zoom_at(anchor, factor);
            "以光标为锚点缩放"
        }
        SpikeCommand::HitTest { x, y } => {
            let hit = hit_view(context.viewport(), (x, y));
            if let Ok(mut guard) = shared.lock() {
                guard.last_hit = Some(hit);
            }
            return None;
        }
        SpikeCommand::Pan { dx, dy } => {
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
            if on {
                let (w, h) = context.viewport().viewport_size;
                let dpr = context.viewport().dpr;
                let margin = HOLE_MARGIN_CSS * dpr;
                let rect = raybend::render::ClipRect {
                    x: margin,
                    y: margin,
                    width: (w - margin * 2.0).max(1.0),
                    height: (h - margin * 2.0).max(1.0),
                };
                context.viewport_mut().clip_rect = Some(rect);
                if let Ok(mut guard) = shared.lock() {
                    guard.hole_css = Some((
                        HOLE_MARGIN_CSS,
                        HOLE_MARGIN_CSS,
                        (rect.width / dpr).max(1.0),
                        (rect.height / dpr).max(1.0),
                    ));
                }
            } else {
                context.viewport_mut().clip_rect = None;
                if let Ok(mut guard) = shared.lock() {
                    guard.hole_css = None;
                }
            }
            context.viewport_mut().refit();
            "洞口开关"
        }
        SpikeCommand::HoleRect {
            x,
            y,
            width,
            height,
        } => {
            let dpr = context.viewport().dpr;
            context.viewport_mut().clip_rect = Some(raybend::render::ClipRect {
                x: x * dpr,
                y: y * dpr,
                width: (width * dpr).max(1.0),
                height: (height * dpr).max(1.0),
            });
            // 洞口变了要按**新洞口**重新适配：适配的参照系是洞口而不是整窗
            // （`fit_modes_use_the_hole_not_the_window` 就是这个口径）。
            // 漏了这一步的后果实测过：首次上报洞口时适配已经按整窗算完了，zoom 停在 0.41，
            // 而按洞口应该是 0.05 上下 —— 洞里看到的是一块放大的局部，不是「整张图适配在洞里」。
            // `Free` 档不动：那时用户已经自己缩放过，重适配会把他的操作抹掉。
            if context.viewport().fit_mode != FitMode::Free {
                context.viewport_mut().refit();
            }
            if let Ok(mut guard) = shared.lock() {
                guard.hole_css = Some((x, y, width, height));
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
            // 窗口尺寸变了：洞口跟着界面走，所以只重算「按内边距」的那种默认洞口
            context.resize(width, height, dpr);
            if let Ok(mut guard) = shared.lock() {
                guard.surface_size = (width, height);
                guard.css_size = (
                    (width as f32 / dpr) as u32,
                    (height as f32 / dpr) as u32,
                );
                guard.viewport = *context.viewport();
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
        image_x: image.0,
        image_y: image.1,
        inside: viewport.hit_test_css(css).is_some(),
        at_center: (image.0 - center.0).abs() <= 2.0 && (image.1 - center.1).abs() <= 2.0,
    }
}

/// 坐标往返最大偏差：CSS → 图像 → 物理 → CSS，绕一圈回来差多少。
///
/// 这是**程序化**的坐标同步检查（人眼那条另说）：屏幕上换算回来必须几乎为 0，
/// 否则就是「图跟不上鼠标」那类漂移。
fn coord_roundtrip_error(viewport: &Viewport) -> f32 {
    let (w, h) = viewport.viewport_size;
    let mut worst = 0.0f32;
    for gx in 0..=8 {
        for gy in 0..=8 {
            let css = (w / 8.0 * gx as f32 / viewport.dpr, h / 8.0 * gy as f32 / viewport.dpr);
            let image = viewport.pointer_to_image(css);
            let back = viewport.physical_to_css(viewport.image_to_physical(image));
            worst = worst.max((back.0 - css.0).abs()).max((back.1 - css.1).abs());
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

fn refresh_window_facts<R: Runtime>(window: &tauri::WebviewWindow<R>, shared: &mut Shared) {
    shared.maximized = window.is_maximized().unwrap_or(false);
    shared.fullscreen = window.is_fullscreen().unwrap_or(false);
    shared.decorated = window.is_decorated().unwrap_or(false);
    if let Ok(Some(monitor)) = window.current_monitor() {
        shared.monitor_scale = monitor.scale_factor() as f32;
        shared.monitor_name = monitor
            .name()
            .cloned()
            .unwrap_or_else(|| "(未命名显示器)".to_string());
    }
    if let Ok(size) = window.inner_size() {
        shared.surface_size = (size.width, size.height);
    }
    let dpr = window.scale_factor().unwrap_or(1.0) as f32;
    shared.viewport.dpr = dpr;
    shared.css_size = (
        (shared.surface_size.0 as f32 / dpr) as u32,
        (shared.surface_size.1 as f32 / dpr) as u32,
    );
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
