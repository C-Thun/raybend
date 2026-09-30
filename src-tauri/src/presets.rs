//! 编辑预设 IPC（`specs/editor-presets.md` §4）。
//!
//! 六个命令统一返回**最新的整库**（`PresetLibraryDto`），前端拿到即回写 store ——
//! 与 `lut.rs` 的 `lut_library` 返回模式一致。纯 DB 操作，不碰磁盘文件。

use crate::db::DbState;
use raybend::store::presets::{self, PresetDirectory, PresetRecord};
use raybend::store::time;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetLibraryDto {
    pub directories: Vec<PresetDirectory>,
    pub presets: Vec<PresetRecord>,
}

fn library<R: Runtime>(app: &AppHandle<R>) -> Result<PresetLibraryDto, String> {
    let db = app.state::<DbState>();
    // 目录表为空时建 default（惰性初始化，照 LUT 的口径）
    db.with(app, |db| {
        db.write(move |conn| {
            presets::ensure_default_directory(conn, time::now_millis())
        })
        .map_err(|error| error.to_string())
    })?;
    db.with(app, |db| {
        db.read(|conn| {
            Ok(PresetLibraryDto {
                directories: presets::directories(conn)?,
                presets: presets::presets(conn)?,
            })
        })
        .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub async fn preset_library<R: Runtime>(app: AppHandle<R>) -> Result<PresetLibraryDto, String> {
    let handle = app.clone();
    crate::source::blocking(move || library(&handle)).await
}

#[tauri::command]
pub async fn preset_create_directory<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    name: String,
) -> Result<PresetLibraryDto, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        let db = handle.state::<DbState>();
        db.with(&handle, |db| {
            db.write(move |conn| {
                presets::create_directory(conn, &id, &name, time::now_millis())
            })
            .map_err(|error| error.to_string())
        })?;
        library(&handle)
    })
    .await
}

#[tauri::command]
pub async fn preset_delete_directory<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<PresetLibraryDto, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        let db = handle.state::<DbState>();
        db.with(&handle, |db| {
            db.write(move |conn| presets::delete_directory(conn, &id))
                .map_err(|error| error.to_string())
        })?;
        library(&handle)
    })
    .await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetCreateArgs {
    pub id: String,
    pub directory_id: String,
    pub name: String,
    /// 大类快照 JSON（**字符串**：序列化由前端完成，后端原样存储并校验可解析）。
    pub payload: String,
}

#[tauri::command]
pub async fn preset_create<R: Runtime>(
    app: AppHandle<R>,
    args: PresetCreateArgs,
) -> Result<PresetLibraryDto, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        let db = handle.state::<DbState>();
        db.with(&handle, |db| {
            db.write(move |conn| {
                presets::create(
                    conn,
                    &args.id,
                    &args.directory_id,
                    &args.name,
                    &args.payload,
                    time::now_millis(),
                )
            })
            .map_err(|error| error.to_string())
        })?;
        library(&handle)
    })
    .await
}

#[tauri::command]
pub async fn preset_delete<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<PresetLibraryDto, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        let db = handle.state::<DbState>();
        db.with(&handle, |db| {
            db.write(move |conn| presets::delete(conn, &id))
                .map_err(|error| error.to_string())
        })?;
        library(&handle)
    })
    .await
}

#[tauri::command]
pub async fn preset_move<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    directory_id: String,
) -> Result<PresetLibraryDto, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        let db = handle.state::<DbState>();
        db.with(&handle, |db| {
            db.write(move |conn| {
                presets::move_to(conn, &id, &directory_id, time::now_millis())
            })
            .map_err(|error| error.to_string())
        })?;
        library(&handle)
    })
    .await
}
