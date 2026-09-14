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
//! **本模块目前只有说明** —— M0-1 阶段不引入 `wgpu` 依赖；
//! M0-2 的渲染可行性验证会在此落地（调试窗口开在 `src-tauri/src/spike_viewport.rs`）。
