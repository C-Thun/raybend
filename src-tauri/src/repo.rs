//! 库（相片仓）相关命令：列表、建库前的探测、建库、重挂载、计数与**重建数据**。
//!
//! 业务规则在 `raybend::store::repository`（`memory/FUNCTION-REPOSITORY.md` §2 的库身份/多路径/在线离线），
//! 这里只做三件事：拿 `app.db` / `catalog.db`、把结构转成前端视图、把耗时活儿丢到后台线程。
//!
//! **计数（2026-09-19 的新口径）**：`photos_count`（相片，不含 `_RAW`）与
//! `images_count`（图片，含 `_RAW`）存在 `app.db` 的 `repositories` + `directories` 里，
//! 由「导入时写」「进目录时增量同步」「重建时全量重算」三条路径维护 ——
//! 列表命令因此**不再逐个打开库的 catalog.db**（老实现那样做，慢盘上几秒起步）。

use std::path::{Path, PathBuf};

use raybend::store::db::{CatalogDb, OpenOpts};
use raybend::store::{repository, time};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use raybend::import::template;

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
    /// **相片数量**（不含 `_RAW/`）。`None` = 还没数过 —— 界面显示「—」，**不是 0**。
    pub photos_count: Option<i64>,
    /// **图片数量**（含 `_RAW/`）。
    pub images_count: Option<i64>,
    /// 探测过几条路径（离线时给「已试过 N 处」的提示）。
    pub tried_paths: usize,
    pub connection: raybend::store::availability::ConnectionStatus,
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

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryLocationErrorDto {
    pub code: String,
}
impl From<raybend::Error> for RepositoryLocationErrorDto {
    fn from(error: raybend::Error) -> Self {
        eprintln!("[repository] 位置操作失败：{error}");
        let code = match &error {
            raybend::Error::InvalidRepositoryLocation(_) => "invalid_path".into(),
            raybend::Error::RepositoryPathInUse(_) => "active_location".into(),
            raybend::Error::Unsupported(_) => "io_failure".into(),
            _ => {
                let status = raybend::store::availability::ConnectionStatus::failed("", &error);
                status.reason.map_or_else(
                    || "not_found".into(),
                    |reason| {
                        serde_json::to_value(reason)
                            .unwrap()
                            .as_str()
                            .unwrap()
                            .into()
                    },
                )
            }
        };
        Self { code }
    }
}

/// 只登记已经核对为目标库的位置，探测线程超时后不会迟到修改 app.db。
#[tauri::command]
pub async fn repository_add_location<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    path: String,
) -> Result<RepositoryViewDto, RepositoryLocationErrorDto> {
    let handle = app.clone();
    let id = repository_id.clone();
    let result = blocking(move || {
        Ok((|| -> raybend::Result<RepositoryViewDto> {
            view_one(&handle, &id).map_err(raybend::Error::Unsupported)?;
            let root = PathBuf::from(path);
            let probe_id = id.clone();
            let probe_root = root.clone();
            let budget = handle.state::<crate::browse::BrowseState>();
            match budget.probes.run_endpoints(
                &id,
                &[raybend::store::availability::endpoint_hint(
                    root.to_string_lossy().as_ref(),
                )],
                raybend::store::availability::PROBE_WAIT,
                move || repository::validate_alternate_location(&probe_id, &probe_root),
            ) {
                Ok(result) => {
                    result?;
                }
                Err(wait) => {
                    return Err(match wait {
                        raybend::store::availability::ProbeWait::Busy => {
                            raybend::Error::StorageProbeBusy
                        }
                        raybend::store::availability::ProbeWait::Timeout => {
                            raybend::Error::StorageProbeTimeout
                        }
                        raybend::store::availability::ProbeWait::WorkerGone => {
                            raybend::Error::WriterGone
                        }
                    });
                }
            }
            let location = root.to_string_lossy().into_owned();
            handle
                .state::<DbState>()
                .with(&handle, |db| {
                    Ok(db.write_tx(move |conn| {
                        repository::add_repository_path(conn, &id, &location, time::now_millis())
                    }))
                })
                .map_err(raybend::Error::Unsupported)??;
            view_one(&handle, &repository_id).map_err(raybend::Error::Unsupported)
        })()
        .map_err(RepositoryLocationErrorDto::from))
    })
    .await
    .map_err(|error| RepositoryLocationErrorDto::from(raybend::Error::Unsupported(error)))?;
    let view = result?;
    if view.online {
        Ok(view)
    } else {
        repository_remount(app, view.id, Some(true))
            .await
            .map_err(|error| RepositoryLocationErrorDto::from(raybend::Error::Unsupported(error)))
    }
}

#[tauri::command]
pub async fn repository_remove_location<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    path: String,
) -> Result<RepositoryViewDto, RepositoryLocationErrorDto> {
    blocking(move || {
        Ok((|| -> raybend::Result<RepositoryViewDto> {
            view_one(&app, &repository_id).map_err(raybend::Error::Unsupported)?;
            app.state::<crate::browse::BrowseState>()
                .sessions
                .with_current(&repository_id, |current| {
                    let active = current
                        .filter(|db| db.ensure_alive().is_ok())
                        .map(|db| db.root().to_path_buf());
                    let id = repository_id.clone();
                    app.state::<DbState>()
                        .with(&app, |db| {
                            Ok(db.write_tx(move |conn| {
                                repository::remove_repository_path(
                                    conn,
                                    &id,
                                    &path,
                                    active.as_deref(),
                                )
                            }))
                        })
                        .map_err(raybend::Error::Unsupported)??;
                    Ok(())
                })?;
            view_one(&app, &repository_id).map_err(raybend::Error::Unsupported)
        })()
        .map_err(RepositoryLocationErrorDto::from))
    })
    .await
    .map_err(|error| RepositoryLocationErrorDto::from(raybend::Error::Unsupported(error)))?
}

impl From<repository::RepositoryView> for RepositoryViewDto {
    fn from(view: repository::RepositoryView) -> Self {
        Self {
            connection: raybend::store::availability::ConnectionStatus::unknown(&view.id),
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
            photos_count: view.photos_count,
            images_count: view.images_count,
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

/// 库列表（含在线状态与两个计数）。
#[tauri::command]
pub async fn repositories_list<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Vec<RepositoryViewDto>, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<DbState>();
        views(&handle, &state)
    })
    .await
}

/// 列表只取登记/计数/已观察状态，不递归扫描、不探测磁盘。
fn views<R: Runtime>(
    app: &AppHandle<R>,
    state: &DbState,
) -> Result<Vec<RepositoryViewDto>, String> {
    let rows = state.with(app, |db| {
        db.read(repository::build_registered_views)
            .map_err(|e| e.to_string())
    })?;
    Ok(rows.into_iter().map(|row| project(app, row)).collect())
}
fn project<R: Runtime>(app: &AppHandle<R>, row: repository::RepositoryView) -> RepositoryViewDto {
    let status = app
        .state::<crate::browse::BrowseState>()
        .connections
        .get(&row.id);
    let mut view = RepositoryViewDto::from(row);
    view.online = status.state == raybend::store::availability::Availability::Online;
    view.root = status.root.clone();
    if let Some(root) = &view.root {
        view.display_path = root.clone();
    }
    view.connection = status;
    view
}
fn view_one<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<RepositoryViewDto, String> {
    // 登记有限且无 I/O；只投影目标，不探测其它库。
    let row = app
        .state::<DbState>()
        .with(app, |db| {
            db.read(repository::build_registered_views)
                .map_err(|e| e.to_string())
        })?
        .into_iter()
        .find(|row| row.id == id)
        .ok_or_else(|| format!("没有这个库：{id}"))?;
    Ok(project(app, row))
}

/// `photos/` 目录名（库内落地目录；`FUTURE G14` 将来可配）。
const DEFAULT_PHOTOS_DIR: &str = "photos";

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
            repository::RootProbe::Existing(meta) => {
                (Some(meta.name.clone()), Some(meta.id.clone()))
            }
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

        raybend::store::location::require_catalog_location(&root).map_err(|e| e.to_string())?;
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

        let meta = match repository::probe_root(&root) {
            repository::RootProbe::Existing(meta) => *meta,
            repository::RootProbe::Broken(reason) => return Err(reason),
            _ => {
                let backups = crate::db::data_dir(&handle)?.join(raybend::store::db::BACKUPS_DIR);
                let created =
                    CatalogDb::create(&root, &name, None, OpenOpts::new(Some(&backups), now))
                        .map_err(|e| e.to_string())?;
                created.meta().clone() // 登记前关掉新建阶段写者；已有库不另开写者。
            }
        };

        let state = handle.state::<DbState>();
        state.with(&handle, |db| {
            db.register_repository(&meta, &root, now)
                .map_err(|e| e.to_string())
        })?;
        handle
            .state::<crate::browse::BrowseState>()
            .lease(&handle, &meta.id)?;
        view_one(&handle, &meta.id)
    })
    .await
}

/// 显式选择同 ID 副本；尚有任务时禁止转写。
#[tauri::command]
pub async fn repository_use_location<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    path: String,
) -> Result<RepositoryViewDto, RepositoryLocationErrorDto> {
    let handle = app.clone();
    let id = repository_id.clone();
    blocking(move || {
        let browse = handle.state::<crate::browse::BrowseState>();
        if handle
            .state::<crate::import::ImportBatches>()
            .unfinished_repository(&id)
            || handle
                .state::<crate::export::ExportState>()
                .unfinished_repository(&id)
        {
            return Err(raybend::Error::RepositoryBusy.to_string());
        }
        let root = PathBuf::from(path);
        repository::validate_alternate_location(&id, &root).map_err(|e| e.to_string())?;
        let identity =
            raybend::store::file_id::FileId::try_read(root.join(repository::CATALOG_FILE_NAME))
                .filter(|v| !v.is_zero())
                .ok_or("此设备没有可靠的库实体身份，无法固定副本选择")?;
        let preferred = raybend::store::availability::PreferredLocation {
            path: root.to_string_lossy().into(),
            identity,
        };
        browse
            .sessions
            .switch_location(&id, || {
                repository::validate_alternate_location(&id, &root)?;
                if raybend::store::file_id::FileId::try_read(
                    root.join(repository::CATALOG_FILE_NAME),
                ) != Some(identity)
                {
                    return Err(raybend::Error::RepositoryIdentityChanged(root.clone()));
                }
                handle
                    .state::<DbState>()
                    .with(&handle, |db| {
                        db.write_tx({
                            let id = id.clone();
                            let path = preferred.path.clone();
                            move |c| {
                                repository::add_repository_path(c, &id, &path, time::now_millis())
                            }
                        })
                        .map_err(|e| e.to_string())?;
                        db.set_setting_json(
                            &raybend::store::availability::preference_key(&id),
                            &preferred,
                        )
                        .map_err(|e| e.to_string())
                    })
                    .map_err(raybend::Error::Unsupported)
            })
            .map_err(|e| e.to_string())?;
        browse.clear_watch(&id);
        browse.publish(
            &handle,
            raybend::store::availability::ConnectionStatus::unknown(&id),
        );
        Ok(())
    })
    .await
    .map_err(|e| RepositoryLocationErrorDto {
        code: if e.contains("未完成任务") {
            "busy"
        } else {
            "io_failure"
        }
        .into(),
    })?;
    repository_remount(app, repository_id, None)
        .await
        .map_err(|_| RepositoryLocationErrorDto {
            code: "io_failure".into(),
        })
}
#[tauri::command]
pub async fn repository_release<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
) -> Result<RepositoryViewDto, String> {
    blocking(move || {
        view_one(&app, &repository_id)?;
        let browse = app.state::<crate::browse::BrowseState>();
        if !browse.sessions.begin_release(&repository_id) {
            return view_one(&app, &repository_id);
        }
        let mut status = raybend::store::availability::ConnectionStatus::unknown(&repository_id);
        status.state = raybend::store::availability::Availability::Releasing;
        browse.publish(&app, status);
        app.state::<crate::import::ImportBatches>()
            .cancel_repository(&repository_id);
        app.state::<crate::export::ExportState>()
            .suspend_repository(&repository_id)?;
        browse.clear_watch(&repository_id);
        browse
            .sessions
            .finish_release(&repository_id)
            .map_err(|e| e.to_string())?;
        let mut status = raybend::store::availability::ConnectionStatus::unknown(&repository_id);
        status.state = raybend::store::availability::Availability::Released;
        browse.publish(&app, status);
        view_one(&app, &repository_id)
    })
    .await
}

/// 重新挂载一个离线库：对**所有登记路径**找一遍，找到就转为在线。
///
/// 找不到**不是错误** —— 返回的视图里 `online = false`，界面照常显示离线徽标
/// （`memory/FUNCTION-REPOSITORY.md` §2.3：用户插上盘再点一次就行）。
#[tauri::command]
pub async fn repository_remount<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    automatic: Option<bool>,
) -> Result<RepositoryViewDto, String> {
    let handle = app.clone();
    blocking(move || {
        let browse = handle.state::<crate::browse::BrowseState>();
        if automatic != Some(true) {
            browse
                .sessions
                .resume(&repository_id)
                .map_err(|e| e.to_string())?;
        }
        if browse.sessions.is_released(&repository_id) {
            return view_one(&handle, &repository_id);
        }
        let app_for_probe = handle.clone();
        let id = repository_id.clone();
        let revision = browse.connections.get(&id).revision;
        let paths = handle.state::<DbState>().with(&handle, |db| {
            db.read(|conn| repository::registered_paths(conn, &id))
                .map_err(|e| e.to_string())
        })?;
        let endpoints = paths
            .iter()
            .map(|path| raybend::store::availability::endpoint_hint(path))
            .collect::<Vec<_>>();
        let outcome = browse.probes.run_endpoints(
            &repository_id,
            &endpoints,
            raybend::store::availability::PROBE_WAIT,
            move || {
                let browse = app_for_probe.state::<crate::browse::BrowseState>();
                use raybend::store::availability::{Availability, Reason};
                let observed = browse.connections.get(&id);
                // 超时/排队只表示尚未得到结果，不能据此中断健康会话。
                if observed.state == Availability::Offline
                    || (observed.state == Availability::Unavailable
                        && !matches!(observed.reason, Some(Reason::Timeout | Reason::Busy)))
                {
                    browse
                        .sessions
                        .invalidate_observed(&id, observed.generation.parse().unwrap_or(0))?;
                    browse.clear_watch(&id);
                }
                browse.acquire_session(&app_for_probe, &id)
            },
        );
        // 探测线程只返回事实；超时或旧请求不会迟到发布 online。
        if browse.connections.get(&repository_id).revision == revision {
            use raybend::store::availability::{Availability, ConnectionStatus, ProbeWait, Reason};
            let mut status = ConnectionStatus::unknown(&repository_id);
            match outcome {
                Ok(Ok(db)) => {
                    status.state = Availability::Online;
                    status.root = Some(db.root().to_string_lossy().into());
                    status.generation = db.generation().to_string();
                }
                Ok(Err(error)) => {
                    eprintln!("[repository] 探测失败：{error}");
                    status = ConnectionStatus::failed(&repository_id, &error);
                }
                Err(wait) => {
                    status.state = if wait == ProbeWait::Busy {
                        Availability::Checking
                    } else {
                        Availability::Unavailable
                    };
                    status.reason = Some(if wait == ProbeWait::Busy {
                        Reason::Busy
                    } else {
                        Reason::Timeout
                    });
                }
            }
            if let Some(published) = browse.publish_if(&handle, status, Some(&revision)) {
                let events = handle.clone();
                let id = repository_id.clone();
                handle.state::<DbState>().with(&handle, |db| {
                    db.write_tx(move |conn| {
                        // 不把过期探测持久化；这里只读 SQL/内存，不做磁盘探测。
                        if events
                            .state::<crate::browse::BrowseState>()
                            .connections
                            .get(&id)
                            .revision
                            != published.revision
                        {
                            return Ok(());
                        }
                        for path in paths {
                            let code = match published.state {
                                Availability::Online
                                    if published.root.as_ref().is_some_and(|root| {
                                        raybend::store::path_semantics::PathForms::new(root)
                                            .folded()
                                            == raybend::store::path_semantics::PathForms::new(&path)
                                                .folded()
                                    }) =>
                                {
                                    "online"
                                }
                                Availability::Offline => "offline",
                                _ => "unknown",
                            };
                            repository::set_path_status(
                                conn,
                                &id,
                                &path,
                                code,
                                published.observed_at,
                            )?;
                        }
                        Ok(())
                    })
                    .map_err(|e| e.to_string())
                })?;
            }
        }
        view_one(&handle, &repository_id)
    })
    .await
}

/// 一个库现在的两个计数（离线或没数过 → `None`）。
///
/// 返回值是 `[photos, images]` 两个数（相片数量 / 图片数量）——
/// 界面上的库卡片只显示前者，齿轮弹窗里两个都显示。
#[tauri::command]
pub async fn repository_counts<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
) -> Result<Option<[i64; 2]>, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<DbState>();
        state.with(&handle, |db| {
            db.read(|conn| repository::totals(conn, &repository_id))
                .map(|counts| counts.map(|c| [c.photos, c.images]))
                .map_err(|e| e.to_string())
        })
    })
    .await
}

/// **进目录时同步计数**（人类 2026-09-19 的要求）。
///
/// 做什么：把 `scopePath` 这个目录在磁盘上的真实文件数读出来（本目录 + 它自己的 `_RAW`），
/// 与 `app.db` 里那行对比 —— 不一样就写回去，并把差值滚到库级汇总上。
///
/// 为什么每次进目录都做：**本地应用实时性优先**（`AGENTS.md` §2 #13）——
/// 程序外面往目录里加/删文件是常事，一次 `readdir` 是微秒级，没必要为省它去承担「数字对不上」。
///
/// 重扫后对**新建**资产采纳旁边的自家 sidecar（`specs/xmp-w1.md` §7）。
/// `report.new_assets` 平时是空的，这里几乎总是空转；只有磁盘上真的出现了新文件才工作。
fn adopt_sidecars_for_report<R: Runtime>(
    handle: &tauri::AppHandle<R>,
    catalog: &std::sync::Arc<raybend::store::db::CatalogDb>,
    root: &std::path::Path,
    report: &raybend::store::rebuild::RescanReport,
) {
    crate::sidecar::queue_sync(handle.clone(),&catalog.meta().id,report.ai_invalidated.clone());
    if report.new_assets.is_empty() {
        return;
    }
    let items: Vec<crate::sidecar::NewAsset> = report
        .new_assets
        .iter()
        .map(|(id, rel)| crate::sidecar::NewAsset {
            asset_id: *id,
            file_abs: root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR)),
        })
        .collect();
    crate::sidecar::adopt_new_assets(handle, catalog, &items);
}

/// 返回：[目录的计数, 库级汇总]。
/// 变更只通过这一条链失效：目录元信息 → 持久小图/预览 → 清单订阅。
fn invalidate_changes<R: Runtime>(
    app: &AppHandle<R>,
    repository_id: &str,
    catalog: &CatalogDb,
    _scope: &str,
    report: &raybend::store::rebuild::RescanReport,
) -> Result<(), String> {
    if report.changed_assets.is_empty() {
        return Ok(());
    }
    catalog.ensure_current().map_err(|e| e.to_string())?;
    let root = catalog.root();
    let full = raybend::display::FullCache::open(root).map_err(|e| e.to_string())?;
    let mut keys = Vec::new();
    for path in &report.changed_paths {
        let abs = root.join(path);
        if let Some(parent) = abs.parent() {
            app.state::<crate::source::SourcesMetaCache>()
                .0
                .lock()
                .map_err(|e| e.to_string())?
                .invalidate_dir(parent);
        }
        keys.push(raybend::thumbnail::worker::cache_key_for(
            &abs,
            &abs.to_string_lossy(),
        ));
        let forms = raybend::store::path_semantics::PathForms::new(&abs.to_string_lossy());
        keys.push(raybend::thumbnail::cache::cache_key(None, forms.folded()));
    }
    for asset in &report.changed_assets {
        catalog.ensure_current().map_err(|e| e.to_string())?;
        full.invalidate_source(*asset).map_err(|e| e.to_string())?;
        let issues = catalog
            .read(|conn| raybend::store::issues::list(conn, *asset))
            .map_err(|e| e.to_string())?;
        for issue in issues {
            keys.push(crate::issues::cache_key(repository_id, *asset, issue.id));
        }
    }
    let dir = crate::thumbs::sources_cache_dir(app)?;
    let thumbs = app
        .state::<crate::thumbs::SourcesThumbs>()
        .get(&dir, time::now_millis())?;
    thumbs
        .write_tx(move |conn| {
            for key in keys {
                raybend::thumbnail::cache::remove_key(conn, &key)?;
            }
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn repository_sync_dir<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    scope_path: Option<String>,
    scope_paths: Option<Vec<String>>,
) -> Result<Option<[[i64; 2]; 2]>, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<DbState>();
        let browse = handle.state::<crate::browse::BrowseState>();
        let _permit = browse.sessions.begin_task(&repository_id).map_err(|e|e.to_string())?;
        let catalog = browse.lease(&handle, &repository_id)?;
        let result = (|| {
        let root = catalog.root().to_path_buf();
        let Some(scope) = scope_path.filter(|path| !path.is_empty()) else {
            return Ok(None);
        };
        let refresh_active = scope_paths.as_ref().is_some_and(Vec::is_empty);
        let mut scopes = scope_paths.unwrap_or_default();
        if refresh_active
            && let Some((id, _, _, watcher)) = handle.state::<crate::browse::BrowseState>().live_watch.lock().map_err(|e| e.to_string())?.as_ref()
                && id == &repository_id { scopes = watcher.active_scopes(); }
        if scopes.len() > raybend::media::watch::MAX_SCOPES * 2 + 2 {
            return Err("同步范围过多".into());
        }
        scopes.push(scope.clone());
        scopes.sort();
        scopes.dedup();
        let disk_gate = browse.disk_gate(&repository_id);
        let _single_flight = disk_gate.lock().map_err(|e| e.to_string())?;
        let generation = catalog.generation();
        let previous = browse.live_watch.lock().map_err(|e| e.to_string())?.take();
        let mut watcher = match previous {
            Some((id, path, old_generation, watcher)) if id == repository_id && path == root && old_generation == generation => watcher,
            retired => {
                drop(retired); // join 和原生监听创建均在全局锁外
                let app = handle.clone(); let id = repository_id.clone();
                raybend::media::watch::CatalogWatcher::new(&root, move |scopes| {
                    if let Err(e) = app.emit("catalog://dirty", serde_json::json!({"repositoryId": id, "scopes": scopes})) { eprintln!("[catalog] 发送目录变更失败：{e}"); }
                })?
            }
        };
        for scoped in &scopes { watcher.visit(scoped)?; }
        watcher.visit(&scope)?;
        catalog.ensure_current().map_err(|e|e.to_string())?;
        let unused = {
            let mut slot = browse.live_watch.lock().map_err(|e|e.to_string())?;
            if slot.is_none() { *slot = Some((repository_id.clone(),root.clone(),generation,watcher)); None } else {Some(watcher)}
        };
        drop(unused);
        let now = time::now_millis();
        let mut current_counts = repository::Counts::default();
        for scoped in &scopes {
            catalog.ensure_current().map_err(|e| e.to_string())?;
            let (report, counts) = raybend::store::rebuild::rescan_scope(&catalog, "photos", scoped, now)
                .map_err(|e| e.to_string())?;
            // 新建资产的 sidecar 采纳（specs/xmp-w1.md §7：手动拷进库的照片+sidecar 自动读回）
            adopt_sidecars_for_report(&handle, &catalog, &root, &report);
            // 缓存的失效在发送事件前完成；调用失败会明确反馈，不能静默显示旧计数。
            invalidate_changes(&handle, &repository_id, &catalog, scoped, &report)?;
            if *scoped == scope {
                current_counts = counts;
            }
            let id = repository_id.clone();
            let updates = report.directory_counts.clone();
            let reset = report.reset_counts;
            catalog.ensure_current().map_err(|e| e.to_string())?;
            let counts_session = std::sync::Arc::clone(&catalog);
            state.with(&handle, |db| {
                db.write_tx(move |conn| {
                    counts_session.ensure_alive()?;
                    if reset { repository::clear_directories(conn, &id)?; }
                    for (path, photos, images) in updates { repository::set_directory_counts(conn, &id, &path, repository::Counts { photos, images }, now)?; }
                    Ok(())
                })
                .map_err(|e| e.to_string())
            })?;
            raybend::store::rebuild::acknowledge_changes(&catalog, &report).map_err(|e| e.to_string())?;
            handle.emit("catalog://changed", serde_json::json!({"repositoryId": repository_id, "scopePath": scoped,
                "assetIds": report.changed_assets, "root": root.to_string_lossy(), "relativePaths": report.changed_paths
            })).map_err(|e| e.to_string())?;
        }
        let totals = state
            .with(&handle, |db| {
                db.read(|conn| repository::totals(conn, &repository_id))
                    .map_err(|e| e.to_string())
            })?
            .unwrap_or_default();
        Ok(Some([
            [current_counts.photos, current_counts.images],
            [totals.photos, totals.images],
        ]))
        })();
        browse.observe_session(&handle, &catalog);
        result
    })
    .await
}

/* ══════════════════════════════════════════════════════════════
 * 重建数据（人类 2026-09-19：齿轮弹窗里的那个按钮）
 * ══════════════════════════════════════════════════════════════ */

/// 重建数据的**进度事件名**（前端据此显示「扫到第几张 / 正在补元数据」）。
pub const REBUILD_EVENT: &str = "db://rebuild";

/// 重建进度（人类 2026-09-19：重建数据要能看到进展，不能黑箱几十秒）。
///
/// `phase` 是**机器可读**的阶段名（`scan` / `apply` / `metadata` / `counts` / `done`）——
/// 句子由前端按当前语言组织，后端不拼人话（i18n 纪律）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebuildProgressDto {
    pub repository_id: String,
    pub phase: String,
    pub done: usize,
    pub total: usize,
}
/// 「重建数据」的结果（给用户看的一句话 + 几个数字）。
#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebuildReportDto {
    /// 扫到多少文件。
    pub scanned: usize,
    /// 新登记进来的文件（库里有记录之外、磁盘上多出来的）。
    pub registered: usize,
    /// 新标记为「磁盘上找不到」的。
    pub missing: usize,
    /// 之前找不到、这次又出现的。
    pub returned: usize,
    /// 路径变了（文件被改名/移动）但认出来的。
    pub renamed: usize,
    /// 补上元数据的资产数（老库那批没有 EXIF 的）。
    pub metadata_filled: usize,
    /// 重建后的**相片数量**。
    pub photos_count: i64,
    /// 重建后的**图片数量**。
    pub images_count: i64,
}

/// 重扫整个库、把 catalog 与 app.db 的计数拉平（耗时，界面上要挡住操作）。
///
/// 四件事，顺序不能反：
///   1. **扫盘**（`photos/` 之下）+ 与 `asset_files` 对比 → 登记新文件、标记缺失、修正路径；
///   2. **重读元数据**（`taken_at` / 宽高 / 朝向为空的老资产）—— 老库那批的根因就是它们空着；
///   3. **重算目录计数**（清空后按磁盘重数一遍）；
///   4. 汇总进 `repositories`（第 3 步里已经顺带做了）。
#[tauri::command]
pub async fn repository_rebuild<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
) -> Result<RebuildReportDto, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<DbState>();
        let browse = handle.state::<crate::browse::BrowseState>();
        let _permit = browse.sessions.begin_task(&repository_id).map_err(|e|e.to_string())?;
        let catalog = browse.lease(&handle, &repository_id)?;
        let result = (|| {
        let root = catalog.root().to_path_buf();
        let now = time::now_millis();
        let disk_gate = browse.disk_gate(&repository_id);
        let _single_flight = disk_gate.lock().map_err(|e| e.to_string())?;

        // ①② 磁盘 ↔ catalog 对齐 + 元数据重读（**带进度**：每一步都往前端报一次）
        let emitter = handle.clone();
        let progress_repo = repository_id.clone();
        let emit_progress = |phase: &str, done: usize, total: usize| {
            let _ = emitter.emit(
                REBUILD_EVENT,
                RebuildProgressDto {
                    repository_id: progress_repo.clone(),
                    phase: phase.to_string(),
                    done,
                    total,
                },
            );
        };
        let rescan = raybend::store::rebuild::rescan_library_with_progress(&catalog, &root, DEFAULT_PHOTOS_DIR, now,
            &mut |p| emit_progress(p.phase, p.done, p.total)).map_err(|e| e.to_string())?;
        // 新建资产的 sidecar 采纳（specs/xmp-w1.md §7 —— 灾难恢复语义：catalog 重建后从 sidecar 读回）
        adopt_sidecars_for_report(&handle, &catalog, &root, &rescan);
        invalidate_changes(&handle, &repository_id, &catalog, DEFAULT_PHOTOS_DIR, &rescan)?;

        // ③④ 计数：清空重来（这一份在 app.db 里）
        let (id, counts) = (repository_id.clone(), rescan.directory_counts.clone());
        catalog.ensure_current().map_err(|e| e.to_string())?;
        let counts_session = std::sync::Arc::clone(&catalog);
        let totals = state
            .with(&handle, move |db| {
                db.write_tx(move |conn| {
                    counts_session.ensure_alive()?;
                    repository::clear_directories(conn, &id)?;
                    for (path, photos, images) in counts {
                        repository::set_directory_counts(conn, &id, &path, repository::Counts { photos, images }, now)?;
                    }
                    Ok(repository::totals(conn, &id)?.unwrap_or_default())
                })
                .map_err(|e| e.to_string())
            })
            .map_err(|e| e.to_string())?;

        raybend::store::rebuild::acknowledge_changes(&catalog, &rescan).map_err(|e| e.to_string())?;
        handle.emit("catalog://changed", serde_json::json!({"repositoryId": repository_id, "scopePath": DEFAULT_PHOTOS_DIR,
            "assetIds": rescan.changed_assets, "root": root.to_string_lossy(), "relativePaths": rescan.changed_paths
        })).map_err(|e| e.to_string())?;
        // 计数（app.db 侧）也报一次：这一段的耗时在大库上不小
        let photos_done = usize::try_from(totals.photos).unwrap_or(0);
        emit_progress("counts", photos_done, photos_done);
        emit_progress("done", photos_done, photos_done);

        Ok(RebuildReportDto {
            scanned: rescan.scanned,
            registered: rescan.registered,
            missing: rescan.missing,
            returned: rescan.returned,
            renamed: rescan.renamed,
            metadata_filled: rescan.metadata_filled,
            photos_count: totals.photos,
            images_count: totals.images,
        })
        })();
        browse.observe_session(&handle, &catalog);
        result
    })
    .await
}

/* ══════════════════════════════════════════════════════════════
 * 库设置：导入模版（M1-6 的「齿轮」）
 * ══════════════════════════════════════════════════════════════ */

/// 库设置里可改的东西（M1 里只有导入模版）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositorySettingsDto {
    /// 库 id。
    pub repository_id: String,
    /// 当前导入模版。
    pub import_template: String,
}

/// 模版预览（**纯函数命令**：不碰库、不碰盘，所以可以在用户打字时随手调）。
///
/// 预览的是「几张示例照片按这个模版会落到哪」—— 包括 `_RAW/` 那条分流规则
/// （它不属于模版本身，但用户看预览时就是想看这个）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplatePreviewDto {
    /// 模版能不能用。
    pub ok: bool,
    /// 不能用时的原因（给用户看的一句话）。
    pub error: Option<String>,
    /// 能用但值得提醒（例如不认识的变量）。
    pub warnings: Vec<String>,
    /// 示例照片的落盘路径（用不了时是空的）。
    pub paths: Vec<String>,
}

/// 一个库当前设置（打不开/离线就报错 —— 设置必须在线改）。
#[tauri::command]
pub async fn repository_settings<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
) -> Result<RepositorySettingsDto, String> {
    let handle = app.clone();
    blocking(move || {
        let catalog = handle
            .state::<crate::browse::BrowseState>()
            .lease(&handle, &repository_id)?;
        let import_template = catalog
            .read(
                |conn| Ok(repository::RepositoryMeta::read(conn, catalog.path())?.import_template),
            )
            .map_err(|e| e.to_string())?;
        Ok(RepositorySettingsDto {
            repository_id: repository_id.clone(),
            import_template,
        })
    })
    .await
}

/// 改一个库的导入模版。
///
/// 两处都要写：库自己的 `catalog.db`（真相源）与 `app.db` 的缓存
/// （离线时界面也要能显示它是什么模版）。
#[tauri::command]
pub async fn repository_set_template<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    template_source: String,
) -> Result<RepositorySettingsDto, String> {
    let handle = app.clone();
    blocking(move || {
        // 先校验：模版坏了就别写进库（否则下次导入才发现）
        template::parse(&template_source).map_err(|e| e.to_string())?;
        let catalog = handle
            .state::<crate::browse::BrowseState>()
            .lease(&handle, &repository_id)?;
        let root = catalog.root().to_path_buf();
        let now = time::now_millis();
        catalog
            .set_import_template(&template_source)
            .map_err(|e| e.to_string())?;
        let mut meta = catalog.meta().clone();
        meta.import_template = template_source.clone();
        drop(catalog);

        let state = handle.state::<DbState>();
        state.with(&handle, |db| {
            db.register_repository(&meta, &root, now)
                .map_err(|e| e.to_string())
        })?;
        Ok(RepositorySettingsDto {
            repository_id,
            import_template: template_source,
        })
    })
    .await
}

/// 模版预览（纯函数：拿几张示例照片渲染一遍）。
#[tauri::command]
#[must_use]
pub fn repository_template_preview(template_source: String) -> TemplatePreviewDto {
    let parsed = match template::parse(&template_source) {
        Ok(parsed) => parsed,
        Err(error) => {
            return TemplatePreviewDto {
                ok: false,
                error: Some(error.to_string()),
                warnings: Vec::new(),
                paths: Vec::new(),
            };
        }
    };
    // 示例照片：同一天的 3 张位图 + 1 张 RAW（后者演示 `_RAW/` 分流）
    let day = time::from_civil(2026, 8, 15, 12, 0, 0).unwrap_or(0);
    let samples = [
        ("P0001", "jpg", 1u64),
        ("P0002", "jpg", 2),
        ("P0003", "JPG", 3),
        ("P0004", "ORF", 4),
    ];
    let mut paths: Vec<String> = Vec::new();
    for (stem, ext, seq) in samples {
        let values = [(3usize, seq)];
        let ctx = raybend::import::template::RenderCtx {
            taken_at: Some(day),
            stem,
            brand: Some("NIKON"),
            model: Some("Z7II"),
            seqs: raybend::import::template::SeqValues::new(&values),
        };
        let rendered = parsed.render(&ctx);
        let (dir, name) = rendered.split_dir_name();
        let dir = dir.map_or(String::from("photos"), |d| format!("photos/{d}"));
        let path = match ext.eq_ignore_ascii_case("ORF") {
            // RAW 有同名位图时进 `_RAW/`（`memory/FUNCTION-REPOSITORY.md` §4.1）
            true => format!("{dir}/_RAW/{name}.{ext}"),
            false => format!("{dir}/{name}.{ext}"),
        };
        paths.push(path);
    }
    TemplatePreviewDto {
        ok: true,
        error: None,
        warnings: parsed.warnings().iter().map(ToString::to_string).collect(),
        paths,
    }
}
