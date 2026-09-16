//! RAW 解码（M2-W1 落地）。
//!
//! 设计约束（见 `AGENTS.md` §6.3）：
//!
//! 1. **后端可插拔**：本模块对外只暴露自己的类型，禁止其它模块直接依赖 rawler
//!    （其 API 不稳定且不遵循 SemVer，未来还可能换成 libraw / rawspeed / zenraw，
//!    候选评估见 `FUTURE.md` §B）；
//! 2. **进程隔离**：解码必须在独立 worker 进程中执行 —— 上游明确声明
//!    「不要把 dnglab/rawler 用于处理不可信文件」，崩溃不得带走主进程；
//! 3. **快路径优先**：RAW 内嵌的 JPEG 预览读取比完整解码快 1–2 个数量级，
//!    浏览与缩略图先走这条路径，完整解码留给后台任务。
//!
//! # 分层
//!
//! | 模块 | 职责 |
//! | --- | --- |
//! | [`backend`] | 公共类型（[`RawImage8`] / [`DecodeRequest`]）与 [`RawBackend`] trait |
//! | [`precheck`] | 进 worker 之前的**廉价预检**：大小 + 文件头魔数 |
//! | [`rawler_backend`] | 唯一实现，内部调用 rawler（**只在 worker 进程里跑**） |
//! | [`worker`] | 常驻子进程客户端 + worker 端主循环（长度前缀协议） |
//!
//! # 谁在用哪条路径
//!
//! * 网格 / 胶片带缩略图 → `max_edge` 有值、`allow_preview = true` → 内嵌预览（毫秒级）
//! * 看图的 `SCREEN` 档 → `allow_preview = true`（内嵌预览够大就用它）
//! * `FULL`（1:1）与将来的显影 → `allow_preview = false` → 完整解码（黑电平 / 白平衡 /
//!   色彩矩阵 / sRGB 伽马，走 rawler 的 `RawDevelop`）
//!
//! 像素格式统一是 **8 位 RGB、行优先紧密排列**（`rgb.len() == width * height * 3`）——
//! 上层（缩略图管线）拿到就能裁切 / 缩放 / 编码，不需要知道 RAW 的位深与 CFA。

pub mod backend;
pub mod precheck;
pub mod rawler_backend;
pub mod worker;

pub use backend::{DecodeRequest, PixelSource, RawBackend, RawError, RawImage8, RawResult};
pub use precheck::{Container, Precheck, precheck};
pub use rawler_backend::RawlerBackend;
pub use worker::{RawWorker, WorkerError};

/// 本项目**内置**的后端名（日志与诊断用）。
pub const DEFAULT_BACKEND: &str = "rawler";
