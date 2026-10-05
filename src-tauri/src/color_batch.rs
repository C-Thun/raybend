//! Thin review/commit adapter. No image math or independent history.
use raybend::store::{
    color_batch::{self, PreparedChange},
    db::CatalogDb,
    develop, marking,
};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};
use tauri::{AppHandle, Emitter, Manager, Runtime};

struct Entry {
    change: PreparedChange,
    path: PathBuf,
    signature: String,
}
struct Plan {
    token: String,
    created: Instant,
    repository_id: String,
    catalog: Arc<CatalogDb>,
    entries: Vec<Entry>,
}
#[derive(Default)]
pub struct ColorBatchState {
    plan: Mutex<Option<Plan>>,
    revision: AtomicU64,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub token: String,
    pub selected: usize,
    pub applicable: usize,
    pub existing: usize,
    pub skipped: Vec<String>,
    pub profile_name: Option<String>,
}

#[tauri::command]
pub async fn color_batch_review<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    asset_ids: Vec<i64>,
    profile_id: Option<String>,
) -> Result<Review, String> {
    let gate = app.state::<ColorBatchState>();
    let revision = gate.revision.fetch_add(1, Ordering::AcqRel) + 1;
    *gate.plan.lock().map_err(|_| "色彩预检状态不可用")? = None;
    crate::source::blocking(move || {
        let ids = color_batch::selection(&asset_ids).map_err(|e| e.to_string())?;
        let catalog = app
            .state::<crate::browse::BrowseState>()
            .lease(&app, &repository_id)?;
        let defaults = app.state::<crate::db::DbState>().with(&app, |db| {
            db.read(raybend::color::defaults::load)
                .map_err(|e| e.to_string())
        })?;
        let profile = profile_id
            .map(|id| {
                raybend::color::ProfileId::try_from(id)
                    .map_err(|e| e.to_string())
                    .and_then(|id| {
                        crate::color_profiles::resolve(
                            &app,
                            &id,
                            raybend::color::icc::IccRole::PhotoInput,
                        )
                        .map_err(|e| e.to_string())
                    })
            })
            .transpose()?;
        let profile_name = profile
            .as_ref()
            .map(|profile| {
                crate::color_profiles::library(&app).map(|library| {
                    library
                        .entries
                        .into_iter()
                        .find(|entry| entry.profile_id == profile.id().as_str())
                        .map_or_else(|| profile.id().as_str().to_owned(), |entry| entry.name)
                })
            })
            .transpose()?;
        let mut entries = Vec::new();
        let mut skipped = Vec::new();
        let mut existing = 0;
        for id in &ids {
            if app
                .state::<ColorBatchState>()
                .revision
                .load(Ordering::Acquire)
                != revision
            {
                return Err("色彩预检已被新请求取代".into());
            }
            let candidate = catalog
                .read(|conn| {
                    let lock: i64 = conn.query_row(
                        "SELECT lock_level FROM assets WHERE id=?1",
                        [*id],
                        |row| row.get(0),
                    )?;
                    let stack = develop::load(conn, *id)?;
                    Ok((
                        lock,
                        develop::edit_target(conn, *id, stack.source_base)?,
                        stack,
                    ))
                })
                .map_err(|e| e.to_string());
            let result = (|| -> Result<Entry, String> {
                let (lock, path, before) = candidate?;
                if lock >= i64::from(marking::LOCK_NO_EDIT) {
                    return Err("照片已锁定".into());
                }
                let path = catalog.root().join(
                    path.ok_or("原文件不可用")?
                        .replace('/', std::path::MAIN_SEPARATOR_STR),
                );
                let _permit = app
                    .state::<crate::browse::BrowseState>()
                    .path_task(&app, &path)?;
                let signature =
                    raybend::media::source::source_signature(&path).map_err(|e| e.to_string())?;
                let after =
                    raybend::color::input::prepare_source(&path, &defaults, profile.as_ref())
                        .map_err(|e| e.to_string())?;
                if signature
                    != raybend::media::source::source_signature(&path).map_err(|e| e.to_string())?
                {
                    return Err("原文件在预检时改变".into());
                }
                Ok(Entry {
                    change: PreparedChange {
                        asset_id: *id,
                        before,
                        after,
                    },
                    path,
                    signature,
                })
            })();
            match result {
                Ok(entry) => {
                    existing += usize::from(entry.change.before.color.is_some());
                    entries.push(entry);
                }
                Err(error) => skipped.push(format!("#{id} · {error}")),
            }
        }
        catalog.ensure_current().map_err(|e| e.to_string())?;
        let token = revision.to_string();
        let review = Review {
            token: token.clone(),
            selected: ids.len(),
            applicable: entries.len(),
            existing,
            skipped,
            profile_name,
        };
        let gate = app.state::<ColorBatchState>();
        let mut slot = gate.plan.lock().map_err(|_| "色彩预检状态不可用")?;
        if gate.revision.load(Ordering::Acquire) != revision {
            return Err("色彩预检已被新请求取代".into());
        }
        *slot = Some(Plan {
            token,
            created: Instant::now(),
            repository_id,
            catalog,
            entries,
        });
        Ok(review)
    })
    .await
}

#[tauri::command]
pub async fn color_batch_commit<R: Runtime>(
    app: AppHandle<R>,
    token: String,
) -> Result<usize, String> {
    let plan = {
        let gate = app.state::<ColorBatchState>();
        let mut slot = gate.plan.lock().map_err(|_| "色彩预检状态不可用")?;
        if slot.as_ref().is_none_or(|plan| plan.token != token) {
            return Err("色彩预检已失效，请重新预检".into());
        }
        slot.take().unwrap()
    };
    crate::source::blocking(move || {
        if plan.created.elapsed()>std::time::Duration::from_secs(600) {return Err("色彩预检已过期，请重新预检".into());}
        plan.catalog.ensure_current().map_err(|e|e.to_string())?;
        let mut permits=Vec::new();
        for entry in &plan.entries {
            permits.push(app.state::<crate::browse::BrowseState>().path_task(&app,&entry.path)?);
            if entry.signature!=raybend::media::source::source_signature(&entry.path).map_err(|e|e.to_string())? {return Err("预检后原文件已改变，请重新预检".into());}
        }
        let entries=plan.entries.iter().map(|entry|entry.change.clone()).collect::<Vec<_>>();
        let change=plan.catalog.write_tx(move |tx|color_batch::apply_reviewed(tx,&entries)).map_err(|e|e.to_string())?;
        let ids=change.ops.iter().map(marking::Op::asset_id).collect::<Vec<_>>();
        if !ids.is_empty() {
            app.state::<crate::browse::BrowseState>().with_undo(&plan.repository_id,|undo|undo.push(change))?;
            if let Ok(cache)=raybend::display::FullCache::open(plan.catalog.root()) {for id in &ids {cache.invalidate(*id);}}
            crate::sidecar::queue_sync(app.clone(),&plan.repository_id,ids.clone());
            let relative_paths=plan.entries.iter().filter(|entry|ids.contains(&entry.change.asset_id)).filter_map(|entry|entry.path.strip_prefix(plan.catalog.root()).ok().map(|path|path.to_string_lossy().replace('\\',"/"))).collect::<Vec<_>>();
            if let Err(error)=app.emit("catalog://changed",serde_json::json!({"repositoryId":plan.repository_id,"scopePath":"","assetIds":ids,"root":plan.catalog.root().to_string_lossy(),"relativePaths":relative_paths})) {eprintln!("[color] notify: {error}");}
        }
        Ok(ids.len())
    }).await
}
