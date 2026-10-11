//! 诊断目录布局（`specs/logging-and-crash-report.md` §3）。
//!
//! ```text
//! <应用数据目录>/
//! ├── logs/                     第一层：运行日志（常驻 · 轮转 · 容量硬上限）
//! ├── crashes/RB-<yymmdd>-<hex>/ 第二层：异常采集（panic / 硬崩溃 / 未正常退出）
//! └── reports/                   第三层：报告包（W3，本次不建）
//! ```
//!
//! 根目录由 `src-tauri` 用 Tauri path API 解析后注入（本 crate 不依赖 tauri）；
//! 这里只负责在它下面拼子目录。

use std::path::{Path, PathBuf};

/// 运行日志目录名。
pub const LOGS_DIR: &str = "logs";
/// 异常采集目录名。
pub const CRASHES_DIR: &str = "crashes";
/// 报告包目录名（第三层，W3 才用）。
pub const REPORTS_DIR: &str = "reports";

/// 日志目录。
#[must_use]
pub fn logs_dir(root: &Path) -> PathBuf {
    root.join(LOGS_DIR)
}

/// 异常采集目录。
#[must_use]
pub fn crashes_dir(root: &Path) -> PathBuf {
    root.join(CRASHES_DIR)
}

/// 报告包目录。
#[must_use]
pub fn reports_dir(root: &Path) -> PathBuf {
    root.join(REPORTS_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subdirectories_sit_under_the_injected_root() {
        let root = Path::new("X:\\data");
        assert!(logs_dir(root).ends_with("logs"));
        assert!(crashes_dir(root).ends_with("crashes"));
        assert!(reports_dir(root).ends_with("reports"));
        assert_eq!(logs_dir(root), root.join("logs"));
    }
}
