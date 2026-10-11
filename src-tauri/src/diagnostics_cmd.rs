//! 诊断目录相关的命令（`specs/logging-and-crash-report.md` §9）。
//!
//! 已做：**打开诊断目录**（W1/W2）、**崩溃后的报告导出**（W3-α，崔总 2026-10-12）：
//! 上次崩溃 → 下次启动弹窗 → 用户点「保存」→ 系统另存为 → 在所选目录产 zip + 提交指南。
//! 仍属第三层（未开工）：一键上传（T1）、复制报告摘要、报告对话框的完整版。

use std::path::PathBuf;

use tauri::AppHandle;
use tauri::Runtime;

use raybend::diagnostics::bundle;

/// 打开诊断根目录（`logs/`、`crashes/` 的父目录）。
///
/// 返回打开的那个路径（界面可用来显示一句带变量的提示）。
#[tauri::command]
pub async fn diagnostics_open_folder<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    let root = diagnostics_root(&app)?;
    // 目录不存在（还没写过日志）时先建出来 —— 资源管理器对不存在的路径只会弹错
    std::fs::create_dir_all(&root).map_err(|error| format!("建不了诊断目录：{error}"))?;
    open_in_file_manager(&root)?;
    Ok(root.to_string_lossy().into_owned())
}

/// 上次崩溃了没？（崩溃后**首次**启动时返回一条 —— 同一次崩溃只问一次，规格 §9）
///
/// 用户「不报告」也会被标记（前端调 [`diagnostics_dismiss_crash`]），
/// **只有出现新的崩溃记录才会再问到用户**。
#[tauri::command]
pub async fn diagnostics_pending_crash<R: Runtime>(app: AppHandle<R>) -> Option<bundle::CrashSummary> {
    let root = diagnostics_root(&app).ok()?;
    let crashes = raybend::diagnostics::paths::crashes_dir(&root);
    let summary = bundle::latest_unprompted(&crashes)?;
    Some(summary)
}

/// 用户「不报告」：标记这一条已问过，下次启动不再弹（崔总 2026-10-12 明确要求）。
#[tauri::command]
pub async fn diagnostics_dismiss_crash<R: Runtime>(
    app: AppHandle<R>,
    crash_dir: String,
) -> Result<(), String> {
    let dir = checked_crash_dir(&app, &crash_dir)?;
    bundle::mark_prompted(&dir);
    Ok(())
}

/// 导出报告包：在 `target`（用户在另存为对话框里选的路径）写 zip，并在同目录写提交指南。
///
/// 存成功也标记「已问过」—— 不许重复打扰用户。
#[tauri::command]
pub async fn diagnostics_export_bundle<R: Runtime>(
    app: AppHandle<R>,
    crash_dir: String,
    target: String,
    include_dump: bool,
) -> Result<bundle::BundleOutcome, String> {
    let root = diagnostics_root(&app)?;
    let dir = checked_crash_dir(&app, &crash_dir)?;
    let target = PathBuf::from(target);
    if !target.is_absolute() {
        return Err("保存路径必须是绝对路径".to_string());
    }
    let logs = raybend::diagnostics::paths::logs_dir(&root);
    let outcome = bundle::export(&dir, &logs, &target, include_dump)?;
    bundle::mark_prompted(&dir);
    log::info!(
        target: "diagnostics",
        "错误报告已导出 zip={} bytes={} include_dump={include_dump}",
        outcome.zip_path,
        outcome.zip_bytes
    );
    Ok(outcome)
}

/**
 * **仅开发构建**：故意 panic 一次，用来验「崩溃 → panic 记录 + 助手落 dump → 下次启动提问」整条链。
 *
 * 为什么留这么个口子：panic 路径在真机上没法从外部触发（总不能等它自己崩），
 * 而「panic 时到底有没有 dump」是发布前必须知道的事 —— 发布构建里直接拒绝调用。
 */
#[tauri::command]
pub async fn diagnostics_self_test_crash<R: Runtime>(_app: AppHandle<R>) -> Result<(), String> {
    if !cfg!(debug_assertions) {
        return Err("自测命令只在开发构建里可用".to_string());
    }
    panic!("（诊断自测）故意制造的 panic —— diagnostics self-test");
}

/// 诊断根目录（优先用已初始化的句柄，拿不到再退回 Tauri 的数据目录）。
fn diagnostics_root<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    raybend::diagnostics::current()
        .map(|diagnostics| diagnostics.root().to_path_buf())
        .or_else(|| crate::db::data_dir(app).ok())
        .ok_or_else(|| "拿不到诊断目录".to_string())
}

/// 前端回传的崩溃目录**必须在 `crashes/` 之内** —— 免得一次笔误就让导出去读别处。
fn checked_crash_dir<R: Runtime>(app: &AppHandle<R>, raw: &str) -> Result<PathBuf, String> {
    let crashes = raybend::diagnostics::paths::crashes_dir(&diagnostics_root(app)?);
    let candidate = std::fs::canonicalize(raw).map_err(|error| format!("崩溃目录不存在：{error}"))?;
    let base = std::fs::canonicalize(&crashes).unwrap_or(crashes);
    if !candidate.starts_with(&base) {
        return Err("崩溃目录不在诊断目录内".to_string());
    }
    let name = candidate
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !name.starts_with("RB-") {
        return Err("不是崩溃记录目录".to_string());
    }
    Ok(candidate)
}

/// 用系统文件管理器打开一个目录。
fn open_in_file_manager(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        // explorer 会把路径当参数打开；它对「程序退出码」不严格，所以不等它
        std::process::Command::new("explorer.exe")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("打不开资源管理器：{error}"))
    }
    #[cfg(not(windows))]
    {
        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        std::process::Command::new(opener)
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("打不开文件管理器：{error}"))
    }
}
