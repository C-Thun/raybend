//! 媒体文件与元数据（M1 落地）。
//!
//! 模块划分：
//!
//! * [`kind`]：文件名解析与类型判定（扩展名、侧车、垃圾文件、配对用的 stem）
//! * [`scan`]：目录扫描（可取消、顺序确定、跳过规则）
//! * [`diff`]：变更追踪（磁盘快照 vs 库记录 → 变更计划；纯函数、绝不删记录）
//! * [`exif`]：EXIF 抽取与拍摄时间来源判定（EXIF → 文件名 → 文件修改时间）
//! * [`source`]：来源目录的列取（目录树的一层 / 中列的照片清单 / 并行补拍时间）
//!
//!
//! 文件身份 `(volume_serial, file_id128)` 已在 `store::file_id` 落地（Windows 用
//! `GetFileInformationByHandleEx(FileIdInfo)`，可跟踪重命名/移动）；路径的三种表示
//! 在 `store::path_semantics`。相关真相与约束见 `AGENTS.md` §7.3。

pub mod diff;
pub mod exif;
pub mod kind;
pub mod meta;
/// 最小 TIFF/RAW 头解析（RW2 这类魔数不是 0x2A 的也要能读）
pub mod tiff;
pub mod meta_cache;
pub mod scan;
pub mod source;

pub use kind::{JunkKind, MediaKind, extension, junk_kind, kind_of_file, stem_folded};
