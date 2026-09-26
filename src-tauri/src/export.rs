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
        let snap = handle
            .state::<BrowseState>()
            .with_catalog(&handle, &repository_id, |db| {
                db.read(|conn| match captured {
                    Some(ref saved) => {
                        raybend::export::validate_captured(conn, &root, &reference, saved)?;
                        Ok(saved.clone())
                    }
                    None => raybend::export::snapshot(conn, &root, &reference),
                })
                .map_err(|e| e.to_string())
            })?;
        let asset = crate::develop::ResolvedAsset {
            repository_id: repository_id.clone(),
            asset_id: reference.asset_id,
            root,
            rel_path: snap.rel_path,
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
