//! 库（相片仓）相关命令：列表、建库前的探测、建库、重挂载、照片数。
//!
//! 业务规则在 `raybend::store::repository`（`REPOSITORY.md` §2 的库身份/多路径/在线离线），
//! 这里只做三件事：拿 `app.db`、开 `catalog.db` 数照片、把结构转成前端视图。
//!
//! ⚠️ **数照片要开另一个库**（`catalog.db`），所以「库列表」这个命令是有 I/O 的 ——
//! 它在后台线程里跑，且每次只开一个只读连接池（不跑迁移、不起写线程）。

use std::path::{Path, PathBuf};

use raybend::store::db::{AppDb, CatalogDb, OpenOpts};
use raybend::store::{assets, pool, repository, time};
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

use crate::db::DbState;
use crate::source::blocking;

/// 一个库在界面上的形状（`design/main.md` §3.3 的库卡片）。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryViewDto {
    pub id: String,
    pub name: String,
    pub import_template: Option<String>,
    pub created_at: i64,
    pub last_opened_at: Option<i64>,
    pub online: bool,
    pub root: Option<String>,
    /// 卡片上显示哪条路径：在线 = 库根，离线 = 上次已知路径。
    pub display_path: String,
    pub paths: Vec<RepositoryPathDto>,
    /// 库里的照片数（离线或读不到时 `None` —— **不是 0**）。
    pub photo_count: Option<i64>,
    /// 探测过几条路径（离线时给「已试过 N 处」的提示）。
    pub tried_paths: usize,
}

/// 一条登记路径。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryPathDto {
    pub path: String,
    /// `online` / `offline` / `unknown`。
    pub status: String,
    pub last_seen_at: Option<i64>,
}

impl From<repository::RepositoryView> for RepositoryViewDto {
    fn from(view: repository::RepositoryView) -> Self {
        Self {
            id: view.id,
            name: view.name,
            import_template: view.import_template,
            created_at: view.created_at,
            last_opened_at: view.last_opened_at,
            online: view.online,
            root: view.root,
            display_path: view.display_path,
            paths: view
                .paths
                .into_iter()
                .map(|p| RepositoryPathDto {
                    path: p.path,
                    status: p.status,
                    last_seen_at: p.last_seen_at,
                })
                .collect(),
            photo_count: view.photo_count,
            tried_paths: view.tried_paths,
        }
    }
}

/// 建库弹窗在按下确认前看到的东西。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryProbeDto {
    /// `notDirectory` / `empty` / `existing` / `broken`。
    pub kind: String,
    /// 目录里已有库时的库名。
    pub name: Option<String>,
    /// 目录里已有库时的库 ID。
    pub repository_id: Option<String>,
    /// 这个 ID 是否**已经登记过** —— 是的话，这次操作是「给已有的库登记一条新路径」。
    pub registered: bool,
    /// `broken` 时的原因（给用户看）。
    pub message: Option<String>,
}

/// 数一个库里有多少张照片（开只读连接池，不跑迁移、不起写线程）。
///
/// 读不到就返回 `None` —— 界面上显示「—」而不是「0 张」，两者含义完全不同。
fn count_photos_in(root: &Path) -> Option<i64> {
    let catalog = root.join(repository::CATALOG_FILE_NAME);
    let pool = pool::ReadPool::open(catalog).ok()?;
    let (assets, _files) = pool.with(assets::counts).ok()?;
    Some(assets)
}

/// 库列表（含在线状态与照片数）。
#[tauri::command]
pub async fn repositories_list<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Vec<RepositoryViewDto>, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<DbState>();
        state.with(&handle, views)
    })
    .await
}

fn views(db: &AppDb) -> Result<Vec<RepositoryViewDto>, String> {
    db.read(|conn| repository::build_views(conn, count_photos_in))
        .map(|views| views.into_iter().map(RepositoryViewDto::from).collect())
        .map_err(|e| e.to_string())
}

/// 探一个目录：里面有没有库？能不能新建？
#[tauri::command]
pub async fn repository_probe<R: Runtime>(
    app: AppHandle<R>,
    path: String,
) -> Result<RepositoryProbeDto, String> {
    let handle = app.clone();
    blocking(move || {
        let probe = repository::probe_root(Path::new(&path));
        let (name, repository_id) = match &probe {
            repository::RootProbe::Existing(meta) => (Some(meta.name.clone()), Some(meta.id.clone())),
            _ => (None, None),
        };
        let message = match &probe {
            repository::RootProbe::Broken(reason) => Some(reason.clone()),
            _ => None,
        };
        let registered = match &repository_id {
            Some(id) => {
                let state = handle.state::<DbState>();
                state.with(&handle, |db| {
                    db.list_repositories()
                        .map(|rows| rows.iter().any(|r| &r.id == id))
                        .map_err(|e| e.to_string())
                })?
            }
            None => false,
        };
        Ok(RepositoryProbeDto {
            kind: probe.code().to_string(),
            name,
            repository_id,
            registered,
            message,
        })
    })
    .await
}

/// 建库，或把一个已有库登记进来。
///
/// * 目录不存在 → **建出来**（用户手打的路径也该能用）；
/// * 目录里没有 `catalog.db` → 新建库（生成新 ID + `photos/`）；
/// * 目录里已有 `catalog.db` → **原样使用它的 ID**（这正是「同库多路径」的实现方式），
///   库名以库内记录为准，弹窗里填的名字不覆盖它；
/// * 有 `catalog.db` 却读不出来 → **拒绝**（绝不覆盖别人可能还在用的库文件）。
#[tauri::command]
pub async fn repository_create<R: Runtime>(
    app: AppHandle<R>,
    path: String,
    name: Option<String>,
) -> Result<RepositoryViewDto, String> {
    let handle = app.clone();
    blocking(move || {
        let root = PathBuf::from(&path);
        let now = time::now_millis();

        std::fs::create_dir_all(&root).map_err(|e| format!("无法创建目录 {path}：{e}"))?;
        if let repository::RootProbe::Broken(reason) = repository::probe_root(&root) {
            return Err(format!(
                "该目录下已经有 catalog.db，但它不是可用的相片库：{reason}"
            ));
        }

        let fallback = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "照片库".to_string());
        let name = name
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or(fallback);

        let backups = crate::db::data_dir(&handle)?.join(raybend::store::db::BACKUPS_DIR);
        let catalog = CatalogDb::create(&root, &name, None, OpenOpts::new(Some(&backups), now))
            .map_err(|e| e.to_string())?;
        let meta = catalog.meta().clone();
        drop(catalog); // 建完就放掉（写线程、读池都不需要一直占着）

        let state = handle.state::<DbState>();
        state.with(&handle, |db| {
            db.register_repository(&meta, &root, now)
                .map_err(|e| e.to_string())?;
            // 返回完整视图（路径状态、照片数都算上）
            views(db)?
                .into_iter()
                .find(|v| v.id == meta.id)
                .ok_or_else(|| "库刚登记完却查不到，请重试".to_string())
        })
    })
    .await
}

/// 重新挂载一个离线库：对**所有登记路径**找一遍，找到就转为在线。
///
/// 找不到**不是错误** —— 返回的视图里 `online = false`，界面照常显示离线徽标
/// （`REPOSITORY.md` §2.3：用户插上盘再点一次就行）。
#[tauri::command]
pub async fn repository_remount<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
) -> Result<RepositoryViewDto, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<DbState>();
        let id = repository_id.clone();
        let now = time::now_millis();
        state.with(&handle, |db| {
            db.write_tx(move |tx| repository::refresh_path_status(tx, &id, now))
                .map_err(|e| e.to_string())?;
            views(db)?
                .into_iter()
                .find(|v| v.id == repository_id)
                .ok_or_else(|| format!("没有这个库：{repository_id}"))
        })
    })
    .await
}

/// 一个库现在有多少张照片（离线 → `None`）。
#[tauri::command]
pub async fn repository_counts<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
) -> Result<Option<i64>, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<DbState>();
        let root = state.with(&handle, |db| {
            db.resolve_repository(&repository_id)
                .map(|resolved| match resolved {
                    repository::RepositoryState::Online { root } => Some(root),
                    repository::RepositoryState::Offline { .. } => None,
                })
                .map_err(|e| e.to_string())
        })?;
        Ok(root.and_then(|root| count_photos_in(&root)))
    })
    .await
}
