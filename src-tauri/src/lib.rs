//! raybend（光伴）桌面外壳。
//!
//! 这一层**刻意保持很薄**：只做「窗口 + WebView + 命令转发」。
//! 业务逻辑全部在 `raybend` crate 内，且该 crate **不依赖 Tauri**
//! —— 这是为 Tauri 3.0 的运行时重构预留的迁移空间（见 `AGENTS.md` §6.2）。
//!
//! M0-2 的渲染可行性验证会在这里另开一个 `spike-viewport` 调试窗口，
//! 主窗口保持不透明、不受影响。

/// 主窗口标签（与 `tauri.conf.json` 的窗口配置、`capabilities/default.json` 对应）。
pub const MAIN_WINDOW_LABEL: &str = "main";

/// 启动应用。
///
/// # Panics
///
/// Tauri 运行时初始化失败时 panic —— 此时进程已无法提供任何功能。
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
