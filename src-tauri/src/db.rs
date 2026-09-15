//! `app.db` 的外壳侧接线：**路径解析 + 命令转发**。
//!
//! 这里刻意不做任何业务：数据底座的逻辑全在 `raybend::store` 内（见 `AGENTS.md` §6.4）。
//! 外壳只负责三件事：
//!
//! 1. 用 Tauri 的 path API 解析出**应用数据目录**（不硬编码、不猜路径 —— `AGENTS.md` §6.2）；
//! 2. 启动时打开 `app.db`（失败**不阻止**应用启动：窗口该出来还是要出来）；
//! 3. 把这几个动作暴露成命令，供前端在 M1-5/M1-6 使用。
//!
//! M1-2 阶段这几个命令的作用是**可验证**：跑起来能看到 `app.db` 真的建出来了、
//! schema 版本对得上、设置在重启后还在。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use raybend::store::db::AppDb;
use raybend::store::location;
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime, State};

/// `app.db` 的持有者（延迟打开；打开失败时保持 `None` 并把错误交给调用方）。
#[derive(Default)]
pub struct DbState {
    inner: Mutex<Option<AppDb>>,
}

impl DbState {
    /// 借出已打开的 `AppDb`；没打开就打开一次。
    fn get_or_open<R: Runtime>(&self, app: &AppHandle<R>) -> Result<(), String> {
        {
            let guard = self.inner.lock().map_err(|_| "内部锁已损坏".to_string())?;
            if guard.is_some() {
                return Ok(());
            }
        }
        let path = data_dir(app)?;
        let db =
            AppDb::open(&path, raybend::store::time::now_millis()).map_err(|e| e.to_string())?;
        let mut guard = self.inner.lock().map_err(|_| "内部锁已损坏".to_string())?;
        *guard = Some(db);
        Ok(())
    }

    /// 在已打开的库上执行一段操作。
    pub(crate) fn with<R: Runtime, T>(
        &self,
        app: &AppHandle<R>,
        f: impl FnOnce(&AppDb) -> Result<T, String>,
    ) -> Result<T, String> {
        self.get_or_open(app)?;
        let guard = self.inner.lock().map_err(|_| "内部锁已损坏".to_string())?;
        let db = guard.as_ref().ok_or_else(|| "数据库尚未打开".to_string())?;
        f(db)
    }
}

/// 应用数据根目录。
///
/// Tauri 给的 `app_local_data_dir()` 本身已经是**应用私有**目录
/// （Windows 上形如 `%LOCALAPPDATA%\<应用标识>`），所以不再往下叠一层目录名。
pub fn data_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map_err(|e| format!("无法解析应用数据目录：{e}"))
}

/// `app.db` 在数据目录下的文件名（`AGENTS.md` §6.4）。
pub const APP_DB_FILE: &str = "app.db";

/// 数据目录 → `app.db` 路径（纯函数，便于测试）。
#[must_use]
pub fn app_db_path(data_dir: &Path) -> PathBuf {
    data_dir.join(APP_DB_FILE)
}

/// 前端可拿到的一组路径。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPaths {
    /// 应用数据目录（`app.db`、`backups/`、`cache/` 都在它下面）。
    pub data_dir: String,
    /// 缓存目录（可能与应用数据目录不同，取决于平台）。
    pub cache_dir: Option<String>,
    /// `app.db` 的完整路径。
    pub app_db: String,
    /// 数据目录的位置性质：`local` / `network` / `cloud-sync`。
    ///
    /// 放在云同步盘上会让 SQLite 有损坏风险，前端要据此提示用户（`AGENTS.md` §6.4）。
    pub data_dir_kind: String,
    /// 云端服务的名字（识别出来时）。
    pub data_dir_provider: Option<String>,
}

/// 返回应用的数据目录与 `app.db` 路径（**不打开库**，用于界面显示与诊断）。
#[tauri::command]
pub fn app_paths<R: Runtime>(app: AppHandle<R>) -> Result<AppPaths, String> {
    let data = data_dir(&app)?;
    let kind = location::classify(data.to_string_lossy().as_ref());
    let cache = app
        .path()
        .app_cache_dir()
        .ok()
        .map(|p| p.to_string_lossy().into_owned());
    Ok(AppPaths {
        app_db: app_db_path(&data).to_string_lossy().into_owned(),
        data_dir: data.to_string_lossy().into_owned(),
        cache_dir: cache,
        data_dir_kind: kind.code().to_string(),
        data_dir_provider: kind.provider().map(ToString::to_string),
    })
}

/// `app.db` 的运行状态（用一个命令就能验收数据底座是否真的工作）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbStatus {
    /// 库文件路径。
    pub path: String,
    /// 当前 schema 版本（应等于程序支持的版本）。
    pub schema_version: i64,
    /// 已登记的库数量。
    pub repositories: usize,
    /// 应用数据目录的位置性质。
    pub data_dir_kind: String,
}

/// 打开（必要时创建）`app.db` 并返回状态。
#[tauri::command]
pub fn db_status<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DbState>,
) -> Result<DbStatus, String> {
    let data_kind = location::classify(data_dir(&app)?.to_string_lossy().as_ref())
        .code()
        .to_string();
    state.with(&app, |db| {
        let version = db
            .read(raybend::store::migration::schema_version)
            .map_err(|e| e.to_string())?;
        let repositories = db.list_repositories().map_err(|e| e.to_string())?.len();
        Ok(DbStatus {
            path: db.path().to_string_lossy().into_owned(),
            schema_version: version,
            repositories,
            data_dir_kind: data_kind,
        })
    })
}

/// 读一条设置。
#[tauri::command]
pub fn setting_get<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DbState>,
    key: String,
) -> Result<Option<String>, String> {
    state.with(&app, |db| db.get_setting(&key).map_err(|e| e.to_string()))
}

/// 写一条设置。
#[tauri::command]
pub fn setting_set<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DbState>,
    key: String,
    value: String,
) -> Result<(), String> {
    state.with(&app, |db| {
        db.set_setting(&key, &value).map_err(|e| e.to_string())
    })
}

/// 启动时尝试打开一次（失败只记日志，不阻止窗口出现）。
pub fn warm_up<R: Runtime>(app: &AppHandle<R>, state: &DbState) {
    match state.get_or_open(app) {
        Ok(()) => {
            if let Ok(paths) = app_paths(app.clone()) {
                println!(
                    "[raybend] 数据底座就绪：{}（位置：{}）",
                    paths.app_db, paths.data_dir_kind
                );
                if paths.data_dir_kind != "local" {
                    eprintln!(
                        "[raybend] ⚠️ 应用数据目录位于非本地位置（{}）—— SQLite 有损坏风险，见 AGENTS.md §6.4",
                        paths.data_dir_kind
                    );
                }
            }
        }
        Err(e) => eprintln!("[raybend] ⚠️ 打开应用数据库失败（不影响窗口显示）：{e}"),
    }
}

#[cfg(test)]
mod tests {
    //! 这一层很薄，逻辑测试交给 `raybend::store`；
    //! 这里只钉住「文件名」这个跨模块约定。

    use super::*;
    use std::path::Path;

    #[test]
    fn app_db_lives_at_the_data_dir_root() {
        // 与 AGENTS.md §6.4 的布局绑死：改这里等于改磁盘布局，要有意识
        assert_eq!(APP_DB_FILE, "app.db");
        assert_eq!(
            app_db_path(Path::new("/data/raybend")),
            PathBuf::from("/data/raybend/app.db")
        );
    }
}
