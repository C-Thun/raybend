//! wgpu 渲染与视口变换（M0-2 落地）。
//!
//! 本模块**独占**视口状态与着色器 —— 这是 `AGENTS.md` §6.1 定下的三条红线：
//!
//! 1. 视口状态由 Rust 独有（`Viewport { zoom, pan_px, rotation, fit_mode, clip_rect, dpr, surface_format }`），
//!    前端只发送交互意图，不做坐标数学；
//! 2. 覆盖层（蒙版、裁剪柄、直方图采样框）复用 Rust 提供的**同一变换矩阵**，
//!    禁止前端自行推导像素对齐；
//! 3. WGSL 源码与 uniform 结构体属于本模块，前端不接触像素格式与色彩空间。
//!
//! # 模块内容（M2-W1 spike）
//!
//! - [`viewport`]：视口状态与坐标变换（图像/物理/CSS 三套坐标，一处收口；纯数学 + 测试）
//! - [`image`]：渲染用的图像（`RenderImage`：真实照片与合成测试图**同一个类型**）
//! - [`color`]：sRGB 8bit → 清屏色（洞口底色的唯一换算处）
//! - [`tier`]：取图档位的迟滞（预览档 / 全尺寸档，编辑器视口用）
//! - [`supervisor`]：长期 GPU worker 的重启策略（panic / 设备丢失都走它）
//! - [`presentation`]：平台呈现的最小适配器（Windows DX12 + DirectComposition）
//! - [`gpu`]：wgpu 上下文（surface 挂窗口、resize、设备丢失与恢复、后端探测）
//! - [`scene`]：spike 用的合成测试图（含 1px 棋盘格与方位标记，用来目视判画质）
//! - [`stats`]：帧时间统计与 spike 报告（自记录，供 `pnpm spike:win` 落盘）
//!
//! 调试窗口开在 `src-tauri/src/spike_viewport.rs`（`label = "spike-viewport"`），
//! **不动主窗口** —— 即使透明挖洞彻底失败也不影响别的部分（`PLAN.md` A.2 的窗口策略）。
//! 主窗口里的编辑视口是 `src-tauri/src/editor.rs`（M3-W2），**复用**同一个 [`gpu::GpuContext`]。

pub mod color;
pub mod gpu;
pub mod image;
pub mod overlay;
pub mod presentation;
pub mod scene;
pub mod stats;
pub mod supervisor;
pub mod tier;
pub mod viewport;
pub mod wavelet;

pub use color::Srgb8;
pub use gpu::{
    GpuContext, GpuError, OffscreenRenderer, RawHandles, RenderOutcome, SurfaceComposition,
    SurfaceDetails,
};
pub use image::RenderImage;
pub use presentation::PresentationAdapter;
pub use stats::{FrameStats, SpikeReport};
pub use supervisor::{RestartPolicy, Verdict};
pub use tier::{ImageTier, tier_for, tier_for_params};
pub use viewport::{AlphaMode, ClipRect, FitMode, Viewport};
