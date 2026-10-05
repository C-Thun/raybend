//! 全局 ICC 资源库的薄 IPC。文件校验/原件保全在核心，app.db 写入走单写者。

use std::path::{Path, PathBuf};

use raybend::color::assets;
use raybend::color::icc::{adobe_rgb_icc, display_p3_icc, srgb_icc};
use raybend::store::color_profiles::{self, Admission, ColorProfileClass, ColorProfileRecord};
use raybend::store::time;
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

use crate::db::DbState;

const MAX_BATCH: usize = 256;

#[tauri::command]
pub async fn color_prepare_photo<R: Runtime>(app: AppHandle<R>, path: String, profile_id: Option<String>) -> Result<raybend::color::PhotoColorState,String> {
    crate::source::blocking(move || {
        let path = Path::new(&path);
        let _permit=app.state::<crate::browse::BrowseState>().path_task(&app,path)?;
        let asset = crate::develop::resolve_asset(&app,path).ok_or("照片未登记到当前库")?;
        let lease = app.state::<crate::browse::BrowseState>().lease(&app,&asset.repository_id)?;
        lease.ensure_current().map_err(|e|e.to_string())?;
        let signature=raybend::media::source::source_signature(path).map_err(|e|e.to_string())?;
        let profile = profile_id.map(|id| raybend::color::ProfileId::try_from(id).map_err(|e|e.to_string())
            .and_then(|id| resolve(&app,&id,raybend::color::icc::IccRole::PhotoInput).map_err(|e|e.to_string()))).transpose()?;
        let defaults = app.state::<DbState>().with(&app,|db| db.read(raybend::color::defaults::load).map_err(|e|e.to_string()))?;
        let color=raybend::color::input::prepare_source(path,&defaults,profile.as_ref()).map_err(|e|e.to_string())?;
        lease.ensure_current().map_err(|e|e.to_string())?;
        if signature != raybend::media::source::source_signature(path).map_err(|e|e.to_string())? {return Err("准备色彩配置时原文件已改变，请重试".into());}
        Ok(color)
    }).await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileEntryDto {
    pub key: String,
    pub profile_id: String,
    pub name: String,
    pub original_filename: String,
    pub profile_class: ColorProfileClass,
    pub built_in: bool,
    pub hidden: bool,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileLibraryDto {
    pub entries: Vec<ProfileEntryDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileImportDto {
    pub library: ProfileLibraryDto,
    pub imported: usize,
    pub duplicates: usize,
    pub restored: usize,
    pub skipped: Vec<String>,
}

fn root<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(crate::db::data_dir(app)?.join("color-profiles"))
}

pub(crate) fn resolve<R: Runtime>(app: &AppHandle<R>, id: &raybend::color::ProfileId, role: raybend::color::icc::IccRole) -> raybend::Result<raybend::color::icc::RgbIcc> {
    let root = root(app).map_err(raybend::Error::Unsupported)?;
    assets::resolve_profile(&root, id, role)
}

#[tauri::command]
pub async fn color_get_defaults<R: Runtime>(app: AppHandle<R>) -> Result<raybend::color::defaults::ColorDefaults, String> {
    crate::source::blocking(move || app.state::<DbState>().with(&app, |db|
        db.read(raybend::color::defaults::load).map_err(|e| e.to_string()))).await
}

#[tauri::command]
pub async fn color_set_defaults<R: Runtime>(app: AppHandle<R>, defaults: raybend::color::defaults::ColorDefaults) -> Result<(), String> {
    crate::source::blocking(move || {
        if let raybend::color::defaults::UntaggedInput::RgbIcc { profile_id } = &defaults.untagged_input {
            resolve(&app, profile_id, raybend::color::icc::IccRole::PhotoInput).map_err(|e| e.to_string())?;
        }
        if let raybend::color::OutputColor::CustomRgbIcc { profile_id } = &defaults.output {
            resolve(&app, profile_id, raybend::color::icc::IccRole::RgbOutput).map_err(|e| e.to_string())?;
        }
        app.state::<DbState>().with(&app, |db|
            db.write(move |conn| raybend::color::defaults::save(conn, &defaults, time::now_millis())).map_err(|e| e.to_string()))
    }).await
}

pub(crate) fn library<R: Runtime>(app: &AppHandle<R>) -> Result<ProfileLibraryDto, String> {
    let mut entries = Vec::new();
    for (key, name, profile) in [
        ("builtin:srgb-v1", "sRGB IEC61966-2.1", srgb_icc()),
        ("builtin:display-p3-v1", "Display P3", display_p3_icc()),
        (
            "builtin:adobe-rgb-1998-v1",
            "Adobe RGB (1998)",
            adobe_rgb_icc(),
        ),
    ] {
        let profile = profile.map_err(|error| error.to_string())?;
        entries.push(ProfileEntryDto {
            key: key.into(),
            profile_id: profile.id().as_str().into(),
            name: name.into(),
            original_filename: String::new(),
            profile_class: ColorProfileClass::Display,
            built_in: true,
            hidden: false,
            available: true,
        });
    }
    let records = app.state::<DbState>().with(app, |db| {
        db.read(|conn| color_profiles::entries(conn, true))
            .map_err(|error| error.to_string())
    })?;
    let root = root(app)?;
    entries.extend(records.into_iter().map(|record| {
        let available = raybend::color::ProfileId::try_from(record.profile_id.clone())
            .ok()
            .is_some_and(|id| assets::asset_path(&root, &id).is_file());
        ProfileEntryDto {
            key: format!("sha256:{}", record.profile_id),
            profile_id: record.profile_id,
            name: record.name,
            original_filename: record.original_filename,
            profile_class: record.profile_class,
            built_in: false,
            hidden: record.hidden,
            available,
        }
    }));
    Ok(ProfileLibraryDto { entries })
}

#[tauri::command]
pub async fn color_profile_library<R: Runtime>(
    app: AppHandle<R>,
) -> Result<ProfileLibraryDto, String> {
    let handle = app.clone();
    crate::source::blocking(move || library(&handle)).await
}

#[tauri::command]
pub async fn color_profile_import_files<R: Runtime>(
    app: AppHandle<R>,
    paths: Vec<String>,
) -> Result<ProfileImportDto, String> {
    if paths.is_empty() || paths.len() > MAX_BATCH {
        return Err("一次请选择 1–256 份 ICC 配置文件".into());
    }
    let handle = app.clone();
    crate::source::blocking(move || {
        let root = root(&handle)?;
        let mut imported = 0;
        let mut duplicates = 0;
        let mut restored = 0;
        let mut skipped = Vec::new();
        for path in paths {
            let source = Path::new(&path);
            let outcome = (|| -> Result<Admission, String> {
                let profile =
                    assets::import_file(source, &root).map_err(|error| error.to_string())?;
                let filename = source
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .ok_or("ICC 文件名无效")?;
                let class = ColorProfileClass::from_icc(profile.class())
                    .ok_or("仅支持照片用途的 RGB ICC")?;
                let entry = ColorProfileRecord {
                    profile_id: profile.id().as_str().into(),
                    name: filename.clone(),
                    original_filename: filename,
                    profile_class: class,
                    hidden: false,
                    created_at: time::now_millis(),
                };
                handle.state::<DbState>().with(&handle, |db| {
                    db.write(move |conn| color_profiles::admit(conn, &entry))
                        .map_err(|error| error.to_string())
                })
            })();
            match outcome {
                Ok(Admission::Imported) => imported += 1,
                Ok(Admission::Duplicate) => duplicates += 1,
                Ok(Admission::Restored) => restored += 1,
                Err(error) => skipped.push(format!("{path}: {error}")),
            }
        }
        Ok(ProfileImportDto {
            library: library(&handle)?,
            imported,
            duplicates,
            restored,
            skipped,
        })
    })
    .await
}

#[tauri::command]
pub async fn color_profile_hide<R: Runtime>(
    app: AppHandle<R>,
    profile_id: String,
) -> Result<ProfileLibraryDto, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        let changed = handle.state::<DbState>().with(&handle, |db| {
            db.write(move |conn| color_profiles::hide(conn, &profile_id))
                .map_err(|error| error.to_string())
        })?;
        if !changed {
            return Err("配置文件不存在或已隐藏".into());
        }
        library(&handle)
    })
    .await
}
