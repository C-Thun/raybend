//! 窗口几何：工作区留白布局（Shift+点击最大化键）与启动还原共用的放置原语。
//!
//! 触发路径：
//! * 前端命令 [`window_fit_work_area`]（Shift+点击最大化键）；
//! * 启动时 [`crate::window_state`] 的 Shift+最大化回退。
//!
//! 规格与验收：`specs/window-work-area-layout.md`、`specs/window-restore.md`。
//!
//! ## 几条容易做错的事
//!
//! 1. **尺寸基准是内容区（client），位置基准是外框（outer）** —— Tauri 的 `set_size`
//!    目标是客户区（Windows 上 tao 已补偿 undecorated 窗口的不可见边框），而
//!    `set_position` 设的是外框原点。要让四边留白对称，位置必须减掉
//!    `inner_position - outer_position` 这个边框偏移；不减的话左右会差一个
//!    不可见边框（8px 级，肉眼可辨的不对称）。
//! 2. **最大化状态下 set_size/set_position 会被系统忽略**，必须先 `unmaximize()`；
//!    而 tao 的恢复是投递到事件循环线程的异步动作 —— 用 getter（`is_maximized`）
//!    当屏障等它落地（窗口消息同队列 FIFO，getter 返回时恢复已完成），不要 sleep 轮询。
//! 3. **最小尺寸约束会钳制目标尺寸**（`tauri.conf.json` 的 `minWidth/minHeight`，逻辑像素）。
//!    2026-10-31 崔总定：算出来的目标**单边不低于最小尺寸**（高算 700 → 800；
//!    宽算 1300 → 1300 原样）。放不下最小窗口的屏幕不在支持范围，保底后超出工作区
//!    就到此为止，不缩小、不换布局、不重试。
//! 4. 启动还原的「离屏判断」只有一次，在 `crate::window_state`；本模块只负责把
//!    已经选定的矩形放上去，并做一次 titlebar 保底。

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow};

/// 位置校验容差（物理像素）：读回偏差不超过它就算到位。
const POSITION_TOLERANCE: i32 = 1;
/// 位置修正的最大轮数：第一轮量偏移失准（跨屏 DPI）与系统调整都能收敛。
/// 这是 1px 级位置修正，不是可见性/回退重试。
const POSITION_FIX_ROUNDS: usize = 2;

/// 四边留白比例（相对工作区边长的占比，0–1）。
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkAreaInsets {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl WorkAreaInsets {
    /// 产品默认：左右各 2.5%、上 2.5%、下 5%
    ///（崔总 2026-10-09 调优：首版 5/5/5/10 全部减半）。
    pub const DEFAULT: Self = Self {
        left: 0.025,
        top: 0.025,
        right: 0.025,
        bottom: 0.05,
    };

    /// 比例必须各自落在 `0.0..1.0`，且左右 / 上下之和小于 1（否则没有内容区）。
    fn validate(self) -> Result<Self, String> {
        for (name, value) in [
            ("left", self.left),
            ("top", self.top),
            ("right", self.right),
            ("bottom", self.bottom),
        ] {
            if !value.is_finite() || !(0.0..1.0).contains(&value) {
                return Err(format!("留白比例 {name} 非法：{value}（应在 0–1 之间）"));
            }
        }
        if self.left + self.right >= 1.0 || self.top + self.bottom >= 1.0 {
            return Err("留白比例非法：左右（或上下）之和必须小于 1".to_string());
        }
        Ok(self)
    }
}

/// 主窗口的最小内框尺寸（逻辑像素）。
///
/// 值来自 `tauri.conf.json` 的 `minWidth/minHeight`，不在代码里另抄一份。
/// 目标矩形是物理像素，应用前必须用目标屏的 `scale_factor` 换算。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MinInnerSize {
    pub width: f64,
    pub height: f64,
}

impl MinInnerSize {
    pub(crate) const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };

    /// 逻辑最小尺寸 → 目标屏的物理像素（单边、四舍五入）。
    pub(crate) fn physical(self, scale: f64) -> PhysicalSize<u32> {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let to_px = |value: f64| -> u32 {
            (value.max(0.0) * scale).round().min(f64::from(u32::MAX)) as u32
        };
        PhysicalSize::new(to_px(self.width), to_px(self.height))
    }
}

/// 从应用配置里读主窗口的最小内框尺寸（逻辑像素）。读不到按 0 处理。
pub(crate) fn main_min_inner_size<R: Runtime>(app: &AppHandle<R>) -> MinInnerSize {
    let config = app.config();
    match config
        .app
        .windows
        .iter()
        .find(|window| window.label == crate::MAIN_WINDOW_LABEL)
    {
        Some(window) => MinInnerSize {
            width: window.min_width.unwrap_or(0.0),
            height: window.min_height.unwrap_or(0.0),
        },
        None => MinInnerSize::ZERO,
    }
}

/// 物理像素矩形（屏幕坐标系；`x`/`y` 可为本屏上的负值，多屏时常见）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    pub(crate) const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub(crate) fn right(self) -> i64 {
        i64::from(self.x) + i64::from(self.width)
    }

    pub(crate) fn bottom(self) -> i64 {
        i64::from(self.y) + i64::from(self.height)
    }

    /// 宽高各自执行最小尺寸保底；位置不动，只向右/下补。
    pub(crate) fn with_min_size(self, min: PhysicalSize<u32>) -> Self {
        Self {
            width: self.width.max(min.width),
            height: self.height.max(min.height),
            ..self
        }
    }
}

impl From<PixelRect> for RectDto {
    fn from(value: PixelRect) -> Self {
        Self {
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
        }
    }
}

impl From<RectDto> for PixelRect {
    fn from(value: RectDto) -> Self {
        Self {
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
        }
    }
}

/// 上报给前端的矩形（物理像素）。前端只用于诊断日志，不做界面逻辑。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RectDto {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// 一次「工作区布局」的结果（物理像素）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowLayoutReport {
    /// 比例算出来的目标**内容区**矩形（已含最小尺寸保底）。
    pub target: RectDto,
    /// 读回的**实际内容区**矩形（被最小尺寸约束钳制时与 `target` 不同）。
    pub inner: RectDto,
    /// 窗口所在屏幕的工作区矩形（比例基准）。
    pub work_area: RectDto,
    /// 本次是否从最大化恢复（前端据此知道图标状态会翻转）。
    pub restored: bool,
    /// 实际内容区尺寸与目标不一致（多因系统约束，见模块头第 3 条）。
    pub size_clamped: bool,
    /// 目标尺寸被最小尺寸保底抬高了（单边，见模块头第 3 条）。
    pub min_clamped: bool,
    /// 保底后的目标大于工作区（屏幕放不下最小窗口；到此为止，不重试）。
    pub exceeds_work_area: bool,
}

/// 纯函数：工作区矩形 + 四边比例 + 最小尺寸 → 目标**内容区**矩形（物理像素）。
///
/// 留白按 `round(边长 × 比例)` 取整；宽高由减法得出，保证「左右之和 / 上下之和」
/// 与工作区边长的差恰好落在留白上（四舍五入的 1px 误差归到右下侧）。
/// 宽高下界为 1，再按**单边**抬到 `min`：最小尺寸保底只补不足的那一边，
/// 左上角不动，向右/下补；保底后可能超出工作区，这是刻意的终止状态。
pub(crate) fn inset_rect(
    work: PixelRect,
    insets: WorkAreaInsets,
    min: PhysicalSize<u32>,
) -> PixelRect {
    let round = |ratio: f64, span: u32| -> i64 { (f64::from(span) * ratio).round() as i64 };
    let left = round(insets.left, work.width);
    let top = round(insets.top, work.height);
    let right = round(insets.right, work.width);
    let bottom = round(insets.bottom, work.height);
    PixelRect {
        x: clamp_i32(i64::from(work.x) + left),
        y: clamp_i32(i64::from(work.y) + top),
        width: ((i64::from(work.width) - left - right).max(1) as u32).max(min.width),
        height: ((i64::from(work.height) - top - bottom).max(1) as u32).max(min.height),
    }
}

/// `i64` 计算结果钳回 `i32`（正常显示器上永远不会触发；防的是病态的尺寸/比例组合）。
fn clamp_i32(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// 边框偏移：内容区原点 − 外框原点（物理像素）。
///
/// Windows 上是 undecorated 窗口的不可见 resize 边框，Linux 上是系统标题栏高度；
/// 同一窗口、同一屏幕下是常数，所以量一次就够 **——但必须在恢复最大化之后量**
/// （最大化状态下这个差不是恢复态的差）。
fn border_offset<R: Runtime>(window: &WebviewWindow<R>) -> (i32, i32) {
    match (window.inner_position(), window.outer_position()) {
        (Ok(inner), Ok(outer)) => (inner.x - outer.x, inner.y - outer.y),
        // 读不到就按 0 处理：位置修正那一步会基于实测值收敛，不会把窗口摆飞
        _ => (0, 0),
    }
}

/// 读回窗口当前的内容区矩形。
pub(crate) fn read_inner<R: Runtime>(window: &WebviewWindow<R>) -> Result<PixelRect, String> {
    let position = window
        .inner_position()
        .map_err(|error| format!("读取窗口位置失败：{error}"))?;
    let size = window
        .inner_size()
        .map_err(|error| format!("读取窗口尺寸失败：{error}"))?;
    Ok(PixelRect {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    })
}

/// 内框 ↔ 外框换算需要的两组差值：原点偏移、外框比内框多出的宽高。
#[cfg(any(windows, test))]
fn frame_metrics<R: Runtime>(
    window: &WebviewWindow<R>,
) -> Result<((i32, i32), (i64, i64)), String> {
    let inner_position = window
        .inner_position()
        .map_err(|error| format!("读取窗口内框位置失败：{error}"))?;
    let outer_position = window
        .outer_position()
        .map_err(|error| format!("读取窗口外框位置失败：{error}"))?;
    let inner_size = window
        .inner_size()
        .map_err(|error| format!("读取窗口内框尺寸失败：{error}"))?;
    let outer_size = window
        .outer_size()
        .map_err(|error| format!("读取窗口外框尺寸失败：{error}"))?;
    Ok((
        (
            inner_position.x - outer_position.x,
            inner_position.y - outer_position.y,
        ),
        (
            i64::from(outer_size.width) - i64::from(inner_size.width),
            i64::from(outer_size.height) - i64::from(inner_size.height),
        ),
    ))
}

/// 目标内框 + 边框差值 → 外框矩形（`WINDOWPLACEMENT.rcNormalPosition` 的口径）。
#[cfg(any(windows, test))]
fn outer_rect_from_parts(inner: PixelRect, border: (i32, i32), frame: (i64, i64)) -> PixelRect {
    let width = (i64::from(inner.width) + frame.0.max(0)).clamp(1, i64::from(u32::MAX));
    let height = (i64::from(inner.height) + frame.1.max(0)).clamp(1, i64::from(u32::MAX));
    PixelRect::new(
        inner.x.saturating_sub(border.0),
        inner.y.saturating_sub(border.1),
        width as u32,
        height as u32,
    )
}

/// 外框矩形 + 边框差值 → 内框矩形。
#[cfg(any(windows, test))]
fn inner_rect_from_parts(outer: PixelRect, border: (i32, i32), frame: (i64, i64)) -> PixelRect {
    let width = (i64::from(outer.width) - frame.0.max(0)).clamp(1, i64::from(u32::MAX));
    let height = (i64::from(outer.height) - frame.1.max(0)).clamp(1, i64::from(u32::MAX));
    PixelRect::new(
        outer.x.saturating_add(border.0),
        outer.y.saturating_add(border.1),
        width as u32,
        height as u32,
    )
}

#[cfg(windows)]
fn native_hwnd<R: Runtime>(
    window: &WebviewWindow<R>,
) -> Result<windows_sys::Win32::Foundation::HWND, String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window
        .window_handle()
        .map_err(|error| format!("读取窗口原生句柄失败：{error}"))?
        .as_raw()
    {
        RawWindowHandle::Win32(handle) => Ok(handle.hwnd.get() as *mut core::ffi::c_void),
        _ => Err("当前窗口不是 Win32 窗口".to_string()),
    }
}

#[cfg(windows)]
fn get_window_placement<R: Runtime>(
    window: &WebviewWindow<R>,
) -> Result<windows_sys::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT, String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowPlacement, WINDOWPLACEMENT};
    let mut placement = WINDOWPLACEMENT::default();
    placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
    // SAFETY: `placement` 是已初始化且长度正确的 WINDOWPLACEMENT 缓冲区；
    // HWND 来自当前存活的 Tauri 窗口。
    let ok = unsafe { GetWindowPlacement(native_hwnd(window)?, &mut placement) };
    if ok == 0 {
        return Err("GetWindowPlacement 失败".to_string());
    }
    Ok(placement)
}

/// 读取 Windows 为「反最大化」保存的正常态矩形，并换算成内框。
///
/// 非 Windows 没有这份额外状态，返回 `None`。
pub(crate) fn read_restore_inner_rect<R: Runtime>(
    window: &WebviewWindow<R>,
) -> Result<Option<PixelRect>, String> {
    #[cfg(not(windows))]
    {
        let _ = window;
        Ok(None)
    }
    #[cfg(windows)]
    {
        let placement = get_window_placement(window)?;
        let rect = placement.rcNormalPosition;
        let outer = PixelRect::new(
            rect.left,
            rect.top,
            (rect.right - rect.left).max(0) as u32,
            (rect.bottom - rect.top).max(0) as u32,
        );
        let (border, frame) = frame_metrics(window)?;
        Ok(Some(inner_rect_from_parts(outer, border, frame)))
    }
}

/// 显式写入 Windows 的 restore 矩形，**不改变**当前 showCmd / 最大化状态。
///
/// 必须做这一步的原因：`tao` 的 `set_size` / `set_position` 最终走异步
/// `SetWindowPos`，启动后紧接着 maximize 时，Windows 可能仍保留窗口创建阶段的
/// 默认 `rcNormalPosition`。只设置当前尺寸并不能保证反最大化回到上次正常态。
pub(crate) fn set_restore_inner_rect<R: Runtime>(
    window: &WebviewWindow<R>,
    target: PixelRect,
) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = (window, target);
        Ok(())
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::RECT;
        use windows_sys::Win32::UI::WindowsAndMessaging::SetWindowPlacement;
        let (border, frame) = frame_metrics(window)?;
        let outer = outer_rect_from_parts(target, border, frame);
        let mut placement = get_window_placement(window)?;
        placement.rcNormalPosition = RECT {
            left: outer.x,
            top: outer.y,
            right: clamp_i32(outer.right()),
            bottom: clamp_i32(outer.bottom()),
        };
        // SAFETY: 仅修改有效 WINDOWPLACEMENT 的 rcNormalPosition；
        // showCmd / flags / 最小化与最大化点保持系统当前值，HWND 属于当前窗口。
        let ok = unsafe { SetWindowPlacement(native_hwnd(window)?, &placement) };
        if ok == 0 {
            return Err("SetWindowPlacement 失败".to_string());
        }
        Ok(())
    }
}

/// 把窗口内容区放到指定矩形：必要时恢复最大化、量边框偏移、设置、读回修正。
///
/// 返回（实际内容区矩形，之前是否处于最大化）。这是 Shift+最大化、启动还原、
/// 最大化前的正常态摆放共用的**唯一**放置实现。
pub(crate) fn set_inner_rect<R: Runtime>(
    window: &WebviewWindow<R>,
    target: PixelRect,
) -> Result<(PixelRect, bool), String> {
    // ① 最大化必须显式恢复；tao 的恢复在事件循环线程执行，用 getter 当屏障等它落地。
    let restored = window.is_maximized().unwrap_or(false);
    if restored {
        window
            .unmaximize()
            .map_err(|error| format!("取消最大化失败：{error}"))?;
        let _ = window.is_maximized();
    }

    // ② 量边框偏移（必须在恢复之后），③ 设尺寸与位置。
    let (offset_x, offset_y) = border_offset(window);
    window
        .set_size(PhysicalSize::new(target.width, target.height))
        .map_err(|error| format!("调整窗口尺寸失败：{error}"))?;
    window
        .set_position(PhysicalPosition::new(
            target.x.saturating_sub(offset_x),
            target.y.saturating_sub(offset_y),
        ))
        .map_err(|error| format!("移动窗口失败：{error}"))?;

    // ④ 读回校验：偏差超过 1px 就用「当前外框 + 偏差」修正 —— 修正量来自实测，
    //    所以首轮偏移量失准（跨屏 DPI 变化等）也能收敛。
    let mut inner = read_inner(window)?;
    for _ in 0..POSITION_FIX_ROUNDS {
        let dx = target.x - inner.x;
        let dy = target.y - inner.y;
        if dx.abs() <= POSITION_TOLERANCE && dy.abs() <= POSITION_TOLERANCE {
            break;
        }
        let outer = window
            .outer_position()
            .map_err(|error| format!("读取窗口外框位置失败：{error}"))?;
        window
            .set_position(PhysicalPosition::new(
                outer.x.saturating_add(dx),
                outer.y.saturating_add(dy),
            ))
            .map_err(|error| format!("修正窗口位置失败：{error}"))?;
        inner = read_inner(window)?;
    }

    Ok((inner, restored))
}

/// 目标工作区 + 比例 + 最小尺寸 → 放窗口；返回布局报告。
///
/// 手动 Shift+点击与启动回退共用这一份，保证两者永远是同一套几何。
pub(crate) fn apply_inset_layout<R: Runtime>(
    window: &WebviewWindow<R>,
    work: PixelRect,
    insets: WorkAreaInsets,
    scale: f64,
) -> Result<WindowLayoutReport, String> {
    let min = main_min_inner_size(window.app_handle()).physical(scale);
    let raw = inset_rect(work, insets, PhysicalSize::new(0, 0));
    let target = inset_rect(work, insets, min);
    let (inner, restored) = set_inner_rect(window, target)?;
    ensure_titlebar_visible(window, work)?;

    let size_clamped = inner.width != target.width || inner.height != target.height;
    let min_clamped = target.width > raw.width || target.height > raw.height;
    let exceeds_work_area = target.width > work.width || target.height > work.height;
    if min_clamped || exceeds_work_area {
        log::info!(
            target: "window",
            "工作区布局触发最小尺寸保底：目标 {}×{}，工作区 {}×{}，minClamped={} exceedsWorkArea={}",
            target.width,
            target.height,
            work.width,
            work.height,
            min_clamped,
            exceeds_work_area,
        );
    }
    if size_clamped {
        eprintln!(
            "[window-layout] 目标尺寸被系统约束钳制：目标 {}×{}，实际 {}×{}（检查 minWidth/minHeight 或屏幕可用区域）",
            target.width, target.height, inner.width, inner.height
        );
    }

    Ok(WindowLayoutReport {
        target: target.into(),
        inner: inner.into(),
        work_area: work.into(),
        restored,
        size_clamped,
        min_clamped,
        exceeds_work_area,
    })
}

/// title 区（内框顶部）是否已经不在工作区内。
///
/// 只看内框 y：title 在顶部，y 在屏内就可见；y 在屏上方或下方都要拉回该屏最高处。
pub(crate) fn titlebar_needs_move(inner_y: i32, work: PixelRect) -> bool {
    let y = i64::from(inner_y);
    let top = i64::from(work.y);
    let bottom = top + i64::from(work.height);
    y < top || y >= bottom
}

/// 三个窗口操作末尾共用的 titlebar 保底：不在画面内就把窗口拉到该屏 y 轴最高处。
///
/// 只做一次；不重新判断、不触发另一轮回退、不循环。
pub(crate) fn ensure_titlebar_visible<R: Runtime>(
    window: &WebviewWindow<R>,
    work: PixelRect,
) -> Result<(), String> {
    let inner = window
        .inner_position()
        .map_err(|error| format!("读取窗口位置失败：{error}"))?;
    if !titlebar_needs_move(inner.y, work) {
        return Ok(());
    }
    let outer = window
        .outer_position()
        .map_err(|error| format!("读取窗口外框位置失败：{error}"))?;
    let (_, offset_y) = border_offset(window);
    window
        .set_position(PhysicalPosition::new(
            outer.x,
            work.y.saturating_sub(offset_y),
        ))
        .map_err(|error| format!("titlebar 保底移动窗口失败：{error}"))?;
    Ok(())
}

/// 把窗口调整成所在屏幕工作区内的留白矩形（Shift+点击最大化键）。
///
/// 语义**幂等**：不管当前是最大化、还原、半屏 snap 还是已经在该布局，结果都是同一矩形；
/// 重复触发不产生变化、不做 toggle（崔总 2026-10-08 拍板）。
///
/// 同步命令（不是 `async`）：Tauri 把同步命令放在主线程执行，窗口调用全部同步落地，
/// 不存在「恢复还没执行完就开始摆位」的竞态；这条路径只有几次窗口 API 调用，不占帧。
///
/// # Errors
/// 取不到窗口所在屏幕、比例非法，或任一窗口 API 失败（都带上下文返回给前端记日志）。
#[tauri::command]
pub fn window_fit_work_area<R: Runtime>(
    window: WebviewWindow<R>,
    insets: Option<WorkAreaInsets>,
) -> Result<WindowLayoutReport, String> {
    let insets = insets.unwrap_or(WorkAreaInsets::DEFAULT).validate()?;

    let monitor = window
        .current_monitor()
        .map_err(|error| format!("取窗口所在屏幕失败：{error}"))?
        .ok_or_else(|| "取窗口所在屏幕失败：系统没有报告任何显示器".to_string())?;
    let work = monitor.work_area();
    let work_rect = PixelRect {
        x: work.position.x,
        y: work.position.y,
        width: work.size.width,
        height: work.size.height,
    };
    apply_inset_layout(&window, work_rect, insets, monitor.scale_factor())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: WorkAreaInsets = WorkAreaInsets::DEFAULT;
    const NO_MIN: PhysicalSize<u32> = PhysicalSize::new(0, 0);

    fn rect(x: i32, y: i32, width: u32, height: u32) -> PixelRect {
        PixelRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn default_insets_leave_two_and_a_half_and_five_on_1080p() {
        // 1920×0.025 = 48；1080×0.025 = 27；1080×0.05 = 54
        assert_eq!(
            inset_rect(rect(0, 0, 1920, 1080), DEFAULT, NO_MIN),
            rect(48, 27, 1920 - 96, 1080 - 81),
        );
    }

    #[test]
    fn negative_origin_multi_monitor_works() {
        // 左副屏常见形态：工作区原点为负
        let out = inset_rect(rect(-1920, -200, 2560, 1440), DEFAULT, NO_MIN);
        assert_eq!(out, rect(-1920 + 64, -200 + 36, 2560 - 128, 1440 - 108));
    }

    #[test]
    fn zero_insets_fill_whole_work_area() {
        let insets = WorkAreaInsets {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        };
        assert_eq!(
            inset_rect(rect(10, 20, 800, 600), insets, NO_MIN),
            rect(10, 20, 800, 600)
        );
    }

    #[test]
    fn odd_sizes_round_each_edge() {
        // 1727×0.025 = 43.175 → 43；1081×0.025 = 27.025 → 27；1081×0.05 = 54.05 → 54
        assert_eq!(
            inset_rect(rect(0, 0, 1727, 1081), DEFAULT, NO_MIN),
            rect(43, 27, 1727 - 86, 1081 - 81),
        );
    }

    #[test]
    fn tiny_work_area_clamps_to_one_pixel_content() {
        // 0.45 + 0.45 < 1（合法），但 10 像素宽两边各取整到 5 → 内容区下界 1
        let insets = WorkAreaInsets {
            left: 0.45,
            top: 0.45,
            right: 0.45,
            bottom: 0.45,
        };
        assert_eq!(
            inset_rect(rect(0, 0, 10, 10), insets, NO_MIN),
            rect(5, 5, 1, 1)
        );
    }

    #[test]
    fn min_size_floor_is_per_axis() {
        // 高算 750-57=693 → 保底 800；宽算 1400-70=1330 ≥ 1200，原样保留。
        assert_eq!(
            inset_rect(rect(0, 0, 1400, 750), DEFAULT, PhysicalSize::new(1200, 800)),
            rect(35, 19, 1330, 800),
        );
    }

    #[test]
    fn min_size_floor_does_not_shrink_larger_side() {
        let out = inset_rect(rect(0, 0, 1920, 1080), DEFAULT, PhysicalSize::new(300, 300));
        assert_eq!(out, rect(48, 27, 1824, 999));
    }

    #[test]
    fn min_size_physical_conversion_uses_monitor_scale() {
        let min = MinInnerSize {
            width: 1200.0,
            height: 800.0,
        };
        assert_eq!(min.physical(1.5), PhysicalSize::new(1800, 1200));
        assert_eq!(min.physical(1.0), PhysicalSize::new(1200, 800));
        // 病态 scale 不能把目标算成 0 或溢出
        assert_eq!(min.physical(0.0), PhysicalSize::new(1200, 800));
        assert_eq!(min.physical(f64::NAN), PhysicalSize::new(1200, 800));
    }

    #[test]
    fn restore_rect_clamps_width_and_height_independently() {
        let actual = rect(100, 200, 763, 1200).with_min_size(PhysicalSize::new(1500, 1000));
        assert_eq!(actual, rect(100, 200, 1500, 1200));
    }

    #[test]
    fn restore_inner_outer_conversion_roundtrips() {
        let inner = rect(3378, 160, 1482, 990);
        // 真机 undecorated 窗口的边框差值量级；外框比内框略大。
        let outer = outer_rect_from_parts(inner, (7, 7), (14, 14));
        assert_eq!(outer, rect(3371, 153, 1496, 1004));
        assert_eq!(inner_rect_from_parts(outer, (7, 7), (14, 14)), inner);

        // 多屏负坐标与极小异常矩形也不产生 0 尺寸。
        let negative = rect(-1900, -1200, 1, 1);
        let outer = outer_rect_from_parts(negative, (8, 8), (16, 16));
        assert_eq!(inner_rect_from_parts(outer, (8, 8), (16, 16)), negative);
        let degenerate = inner_rect_from_parts(rect(0, 0, 4, 4), (0, 0), (16, 16));
        assert_eq!((degenerate.width, degenerate.height), (1, 1));
    }

    #[test]
    fn validate_rejects_out_of_range_and_sum_one() {
        let base = WorkAreaInsets {
            left: 0.1,
            top: 0.1,
            right: 0.1,
            bottom: 0.1,
        };
        assert!(base.validate().is_ok());
        assert!(WorkAreaInsets { left: -0.1, ..base }.validate().is_err());
        assert!(WorkAreaInsets { left: 1.0, ..base }.validate().is_err());
        assert!(
            WorkAreaInsets {
                left: f64::NAN,
                ..base
            }
            .validate()
            .is_err()
        );
        assert!(
            WorkAreaInsets {
                left: 0.5,
                right: 0.5,
                ..base
            }
            .validate()
            .is_err(),
            "左右之和 = 1 会算出 0 宽度，必须拒绝"
        );
        assert!(
            WorkAreaInsets {
                top: 0.9,
                bottom: 0.2,
                ..base
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn titlebar_check_only_looks_at_vertical_visibility() {
        let work = rect(0, 0, 1920, 1080);
        assert!(!titlebar_needs_move(0, work));
        assert!(!titlebar_needs_move(539, work));
        assert!(titlebar_needs_move(-1, work));
        assert!(titlebar_needs_move(1080, work));

        // 副屏在上方：工作区 y 为负
        let above = rect(0, -1440, 2560, 1400);
        assert!(!titlebar_needs_move(-1440, above));
        assert!(titlebar_needs_move(-1441, above));
        assert!(titlebar_needs_move(-40, above), "工作区下界是 -40");
    }
}
