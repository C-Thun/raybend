//! 媒体文件与元数据（M1 落地）。
//!
//! 模块划分：
//!
//! * [`kind`]：文件名解析与类型判定（扩展名、侧车、垃圾文件、配对用的 stem）
//! * [`scan`]：目录扫描（可取消、顺序确定、跳过规则）
//! * [`diff`]：变更追踪（磁盘快照 vs 库记录 → 变更计划；纯函数、绝不删记录）
//!
//! 计划承载（后续波次）：EXIF 抽取（`exif`）。
//!
//! 文件身份 `(volume_serial, file_id128)` 已在 `store::file_id` 落地（Windows 用
//! `GetFileInformationByHandleEx(FileIdInfo)`，可跟踪重命名/移动）；路径的三种表示
//! 在 `store::path_semantics`。相关真相与约束见 `AGENTS.md` §7.3。

pub mod diff;
pub mod kind;
pub mod scan;

pub use kind::{JunkKind, MediaKind, extension, junk_kind, kind_of_file, stem_folded};
