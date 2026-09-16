//! 缩略图与预览（M1-3 落地）。
//!
//! 模块划分：
//!
//! * [`render`]：解码 → 按 EXIF 朝向摆正 → 缩放 → 编码 JPEG；
//!   RAW 先走 [`crate::raw`]（内嵌预览优先，worker 进程隔离），解不开才用占位图
//! * [`cache`]：`<app data>/cache/<repository_id>/thumbs.db`（独立小库，派生数据）
//! * [`worker`]：消费 `app.db` 的 `jobs` 队列，支持取消与崩溃续跑
//!
//! # 尺度
//!
//! 第一阶段只有两个：`GRID`（长边 384，浏览网格）与 `STRIP`（长边 192，胶片带）。
//! `SCREEN`（视口分辨率）与 `FULL`（1:1）留到后面的里程碑。
//!
//! # 缓存键与失效
//!
//! 键 = `(cache_key, size_class, render_sig)`：
//!
//! * `cache_key` 优先用**文件身份**（`volume_serial` + `file_id`）—— 改名、移动后仍然命中；
//!   读不到身份才退回折叠路径。**没入库的源文件也能用同一套缓存**（导入工作区要用）。
//! * `render_sig` 含管线版本 —— 算法升级后旧缓存自动成孤儿并被 GC 收走，
//!   **不需要写数据迁移**。
//!
//! # 红线（`AGENTS.md` §6.5）
//!
//! **删掉整个缓存目录后，功能降级但完全可用。** 缓存库里没有任何信息是不可再生的。

pub mod cache;
pub mod render;
pub mod worker;

pub use cache::{CacheStats, GcOutcome, ThumbsDb, cache_key};
pub use render::{SizeClass, Thumb, render_file, render_sig};
pub use worker::{
    Claimed, Outcome, QueueStats, RunStats, ThumbJob, cache_key_for, render_now,
};
