//! 机型基础曲线档案的薄 IPC：catalog 提供机型，app.db 提供全局档案。

use crate::browse::BrowseState;
use crate::db::DbState;
use raybend::store::base_curve::{self, BaseCurveProfile};
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseCurveProfileDto {
    pub id: String,
    pub name: String,
    pub points: Vec<[f32; 2]>,
    pub sample_count: i64,
}

impl From<BaseCurveProfile> for BaseCurveProfileDto {
    fn from(profile: BaseCurveProfile) -> Self {
        Self {
            id: profile.id.to_string(),
            name: profile.name,
            points: profile.points,
            sample_count: profile.sample_count,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseCurveLibraryDto {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub profiles: Vec<BaseCurveProfileDto>,
}

pub fn camera_of_asset<R: Runtime>(
    app: &AppHandle<R>,
    repository_id: &str,
    asset_id: i64,
) -> Result<Option<(String, String)>, String> {
    let state = app.state::<BrowseState>();
    state.with_catalog(app, repository_id, |db| {
        let pair = db
            .read(|conn| {
                let mut statement =
                    conn.prepare("SELECT camera_make, camera_model FROM assets WHERE id = ?1")?;
                let pair = statement.query_row([asset_id], |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                })?;
                Ok(pair)
            })
            .map_err(|e| e.to_string())?;
        Ok(match pair {
            (Some(make), Some(model)) if base_curve::camera_key(&make, &model).is_some() => {
                Some((make, model))
            }
            _ => None,
        })
    })
}

pub fn library_for<R: Runtime>(
    app: &AppHandle<R>,
    repository_id: &str,
    asset_id: i64,
) -> Result<BaseCurveLibraryDto, String> {
    let Some((make, model)) = camera_of_asset(app, repository_id, asset_id)? else {
        return Ok(BaseCurveLibraryDto {
            camera_make: None,
            camera_model: None,
            profiles: Vec::new(),
        });
    };
    let db = app.state::<DbState>();
    let profiles = db.with(app, |db| {
        db.read(|conn| base_curve::list(conn, &make, &model))
            .map_err(|e| e.to_string())
    })?;
    Ok(BaseCurveLibraryDto {
        camera_make: Some(make),
        camera_model: Some(model),
        profiles: profiles.into_iter().map(Into::into).collect(),
    })
}

#[tauri::command]
pub async fn base_curve_profiles<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    asset_id: i64,
) -> Result<BaseCurveLibraryDto, String> {
    let handle = app.clone();
    crate::source::blocking(move || library_for(&handle, &repository_id, asset_id)).await
}

#[tauri::command]
pub async fn base_curve_rename<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    asset_id: i64,
    profile_id: String,
    name: String,
) -> Result<BaseCurveProfileDto, String> {
    crate::source::blocking(move || {
        let (make, model) = camera_of_asset(&app, &repository_id, asset_id)?
            .ok_or("缺少相机品牌或型号，不能修改基础曲线档案")?;
        let id = profile_id
            .parse::<i64>()
            .map_err(|_| "无效基础曲线档案 ID")?;
        app.state::<DbState>().with(&app, |db| {
            db.write(move |conn| base_curve::rename(conn, id, &make, &model, &name))
                .map(Into::into)
                .map_err(|e| e.to_string())
        })
    })
    .await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoAdjustDto {
    pub profile: BaseCurveProfileDto,
    pub exposure: f64,
    pub contrast: f64,
    pub saturation: f64,
}

/// 仅显式点击后执行；读 RAW 与成对直出，再写入同机型档案库。
#[tauri::command]
pub async fn base_curve_auto_adjust<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    asset_id: i64,
) -> Result<AutoAdjustDto, String> {
    let (make, model) = camera_of_asset(&app, &repository_id, asset_id)?
        .ok_or("缺少相机品牌或型号，不能生成基础曲线")?;
    let raw = crate::develop::develop_edit_target(
        app.clone(),
        repository_id.clone(),
        asset_id,
        Some("raw".into()),
    )
    .await?;
    let bitmap = crate::develop::develop_edit_target(
        app.clone(),
        repository_id,
        asset_id,
        Some("sooc".into()),
    )
    .await?;
    if !raw.has_raw || !bitmap.has_bitmap {
        return Err("自动调整需要同张照片的 RAW 与直出位图".into());
    }
    let raw_path = raw.path.ok_or("RAW 文件不可用")?;
    let bitmap_path = bitmap.path.ok_or("直出位图不可用")?;
    let handle = app.clone();
    crate::source::blocking(move || {
        let request = raybend::raw::DecodeRequest::thumb(&raw_path, 1024).with_preview(false);
        let decoded = raybend::raw::worker::shared()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .decode_linear(&request)
            .map_err(|e| format!("RAW 线性解码失败：{e}"))?;
        let raw_rgb: Vec<u8> = decoded
            .rgb
            .iter()
            .map(|value| {
                (raybend::develop::linear_to_srgb(*value as f32 / 65535.0).clamp(0.0, 1.0) * 255.0)
                    .round() as u8
            })
            .collect();
        let sooc = image::open(&bitmap_path)
            .map_err(|e| format!("直出位图解码失败：{e}"))?
            .thumbnail(1024, 1024)
            .to_rgb8();
        let analysis = raybend::develop::base_curve::analyze(&raw_rgb, sooc.as_raw())?;
        let db = handle.state::<DbState>();
        let profile = db.with(&handle, |db| {
            db.write(move |conn| {
                let previous = base_curve::list(conn, &make, &model)?;
                let best = previous
                    .into_iter()
                    .filter_map(|profile| {
                        raybend::develop::base_curve::distance(&profile.points, &analysis.points)
                            .ok()
                            .map(|distance| (distance, profile))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                if let Some((distance, mut profile)) = best
                    && distance <= raybend::develop::base_curve::MATCH_THRESHOLD
                {
                    base_curve::record_match(conn, profile.id)?;
                    profile.sample_count += 1;
                    return Ok(profile);
                }
                base_curve::create(
                    conn,
                    &make,
                    &model,
                    &analysis.points,
                    raybend::store::time::now_millis(),
                )
            })
            .map_err(|e| e.to_string())
        })?;
        Ok(AutoAdjustDto {
            profile: profile.into(),
            exposure: analysis.tone.exposure,
            contrast: analysis.tone.contrast,
            saturation: analysis.tone.saturation,
        })
    })
    .await
}
