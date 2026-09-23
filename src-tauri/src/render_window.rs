//! **窗口 ↔ wgpu 的最小胶水**（spike 与编辑视口共用这一份）。
//!
//! `crates/raybend` 里的渲染模块不依赖 Tauri（`AGENTS.md` §4），它只接受一对
//! `raw-window-handle` 裸句柄。从 Tauri 窗口上把这对句柄取下来是 `src-tauri` 的事 ——
//! 而这件事**只该有一份实现**（spike 调试窗口与产品主窗口都要它）。
//!
//! # 安全契约（写在这里，因为两个调用方都得遵守）
//!
//! `GpuContext` 用 `create_surface_unsafe` 拿到 surface，**它比窗口对象活得久**。
//! 所以调用方必须保证：**窗口先建、后建上下文；关窗时先丢上下文、再丢窗口**。
//! 产品路径（主窗口）由 Tauri 自己保证（主窗口与进程同生共死）；spike 窗口由
//! `spike_viewport.rs` 的会话生命周期保证。

use raybend::render::RawHandles;
use tauri::Runtime;

/// 从窗口上取一对裸句柄（rwh 0.6）。
///
/// # Errors
/// 窗口还没落地（没有原生句柄）时返回错误文本 —— 调用方应当把它报给界面，而不是自己猜。
pub fn raw_handles<R: Runtime>(window: &tauri::WebviewWindow<R>) -> Result<RawHandles, String> {
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

/// 窗口**客户区**的物理像素尺寸 —— surface 该配多大就是它。
///
/// ⚠️ **不要用 DOM 的 `innerWidth × devicePixelRatio` 代替**：`innerWidth` 是取整过的 CSS 值，
/// 乘回去最多能差 1 像素（swapchain 比窗口小 1px → 最外一圈被拉伸）。
/// DOM 那份 DPR 是**洞口与指针换算**的口径，两者用途不同（`AGENTS.md` §7.9 的四种量）。
#[must_use]
pub fn client_size<R: Runtime>(window: &tauri::WebviewWindow<R>) -> (u32, u32) {
    window
        .inner_size()
        .map(|size| (size.width.max(1), size.height.max(1)))
        .unwrap_or((1280, 820))
}
