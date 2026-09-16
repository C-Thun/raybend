//! raybend（光伴）桌面外壳。
//!
//! 这一层**刻意保持很薄**：只做「窗口 + WebView + 命令转发」。
//! 业务逻辑全部在 `raybend` crate 内，且该 crate **不依赖 Tauri**
//! —— 这是为 Tauri 3.0 的运行时重构预留的迁移空间（见 `AGENTS.md` §6.2）。
//!
//! M0-2 的渲染可行性验证会在这里另开一个 `spike-viewport` 调试窗口，
//! 主窗口保持不透明、不受影响。

pub mod db;
mod import;
pub mod repo;
pub mod source;
pub mod thumbs;

/// IPC 契约测试（拉把 Rust 序列化出的键名与前端 ts 镜像对齐）。
#[cfg(test)]
mod contract;

/// 主窗口标签（与 `tauri.conf.json` 的窗口配置、`capabilities/default.json` 对应）。
pub const MAIN_WINDOW_LABEL: &str = "main";

/// 启动应用。
///
/// # Panics
///
/// Tauri 运行时初始化失败时 panic —— 此时进程已无法提供任何功能。
pub fn run() {
    tauri::Builder::default()
        // 目录选择器（建库弹窗的「浏览…」）。官方插件：Windows 走原生对话框。
        .plugin(tauri_plugin_dialog::init())
        .manage(db::DbState::default())
        .manage(thumbs::SourcesThumbs::default())
        // 浏览过的目录的元信息缓存（会话级内存，不落盘 —— 见 plans/photo-meta-and-tile-display.md）
        .manage(source::SourcesMetaCache::default())
        // ⚠️ **导入批次表必须注册**：漏了它，`app.state::<ImportBatches>()` 一调用就 panic
        // （`state() called before manage()`），而且**编译期不报**。真机踩过：
        // 导入点下去没反应、弹窗空着、连「取消导入」都卡住（命令 panic 后 JS 的 promise
        // 永远不 settle）。下面 `every_state_type_is_managed` 那个测试就是用来防复发的。
        .manage(import::ImportBatches::default())
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
            source::source_paths_status,
            source::dir_meta_ensure,
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
            repo::repository_settings,
            repo::repository_set_template,
            repo::repository_template_preview,
            import::import_precheck,
            import::import_start,
            import::import_pause,
            import::import_resume,
            import::import_cancel,
            import::import_status,
            import::import_errors_export,
            import::import_interrupted,
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    /// 本 crate 里所有可能提到状态类型的源文件（新增文件时补进来）。
    const SOURCES: &[(&str, &str)] = &[
        ("lib.rs", include_str!("lib.rs")),
        ("db.rs", include_str!("db.rs")),
        ("import.rs", include_str!("import.rs")),
        ("repo.rs", include_str!("repo.rs")),
        ("source.rs", include_str!("source.rs")),
        ("thumbs.rs", include_str!("thumbs.rs")),
    ];

    /// 把 `...::Foo::default()` / `...::Foo` 收敛成**类型名** `Foo`。
    ///
    /// 关键是要把方法调用那一段（`default()`）丢掉 —— 它长得像类型但其实是调用。
    fn type_name(path: &str) -> String {
        let parts: Vec<&str> = path
            .split("::")
            .map(str::trim)
            .filter(|part| !part.is_empty() && !part.contains('('))
            .collect();
        parts.last().copied().unwrap_or_default().to_string()
    }

    /// 取出 `prefix` 之后**括号配平**的那一段（够用即可，不做完整解析）。
    ///
    /// 必须配平：`.manage(db::DbState::default())` 里第一个 `)` 是 `default()` 的，
    /// 按「找第一个右括号」切会把类型名切成 `db::DbState::default(`（第一版就是这么错的）。
    fn balanced_after<'a>(text: &'a str, prefix: &str, open: char, close: char) -> Vec<&'a str> {
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(index) = rest.find(prefix) {
            let tail = &rest[index + prefix.len()..];
            let mut depth = 1usize; // prefix 自带一个 open
            let mut end = None;
            for (offset, ch) in tail.char_indices() {
                if ch == open {
                    depth += 1;
                } else if ch == close {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(offset);
                        break;
                    }
                }
            }
            match end {
                Some(offset) => {
                    out.push(&tail[..offset]);
                    rest = &tail[offset..];
                }
                None => break,
            }
        }
        out
    }

    /// 每条命令用 `app.state::<T>()` 取的状态，**都必须在 builder 里 manage 过**。
    ///
    /// 这一条看着像废话，但漏注册的后果是「运行期 panic + 前端卡死」而不是编译错误，
    /// 且只在真机点那条命令时才炸（2026-09-16：`ImportBatches` 漏了，整个导入挂掉）。
    #[test]
    fn every_state_type_is_managed() {
        let mut managed: HashSet<String> = HashSet::new();
        let mut used: Vec<(String, String)> = Vec::new();

        for (file, text) in SOURCES {
            for path in balanced_after(text, ".manage(", '(', ')') {
                managed.insert(type_name(path));
            }
            for path in balanced_after(text, "state::<", '<', '>') {
                used.push(((*file).to_string(), type_name(path)));
            }
        }

        // `state::<T>()` 里的 T 也可能是泛型参数（`app.state::<Self>()` 之类）——
        // 这里只要求「要么 manage 过，要么是明显的非状态类型（全大写或含泛型）」
        let missing: Vec<String> = used
            .iter()
            // 单字母是泛型占位符（`state::<T>`），不是状态类型
            .filter(|(_, name)| name.len() > 1 && !managed.contains(name))
            .map(|(file, name)| format!("{file}: {name}"))
            .collect();

        assert!(
            missing.is_empty(),
            "这些类型被 app.state::<T>() 取用，但没在 builder 里 .manage() 注册：\n{}",
            missing.join("\n")
        );
    }
}
