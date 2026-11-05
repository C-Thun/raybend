//! LUT 应用库 IPC。文件拷贝进 app data，数据库只记录稳定 ID 和相对路径。

use crate::db::DbState;
use raybend::develop::lut::{Lut, LutChain, LutChainLayer};
use raybend::develop::lut_import::{
    cached_cover, copy_for_import, discover, file_hash, import_cover, source_cube_exists,
};
use raybend::store::luts::{self, Admission, LutCategory, LutRecord};
use raybend::store::time;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, Runtime};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// LUT 导入进度事件名（前端 `onLutImportProgress` 监听的就是它）。
pub const LUT_IMPORT_PROGRESS_EVENT: &str = "lut://import-progress";

/// 进度事件的节流间隔 —— 每个文件都会想发一条，太密没有意义，
/// 但**第一条与最后一条必须发**（否则界面会从「没反应」直接跳到 100%）。
const PROGRESS_THROTTLE: Duration = Duration::from_millis(80);

/// 一次 LUT 导入的进度快照。
///
/// `phase` 三档：`scan`（**还没有总数**，界面走不确定态）/ `copy`（总数已知，逐张处理）/ `done`（收尾）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LutImportProgressDto {
    pub phase: &'static str,
    pub done: usize,
    pub total: usize,
    /// 正在处理的文件名（不含目录）
    pub current: Option<String>,
    pub imported: usize,
    pub duplicates: usize,
    pub skipped: usize,
    pub cancelled: bool,
}

/// 取消开关：同一时刻只有一次 LUT 导入（前端用 `busy` 门控），所以一个布尔量就够。
///
/// 取消是**真中止**：在处理完当前文件之后停下，已导入的保留（与照片导入的取消同一条承诺）。
#[derive(Default)]
pub struct LutImportCancel(AtomicBool);

impl LutImportCancel {
    pub fn request(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_requested(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    pub fn reset(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// 进度事件节流器（保证首条一定发）。
struct ProgressThrottle {
    last: Option<Instant>,
}

impl ProgressThrottle {
    fn new() -> Self {
        Self { last: None }
    }

    /// `force` = 无视节流（首条 / 末条 / 阶段切换）
    fn ready(&mut self, force: bool) -> bool {
        let now = Instant::now();
        let due = self
            .last
            .is_none_or(|last| now.duration_since(last) >= PROGRESS_THROTTLE);
        if force || due {
            self.last = Some(now);
            true
        } else {
            false
        }
    }
}

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

pub(crate) fn root<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
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

/// 已解析 LUT 的缓存（按 id）。
///
/// 一条链里同一支只会读一次文件；编辑器会话、缩略图批次各自持一份即可。
/// 上限 16 支：面板里滚来滚去也不会无限长（超出就整批丢掉重建，换一次卡顿可忽略）。
#[derive(Default)]
pub struct LutCache {
    entries: std::collections::HashMap<String, Arc<Lut>>,
}

impl LutCache {
    /// 取一支（未命中就读文件）。
    ///
    /// # Errors
    /// 记录不存在 / 文件丢失 / 解析失败。
    pub fn get<R: Runtime>(&mut self, app: &AppHandle<R>, id: &str) -> Result<Arc<Lut>, String> {
        if let Some(lut) = self.entries.get(id) {
            return Ok(Arc::clone(lut));
        }
        let lut = resolve(app, id)?;
        if self.entries.len() >= 16 {
            self.entries.clear();
        }
        self.entries.insert(id.to_string(), Arc::clone(&lut));
        Ok(lut)
    }
}

/// 解析结果：能渲染的链 + 解不出来的层。
///
/// 为什么要分开：缩略图/编辑器遇到缺文件应该**跳过这一层继续出图**，
/// 而导出必须**报错**（静默少一层就是另一张图）。所以不在这里替调用方做决定。
pub struct ResolvedChain {
    pub chain: Option<Arc<LutChain>>,
    pub missing: Vec<String>,
}

/// **把编辑栈的链解析成可渲染的一条链**（CPU / GPU / 缩略图 / 导出共用这一份）。
///
/// 规则（`specs/editor-lut-chain.md` §1）：
/// * 总开关关、链为空、或没有一层开着 → `chain = None`（= 不套 LUT）；
/// * 单层关着 → 跳过该层；
/// * 某层文件丢了 → **跳过这一层并记进 `missing`**，其余层照常。
pub fn resolve_chain<R: Runtime>(
    app: &AppHandle<R>,
    layers: &[raybend::store::develop::LutLayer],
    master: Option<bool>,
    cache: &mut LutCache,
) -> ResolvedChain {
    let mut missing = Vec::new();
    if master != Some(true) || layers.is_empty() {
        return ResolvedChain {
            chain: None,
            missing,
        };
    }
    if layers.len() > raybend::store::develop::MAX_LUT_LAYERS {
        // 外部坏数据也不能让 CPU（全部层）与 GPU（前 8 层）各画一张图。
        // 正常写入会被 `DevelopStack::validate` 拒绝；这里让预览/缩略图安全降级，
        // 导出则由 `verify_capture` 判定「链需要 LUT 但没有可渲染结果」而失败。
        eprintln!("[lut] 链超过 {} 层，拒绝渲染", raybend::store::develop::MAX_LUT_LAYERS);
        return ResolvedChain {
            chain: None,
            missing,
        };
    }
    let mut resolved = Vec::new();
    for layer in layers.iter().filter(|layer| layer.enabled) {
        match cache.get(app, &layer.id) {
            Ok(lut) => resolved.push(LutChainLayer {
                lut,
                strength: layer.strength.clamp(0.0, 1.0),
            }),
            Err(error) => {
                eprintln!("[lut] 链里的一层不可用（{}）：{error}", layer.id);
                missing.push(layer.id.clone());
            }
        }
    }
    let chain = (!resolved.is_empty()).then(|| Arc::new(LutChain::new(resolved)));
    ResolvedChain { chain, missing }
}

/// 一次性解析（不跨调用缓存）——缩略图 / 定稿 / 导出这些批次路径用。
pub fn resolve_stack_chain<R: Runtime>(
    app: &AppHandle<R>,
    stack: &raybend::store::develop::DevelopStack,
) -> ResolvedChain {
    resolve_chain(
        app,
        &stack.lut_chain,
        stack.lut_enabled,
        &mut LutCache::default(),
    )
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
    /// 本次导入是不是被用户中止的（中止也返回已完成的那部分）
    pub cancelled: bool,
}

/// 中止正在跑的 LUT 导入（在处理完当前文件后停下，已导入的保留）。
#[tauri::command]
pub fn lut_import_cancel<R: Runtime>(app: AppHandle<R>) {
    app.state::<LutImportCancel>().request();
}

#[tauri::command]
pub async fn lut_import_directory<R: Runtime>(
    app: AppHandle<R>,
    path: String,
    category_id: String,
) -> Result<LutImportDto, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        let cancel = handle.state::<LutImportCancel>();
        cancel.reset();
        /*
         * 进度事件：
         *   scan ── 还没有总数（界面走不确定态，不编百分比）
         *   copy ── 总数已知，每个文件一条（节流），`current` 就是「它没卡住」的证据
         *   done ── 无论完成还是中止都发最后一条，让进度条落地
         */
        let emit = |phase: &'static str,
                    done: usize,
                    total: usize,
                    current: Option<String>,
                    imported: usize,
                    duplicates: usize,
                    skipped: usize,
                    cancelled: bool| {
            // 事件发不出去（窗口关了）不该影响导入
            let _ = handle.emit(
                LUT_IMPORT_PROGRESS_EVENT,
                LutImportProgressDto {
                    phase,
                    done,
                    total,
                    current,
                    imported,
                    duplicates,
                    skipped,
                    cancelled,
                },
            );
        };
        emit("scan", 0, 0, None, 0, 0, 0, false);
        backfill_hashes(&handle)?;
        let source = Path::new(&path);
        let files = discover(source).map_err(|error| error.to_string())?;
        let total = files.len();
        emit("copy", 0, total, None, 0, 0, 0, false);
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
        let mut cancelled = false;
        // 已经处理过的候选文件数（含重复 / 跳过）。中止时必须保留真实进度，
        // 不能拿 total 冒充 done，否则界面会虚报 100%。
        let mut processed = 0;
        let mut throttle = ProgressThrottle::new();
        for (index, file) in files.into_iter().enumerate() {
            // 取消在两张之间生效：当前这张做完再停（硬中断会把半份副本留在库里）。
            if cancel.is_requested() {
                cancelled = true;
                break;
            }
            processed = index + 1;
            if throttle.ready(index == 0) {
                let name = file
                    .file_name()
                    .map(|value| value.to_string_lossy().into_owned());
                emit(
                    "copy",
                    index,
                    total,
                    name,
                    imported,
                    duplicates,
                    skipped.len(),
                    false,
                );
            }
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
        emit(
            "done",
            processed,
            total,
            None,
            imported,
            duplicates,
            skipped.len(),
            cancelled,
        );
        Ok(LutImportDto {
            library: library(&handle, &[])?,
            imported,
            duplicates,
            restored,
            skipped,
            cancelled,
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

#[cfg(test)]
mod tests {
    use super::{LutImportCancel, LutImportProgressDto, PROGRESS_THROTTLE, ProgressThrottle};

    /// 前端按 `camelCase` 读字段（`done` / `total` / `current` / `imported` …）。
    /// 这条测试是**跨语言契约**：改了字段名或大小写，前端会静默读不到数。
    #[test]
    fn progress_dto_is_camel_case() {
        let json = serde_json::to_value(LutImportProgressDto {
            phase: "copy",
            done: 7,
            total: 12,
            current: Some("Kodak 2383.cube".into()),
            imported: 6,
            duplicates: 1,
            skipped: 0,
            cancelled: false,
        })
        .unwrap();
        for key in [
            "phase",
            "done",
            "total",
            "current",
            "imported",
            "duplicates",
            "skipped",
            "cancelled",
        ] {
            assert!(json.get(key).is_some(), "缺字段 {key}");
        }
        assert_eq!(json["phase"], "copy");
        assert_eq!(json["current"], "Kodak 2383.cube");
    }

    /// 扫描阶段没有总数：`total = 0`、`current = null`（前端据此走不确定态）。
    #[test]
    fn scan_progress_has_no_total() {
        let json = serde_json::to_value(LutImportProgressDto {
            phase: "scan",
            done: 0,
            total: 0,
            current: None,
            imported: 0,
            duplicates: 0,
            skipped: 0,
            cancelled: false,
        })
        .unwrap();
        assert_eq!(json["total"], 0);
        assert!(json["current"].is_null());
    }

    /// 首条必须发（否则界面从「没反应」直接跳到 100%）。
    #[test]
    fn throttle_lets_the_first_event_through() {
        let mut throttle = ProgressThrottle::new();
        assert!(throttle.ready(false));
        // 紧跟着的第二条被节流掉
        assert!(!throttle.ready(false));
        // force 无视节流
        assert!(throttle.ready(true));
    }

    /// 节流间隔是一个「小但非零」的窗口——改大了会看不出推进，改小了事件太密。
    #[test]
    fn throttle_window_is_bounded() {
        assert!(PROGRESS_THROTTLE.as_millis() >= 30);
        assert!(PROGRESS_THROTTLE.as_millis() <= 500);
    }

    /// 取消是一次性的显式动作：请求后一直为真，下一次导入开始时才复位。
    #[test]
    fn cancel_flag_resets_per_import() {
        let cancel = LutImportCancel::default();
        assert!(!cancel.is_requested());
        cancel.request();
        assert!(cancel.is_requested());
        // 连读两次仍然为真（不能「读一次就清」——循环每张都要问一次）
        assert!(cancel.is_requested());
        cancel.reset();
        assert!(!cancel.is_requested());
    }
}
