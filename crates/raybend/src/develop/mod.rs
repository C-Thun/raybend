//! **显影（develop）**：M3 编辑的核心 —— 参数模型、色彩数学、曲线与管线。
//!
//! ```text
//! develop/
//! ├── params.rs    参数表（与 src/api/develop-params.json 逐条对齐）+ DevelopParams
//! ├── color.rs     sRGB 传递函数、色温 ↔ 色度、白平衡增益（唯一一份色彩数学）
//! ├── curve.rs     单调三次曲线（RGB / R / G / B）
//! └── pipeline.rs  线性像素 → 参数 → 8bit 显示（参考实现 + LUT 快路径）
//! ```
//!
//! # 与邻居的分工
//!
//! | 谁 | 干什么 |
//! | --- | --- |
//! | [`crate::raw`] | 把 RAW 解成**线性 sRGB u16**（`RawDevelop` 去掉 `SRgb` 步） |
//! | `develop`（本模块） | 参数与数学：怎么把线性像素按参数变成显示像素 |
//! | [`crate::display`] / [`crate::thumbnail`] | 取图与缓存（把本模块的输出编码成 JPEG 或交给 GPU） |
//! | `src-tauri` | IPC 边界与渲染线程（参数一变就重跑本模块） |
//!
//! 本模块**不依赖** `render`（wgpu）与 `store`（SQLite）：它是纯数学，
//! 这样才好在离屏例子、单测与「缩略图那条 CPU 路」上复用同一份实现。
//!
//! # 只允许一套实现（`AGENTS.md` §2.12）
//!
//! 管线数学只有这里一份；`render::RenderImage` 是纹理用的容器、`thumbnail` 是编码与缓存，
//! 它们都不许自己再算一遍色调。GPU 化（W4+）时也是**同一组测试向量喂两条路**做交叉验证，
//! 不是另写一份。

pub mod color;
pub mod curve;
pub mod params;
pub mod pipeline;

pub use color::{linear_to_srgb, srgb_to_linear, temperature_gain_ratio};
pub use curve::{Curve, CurveChannel, CurveSet};
pub use params::{Baseline, DevelopParams, Origin, ParamSpec, PARAMS, spec};
pub use pipeline::{
    LinearImage, apply_chroma, blacks_curve, chain_linear, contrast_curve, encode_and_curve,
    highlights_curve, map_pixel_exact, render_rgb8,
};
