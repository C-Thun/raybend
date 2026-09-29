//! 导出准备命令；核心校验与取稿在 raybend::export，外壳仅转发和复用预览生成。
use crate::browse::BrowseState;
use raybend::export::{AssetVariants, Preset, PresetValidation, VariantRef, VariantSnapshot};
use tauri::{AppHandle, Manager, Runtime};

#[tauri::command]
pub async fn export_variants<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    asset_ids: Vec<i64>,
) -> Result<Vec<AssetVariants>, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        handle
            .state::<BrowseState>()
            .with_catalog(&handle, &repository_id, |db| {
                db.read(|conn| raybend::export::summaries(conn, &asset_ids))
                    .map_err(|e| e.to_string())
            })
    })
    .await
}
#[tauri::command]
pub async fn export_snapshots<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    references: Vec<VariantRef>,
) -> Result<Vec<VariantSnapshot>, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        raybend::export::validate_batch(&references.iter().map(|r| r.asset_id).collect::<Vec<_>>())
            .map_err(|e| e.to_string())?;
        handle
            .state::<BrowseState>()
            .with_catalog(&handle, &repository_id, |db| {
                let root = db.root();
                db.read(|conn| {
                    references
                        .iter()
                        .map(|r| raybend::export::snapshot(conn, root, r))
                        .collect()
                })
                .map_err(|e| e.to_string())
            })
    })
    .await
}
#[tauri::command]
pub async fn export_preset_validate(preset: Preset) -> Result<PresetValidation, String> {
    crate::source::blocking(move || Ok(preset.validate(true))).await
}
#[tauri::command]
pub async fn export_variant_image<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    reference: VariantRef,
    size: String,
    captured: Option<VariantSnapshot>,
    raw_original: Option<bool>,
) -> Result<tauri::ipc::Response, String> {
    let handle = app.clone();
    let bytes = crate::source::blocking(move || {
        let class = raybend::thumbnail::SizeClass::parse(&size).ok_or("不支持此预览尺寸")?;
        if !matches!(
            class,
            raybend::thumbnail::SizeClass::Grid
                | raybend::thumbnail::SizeClass::Strip
                | raybend::thumbnail::SizeClass::Screen
        ) {
            return Err("导出只提供显示档预览".into());
        }
        let browse = handle.state::<BrowseState>();
        let _permit = browse
            .sessions
            .begin_task(&repository_id)
            .map_err(|e| e.to_string())?;
        let catalog = browse.lease(&handle, &repository_id)?;
        let result = (|| {
            let root = catalog.root().to_path_buf();
            let (snap, rel_path) = catalog
                .read(|conn| match captured {
                    Some(ref saved) => {
                        raybend::export::validate_captured(conn, &root, &reference, saved)?;
                        let path = raybend::export::resolve_captured_source(conn, &root, saved)?;
                        let rel = path
                            .strip_prefix(root.canonicalize()?)
                            .map_err(|_| {
                                raybend::Error::Unsupported("导出源位于照片库之外".into())
                            })?
                            .to_string_lossy()
                            .replace('\\', "/");
                        Ok((saved.clone(), rel))
                    }
                    None => {
                        let snap = raybend::export::snapshot(conn, &root, &reference)?;
                        let rel = snap.rel_path.clone();
                        Ok((snap, rel))
                    }
                })
                .map_err(|e| e.to_string())?;
            let asset = crate::develop::ResolvedAsset {
                repository_id: repository_id.clone(),
                asset_id: reference.asset_id,
                root,
                rel_path,
            };
            catalog.ensure_current().map_err(|e| e.to_string())?;
            if let Some(issue_id) = reference.variant.strip_prefix("issue:").and_then(|id| id.parse::<i64>().ok()) {
                let bytes = if class == raybend::thumbnail::SizeClass::Screen {
                    crate::issues::preview_bytes(&handle, &repository_id, reference.asset_id, issue_id)?
                } else {
                    crate::issues::thumb_bytes(&handle, &repository_id, reference.asset_id, issue_id, class)?
                };
                catalog.ensure_current().map_err(|e| e.to_string())?;
                return Ok(bytes);
            }
            if reference.variant == "raw" && class != raybend::thumbnail::SizeClass::Screen {
                if raw_original != Some(false)
                    && let Some(bytes) = crate::thumbs::raw_original_thumb(&handle, &asset, class)? { return Ok(bytes); }
                if raw_original == Some(true) { return Ok(Vec::new()); }
            }
            // captured latest 可能早于当前栈：仅 hash 相同才允许复用 latest preview。
            let current_hash = catalog.read(|conn| {
                let current = raybend::store::develop::load(conn, reference.asset_id)?;
                raybend::store::issues::profile_hash(&current)
            }).map_err(|e| e.to_string())?;
            let preview_variant = if reference.variant == "latest" && current_hash == snap.profile_hash {
                "latest".to_string()
            } else if reference.variant == "sooc" { "sooc".to_string() }
            else { format!("export-{}-{}", reference.variant, snap.profile_hash) };
            let bytes = if class == raybend::thumbnail::SizeClass::Screen {
                crate::thumbs::render_profile_cached(&handle, &asset, &snap.stack, &preview_variant)?
            } else {
                let db = handle.state::<crate::thumbs::SourcesThumbs>().get(
                    &crate::thumbs::sources_cache_dir(&handle)?, raybend::store::time::now_millis())?;
                let source = crate::develop::source_path_of(&handle, &asset, snap.stack.source_base)
                    .ok_or("定稿源文件不可用")?;
                crate::thumbs::render_profile_thumb(&handle, &db, &asset, &source, &snap.stack,
                    class, Some(&preview_variant)).map_err(|e| e.to_string())?
            };
            catalog.ensure_current().map_err(|e| e.to_string())?;
            Ok(bytes)
        })();
        browse.observe_session(&handle, &catalog);
        result
    })
    .await?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[derive(serde::Serialize)]
pub struct VariantDetails {
    width: u32,
    height: u32,
    histogram: crate::thumbs::HistogramDto,
}
#[tauri::command]
pub async fn export_variant_details<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    captured: VariantSnapshot,
) -> Result<VariantDetails, String> {
    crate::source::blocking(move || {
        let prepared = prepare(&app, &repository_id, &captured)?.ok_or("定稿已变更或删除")?;
        let (lens, lut) = resources(&app, &repository_id, &captured)?;
        let image = raybend::export::render_captured(
            &prepared.source,
            &captured,
            lens.as_ref(),
            lut.as_deref(),
            0,
        )
        .map_err(|e| e.to_string())?;
        let (width, height) = image.dimensions();
        let rgb = image::DynamicImage::ImageRgb16(image)
            .thumbnail(512, 512)
            .to_rgb8();
        Ok(VariantDetails {
            width,
            height,
            histogram: raybend::display::histogram::histogram_of_image(&rgb, 86).into(),
        })
    })
    .await
}

/// Single-file smoke/API entry; W5 will drive this core from independent preset workers.
#[tauri::command]
pub async fn export_tiff16<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    captured: VariantSnapshot,
    target: String,
    max_edge: u32,
) -> Result<(u32, u32), String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        let _permit = handle
            .state::<BrowseState>()
            .sessions
            .begin_task(&repository_id)
            .map_err(|e| e.to_string())?;
        let catalog = handle
            .state::<BrowseState>()
            .lease(&handle, &repository_id)?;
        let source = catalog
            .read(|conn| raybend::export::resolve_captured_source(conn, catalog.root(), &captured))
            .map_err(|e| e.to_string())?;
        let (lens, lut) = resources(&handle, &repository_id, &captured)?;
        raybend::export::write_tiff16_checked(
            &source,
            std::path::Path::new(&target),
            &captured,
            lens.as_ref(),
            lut.as_deref(),
            max_edge,
            || catalog.ensure_current(),
        )
        .map_err(|e| e.to_string())
    })
    .await
}

#[derive(Default)]
pub struct ExportState {
    engine: std::sync::Mutex<Option<raybend::export::jobs::Engine>>,
    leases: std::sync::Arc<
        std::sync::Mutex<
            std::collections::HashMap<
                (String, String),
                std::sync::Arc<raybend::store::session::TaskCatalog>,
            >,
        >,
    >,
    targets: std::sync::Arc<
        std::sync::Mutex<
            std::collections::HashMap<
                (String, String),
                std::sync::Arc<raybend::store::session::SourceIdentity>,
            >,
        >,
    >,
}
impl ExportState {
    pub(crate) fn suspend_repository(&self, id: &str) -> Result<(), String> {
        if let Some(engine) = self.engine.lock().map_err(|_| "内部锁已损坏")?.as_ref() {
            engine.suspend_repository(id)?;
        }
        Ok(())
    }
    pub(crate) fn unfinished_repository(&self, id: &str) -> bool {
        self.engine.lock().unwrap().as_ref().is_some_and(|engine| {
            engine.view().queues.values().flatten().any(|item| {
                item.repository_id == id
                    && matches!(item.status.as_str(), "pending" | "running" | "waiting")
            })
        })
    }
    pub(crate) fn has_unfinished(&self) -> Result<bool, String> {
        Ok(self
            .engine
            .lock()
            .map_err(|_| "内部锁已损坏")?
            .as_ref()
            .is_some_and(|engine| {
                let view = engine.view();
                !view.enabled.is_empty()
                    || view
                        .queues
                        .values()
                        .flatten()
                        .any(|item| item.status == "running")
            }))
    }
    fn engine<R: Runtime>(&self, app: &AppHandle<R>) -> raybend::export::jobs::Engine {
        let mut guard = self.engine.lock().unwrap();
        guard
            .get_or_insert_with(|| {
                let worker = app.clone();
                let events = app.clone();
                // 同一队列代次/库在还有任务时固定租约。掉线后的后续项目
                // 使用已作废租约失败，不静默切同 ID 副本；显式重试可开新租约。
                let leases = std::sync::Arc::clone(&self.leases);
                let targets = std::sync::Arc::clone(&self.targets);
                let task_leases = std::sync::Arc::clone(&leases);
                let cleanup_targets = std::sync::Arc::clone(&targets);
                raybend::export::jobs::Engine::new(
                    move |item| {
                        let key = (
                            item.id.split(':').next().unwrap_or_default().to_string(),
                            item.repository_id.clone(),
                        );
                        let existing = task_leases.lock().unwrap().get(&key).cloned();
                        use raybend::export::jobs::Outcome;
                        let browse = worker.state::<BrowseState>();
                        let _permit = match browse.sessions.begin_task(&item.repository_id) {
                            Ok(permit) => permit,
                            Err(_) => return Err("storage.released".into()),
                        };
                        let Some(task) = existing else {
                            return Err("导出任务缺少原库身份，请重新入队".into());
                        };
                        let mut catalog = task.current();
                        if catalog.ensure_current().is_err() {
                            let browse = worker.state::<BrowseState>();
                            browse.observe_session(&worker, &catalog);
                            if !browse
                                .lease(&worker, &item.repository_id)
                                .is_ok_and(|next| task.install(next).is_ok())
                            {
                                return Ok(Outcome::Waiting("storage.wait.repository".into()));
                            }
                            catalog = task.current();
                        }
                        let target_key = (key.0.clone(), item.preset.directory.clone());
                        let target = targets
                            .lock()
                            .unwrap()
                            .get(&target_key)
                            .cloned()
                            .ok_or("导出目标缺少设备身份")?;
                        if !target.ready() {
                            return Ok(Outcome::Waiting("storage.wait.target".into()));
                        }
                        let result = run_item(&worker, item, &catalog, &target);
                        worker
                            .state::<BrowseState>()
                            .observe_session(&worker, &catalog);
                        if result.is_ok() {
                            return result;
                        }
                        if catalog.ensure_current().is_err() {
                            return Ok(Outcome::Waiting("storage.wait.repository".into()));
                        }
                        if !target.ready() {
                            return Ok(Outcome::Waiting("storage.wait.target".into()));
                        }
                        result
                    },
                    move |view| {
                        use tauri::Emitter;
                        let active: std::collections::HashSet<_> = view
                            .queues
                            .values()
                            .flatten()
                            .filter(|item| {
                                matches!(item.status.as_str(), "pending" | "running" | "waiting")
                            })
                            .map(|item| {
                                (
                                    item.id.split(':').next().unwrap_or_default().to_string(),
                                    item.repository_id.clone(),
                                )
                            })
                            .collect();
                        let retired = {
                            let mut guard = leases.lock().unwrap();
                            let keys: Vec<_> = guard
                                .keys()
                                .filter(|key| !active.contains(*key))
                                .cloned()
                                .collect();
                            keys.into_iter()
                                .filter_map(|key| guard.remove(&key))
                                .collect::<Vec<_>>()
                        };
                        drop(retired);
                        let active_targets: std::collections::HashSet<_> = view
                            .queues
                            .values()
                            .flatten()
                            .filter(|item| {
                                matches!(item.status.as_str(), "pending" | "running" | "waiting")
                            })
                            .map(|item| {
                                (view.generation.to_string(), item.preset.directory.clone())
                            })
                            .collect();
                        cleanup_targets
                            .lock()
                            .unwrap()
                            .retain(|key, _| active_targets.contains(key));
                        let _ = events.emit("export://state", view);
                    },
                )
            })
            .clone()
    }
}
pub(crate) struct Prepared {
    _permit: Option<raybend::store::session::TaskPermit>,
    pub(crate) catalog: std::sync::Arc<raybend::store::db::CatalogDb>,
    pub(crate) source: std::path::PathBuf,
    naming: raybend::export::output::Naming,
    author: Option<String>,
    description: Option<String>,
    tags: Vec<i64>,
    /// 导出尾号（I00–I99 / ISO / IRA / ILA）；latest 在这里就完成哈希匹配判定
    pub(crate) suffix: String,
}
pub(crate) fn prepare<R: Runtime>(
    app: &AppHandle<R>,
    repository: &str,
    captured: &VariantSnapshot,
) -> Result<Option<Prepared>, String> {
    let permit = app
        .state::<BrowseState>()
        .sessions
        .begin_task(repository)
        .map_err(|e| e.to_string())?;
    let catalog = app.state::<BrowseState>().lease(app, repository)?;
    let mut prepared = prepare_with_catalog(catalog, captured)?;
    if let Some(prepared) = &mut prepared {
        prepared._permit = Some(permit);
    }
    Ok(prepared)
}
fn prepare_with_catalog(
    catalog: std::sync::Arc<raybend::store::db::CatalogDb>,
    captured: &VariantSnapshot,
) -> Result<Option<Prepared>, String> {
    let root = catalog.root().to_path_buf();
    catalog.read(|conn| {
        if !raybend::export::still_current(conn,&root,captured)? { return Ok(None); }
        let source=raybend::export::resolve_captured_source(conn,&root,captured)?;
        let suffix=raybend::export::issue_suffix(conn,&captured.reference)?;
        let(taken_at,brand,model,author,description)=conn.query_row("SELECT taken_at,camera_make,camera_model,author,description FROM assets WHERE id=?1",[captured.reference.asset_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)))?;
        Ok(Some(Prepared{_permit:None,catalog:std::sync::Arc::clone(&catalog),source,naming:raybend::export::output::Naming{taken_at,brand,model},author,description,tags:raybend::store::tags::tags_of_asset(conn,captured.reference.asset_id)?,suffix}))
    }).map_err(|e|e.to_string())
}

pub(crate) fn resources<R: Runtime>(
    app: &AppHandle<R>,
    repository: &str,
    captured: &VariantSnapshot,
) -> Result<
    (
        Option<raybend::develop::lens::LensCorrection>,
        Option<std::sync::Arc<raybend::develop::lut::Lut>>,
    ),
    String,
> {
    let stack = &captured.stack;
    let lens = crate::lens::render_correction(
        app,
        repository,
        captured.reference.asset_id,
        stack.lens_profile.as_deref(),
        stack.lens_enabled,
    );
    if stack.lens_enabled != Some(false)
        && stack.lens_profile.as_deref().is_some_and(|p| p != "none")
        && lens.is_none()
    {
        return Err("导出定稿引用的镜头配置不可用".into());
    }
    let lut = stack
        .lut_id
        .as_deref()
        .filter(|_| stack.lut_enabled == Some(true))
        .map(|id| crate::lut::resolve(app, id))
        .transpose()?;
    Ok((lens, lut))
}
pub(crate) fn metadata_for<R: Runtime>(
    app: &AppHandle<R>,
    prepared: &Prepared,
) -> Result<raybend::export::metadata::Metadata, String> {
    use raybend::export::metadata::Metadata;
    let mut metadata = Metadata::read(&prepared.source).map_err(|e| e.to_string())?;
    if prepared.author.is_some() {
        metadata.author = prepared.author.clone();
    }
    if prepared.description.is_some() {
        metadata.description = prepared.description.clone();
    }
    let catalog_keywords: Vec<String> = app.state::<crate::db::DbState>().with(app, |db| {
        db.read(|conn| {
            let tags = raybend::store::tags::list_all(conn)?;
            Ok(tags
                .into_iter()
                .filter(|tag| prepared.tags.contains(&tag.id))
                .map(|tag| tag.name)
                .collect())
        })
        .map_err(|e| e.to_string())
    })?;
    metadata.keywords.extend(catalog_keywords);
    metadata.keywords.sort();
    metadata.keywords.dedup();
    Ok(metadata)
}
fn run_item<R: Runtime>(
    app: &AppHandle<R>,
    item: &raybend::export::jobs::Item,
    catalog: &std::sync::Arc<raybend::store::db::CatalogDb>,
    target_identity: &raybend::store::session::SourceIdentity,
) -> Result<raybend::export::jobs::Outcome, String> {
    use raybend::export::jobs::Outcome;
    let Some(prepared) = prepare_with_catalog(std::sync::Arc::clone(catalog), &item.snapshot)?
    else {
        return Ok(Outcome::Skipped);
    };
    if let Some(path) = raybend::export::output::skip_existing(
        &prepared.source,
        &item.preset,
        &prepared.naming,
        item.sequence,
        &prepared.suffix,
    )
    .map_err(|e| e.to_string())?
    {
        return Ok(Outcome::FileExists(path.to_string_lossy().into_owned()));
    }
    let metadata = metadata_for(app, &prepared)?;
    let (lens, lut) = resources(app, &item.repository_id, &item.snapshot)?;
    let target = raybend::export::output::execute_checked(
        &prepared.source,
        &item.snapshot,
        &item.preset,
        &prepared.naming,
        &metadata,
        item.sequence,
        lens.as_ref(),
        lut.as_deref(),
        || {
            prepared.catalog.ensure_current()?;
            if target_identity.ready() {
                Ok(())
            } else {
                Err(raybend::Error::SessionExpired)
            }
        },
        &prepared.suffix,
    )
    .map_err(|e| e.to_string())?;
    Ok(match target {
        raybend::export::output::Publication::Written(path) => {
            Outcome::Done(path.to_string_lossy().into_owned())
        }
        raybend::export::output::Publication::Skipped(path) => {
            Outcome::FileExists(path.to_string_lossy().into_owned())
        }
    })
}
#[tauri::command]
pub async fn export_queue<R: Runtime>(
    app: AppHandle<R>,
    action: String,
    generation: Option<u64>,
    items: Option<Vec<raybend::export::jobs::Item>>,
    preset_id: Option<String>,
    ids: Option<Vec<String>>,
) -> Result<raybend::export::jobs::View, String> {
    crate::source::blocking(move || {
        let engine = app.state::<ExportState>().engine(&app);
        let ids: std::collections::BTreeSet<_> = ids.unwrap_or_default().into_iter().collect();
        match action.as_str() {
            "status" => Ok(engine.view()),
            "enqueue" => {
                let generation = generation.ok_or("缺少队列代次")?;
                let items = items.unwrap_or_default();
                let state = app.state::<ExportState>();
                let permits = items
                    .iter()
                    .map(|item| {
                        app.state::<BrowseState>()
                            .sessions
                            .begin_task(&item.repository_id)
                            .map_err(|e| e.to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                for item in &items {
                    let key = (generation.to_string(), item.repository_id.clone());
                    if !state.leases.lock().unwrap().contains_key(&key) {
                        let db = app
                            .state::<BrowseState>()
                            .lease(&app, &item.repository_id)?;
                        state.leases.lock().unwrap().entry(key).or_insert_with(|| {
                            std::sync::Arc::new(raybend::store::session::TaskCatalog::new(db))
                        });
                    }
                    state
                        .targets
                        .lock()
                        .unwrap()
                        .entry((generation.to_string(), item.preset.directory.clone()))
                        .or_insert_with(|| {
                            std::sync::Arc::new(
                                raybend::store::session::SourceIdentity::capture_output(
                                    std::path::Path::new(&item.preset.directory),
                                ),
                            )
                        });
                }
                let result = engine.enqueue(generation, items);
                drop(permits);
                result
            }
            "wake" => engine.wake(),
            "enable" | "disable" => {
                engine.enable(preset_id.ok_or("未选择预设")?, action == "enable")
            }
            "stop" => engine.stop_all(),
            "reset" => engine.reset(),
            "remove" => engine.remove(&ids),
            "retry" => engine.retry(&ids),
            _ => Err("无效导出队列操作".into()),
        }
    })
    .await
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPreview {
    target: String,
    width: u32,
    height: u32,
}
#[tauri::command]
/// 暂不启用：仅保留底层目标名/尺寸诊断能力，无产品入口。
pub async fn export_preview<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    reference: VariantRef,
    preset: Preset,
) -> Result<ExportPreview, String> {
    crate::source::blocking(move || {
        if !preset.validate(false).errors.is_empty() {
            return Err("请先修正导出设置".into());
        }
        let _permit = app
            .state::<BrowseState>()
            .sessions
            .begin_task(&repository_id)
            .map_err(|e| e.to_string())?;
        let catalog = app.state::<BrowseState>().lease(&app, &repository_id)?;
        let snapshot = catalog
            .read(|conn| raybend::export::snapshot(conn, catalog.root(), &reference))
            .map_err(|e| e.to_string())?;
        let p = prepare_with_catalog(catalog, &snapshot)?.ok_or("定稿已失效")?;
        let (lens, lut) = resources(&app, &repository_id, &snapshot)?;
        let image = raybend::export::render_for_preset(
            &p.source,
            &snapshot,
            lens.as_ref(),
            lut.as_deref(),
            &preset,
        )
        .map_err(|e| e.to_string())?;
        let relative =
            raybend::export::output::relative_name(&preset, &p.source, &p.naming, 1, &p.suffix)
                .map_err(|e| e.to_string())?;
        Ok(ExportPreview {
            target: std::path::Path::new(&preset.directory)
                .join(relative)
                .to_string_lossy()
                .into(),
            width: image.width(),
            height: image.height(),
        })
    })
    .await
}
#[tauri::command]
/// 暂不启用：预设文件交换，无产品入口。
pub async fn export_presets_file(path: String, content: Option<String>) -> Result<String, String> {
    crate::source::blocking(move || {
        raybend::export::presets::file(std::path::Path::new(&path), content)
            .map_err(|e| e.to_string())
    })
    .await
}
