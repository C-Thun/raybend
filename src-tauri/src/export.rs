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
        let root = crate::browse::resolve_root(&handle, &repository_id)?;
        handle
            .state::<BrowseState>()
            .with_catalog(&handle, &repository_id, |db| {
                db.read(|conn| {
                    references
                        .iter()
                        .map(|r| raybend::export::snapshot(conn, &root, r))
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
        let root = crate::browse::resolve_root(&handle, &repository_id)?;
        let (snap, rel_path) =
            handle
                .state::<BrowseState>()
                .with_catalog(&handle, &repository_id, |db| {
                    db.read(|conn| match captured {
                        Some(ref saved) => {
                            raybend::export::validate_captured(conn, &root, &reference, saved)?;
                            let path =
                                raybend::export::resolve_captured_source(conn, &root, saved)?;
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
                    .map_err(|e| e.to_string())
                })?;
        let asset = crate::develop::ResolvedAsset {
            repository_id: repository_id.clone(),
            asset_id: reference.asset_id,
            root,
            rel_path,
        };
        let profile_key = format!(
            "export-{}-{}",
            reference.variant.replace(':', "-"),
            snap.profile_hash
        );
        let preview =
            crate::thumbs::render_profile_cached(&handle, &asset, &snap.stack, &profile_key)?;
        if class == raybend::thumbnail::SizeClass::Screen {
            return Ok(preview);
        }
        // 尺寸适配复用 image 与现有 AVIF 编码；前端不碰像素。
        let image = image::load_from_memory_with_format(&preview, image::ImageFormat::Avif)
            .map_err(|e| e.to_string())?;
        let image = raybend::thumbnail::render::clamp_display_aspect(
            image,
            raybend::thumbnail::render::MAX_DISPLAY_ASPECT,
        );
        let image = image
            .thumbnail(class.long_edge(), class.long_edge())
            .to_rgb8();
        raybend::thumbnail::render::encode_avif(image.as_raw(), image.width(), image.height())
            .map_err(|e| e.to_string())
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
        let root = crate::browse::resolve_root(&handle, &repository_id)?;
        let source = handle
            .state::<BrowseState>()
            .with_catalog(&handle, &repository_id, |db| {
                db.read(|conn| raybend::export::resolve_captured_source(conn, &root, &captured))
                    .map_err(|e| e.to_string())
            })?;
        let (lens, lut) = resources(&handle, &repository_id, &captured)?;
        raybend::export::write_tiff16(
            &source,
            std::path::Path::new(&target),
            &captured,
            lens.as_ref(),
            lut.as_deref(),
            max_edge,
        )
        .map_err(|e| e.to_string())
    })
    .await
}

#[derive(Default)]
pub struct ExportState(std::sync::Mutex<Option<raybend::export::jobs::Engine>>);
impl ExportState {
    pub(crate) fn has_unfinished(&self) -> Result<bool, String> {
        Ok(self
            .0
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
        let mut guard = self.0.lock().unwrap();
        guard
            .get_or_insert_with(|| {
                let worker = app.clone();
                let events = app.clone();
                raybend::export::jobs::Engine::new(
                    move |item| run_item(&worker, item),
                    move |view| {
                        use tauri::Emitter;
                        let _ = events.emit("export://state", view);
                    },
                )
            })
            .clone()
    }
}
pub(crate) struct Prepared {
    pub(crate) source: std::path::PathBuf,
    naming: raybend::export::output::Naming,
    author: Option<String>,
    description: Option<String>,
    tags: Vec<i64>,
}
pub(crate) fn prepare<R: Runtime>(
    app: &AppHandle<R>,
    repository: &str,
    captured: &VariantSnapshot,
) -> Result<Option<Prepared>, String> {
    let root = crate::browse::resolve_root(app, repository)?;
    app.state::<BrowseState>().with_catalog(app,repository,|db|db.read(|conn|{
        // Current identity is checked once immediately before execution. The immutable
        // enqueued stack remains the rendering input, never a live latest edit.
        if !raybend::export::still_current(conn,&root,captured)?{return Ok(None)}
        let source=raybend::export::resolve_captured_source(conn,&root,captured)?;
        let(taken_at,brand,model,author,description)=conn.query_row("SELECT taken_at,camera_make,camera_model,author,description FROM assets WHERE id=?1",[captured.reference.asset_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)))?;
        Ok(Some(Prepared{source,naming:raybend::export::output::Naming{taken_at,brand,model},author,description,tags:raybend::store::tags::tags_of_asset(conn,captured.reference.asset_id)?}))
    }).map_err(|e|e.to_string()))
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
) -> Result<raybend::export::jobs::Outcome, String> {
    use raybend::export::jobs::Outcome;
    let Some(prepared) = prepare(app, &item.repository_id, &item.snapshot)? else {
        return Ok(Outcome::Skipped);
    };
    if let Some(path) = raybend::export::output::skip_existing(
        &prepared.source,
        &item.preset,
        &prepared.naming,
        item.sequence,
    )
    .map_err(|e| e.to_string())?
    {
        return Ok(Outcome::FileExists(path.to_string_lossy().into_owned()));
    }
    let metadata = metadata_for(app, &prepared)?;
    let (lens, lut) = resources(app, &item.repository_id, &item.snapshot)?;
    let target = raybend::export::output::execute(
        &prepared.source,
        &item.snapshot,
        &item.preset,
        &prepared.naming,
        &metadata,
        item.sequence,
        lens.as_ref(),
        lut.as_deref(),
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
                engine.enqueue(generation.ok_or("缺少队列代次")?, items.unwrap_or_default())
            }
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
        let root = crate::browse::resolve_root(&app, &repository_id)?;
        let snapshot = app
            .state::<BrowseState>()
            .with_catalog(&app, &repository_id, |db| {
                db.read(|conn| raybend::export::snapshot(conn, &root, &reference))
                    .map_err(|e| e.to_string())
            })?;
        let p = prepare(&app, &repository_id, &snapshot)?.ok_or("定稿已失效")?;
        let (lens, lut) = resources(&app, &repository_id, &snapshot)?;
        let image = raybend::export::render_for_preset(
            &p.source,
            &snapshot,
            lens.as_ref(),
            lut.as_deref(),
            &preset,
        )
        .map_err(|e| e.to_string())?;
        let relative = raybend::export::output::relative_name(&preset, &p.source, &p.naming, 1)
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
