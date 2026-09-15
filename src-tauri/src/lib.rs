//! raybend（光伴）桌面外壳。
//!
//! 这一层**刻意保持很薄**：只做「窗口 + WebView + 命令转发」。
//! 业务逻辑全部在 `raybend` crate 内，且该 crate **不依赖 Tauri**
//! —— 这是为 Tauri 3.0 的运行时重构预留的迁移空间（见 `AGENTS.md` §6.2）。
//!
//! M0-2 的渲染可行性验证会在这里另开一个 `spike-viewport` 调试窗口，
//! 主窗口保持不透明、不受影响。

pub mod db;
pub mod repo;
pub mod source;
pub mod thumbs;

/// 主窗口标签（与 `tauri.conf.json` 的窗口配置、`capabilities/default.json` 对应）。
pub const MAIN_WINDOW_LABEL: &str = "main";

/// 启动应用。
///
/// # Panics
///
/// Tauri 运行时初始化失败时 panic —— 此时进程已无法提供任何功能。
pub fn run() {
    tauri::Builder::default()
        .manage(db::DbState::default())
        .manage(thumbs::SourcesThumbs::default())
        .invoke_handler(tauri::generate_handler![
            // ── 数据底座的诊断与设置（M1-2）──
            db::app_paths,
            db::db_status,
            db::setting_get,
            db::setting_set,
            // ── 来源：最近目录 / 驱动器 / 目录树 / 照片清单 ──
            source::recent_dirs_list,
            source::recent_dir_remember,
            source::recent_dir_forget,
            source::volumes_list,
            source::dir_list,
            source::source_scan,
            source::source_count,
            source::source_times,
            source::file_exif,
            // ── 缩略图（未入库的源文件也要能出图）──
            thumbs::thumb_get,
            thumbs::thumb_sources_stats,
            // ── 库（相片仓）──
            repo::repositories_list,
            repo::repository_probe,
            repo::repository_create,
            repo::repository_remount,
            repo::repository_counts,
        ])
        .setup(|app| {
            use tauri::Manager;
            // 先把数据底座打开（命令也可以懒打开，这里做一次是为了启动日志能立刻反映问题）。
            // **失败不阻止启动**：窗口该出来还是要出来，错误让前端在需要时再报。
            let state = app.state::<db::DbState>();
            db::warm_up(app.handle(), &state);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
