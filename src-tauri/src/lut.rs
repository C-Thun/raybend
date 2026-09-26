//! LUT 应用库 IPC。文件拷贝进 app data，数据库只记录稳定 ID 和相对路径。

use crate::db::DbState;
use raybend::develop::lut::Lut;
use raybend::develop::lut_import::{
    cached_cover, copy_for_import, discover, file_hash, import_cover, source_cube_exists,
};
use raybend::store::luts::{self, Admission, LutCategory, LutRecord};
use raybend::store::time;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Manager, Runtime};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LutLibraryDto {
    pub categories: Vec<LutCategory>,
    pub entries: Vec<LutEntryDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LutEntryDto {
    #[serde(flatten)]
    pub record: LutRecord,
    pub available: bool,
    pub cover_available: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyCategory {
    pub id: String,
    pub name: String,
}

fn root<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(crate::db::data_dir(app)?.join("luts"))
}

pub fn resolve<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<Arc<Lut>, String> {
    let db = app.state::<DbState>();
    let record = db
        .with(app, |db| {
            db.read(|conn| luts::get(conn, id))
                .map_err(|err| err.to_string())
        })?
        .ok_or_else(|| format!("LUT {id} 已不在应用库中"))?;
    let path = root(app)?.join(&record.file_rel_path);
    if !path.is_file() {
        return Err(format!("LUT 文件已丢失：{}", record.original_filename));
    }
    Lut::load(&path)
        .map(Arc::new)
        .map_err(|error| error.to_string())
}

fn backfill_hashes<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let db = app.state::<DbState>();
    let root = root(app)?;
    let records = db.with(app, |db| {
        db.read(|conn| luts::entries(conn, true))
            .map_err(|e| e.to_string())
    })?;
    // 文件 IO 不放在 DB 写线程；缺失文件保留 NULL，下次读取再尝试。
    let updates: Vec<_> = records
        .into_iter()
        .filter(|entry| entry.file_hash.is_none())
        .filter_map(|entry| match file_hash(&root.join(&entry.file_rel_path)) {
            Ok(hash) => Some((entry.id, hash)),
            Err(error) => {
                eprintln!("[raybend] LUT {} 无法补录哈希：{error}", entry.id);
                None
            }
        })
        .collect();
    if !updates.is_empty() {
        db.with(app, |db| {
            db.write_tx(move |conn| {
                for (id, hash) in updates {
                    luts::backfill_hash(conn, &id, &hash)?;
                }
                Ok(())
            })
            .map_err(|e| e.to_string())
        })?;
    }
    Ok(())
}

fn admit<R: Runtime>(app: &AppHandle<R>, entry: LutRecord) -> Result<Admission, String> {
    app.state::<DbState>().with(app, |db| {
        db.write(move |conn| luts::admit(conn, &entry))
            .map_err(|e| e.to_string())
    })
}

fn library<R: Runtime>(
    app: &AppHandle<R>,
    legacy: &[LegacyCategory],
) -> Result<LutLibraryDto, String> {
    backfill_hashes(app)?;
    let db = app.state::<DbState>();
    let legacy = legacy.to_vec();
    db.with(app, |db| {
        db.write(move |conn| {
            for category in legacy {
                if luts::categories(conn)?.iter().all(|current| {
                    current.id != category.id && !current.name.eq_ignore_ascii_case(&category.name)
                }) {
                    luts::create_category(conn, &category.id, &category.name, time::now_millis())?;
                }
            }
            if luts::categories(conn)?.is_empty() {
                luts::create_category(conn, "default", "默认分类", time::now_millis())?;
            }
            Ok(())
        })
        .map_err(|error| error.to_string())
    })?;
    let (categories, entries) = db.with(app, |db| {
        db.read(|conn| Ok((luts::categories(conn)?, luts::entries(conn, true)?)))
            .map_err(|error| error.to_string())
    })?;
    let root = root(app)?;
    Ok(LutLibraryDto {
        categories,
        entries: entries
            .into_iter()
            .map(|record| {
                let available = root.join(&record.file_rel_path).is_file();
                let cover_available = root.join(&record.cover_rel_path).is_file() || available;
                LutEntryDto {
                    record,
                    available,
                    cover_available,
                }
            })
            .collect(),
    })
}

#[tauri::command]
pub async fn lut_library<R: Runtime>(
    app: AppHandle<R>,
    legacy_categories: Option<Vec<LegacyCategory>>,
) -> Result<LutLibraryDto, String> {
    let handle = app.clone();
    crate::source::blocking(move || library(&handle, &legacy_categories.unwrap_or_default())).await
}

#[tauri::command]
pub async fn lut_create_category<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    name: String,
) -> Result<LutLibraryDto, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        let db = handle.state::<DbState>();
        db.with(&handle, |db| {
            db.write(move |conn| luts::create_category(conn, &id, &name, time::now_millis()))
                .map_err(|error| error.to_string())
        })?;
        library(&handle, &[])
    })
    .await
}

fn new_directory(root: &Path) -> Result<(String, PathBuf), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    for _ in 0..100 {
        let count = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let id = format!(
            "lut-{:x}-{:x}-{:x}",
            time::now_millis(),
            std::process::id(),
            count
        );
        let dir = root.join(&id);
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok((id, dir)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("无法分配新的 LUT 内部目录".into())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LutImportDto {
    pub library: LutLibraryDto,
    pub imported: usize,
    pub duplicates: usize,
    pub restored: usize,
    pub skipped: Vec<String>,
}

#[tauri::command]
pub async fn lut_import_directory<R: Runtime>(
    app: AppHandle<R>,
    path: String,
    category_id: String,
) -> Result<LutImportDto, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        backfill_hashes(&handle)?;
        let source = Path::new(&path);
        let files = discover(source).map_err(|error| error.to_string())?;
        let root = root(&handle)?;
        let db = handle.state::<DbState>();
        let valid_category = db.with(&handle, |db| {
            db.read(|conn| {
                Ok(luts::categories(conn)?
                    .iter()
                    .any(|item| item.id == category_id))
            })
            .map_err(|error| error.to_string())
        })?;
        if !valid_category {
            return Err("LUT 分类不存在".into());
        }
        let mut imported = 0;
        let mut duplicates = 0;
        let mut restored = 0;
        let mut skipped = Vec::new();
        for file in files {
            // .cube 的同名伴生图不能再作为 Hald 导入，后缀比较不区分大小写。
            if file
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| {
                    ["png", "tif", "tiff"].contains(&ext.to_ascii_lowercase().as_str())
                })
                && source_cube_exists(&file)
            {
                continue;
            }
            let digest = match file_hash(&file) {
                Ok(hash) => hash,
                Err(error) => {
                    skipped.push(format!("{}: {error}", file.display()));
                    continue;
                }
            };
            let existing = db.with(&handle, |db| {
                db.read(|conn| luts::find_by_hash(conn, &digest))
                    .map_err(|error| error.to_string())
            })?;
            if let Some(mut entry) = existing {
                let internal = root.join(&entry.file_rel_path);
                if file_hash(&internal).is_ok_and(|actual| actual == digest) {
                    // 已有正确副本无需解码 / 烘焙；缺图时用本次来源修复。
                    if !root.join(&entry.cover_rel_path).is_file() {
                        match Lut::load(&internal).and_then(|lut| {
                            import_cover(&file, &lut, &root.join(&entry.cover_rel_path))
                        }) {
                            Ok(()) => {}
                            Err(error) => {
                                skipped.push(format!("{}: {error}", file.display()));
                                continue;
                            }
                        }
                    }
                    entry.category_id = category_id.clone();
                    match admit(&handle, entry) {
                        Ok(Admission::Restored(_)) => restored += 1,
                        Ok(Admission::Duplicate(_)) => duplicates += 1,
                        Ok(Admission::Imported) => imported += 1,
                        Err(error) => skipped.push(format!("{}: {error}", file.display())),
                    }
                    continue;
                }
            }
            let (id, dir) = new_directory(&root)?;
            let extension = file
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("cube")
                .to_ascii_lowercase();
            let file_rel_path = format!("{id}/source.{extension}");
            let cover_rel_path = format!("{id}/cover.webp");
            let internal = root.join(&file_rel_path);
            let outcome = (|| -> Result<Admission, String> {
                // 以真正写入私有目录的字节为准，不能只信初筛时的外部路径。
                let hash = copy_for_import(&file, &internal).map_err(|e| e.to_string())?;
                let lut = Lut::load(&internal).map_err(|e| e.to_string())?;
                import_cover(&file, &lut, &root.join(&cover_rel_path))
                    .map_err(|e| e.to_string())?;
                let filename = file
                    .file_name()
                    .map_or_else(String::new, |value| value.to_string_lossy().into_owned());
                let entry = LutRecord {
                    id: id.clone(),
                    category_id: category_id.clone(),
                    name: filename.clone(),
                    original_filename: filename,
                    original_path: file.to_string_lossy().into_owned(),
                    file_rel_path,
                    cover_rel_path,
                    format: if extension == "cube" { "cube" } else { "hald" }.into(),
                    hidden: false,
                    created_at: time::now_millis(),
                    file_hash: Some(hash.clone()),
                };
                let result = admit(&handle, entry)?;
                if let Admission::Duplicate(ref existing_id)
                | Admission::Restored(ref existing_id) = result
                {
                    let existing = db
                        .with(&handle, |db| {
                            db.read(|conn| luts::get(conn, existing_id))
                                .map_err(|e| e.to_string())
                        })?
                        .ok_or("已有 LUT 条目丢失")?;
                    let path = root.join(existing.file_rel_path);
                    if !file_hash(&path).is_ok_and(|actual| actual == hash) {
                        copy_for_import(&internal, &path).map_err(|e| e.to_string())?;
                    }
                    let cover = root.join(existing.cover_rel_path);
                    if !cover.is_file() {
                        import_cover(&file, &lut, &cover).map_err(|e| e.to_string())?;
                    }
                }
                Ok(result)
            })();
            match outcome {
                Ok(Admission::Imported) => imported += 1,
                Ok(result) => {
                    let _ = std::fs::remove_dir_all(&dir);
                    match result {
                        Admission::Restored(_) => restored += 1,
                        _ => duplicates += 1,
                    }
                }
                Err(error) => {
                    let _ = std::fs::remove_dir_all(&dir);
                    skipped.push(format!("{}: {error}", file.display()));
                }
            }
        }
        Ok(LutImportDto {
            library: library(&handle, &[])?,
            imported,
            duplicates,
            restored,
            skipped,
        })
    })
    .await
}

#[tauri::command]
pub async fn lut_cover<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<tauri::ipc::Response, String> {
    let handle = app.clone();
    let bytes = crate::source::blocking(move || {
        let db = handle.state::<DbState>();
        let entry = db
            .with(&handle, |db| {
                db.read(|conn| luts::get(conn, &id))
                    .map_err(|error| error.to_string())
            })?
            .ok_or("LUT 不存在")?;
        let root = root(&handle)?;
        cached_cover(
            &root.join(entry.file_rel_path),
            &root.join(entry.cover_rel_path),
            Path::new(&entry.original_path),
        )
        .map_err(|error| error.to_string())
    })
    .await?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub async fn lut_hide<R: Runtime>(app: AppHandle<R>, id: String) -> Result<LutLibraryDto, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        let db = handle.state::<DbState>();
        db.with(&handle, |db| {
            db.write(move |conn| luts::hide(conn, &id))
                .map_err(|error| error.to_string())
        })?;
        // 文件保留；旧定稿和 latest 里的引用仍可渲染。
        library(&handle, &[])
    })
    .await
}
