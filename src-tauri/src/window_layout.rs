//! 窗口「工作区布局」：把窗口调整成所在屏幕工作区内的留白矩形。
//!
//! 触发路径：titlebar 最大化键的 **Shift+点击** → 前端命令 [`window_fit_work_area`]。
//! 规格与验收：`specs/window-work-area-layout.md`（决策记录：`todos/2026-10-08-window-work-area-layout.md`）。
//!
//! ## 三条容易做错的事
//!
//! 1. **尺寸基准是内容区（client），位置基准是外框（outer）** —— Tauri 的 `set_size`
//!    目标是客户区（Windows 上 tao 已补偿 undecorated 窗口的不可见边框），而
//!    `set_position` 设的是外框原点。要让四边留白对称，位置必须减掉
//!    `inner_position - outer_position` 这个边框偏移；不减的话左右会差一个
//!    不可见边框（8px 级，肉眼可辨的不对称）。
//! 2. **最大化状态下 set_size/set_position 会被系统忽略**，必须先 `unmaximize()`；
//!    而 tao 的恢复是投递到事件循环线程的异步动作 —— 用 getter（`is_maximized`）
//!    当屏障等它落地（窗口消息同队列 FIFO，getter 返回时恢复已完成），不要 sleep 轮询。
//! 3. **最小尺寸约束会钳制目标尺寸**（`tauri.conf.json` 的 `minWidth/minHeight`），
//!    所以最后要**读回实际值**再返回，不假装成功。

use serde::{Deserialize, Serialize};
use tauri::{PhysicalPosition, PhysicalSize, Runtime, WebviewWindow};

/// 位置校验容差（物理像素）：读回偏差不超过它就算到位。
const POSITION_TOLERANCE: i32 = 1;
/// 位置修正的最大轮数：第一轮量偏移失准（跨屏 DPI）与系统调整都能收敛。
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

/// 物理像素矩形（屏幕坐标系；`x`/`y` 可为本屏上的负值，多屏时常见）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PixelRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
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
    /// 比例算出来的目标**内容区**矩形。
    pub target: RectDto,
    /// 读回的**实际内容区**矩形（被最小尺寸约束钳制时与 `target` 不同）。
    pub inner: RectDto,
    /// 窗口所在屏幕的工作区矩形（比例基准）。
    pub work_area: RectDto,
    /// 本次是否从最大化恢复（前端据此知道图标状态会翻转）。
    pub restored: bool,
    /// 实际内容区尺寸与目标不一致（多因 `minWidth`/`minHeight` 约束，见模块头第 3 条）。
    pub size_clamped: bool,
}

/// 纯函数：工作区矩形 + 四边比例 → 目标**内容区**矩形（物理像素）。
///
/// 留白按 `round(边长 × 比例)` 取整；宽高由减法得出，保证「左右之和 / 上下之和」
/// 与工作区边长的差恰好落在留白上（四舍五入的 1px 误差归到右下侧）。
/// 宽高下界为 1：极小工作区上宁可给 1px 也不给 0（0 尺寸窗口没有意义）。
fn inset_rect(work: PixelRect, insets: WorkAreaInsets) -> PixelRect {
    let round = |ratio: f64, span: u32| -> i64 { (f64::from(span) * ratio).round() as i64 };
    let left = round(insets.left, work.width);
    let top = round(insets.top, work.height);
    let right = round(insets.right, work.width);
    let bottom = round(insets.bottom, work.height);
    PixelRect {
        x: clamp_i32(i64::from(work.x) + left),
        y: clamp_i32(i64::from(work.y) + top),
        width: (i64::from(work.width) - left - right).max(1) as u32,
        height: (i64::from(work.height) - top - bottom).max(1) as u32,
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
fn read_inner<R: Runtime>(window: &WebviewWindow<R>) -> Result<PixelRect, String> {
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
    let target = inset_rect(work_rect, insets);

    // ① 最大化必须显式恢复；tao 的恢复在事件循环线程执行，用 getter 当屏障等它落地。
    let restored = window.is_maximized().unwrap_or(false);
    if restored {
        window
            .unmaximize()
            .map_err(|error| format!("取消最大化失败：{error}"))?;
        let _ = window.is_maximized();
    }

    // ② 量边框偏移（必须在恢复之后），③ 设尺寸与位置。
    let (offset_x, offset_y) = border_offset(&window);
    window
        .set_size(PhysicalSize::new(target.width, target.height))
        .map_err(|error| format!("调整窗口尺寸失败：{error}"))?;
    window
        .set_position(PhysicalPosition::new(
            target.x - offset_x,
            target.y - offset_y,
        ))
        .map_err(|error| format!("移动窗口失败：{error}"))?;

    // ④ 读回校验：偏差超过 1px 就用「当前外框 + 偏差」修正 —— 修正量来自实测，
    //    所以首轮偏移量失准（跨屏 DPI 变化等）也能收敛。
    let mut inner = read_inner(&window)?;
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
            .set_position(PhysicalPosition::new(outer.x + dx, outer.y + dy))
            .map_err(|error| format!("修正窗口位置失败：{error}"))?;
        inner = read_inner(&window)?;
    }

    let size_clamped = inner.width != target.width || inner.height != target.height;
    if size_clamped {
        eprintln!(
            "[window-layout] 目标尺寸被系统约束钳制：目标 {}×{}，实际 {}×{}（检查 minWidth/minHeight 或屏幕可用区域）",
            target.width, target.height, inner.width, inner.height
        );
    }

    Ok(WindowLayoutReport {
        target: target.into(),
        inner: inner.into(),
        work_area: work_rect.into(),
        restored,
        size_clamped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: WorkAreaInsets = WorkAreaInsets::DEFAULT;

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
            inset_rect(rect(0, 0, 1920, 1080), DEFAULT),
            rect(48, 27, 1920 - 96, 1080 - 81),
        );
    }

    #[test]
    fn negative_origin_multi_monitor_works() {
        // 左副屏常见形态：工作区原点为负
        let out = inset_rect(rect(-1920, -200, 2560, 1440), DEFAULT);
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
            inset_rect(rect(10, 20, 800, 600), insets),
            rect(10, 20, 800, 600)
        );
    }

    #[test]
    fn odd_sizes_round_each_edge() {
        // 1727×0.025 = 43.175 → 43；1081×0.025 = 27.025 → 27；1081×0.05 = 54.05 → 54
        assert_eq!(
            inset_rect(rect(0, 0, 1727, 1081), DEFAULT),
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
        assert_eq!(inset_rect(rect(0, 0, 10, 10), insets), rect(5, 5, 1, 1));
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
        assert!(WorkAreaInsets { left: f64::NAN, ..base }.validate().is_err());
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
}
