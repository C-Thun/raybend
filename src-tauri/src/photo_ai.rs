//! AI 薄壳：现有 app/catalog 租约、资源路径和事件。推理/事务守卫均在核心库。
use crate::{
    browse::BrowseState,
    db::{DbState, data_dir},
    source::blocking,
};
#[cfg(feature = "photo-ai-runtime")]
use raybend::store::{organization, photo_tags, tags};
use raybend::{ai, store::time};
#[cfg(feature = "photo-ai-runtime")]
use std::sync::Arc;
use std::{
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
#[cfg(feature = "photo-ai-runtime")]
use tauri::Emitter;
use tauri::{AppHandle, Manager, Runtime};
#[derive(Default)]
pub struct PhotoAiState {
    coordinator: ai::coordinator::Coordinator,
    model_operation: Mutex<()>,
    foreground: AtomicBool,
    #[cfg(feature = "photo-ai-runtime")]
    running: AtomicBool,
    #[cfg(feature = "photo-ai-runtime")]
    shutdown: ai::worker::Shutdown,
}
#[derive(Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    state: &'static str,
    manifest_sha256: Option<String>,
    bytes: Option<u64>,
    bundled: bool,
    compiled: bool,
    build_marker: &'static str,
}
fn models<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("ai-models"))
}
#[cfg(feature = "photo-ai-runtime")]
fn library<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let file = if cfg!(windows) {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so.1.28.0"
    };
    Ok(app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("ai-runtime")
        .join(file))
}
fn bundled_model<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("ai-model"))
}
#[cfg(feature = "photo-ai-runtime")]
const BUILD_MARKER: &str = "raybend-photo-ai-build-v1=on";
#[cfg(not(feature = "photo-ai-runtime"))]
const BUILD_MARKER: &str = "raybend-photo-ai-build-v1=off";
impl ModelStatus {
    fn new(state: &'static str, manifest_sha256: Option<String>, bytes: Option<u64>, bundled: bool) -> Self {
        Self {state, manifest_sha256, bytes, bundled, compiled: cfg!(feature = "photo-ai-runtime"), build_marker: BUILD_MARKER}
    }
}
#[tauri::command]
pub async fn photo_ai_model_status<R: Runtime>(app: AppHandle<R>) -> Result<ModelStatus, String> {
    blocking(move || {
        if !cfg!(feature = "photo-ai-runtime") {
            return Ok(ModelStatus::new("disabled", None, None, false));
        }
        let bundled = bundled_model(&app)?.join("image_encoder.onnx").is_file();
        if ai::registry::APPROVED_MANIFESTS.is_empty() {
            return Ok(ModelStatus::new("unpublished", None, None, bundled));
        }
        let model = match ai::registry::selected(&models(&app)?, ai::registry::APPROVED_MANIFESTS) {
            Ok(model) => model,
            Err(_) => return Ok(ModelStatus::new("damaged", None, None, bundled)),
        };
        let Some((hash, pack)) = model else {
            return Ok(ModelStatus::new("missing", None, None, bundled));
        };
        #[cfg(feature = "photo-ai-runtime")]
        let available = library(&app)?.is_file();
        #[cfg(not(feature = "photo-ai-runtime"))]
        let available = false;
        Ok(ModelStatus::new(if available { "ready" } else { "missing" }, Some(hash), Some(pack.manifest.files.iter().map(|f| f.bytes).sum()), bundled))
    }).await
}
#[tauri::command]
pub async fn photo_ai_model_install<R: Runtime>(
    app: AppHandle<R>,
    directory: Option<String>,
) -> Result<(), String> {
    blocking(move || {
        let state = app.state::<PhotoAiState>();
        let _operation = state.model_operation.lock().map_err(|_| "模型操作锁损坏")?;
        #[cfg(feature = "photo-ai-runtime")]
        {
            let busy = app.state::<DbState>().with(&app, |db| {
                db.read(ai::jobs::has_running).map_err(|e| e.to_string())
            })?;
            if busy {
                return Err("模型正在识别，请先暂停任务并等待当前照片完成".into());
            }
            let source = directory.map_or_else(|| bundled_model(&app), |p| Ok(PathBuf::from(p)))?;
            ai::registry::install(
                &models(&app)?,
                &source,
                ai::registry::APPROVED_MANIFESTS,
                |pack| {
                    let lib = library(&app).map_err(raybend::Error::Unsupported)?;
                    let mut worker = ai::worker::Worker::with_shutdown(
                        std::env::current_exe()?,
                        lib,
                        pack.join("image_encoder.onnx"),
                        state.shutdown.clone(),
                    );
                    worker
                        .probe()
                        .map_err(|e| raybend::Error::Unsupported(e.to_string()))
                },
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        }
        #[cfg(not(feature = "photo-ai-runtime"))]
        {
            let _ = directory;
            Err("当前构建不包含 CPU 标签推理运行库".into())
        }
    })
    .await
}
#[tauri::command]
pub async fn photo_ai_model_uninstall<R: Runtime>(app: AppHandle<R>) -> Result<bool, String> {
    blocking(move || {
        let state = app.state::<PhotoAiState>();
        let _operation = state.model_operation.lock().map_err(|_| "模型操作锁损坏")?;
        let busy = app.state::<DbState>().with(&app, |db| {
            db.read(ai::jobs::has_running).map_err(|e| e.to_string())
        })?;
        ai::registry::uninstall(&models(&app)?, ai::registry::APPROVED_MANIFESTS, busy)
            .map_err(|e| e.to_string())
    })
    .await
}
#[tauri::command]
pub async fn photo_ai_tasks<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Vec<ai::status::Task>, String> {
    blocking(move || {
        app.state::<DbState>().with(&app, |db| {
            db.read(ai::status::tasks).map_err(|e| e.to_string())
        })
    })
    .await
}
#[tauri::command]
pub fn photo_ai_foreground<R: Runtime>(app: AppHandle<R>, busy: bool) {
    app.state::<PhotoAiState>()
        .foreground
        .store(busy, Ordering::Relaxed);
}
#[tauri::command]
pub async fn photo_ai_task_action<R: Runtime>(
    app: AppHandle<R>,
    id: i64,
    action: String,
) -> Result<bool, String> {
    blocking(move || {
        if action == "resume" || action == "retry" {
            ready(&app)?;
        }
        if action == "resume" {
            app.state::<DbState>().with(&app, |db| {
                let rows = db.list_repositories().map_err(|e| e.to_string())?;
                db.write(move |c| {
                    for repo in rows {
                        ai::jobs::wake_repository(c, &repo.id, time::now_millis())?;
                    }
                    Ok(())
                })
                .map_err(|e| e.to_string())
            })?;
        }
        let result = app.state::<DbState>().with(&app, |db| {
            app.state::<PhotoAiState>()
                .coordinator
                .action(db, id, &action)
                .map_err(|e| e.to_string())
        })?;
        if action == "resume" && result {
            wake(&app)?;
        }
        Ok(result)
    })
    .await
}
fn ready<R: Runtime>(app: &AppHandle<R>) -> Result<String, String> {
    if ai::registry::APPROVED_MANIFESTS.is_empty() {
        return Err("标签模型尚未通过质量验证，不能提交正式标签".into());
    }
    #[cfg(not(feature = "photo-ai-runtime"))]
    {
        let _ = app;
        Err("此构建未包含 AI 运行库".into())
    }
    #[cfg(feature = "photo-ai-runtime")]
    {
        if !library(app)?.is_file() {
            return Err("缺少应用自带的 CPU 推理运行库".into());
        }
        ai::registry::selected(&models(app)?, ai::registry::APPROVED_MANIFESTS)
            .map_err(|e| e.to_string())?
            .map(|p| p.0)
            .ok_or_else(|| "请先安装标签模型".into())
    }
}
#[tauri::command]
pub async fn photo_ai_start<R: Runtime>(
    app: AppHandle<R>,
    range: ai::range::Range,
    rerun: bool,
    zh: bool,
) -> Result<i64, String> {
    blocking(move || {
        let repositories = range.repositories().map_err(|e| e.to_string())?;
        // 在模型操作锁内登记父任务，卸载据此保护范围准备和在途任务。
        let id = {
            let state = app.state::<PhotoAiState>();
            let _operation = state.model_operation.lock().map_err(|_| "模型操作锁损坏")?;
            let pipeline = ready(&app)?;
            app.state::<DbState>().with(&app, |db| {
                db.write(move |c| {
                    ai::jobs::create_with_options(
                        c,
                        if zh {
                            "照片标签识别"
                        } else {
                            "Photo tagging"
                        },
                        &pipeline,
                        rerun,
                        zh,
                        time::now_millis(),
                    )
                })
                .map_err(|e| e.to_string())
            })?
        };
        let prepare = (|| {
            for repo in repositories {
                app.state::<BrowseState>()
                    .with_catalog(&app, &repo, |catalog| {
                        catalog
                            .read(|c| {
                                ai::range::freeze(c, &repo, &range, |page| {
                                    app.state::<DbState>()
                                        .with(&app, |db| {
                                            db.write(move |c| {
                                                ai::jobs::append_page(
                                                    c,
                                                    id,
                                                    &page,
                                                    time::now_millis(),
                                                )
                                            })
                                            .map_err(|e| e.to_string())
                                        })
                                        .map(|_| ())
                                        .map_err(raybend::Error::Unsupported)
                                })
                            })
                            .map_err(|e| e.to_string())
                    })?;
            }
            app.state::<DbState>().with(&app, |db| {
                db.write(move |c| ai::jobs::finish_prepare(c, id, time::now_millis()))
                    .map_err(|e| e.to_string())
            })
        })();
        if let Err(error) = prepare {
            let message = error.clone();
            app.state::<DbState>().with(&app, |db| {
                db.write(move |c| ai::jobs::fail_prepare(c, id, &message, time::now_millis()))
                    .map_err(|e| e.to_string())
            })?;
            return Err(error);
        }
        wake(&app)?;
        Ok(id)
    })
    .await
}
#[cfg(not(feature = "photo-ai-runtime"))]
fn wake<R: Runtime>(_: &AppHandle<R>) -> Result<(), String> {
    Err("此构建未包含 AI 运行库".into())
}
#[cfg(feature = "photo-ai-runtime")]
fn wake<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    if app
        .state::<PhotoAiState>()
        .running
        .swap(true, Ordering::AcqRel)
    {
        return Ok(());
    }
    let handle = app.clone();
    std::thread::Builder::new()
        .name("photo-ai".into())
        .spawn(move || {
            use ai::service::Engine;
            let result = (|| -> Result<(), String> {
                let mut engine = ai::service::RuntimeEngine::with_shutdown(
                    std::env::current_exe().map_err(|e| e.to_string())?,
                    library(&handle)?,
                    handle.state::<PhotoAiState>().shutdown.clone(),
                );
                let host = HostAdapter {
                    app: handle.clone(),
                    cached: Mutex::new(None),
                    active_permit: Mutex::new(None),
                };
                let mut idle = std::time::Instant::now();
                while !handle.state::<PhotoAiState>().shutdown.stopped() {
                    let outcome = ai::service::step(&host, &mut engine);
                    host.release_permit();
                    match outcome {
                        Ok(true) => idle = std::time::Instant::now(),
                        Ok(false) => {
                            if idle.elapsed().as_secs() >= 30 {
                                engine.unload();
                            }
                            std::thread::sleep(std::time::Duration::from_millis(500));
                        }
                        Err(error) => return Err(error.to_string()),
                    }
                }
                Ok(())
            })();
            handle
                .state::<PhotoAiState>()
                .running
                .store(false, Ordering::Release);
            if let Err(error) = result {
                eprintln!("[photo-ai] {error}");
            }
        })
        .map_err(|e| {
            app.state::<PhotoAiState>()
                .running
                .store(false, Ordering::Release);
            e.to_string()
        })?;
    Ok(())
}
#[cfg(feature = "photo-ai-runtime")]
struct HostAdapter<R: Runtime> {
    app: AppHandle<R>,
    cached: Mutex<Option<(String, Arc<ai::pack::Verified>)>>,
    active_permit: Mutex<Option<raybend::store::session::TaskPermit>>,
}
#[cfg(feature = "photo-ai-runtime")]
fn core(error: String) -> raybend::Error {
    raybend::Error::Unsupported(error)
}
#[cfg(feature = "photo-ai-runtime")]
impl<R: Runtime> HostAdapter<R> {
    fn release_permit(&self) {
        if let Ok(mut slot) = self.active_permit.lock() {
            slot.take();
        }
    }
    fn changed(&self, repo: &str, asset_id: i64, delta: tags::TagDelta) {
        if !delta.is_empty() {
            if let Err(error) = self.app.state::<DbState>().with(&self.app, |db| {
                db.write(move |c| tags::apply_delta(c, &delta))
                    .map_err(|e| e.to_string())
            }) {
                eprintln!("[photo-ai] 标签排序计数：{error}");
            }
        }
        crate::sidecar::queue_sync(self.app.clone(), repo, vec![asset_id]);
        let root = crate::browse::resolve_root(&self.app, repo).ok();
        let _=self.app.emit("catalog://changed",serde_json::json!({"repositoryId":repo,"scopePath":"photos","assetIds":[asset_id],"root":root.map(|p|p.to_string_lossy().into_owned()).unwrap_or_default(),"relativePaths":[]}));
    }
}
#[cfg(feature = "photo-ai-runtime")]
impl<R: Runtime> ai::service::Host for HostAdapter<R> {
    fn foreground_busy(&self) -> bool {
        self.app
            .state::<PhotoAiState>()
            .foreground
            .load(Ordering::Relaxed)
    }
    fn claim(&self) -> raybend::Result<Option<(ai::jobs::Claim, ai::jobs::Run)>> {
        let state = self.app.state::<PhotoAiState>();
        let _operation = state
            .model_operation
            .lock()
            .map_err(|_| core("模型操作锁损坏".into()))?;
        self.app
            .state::<DbState>()
            .with(&self.app, |db| {
                db.write(|c| {
                    let Some(claim) = ai::jobs::claim_next(c, time::now_millis())? else {
                        return Ok(None);
                    };
                    let run = ai::jobs::context(c, claim.item.run_id)?;
                    Ok(Some((claim, run)))
                })
                .map_err(|e| e.to_string())
            })
            .map_err(core)
    }
    fn model(&self, pipeline: &str) -> raybend::Result<Arc<ai::pack::Verified>> {
        let mut cache = self
            .cached
            .lock()
            .map_err(|_| core("模型状态锁损坏".into()))?;
        if let Some((hash, pack)) = cache.as_ref() {
            if hash == pipeline {
                return Ok(pack.clone());
            }
        }
        if !ai::registry::APPROVED_MANIFESTS.contains(&pipeline) {
            return Err(core("任务模型不在可信清单内，请重新创建任务".into()));
        }
        let pack = Arc::new(ai::pack::verify(
            &models(&self.app).map_err(core)?.join(pipeline),
            pipeline,
            true,
        )?);
        *cache = Some((pipeline.into(), pack.clone()));
        Ok(pack)
    }
    fn prepare(
        &self,
        claim: &ai::jobs::Claim,
        run: &ai::jobs::Run,
    ) -> raybend::Result<ai::service::Availability> {
        let repo = &claim.item.photo.repository_id;
        let catalog = match self.app.state::<BrowseState>().lease(&self.app, repo) {
            Ok(c) => c,
            Err(_) => return Ok(ai::service::Availability::Offline),
        };
        // 复用现有路径任务租约，原图读取与推理期间阻止释放库；一张结束后归还。
        let permit = match self
            .app
            .state::<BrowseState>()
            .path_task(&self.app, &catalog.root().join("photos"))
        {
            Ok(Some(p)) => p,
            _ => return Ok(ai::service::Availability::Offline),
        };
        *self
            .active_permit
            .lock()
            .map_err(|_| core("AI 任务租约锁损坏".into()))? = Some(permit);
        let pack = self.model(&run.pipeline_sha256)?;
        let root = catalog.root().to_path_buf();
        let photo = claim.item.photo.clone();
        let pipeline = run.pipeline_sha256.clone();
        let rerun = run.rerun;
        let zh = run.zh;
        let (prepared, names, delta, invalidated) = catalog.write_tx(move |c| {
            let before = photo_tags::effective_ids(c, photo.asset_id)?;
            let valid = photo_tags::snapshot(c, photo.asset_id)?
                .result
                .is_some_and(|r| r.valid);
            let prepared = ai::session::prepare(
                c,
                &root,
                photo.clone(),
                &pipeline,
                rerun,
                time::now_millis(),
            )?;
            let names = ai::result::labels(c, &pack, zh)?;
            let after = photo_tags::effective_ids(c, photo.asset_id)?;
            let invalidated = valid
                && !photo_tags::snapshot(c, photo.asset_id)?
                    .result
                    .is_some_and(|r| r.valid);
            Ok((
                prepared,
                names,
                tags::projected_delta(&before, &after),
                invalidated,
            ))
        })?;
        let dictionary = self
            .app
            .state::<DbState>()
            .with(&self.app, |db| {
                db.write(move |c| tags::ensure_names(c, &names, time::now_millis()))
                    .map_err(|e| e.to_string())
            })
            .map_err(core)?;
        catalog.write(move |c| organization::sync_legacy_terms(c, &dictionary))?;
        if invalidated {
            self.changed(repo, claim.item.photo.asset_id, delta);
        }
        Ok(ai::service::Availability::Photo(prepared))
    }
    fn finish(
        &self,
        claim: &ai::jobs::Claim,
        run: &ai::jobs::Run,
        lease: Option<&ai::session::Lease>,
        pack: &ai::pack::Verified,
        features: &[f32],
    ) -> raybend::Result<bool> {
        if self.app.state::<PhotoAiState>().shutdown.stopped() {
            return Ok(false);
        }
        self.app
            .state::<DbState>()
            .with(&self.app, |db| {
                self.app
                    .state::<PhotoAiState>()
                    .coordinator
                    .finish_as(db, claim, lease.is_none(), || {
                        let Some(lease) = lease else {
                            return Ok(true);
                        };
                        let lease = lease.clone();
                        let repo = lease.photo.repository_id.clone();
                        let asset = lease.photo.asset_id;
                        let pack = pack.clone();
                        let features = features.to_vec();
                        let pipeline = run.pipeline_sha256.clone();
                        let zh = run.zh;
                        let (committed, delta, invalidated) = self
                            .app
                            .state::<BrowseState>()
                            .with_catalog(&self.app, &repo, |catalog| {
                                let root = catalog.root().to_path_buf();
                                catalog
                                    .write_tx(move |c| {
                                        let before = photo_tags::effective_ids(c, asset)?;
                                        let old_valid = photo_tags::snapshot(c, asset)?
                                            .result
                                            .is_some_and(|r| r.valid);
                                        let committed = ai::session::commit(
                                            c,
                                            &root,
                                            &lease,
                                            &pack,
                                            &features,
                                            &pipeline,
                                            zh,
                                            time::now_millis(),
                                        )?;
                                        let after = photo_tags::effective_ids(c, asset)?;
                                        Ok((
                                            committed,
                                            tags::projected_delta(&before, &after),
                                            old_valid
                                                && !photo_tags::snapshot(c, asset)?
                                                    .result
                                                    .is_some_and(|r| r.valid),
                                        ))
                                    })
                                    .map_err(|e| e.to_string())
                            })
                            .map_err(core)?;
                        if committed || invalidated {
                            self.changed(&repo, asset, delta);
                        }
                        Ok(committed)
                    })
                    .map_err(|e| e.to_string())
            })
            .map_err(core)
    }
    fn discard(&self, lease: &ai::session::Lease) -> raybend::Result<()> {
        let lease = lease.clone();
        let repo = lease.photo.repository_id.clone();
        if let Err(error) = self
            .app
            .state::<BrowseState>()
            .with_catalog(&self.app, &repo, |db| {
                db.write(move |c| ai::session::discard(c, &lease))
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            })
        {
            eprintln!("[photo-ai] 原库暂不可用，旧 attempt 将由下次识别覆盖：{error}");
        }
        Ok(())
    }
    fn offline(&self, claim: &ai::jobs::Claim) -> raybend::Result<()> {
        let claim = claim.clone();
        self.app
            .state::<DbState>()
            .with(&self.app, |db| {
                db.write(move |c| ai::jobs::wait_offline(c, &claim, time::now_millis()))
                    .map_err(|e| e.to_string())
            })
            .map_err(core)
    }
    fn fail(&self, claim: &ai::jobs::Claim, error: &str) -> raybend::Result<()> {
        let claim = claim.clone();
        let error = error.to_owned();
        self.app
            .state::<DbState>()
            .with(&self.app, |db| {
                db.write(move |c| ai::jobs::fail(c, &claim, &error, time::now_millis()))
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            })
            .map_err(core)
    }
}

pub fn shutdown<R: Runtime>(app: &AppHandle<R>) {
    #[cfg(feature = "photo-ai-runtime")]
    app.state::<PhotoAiState>().shutdown.stop();
    #[cfg(not(feature = "photo-ai-runtime"))]
    let _ = app;
}

#[cfg(test)]
mod capability_tests {
    use super::*;
    #[test]
    fn model_status_exposes_native_build_capability() {
        let status = ModelStatus::new("disabled", None, None, false);
        assert_eq!(status.compiled, cfg!(feature = "photo-ai-runtime"));
        assert_eq!(status.build_marker.ends_with("=on"), status.compiled);
        let serialized = serde_json::to_value(status).unwrap();
        assert_eq!(serialized["compiled"], cfg!(feature = "photo-ai-runtime"));
        assert_eq!(serialized["buildMarker"], BUILD_MARKER);
    }
}
