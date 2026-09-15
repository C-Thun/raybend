//! raybend（光伴）核心库。
//!
//! 本 crate 承载全部业务逻辑，**不依赖 Tauri**。这样做的目的是：
//!
//! 1. 为 Tauri 3.0 的运行时重构（`wry`/`cef` feature flag 移除、运行时 crate 化）
//!    留出迁移空间 —— 见 `AGENTS.md` §6.2；
//! 2. 未来可复用同一套代码做 CLI / 无头模式（见 `FUTURE.md` §G7）。
//!
//! 模块划分对应 M0 的各验证波次：
//!
//! - [`media`]：媒体文件与元数据
//! - [`index`]：目录索引与本地数据库（M0-4）
//! - [`raw`]：RAW 解码，后端可插拔（M0-3）
//! - [`thumbnail`]：缩略图与预览（M0-5）
//! - [`render`]：wgpu 渲染与视口变换（M0-2）
//! - [`store`]：数据底座（SQLite 打开/迁移/备份/写并发，M1-2）

pub mod index;
pub mod media;
pub mod raw;
pub mod render;
pub mod store;
pub mod thumbnail;

mod error;

pub use error::{Error, Result};

/// 核心库版本（编译期的 crate 版本）。
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
