//! 窗口还原：关闭时记录四角点位比例 + 屏幕身份，启动时只判断一次。
//!
//! 规格：`specs/window-restore.md`。设计口径（崔总 2026-10-31）：
//! * 记录的是**四角点位占所在屏工作区的比例**，不是「按比例缩放窗口」；
//! * 启动只判断一次：记录是否在、四角比例是否都在 0–1；
//! * 三种操作：还原（按四角映射）/ Shift+最大化回退 / 最大化；
//! * 每个操作末尾做一次 titlebar 保底，撞上最小尺寸就到此为止，不循环、不重试；
//! * 最小化关闭 = 清除记忆；最大化关闭 = 下次定在该屏并自动最大化；
//! * 只在正常关闭时写 `windows-state.json`；崩溃保留上一次正常关闭的文件。
//!
//! 文件位置：`%LOCALAPPDATA%\com.cthun.raybend\windows-state.json`（`db::data_dir`）。
//! 删除该文件即可重置窗口记忆。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, PhysicalSize, Runtime, WebviewWindow, WindowEvent};

use crate::db;
use crate::window_layout::{self, MinInnerSize, PixelRect, WorkAreaInsets};

/// 状态文件版本；未知版本按「无记录」处理（不迁移、不猜）。
const STATE_VERSION: u32 = 1;
/// 状态文件名（崔总指定：独立的 `windows-state.json`）。
const STATE_FILE: &str = "windows-state.json";

/// 四角点位比例（相对记录时所在屏的工作区；0–1 表示完整落在屏内）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Corners {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// 保存时的一块显示器。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MonitorSnapshot {
    /// Tauri `Monitor::name()`：Windows 上是 `\\.\DISPLAYn`（GDI 设备名，不保证永久）。
    pub name: Option<String>,
    /// Windows PnP 设备路径（`EnumDisplayDevicesW` + `EDD_GET_DEVICE_INTERFACE_NAME`）。
    pub pnp_id: Option<String>,
    /// 显示器矩形（物理像素）。
    pub rect: PixelRect,
    /// 工作区矩形（排除任务栏；四角比例的基准）。
    pub work: PixelRect,
    pub scale: f64,
}

/// 一条主窗口记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WindowRecord {
    /// 内框绝对矩形（保留给诊断、最大化时的正常态还原）。
    pub inner: PixelRect,
    /// 四角点位比例（真正的还原依据）。
    pub corners: Corners,
    pub maximized: bool,
    pub monitor: MonitorSnapshot,
    /// 保存时全部显示器快照（只进日志，不参与决策）。
    #[serde(default)]
    pub topology: Vec<MonitorSnapshot>,
    pub saved_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StateFile {
    version: u32,
    #[serde(default)]
    main: Option<WindowRecord>,
}

/// 启动决策的三种操作（外加两种回退情况，都是 Fit）。
#[derive(Debug, Clone, PartialEq)]
enum StartupAction {
    /// 无记录 / 记录不可用 / 原屏不可识别 / 四角越界：目标屏 Shift+最大化回退。
    Fit(MonitorSnapshot),
    /// 按四角点位比例还原。
    Restore {
        monitor: MonitorSnapshot,
        rect: PixelRect,
    },
    /// 定在该屏并最大化（记录为最大化、原屏可识别）。
    Maximize {
        monitor: MonitorSnapshot,
        normal: PixelRect,
    },
}

impl StartupAction {
    fn case(&self) -> &'static str {
        match self {
            Self::Fit(_) => "fit",
            Self::Restore { .. } => "restore",
            Self::Maximize { .. } => "maximize",
        }
    }
}

/// 运行期的窗口记忆（只在进程内；关闭时落 `windows-state.json`）。
#[derive(Default)]
struct WindowMemory {
    started: bool,
    restoring: bool,
    /// 最后一次非最大化、非最小化的内框矩形；最大化关闭时用它做「正常态」。
    normal_inner: Option<PixelRect>,
    maximized: bool,
    /// 启动记录为最大化：正常态已摆好，等 reveal 前一刻再 maximize（避免在闪屏期间
    /// 把隐藏的主窗口提前显示出来）。
    pending_maximize: bool,
    monitor: Option<MonitorSnapshot>,
}

/// 托管在 `AppHandle` 上的窗口记忆。
#[derive(Default)]
pub(crate) struct WindowStateStore {
    inner: std::sync::Mutex<WindowMemory>,
}

impl WindowStateStore {
    fn lock(&self) -> std::sync::MutexGuard<'_, WindowMemory> {
        self.inner.lock().unwrap_or_else(|error| error.into_inner())
    }
}

/// 启动时在主窗口上装好事件与还原逻辑（只调一次）。
pub(crate) fn install<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    let handle = app.clone();
    let win = window.clone();
    window.on_window_event(move |event| handle_window_event(&handle, &win, event));
    apply_on_startup(app, window);
}

/// 正常退出兜底：`CloseRequested` 没走到时（例如 `app.exit()`）再写一次。
/// 窗口已经销毁就跳过——`CloseRequested` 已经写过，崩溃时也不会走到这里。
pub(crate) fn save_on_exit<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(crate::MAIN_WINDOW_LABEL) {
        save_on_close(app, &window);
    }
}

/// 启动记录为最大化时，在 `show()` 前一刻再真正 maximize。
///
/// 为什么延后：Windows 的 `maximize()` 会 `ShowWindow`，在闪屏期间对隐藏主窗口调用
/// 可能让它提前露出来。这里只把正常态先摆好；到 reveal 前再最大化（若它自己显示，
/// 刚好就是 reveal 的那一刻），然后由调用方 `show()`。
pub(crate) fn apply_pending_maximize<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    let store = app.state::<WindowStateStore>();
    let monitor = {
        let mut memory = store.lock();
        if !memory.pending_maximize {
            return;
        }
        memory.pending_maximize = false;
        memory.maximized = true;
        memory.restoring = true;
        memory.monitor.clone()
    };

    if let Err(error) = window.maximize() {
        log::warn!(target: "window", "启动最大化失败：{error}");
    }
    // 与 window_layout 的 unmaximize 一样，用 getter 当一次屏障（窗口消息同队列 FIFO）。
    let _ = window.is_maximized();
    if let Some(monitor) = monitor
        && let Err(error) = window_layout::ensure_titlebar_visible(window, monitor.work)
    {
        log::warn!(target: "window", "最大化后 titlebar 保底失败：{error}");
    }
    store.lock().restoring = false;
}

/// 主窗口 show 之后同步一次 Windows restore 矩形。
///
/// 启动阶段的 SetWindowPos 是异步的，maximize 后立即写仍可能被后排队的消息覆盖；
/// show 之后这些几何消息已经落地，此时写入的正常态才会稳定留在 rcNormalPosition 里。
pub(crate) fn sync_restore_after_show<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    let normal = app.state::<WindowStateStore>().lock().normal_inner;
    if let Some(normal) = normal
        && let Err(error) = window_layout::set_restore_inner_rect(window, normal)
    {
        log::warn!(target: "window", "show 后同步正常态 restore 矩形失败：{error}");
    }
}

/// 标题栏与命令面板的「最大化 / 还原」唯一入口。
///
/// 反最大化不能直接交给系统：Windows 可能持有窗口创建阶段的默认 restore 矩形。
/// 优先取内存里最后一次正常态，先写 restore 矩形，再还原并按宽高分别做最小尺寸保底。
/// 返回操作后的最大化状态，前端仍会通过 `isMaximized()` 再读一次真实值。
#[tauri::command]
pub fn window_toggle_maximize<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Result<bool, String> {
    let maximized = window
        .is_maximized()
        .map_err(|error| format!("读取最大化状态失败：{error}"))?;
    if !maximized {
        window
            .maximize()
            .map_err(|error| format!("最大化失败：{error}"))?;
        return Ok(true);
    }

    let store = app.state::<WindowStateStore>();
    let cached_normal = store.lock().normal_inner;
    let scale = current_monitor(&window).map_or(1.0, |monitor| monitor.scale);
    let min = window_layout::main_min_inner_size(&app).physical(scale);
    // 在反最大化**之前**确定目标：缓存正常态优先，系统 restore 矩形只作兜底。
    let target = match cached_normal {
        Some(normal) => Some(normal.with_min_size(min)),
        None => {
            window_layout::read_restore_inner_rect(&window)?.map(|normal| normal.with_min_size(min))
        }
    };
    if let Some(target) = target {
        // 先修正系统 restore 矩形，避免 SW_RESTORE 先跳到未知的小默认尺寸。
        // 写入失败不阻断还原：后面的 set_inner_rect 仍会按同一目标复核。
        if let Err(error) = window_layout::set_restore_inner_rect(&window, target) {
            log::warn!(target: "window", "反最大化前写入 restore 矩形失败：{error}");
        }
    }

    window
        .unmaximize()
        .map_err(|error| format!("取消最大化失败：{error}"))?;
    let _ = window.is_maximized();
    let actual = window_layout::read_inner(&window)?;
    let target = target.unwrap_or(actual).with_min_size(min);
    let (actual, _) = window_layout::set_inner_rect(&window, target)?;
    // 缓存已校验的目标，不用异步 SetWindowPos 尚未落地时的读回旧值污染正常态。
    store.lock().normal_inner = Some(target);
    log::info!(
        target: "window",
        "反最大化：cached={} target={target:?} actual={actual:?} min={}×{}",
        cached_normal.is_some(),
        min.width,
        min.height,
    );
    Ok(false)
}

fn handle_window_event<R: Runtime>(
    app: &AppHandle<R>,
    window: &WebviewWindow<R>,
    event: &WindowEvent,
) {
    match event {
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
            let store = app.state::<WindowStateStore>();
            let mut memory = store.lock();
            if memory.restoring || memory.pending_maximize {
                return;
            }
            let maximized = window.is_maximized().unwrap_or(false);
            let minimized = window.is_minimized().unwrap_or(false);
            if maximized || minimized {
                memory.maximized = maximized;
                return;
            }
            if let Ok(inner) = window_layout::read_inner(window) {
                memory.normal_inner = Some(inner);
                memory.maximized = false;
            }
        }
        WindowEvent::CloseRequested { .. } => save_on_close(app, window),
        _ => {}
    }
}

/// 启动还原：只执行一次；失败只记日志，不重试。
fn apply_on_startup<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    let store = app.state::<WindowStateStore>();
    {
        let mut memory = store.lock();
        if memory.started {
            return;
        }
        memory.started = true;
        memory.restoring = true;
    }

    match restore_window(app, window) {
        Ok(Some(applied)) => {
            let mut memory = store.lock();
            memory.normal_inner = Some(applied.normal);
            memory.maximized = applied.maximized;
            memory.pending_maximize = applied.pending_maximize;
            memory.monitor = Some(applied.monitor);
        }
        Ok(None) => {
            log::warn!(target: "window", "启动还原跳过：系统没有报告可用显示器");
        }
        Err(error) => {
            log::warn!(target: "window", "启动窗口还原失败：{error}");
        }
    }

    store.lock().restoring = false;
}

struct Applied {
    normal: PixelRect,
    maximized: bool,
    pending_maximize: bool,
    monitor: MonitorSnapshot,
}

fn restore_window<R: Runtime>(
    app: &AppHandle<R>,
    window: &WebviewWindow<R>,
) -> Result<Option<Applied>, String> {
    let monitors = window
        .available_monitors()
        .map_err(|error| format!("枚举显示器失败：{error}"))?;
    if monitors.is_empty() {
        return Ok(None);
    }
    let snapshots: Vec<MonitorSnapshot> = monitors.iter().map(monitor_snapshot).collect();
    let primary = window
        .primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| monitor_snapshot(&monitor))
        .unwrap_or_else(|| snapshots[0].clone());
    let record = load_state(app);
    let min = window_layout::main_min_inner_size(app);
    let action = decide(record.as_ref(), &snapshots, &primary, min);
    log::info!(
        target: "window",
        "启动窗口还原：action={} record={} monitors={} primary={}",
        action.case(),
        record.is_some(),
        snapshots.len(),
        primary.name.as_deref().unwrap_or("?"),
    );

    match action {
        StartupAction::Fit(monitor) => {
            let report = window_layout::apply_inset_layout(
                window,
                monitor.work,
                WorkAreaInsets::DEFAULT,
                monitor.scale,
            )?;
            let inner: PixelRect = report.target.into();
            Ok(Some(Applied {
                normal: inner,
                maximized: false,
                pending_maximize: false,
                monitor,
            }))
        }
        StartupAction::Restore { monitor, rect } => {
            let (_actual, _) = window_layout::set_inner_rect(window, rect)?;
            window_layout::ensure_titlebar_visible(window, monitor.work)?;
            log::info!(
                target: "window",
                "按四角比例还原：target={rect:?} actual={_actual:?}"
            );
            Ok(Some(Applied {
                normal: rect,
                maximized: false,
                pending_maximize: false,
                monitor,
            }))
        }
        StartupAction::Maximize { monitor, normal } => {
            let (_actual, _) = window_layout::set_inner_rect(window, normal)?;
            window_layout::ensure_titlebar_visible(window, monitor.work)?;
            // 先不 maximize：等 reveal 前一刻再做，避免闪屏期间把隐藏主窗口提前显示。
            Ok(Some(Applied {
                normal,
                maximized: true,
                pending_maximize: true,
                monitor,
            }))
        }
    }
}

/// 纯决策：只在这里做一次「记录是否可用 + 原屏是否可识别 + 四角是否越界」。
fn decide(
    record: Option<&WindowRecord>,
    monitors: &[MonitorSnapshot],
    primary: &MonitorSnapshot,
    min: MinInnerSize,
) -> StartupAction {
    let Some(record) = record else {
        return StartupAction::Fit(primary.clone());
    };
    let Some(monitor) = identify(&record.monitor, monitors) else {
        return StartupAction::Fit(primary.clone());
    };
    let min_physical = min.physical(monitor.scale);

    if record.maximized {
        let normal = if corners_valid(&record.corners) {
            corners_to_rect(&record.corners, monitor.work, min_physical)
        } else {
            window_layout::inset_rect(monitor.work, WorkAreaInsets::DEFAULT, min_physical)
        };
        return StartupAction::Maximize {
            monitor: monitor.clone(),
            normal,
        };
    }

    if corners_valid(&record.corners) {
        StartupAction::Restore {
            monitor: monitor.clone(),
            rect: corners_to_rect(&record.corners, monitor.work, min_physical),
        }
    } else {
        StartupAction::Fit(monitor.clone())
    }
}

/// 屏幕识别优先级：PnP 路径 → GDI 名 + rect → GDI 名 → rect → 不可识别。
fn identify<'a>(
    record: &MonitorSnapshot,
    current: &'a [MonitorSnapshot],
) -> Option<&'a MonitorSnapshot> {
    if let Some(pnp) = non_empty(&record.pnp_id)
        && let Some(found) = current
            .iter()
            .find(|monitor| non_empty(&monitor.pnp_id) == Some(pnp))
    {
        return Some(found);
    }
    if let Some(name) = non_empty(&record.name) {
        if let Some(found) = current
            .iter()
            .find(|monitor| non_empty(&monitor.name) == Some(name) && monitor.rect == record.rect)
        {
            return Some(found);
        }
        if let Some(found) = current
            .iter()
            .find(|monitor| non_empty(&monitor.name) == Some(name))
        {
            return Some(found);
        }
    }
    current.iter().find(|monitor| monitor.rect == record.rect)
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|text| !text.is_empty())
}

/// 内框矩形 → 四角比例（相对工作区）。工作区为 0 时返回 `None`。
fn corners_from(inner: PixelRect, work: PixelRect) -> Option<Corners> {
    if work.width == 0 || work.height == 0 {
        return None;
    }
    let width = f64::from(work.width);
    let height = f64::from(work.height);
    Some(Corners {
        x1: (i64::from(inner.x) - i64::from(work.x)) as f64 / width,
        y1: (i64::from(inner.y) - i64::from(work.y)) as f64 / height,
        x2: (inner.right() - i64::from(work.x)) as f64 / width,
        y2: (inner.bottom() - i64::from(work.y)) as f64 / height,
    })
}

/// 四角比例是否有效：都在 0–1（含 1e-6 容差）且宽高为正。
fn corners_valid(corners: &Corners) -> bool {
    const EPS: f64 = 1e-6;
    let in_unit = |value: f64| value.is_finite() && (-EPS..=1.0 + EPS).contains(&value);
    in_unit(corners.x1)
        && in_unit(corners.y1)
        && in_unit(corners.x2)
        && in_unit(corners.y2)
        && corners.x2 > corners.x1 + EPS
        && corners.y2 > corners.y1 + EPS
}

/// 四角比例 × 工作区 → 目标内框矩形；宽高单边做最小尺寸保底。
fn corners_to_rect(corners: &Corners, work: PixelRect, min: PhysicalSize<u32>) -> PixelRect {
    let map_x = |ratio: f64| -> i64 {
        (f64::from(work.x) + ratio.clamp(0.0, 1.0) * f64::from(work.width)).round() as i64
    };
    let map_y = |ratio: f64| -> i64 {
        (f64::from(work.y) + ratio.clamp(0.0, 1.0) * f64::from(work.height)).round() as i64
    };
    let x1 = map_x(corners.x1);
    let y1 = map_y(corners.y1);
    let x2 = map_x(corners.x2);
    let y2 = map_y(corners.y2);
    let width = (x2 - x1).clamp(1, i64::from(u32::MAX)) as u32;
    let height = (y2 - y1).clamp(1, i64::from(u32::MAX)) as u32;
    PixelRect::new(
        x1.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        y1.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        width,
        height,
    )
    .with_min_size(min)
}

fn monitor_snapshot(monitor: &tauri::Monitor) -> MonitorSnapshot {
    let name = monitor.name().cloned();
    let pnp_id = name.as_deref().and_then(monitor_pnp_id);
    let work = monitor.work_area();
    MonitorSnapshot {
        name,
        pnp_id,
        rect: PixelRect::new(
            monitor.position().x,
            monitor.position().y,
            monitor.size().width,
            monitor.size().height,
        ),
        work: PixelRect::new(
            work.position.x,
            work.position.y,
            work.size.width,
            work.size.height,
        ),
        scale: monitor.scale_factor(),
    }
}

fn current_monitor<R: Runtime>(window: &WebviewWindow<R>) -> Option<MonitorSnapshot> {
    window
        .current_monitor()
        .ok()
        .flatten()
        .map(|monitor| monitor_snapshot(&monitor))
}

#[cfg(not(windows))]
fn monitor_pnp_id(_name: &str) -> Option<String> {
    None
}

#[cfg(windows)]
fn monitor_pnp_id(name: &str) -> Option<String> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Graphics::Gdi::{DISPLAY_DEVICEW, EnumDisplayDevicesW};
    use windows_sys::Win32::UI::WindowsAndMessaging::EDD_GET_DEVICE_INTERFACE_NAME;

    let device: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut display = DISPLAY_DEVICEW::default();
    display.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
    // SAFETY: `device` 是以 NUL 结尾的 UTF-16 字符串；`display` 是已初始化的合法缓冲区。
    let ok = unsafe {
        EnumDisplayDevicesW(
            device.as_ptr(),
            0,
            &mut display,
            EDD_GET_DEVICE_INTERFACE_NAME,
        )
    };
    if ok == 0 {
        return None;
    }
    let end = display.DeviceID.iter().position(|unit| *unit == 0)?;
    if end == 0 {
        return None;
    }
    let id = std::ffi::OsString::from_wide(&display.DeviceID[..end])
        .to_string_lossy()
        .into_owned();
    (!id.is_empty()).then_some(id)
}

fn save_on_close<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    if window.is_minimized().unwrap_or(false) {
        save_state(app, None);
        log::info!(target: "window", "最小化关闭：清除窗口尺寸记忆");
        return;
    }

    let store = app.state::<WindowStateStore>();
    let (cached_inner, cached_monitor, pending_maximize) = {
        let memory = store.lock();
        (
            memory.normal_inner,
            memory.monitor.clone(),
            memory.pending_maximize,
        )
    };
    let maximized = window.is_maximized().unwrap_or(false) || pending_maximize;
    let normal_inner = if maximized {
        cached_inner.or_else(|| {
            current_monitor(window).map(|monitor| {
                let min =
                    window_layout::main_min_inner_size(window.app_handle()).physical(monitor.scale);
                window_layout::inset_rect(monitor.work, WorkAreaInsets::DEFAULT, min)
            })
        })
    } else {
        window_layout::read_inner(window).ok().or(cached_inner)
    };
    let Some(normal_inner) = normal_inner else {
        log::warn!(target: "window", "关闭时读不到窗口矩形，保留原有状态文件");
        return;
    };
    let Some(monitor) = current_monitor(window).or(cached_monitor) else {
        log::warn!(target: "window", "关闭时读不到窗口所在屏幕，保留原有状态文件");
        return;
    };
    let topology = window
        .available_monitors()
        .map(|monitors| monitors.iter().map(monitor_snapshot).collect())
        .unwrap_or_else(|_| vec![monitor.clone()]);
    let Some(record) = make_record(normal_inner, &monitor, topology, maximized) else {
        log::warn!(target: "window", "窗口矩形/屏幕数据非法，保留原有状态文件");
        return;
    };
    save_state(app, Some(&record));
    log::info!(
        target: "window",
        "关闭时记录窗口：maximized={} inner={normal_inner:?} monitor={:?}",
        maximized,
        monitor.name,
    );
}

fn make_record(
    inner: PixelRect,
    monitor: &MonitorSnapshot,
    topology: Vec<MonitorSnapshot>,
    maximized: bool,
) -> Option<WindowRecord> {
    let corners = corners_from(inner, monitor.work)?;
    Some(WindowRecord {
        inner,
        corners,
        maximized,
        monitor: monitor.clone(),
        topology,
        saved_at: raybend::store::time::now_millis(),
    })
}

fn state_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(db::data_dir(app)?.join(STATE_FILE))
}

fn load_state<R: Runtime>(app: &AppHandle<R>) -> Option<WindowRecord> {
    let path = state_path(app).ok()?;
    match load_from_path(&path) {
        Ok(record) => record,
        Err(error) => {
            log::warn!(target: "window", "读取窗口状态失败：{error}");
            None
        }
    }
}

fn load_from_path(path: &Path) -> Result<Option<WindowRecord>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("读取 {} 失败：{error}", path.display())),
    };
    let state: StateFile = serde_json::from_str(&text)
        .map_err(|error| format!("解析 {} 失败：{error}", path.display()))?;
    if state.version != STATE_VERSION {
        return Err(format!(
            "未知的窗口状态版本 {}（当前 {STATE_VERSION}），按无记录处理",
            state.version
        ));
    }
    match state.main {
        Some(record) if record_is_valid(&record) => Ok(Some(record)),
        Some(_) => Err("窗口状态字段非法，按无记录处理".to_string()),
        None => Ok(None),
    }
}

fn record_is_valid(record: &WindowRecord) -> bool {
    record.inner.width > 0
        && record.inner.height > 0
        && record.monitor.rect.width > 0
        && record.monitor.rect.height > 0
        && record.monitor.work.width > 0
        && record.monitor.work.height > 0
        && record.monitor.scale.is_finite()
        && record.monitor.scale > 0.0
        && [
            record.corners.x1,
            record.corners.y1,
            record.corners.x2,
            record.corners.y2,
        ]
        .iter()
        .all(|value| value.is_finite())
}

fn save_state<R: Runtime>(app: &AppHandle<R>, record: Option<&WindowRecord>) {
    let path = match state_path(app) {
        Ok(path) => path,
        Err(error) => {
            log::warn!(target: "window", "写入窗口状态失败：{error}");
            return;
        }
    };
    match save_to_path(&path, record) {
        Ok(()) => log::info!(
            target: "window",
            "窗口状态已写入：{}（main={}）",
            path.display(),
            record.is_some()
        ),
        Err(error) => log::warn!(target: "window", "写入窗口状态失败：{error}"),
    }
}

fn save_to_path(path: &Path, record: Option<&WindowRecord>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("创建目录 {} 失败：{error}", parent.display()))?;
    }
    let state = StateFile {
        version: STATE_VERSION,
        main: record.cloned(),
    };
    let bytes = serde_json::to_vec_pretty(&state)
        .map_err(|error| format!("序列化窗口状态失败：{error}"))?;
    let tmp = path.with_file_name(format!("{STATE_FILE}.tmp"));
    std::fs::write(&tmp, bytes).map_err(|error| format!("写入 {} 失败：{error}", tmp.display()))?;
    if let Err(error) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("替换 {} 失败：{error}", path.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: MinInnerSize = MinInnerSize {
        width: 1200.0,
        height: 800.0,
    };

    fn rect(x: i32, y: i32, width: u32, height: u32) -> PixelRect {
        PixelRect::new(x, y, width, height)
    }

    fn monitor(
        name: &str,
        pnp: Option<&str>,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> MonitorSnapshot {
        MonitorSnapshot {
            name: Some(name.to_string()),
            pnp_id: pnp.map(str::to_string),
            rect: rect(x, y, width, height),
            work: rect(x, y, width, height - 40),
            scale: 1.0,
        }
    }

    fn record(corners: Corners, monitor: MonitorSnapshot) -> WindowRecord {
        WindowRecord {
            inner: rect(200, 100, 1600, 1000),
            corners,
            maximized: false,
            monitor,
            topology: vec![],
            saved_at: 0,
        }
    }

    #[test]
    fn corners_roundtrip_on_the_same_work_area() {
        let work = rect(1920, 0, 2560, 1400);
        let inner = rect(2000, 100, 1600, 1000);
        let corners = corners_from(inner, work).unwrap();
        let back = corners_to_rect(&corners, work, PhysicalSize::new(0, 0));
        assert_eq!(back, inner);
    }

    #[test]
    fn corners_map_to_a_different_work_area() {
        let work = rect(0, 0, 1000, 1000);
        let corners = Corners {
            x1: 0.2,
            y1: 0.2,
            x2: 0.8,
            y2: 0.8,
        };
        assert_eq!(
            corners_to_rect(&corners, work, PhysicalSize::new(0, 0)),
            rect(200, 200, 600, 600)
        );
    }

    #[test]
    fn corners_allow_exact_edges_and_reject_overflow() {
        let valid = Corners {
            x1: 0.0,
            y1: 0.0,
            x2: 1.0,
            y2: 1.0,
        };
        assert!(corners_valid(&valid));
        assert!(!corners_valid(&Corners { x2: 1.001, ..valid }));
        assert!(!corners_valid(&Corners {
            x1: -0.001,
            ..valid
        }));
        assert!(!corners_valid(&Corners { x1: 1.0, ..valid }));
        assert!(!corners_valid(&Corners {
            y1: f64::NAN,
            ..valid
        }));
    }

    #[test]
    fn corners_to_rect_clamps_min_per_axis() {
        let work = rect(0, 0, 1000, 1000);
        let tiny = Corners {
            x1: 0.1,
            y1: 0.1,
            x2: 0.3,
            y2: 0.3,
        };
        assert_eq!(
            corners_to_rect(&tiny, work, PhysicalSize::new(500, 300)),
            rect(100, 100, 500, 300)
        );
    }

    #[test]
    fn identify_prefers_pnp_over_name_and_geometry() {
        let recorded = monitor("DISPLAY1", Some("pnp-a"), 0, 0, 1920, 1080);
        let current = vec![
            monitor("DISPLAY2", Some("pnp-a"), 1920, 0, 2560, 1440),
            monitor("DISPLAY1", None, 0, 0, 1920, 1080),
        ];
        assert_eq!(
            identify(&recorded, &current).unwrap().name.as_deref(),
            Some("DISPLAY2")
        );
    }

    #[test]
    fn identify_falls_back_to_name_then_geometry() {
        let recorded = monitor("DISPLAY1", None, 0, 0, 1920, 1080);
        let by_name = vec![monitor("DISPLAY1", None, 1920, 0, 2560, 1440)];
        assert_eq!(identify(&recorded, &by_name).unwrap().rect.x, 1920);

        let by_rect = vec![monitor("DISPLAY9", None, 0, 0, 1920, 1080)];
        assert_eq!(
            identify(&recorded, &by_rect).unwrap().name.as_deref(),
            Some("DISPLAY9")
        );

        let none = vec![monitor("DISPLAY9", None, 1920, 0, 2560, 1440)];
        assert!(identify(&recorded, &none).is_none());
    }

    #[test]
    fn decision_without_record_fits_primary() {
        let primary = monitor("DISPLAY1", None, 0, 0, 1920, 1080);
        let action = decide(None, &[primary.clone()], &primary, MIN);
        assert_eq!(action, StartupAction::Fit(primary));
    }

    #[test]
    fn decision_restores_valid_corners() {
        let target = monitor("DISPLAY2", None, 1920, 0, 2560, 1440);
        let corners = Corners {
            x1: 0.1,
            y1: 0.1,
            x2: 0.9,
            y2: 0.9,
        };
        let record = record(corners, target.clone());
        let action = decide(Some(&record), &[target.clone()], &target, MIN);
        match action {
            StartupAction::Restore { monitor, rect } => {
                assert_eq!(monitor, target);
                assert_eq!(rect.x, 1920 + 256);
                assert_eq!(rect.width, 2560 - 512);
            }
            other => panic!("expected restore, got {other:?}"),
        }
    }

    #[test]
    fn decision_falls_back_when_corners_overflow() {
        let target = monitor("DISPLAY2", None, 1920, 0, 2560, 1440);
        let corners = Corners {
            x1: 0.1,
            y1: 0.1,
            x2: 1.2,
            y2: 0.9,
        };
        let record = record(corners, target.clone());
        assert_eq!(
            decide(Some(&record), &[target.clone()], &target, MIN),
            StartupAction::Fit(target)
        );
    }

    #[test]
    fn decision_falls_back_to_primary_when_monitor_is_gone() {
        let recorded = monitor("DISPLAY2", None, 1920, 0, 2560, 1440);
        let primary = monitor("DISPLAY1", None, 0, 0, 1920, 1080);
        let record = record(
            Corners {
                x1: 0.1,
                y1: 0.1,
                x2: 0.9,
                y2: 0.9,
            },
            recorded,
        );
        assert_eq!(
            decide(Some(&record), &[primary.clone()], &primary, MIN),
            StartupAction::Fit(primary)
        );
    }

    #[test]
    fn maximized_record_is_restored_as_maximize_on_the_same_monitor() {
        let target = monitor("DISPLAY2", None, 1920, 0, 2560, 1440);
        let mut record = record(
            Corners {
                x1: 0.1,
                y1: 0.1,
                x2: 0.9,
                y2: 0.9,
            },
            target.clone(),
        );
        record.maximized = true;
        match decide(Some(&record), &[target.clone()], &target, MIN) {
            StartupAction::Maximize { monitor, normal } => {
                assert_eq!(monitor, target);
                assert_eq!(normal.width, 2560 - 512);
            }
            other => panic!("expected maximize, got {other:?}"),
        }
    }

    #[test]
    fn maximized_record_with_missing_monitor_falls_back() {
        let recorded = monitor("DISPLAY2", None, 1920, 0, 2560, 1440);
        let primary = monitor("DISPLAY1", None, 0, 0, 1920, 1080);
        let mut record = record(
            Corners {
                x1: 0.1,
                y1: 0.1,
                x2: 0.9,
                y2: 0.9,
            },
            recorded,
        );
        record.maximized = true;
        assert_eq!(
            decide(Some(&record), &[primary.clone()], &primary, MIN),
            StartupAction::Fit(primary)
        );
    }

    #[test]
    fn state_file_roundtrips_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STATE_FILE);
        let monitor = monitor("DISPLAY1", None, 0, 0, 1920, 1080);
        let record = record(
            Corners {
                x1: 0.1,
                y1: 0.2,
                x2: 0.9,
                y2: 0.8,
            },
            monitor,
        );
        save_to_path(&path, Some(&record)).unwrap();
        assert_eq!(load_from_path(&path).unwrap(), Some(record));

        save_to_path(&path, None).unwrap();
        assert_eq!(load_from_path(&path).unwrap(), None);
    }

    #[test]
    fn unknown_version_and_bad_records_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STATE_FILE);

        std::fs::write(&path, r#"{"version":2,"main":null}"#).unwrap();
        assert!(load_from_path(&path).is_err());

        let monitor = monitor("DISPLAY1", None, 0, 0, 1920, 1080);
        let mut bad = record(
            Corners {
                x1: 0.1,
                y1: 0.1,
                x2: 0.9,
                y2: 0.9,
            },
            monitor,
        );
        bad.monitor.work.width = 0;
        let json = serde_json::to_string(&StateFile {
            version: STATE_VERSION,
            main: Some(bad),
        })
        .unwrap();
        std::fs::write(&path, json).unwrap();
        assert!(load_from_path(&path).is_err());
    }

    #[test]
    fn missing_file_is_no_record() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            load_from_path(&dir.path().join("missing.json")).unwrap(),
            None
        );
    }
}
