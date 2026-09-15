//! 「来源」相关命令：最近目录、驱动器、目录树的一层、照片清单、拍摄时间。
//!
//! 这一层**只做三件事**：参数转换、后台线程调度、错误转字符串。
//! 业务规则全在 `raybend` crate 内（`store::recent` / `store::volumes` / `media::source`），
//! 那边有单测；这里保持薄，`AGENTS.md` §4 的分层原则。
//!
//! # 为什么命令都是 `async`
//!
//! Tauri 里**同步命令跑在主线程**上 —— 读一个目录、扫一遍目录都可能几百毫秒，
//! 放在主线程上就是「界面卡住」。所以凡是碰文件系统的命令都写成 `async`，
//! 真正的活在 `spawn_blocking` 里干（见 [`blocking`]）。

use std::path::{Path, PathBuf};

use raybend::media::source;
use raybend::store::{recent, volumes};
use serde::Serialize;
use tauri::{AppHandle, Runtime, State};

use crate::db::DbState;

/// 设置键：最近目录保留多少条（用户可在设置里改；读不到就用默认值）。
pub const RECENT_LIMIT_KEY: &str = "source.recent_limit";

/// 默认保留条数。
pub const DEFAULT_RECENT_LIMIT: usize = recent::DEFAULT_LIMIT;

/// 在后台线程里干一件阻塞的活。
///
/// `F` 返回 `Result<T, String>`：错误在这一层就已经是「给用户看的话」了。
pub async fn blocking<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("后台任务失败：{e}"))?
}

// ---------------------------------------------------------------------------
// 视图类型（`src/api/types.ts` 是它们的镜像）
// ---------------------------------------------------------------------------

/// 最近导入过的一个目录。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentDirView {
    pub path: String,
    pub include_subdirs: bool,
    pub used_at: i64,
    pub use_count: i64,
}

impl From<recent::RecentDir> for RecentDirView {
    fn from(row: recent::RecentDir) -> Self {
        Self {
            path: row.path,
            include_subdirs: row.include_subdirs,
            used_at: row.used_at,
            use_count: row.use_count,
        }
    }
}

/// 一个可选来源（驱动器 / 挂载点）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeView {
    pub path: String,
    /// `local` / `removable` / `optical` / `network` / `cloud` / `unknown`。
    pub kind: String,
    /// 中文名（提示文案用）。
    pub kind_label: String,
}

/// 目录树里的一个子目录。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntryView {
    pub name: String,
    pub path: String,
}

/// 中列里的一张照片。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceItemView {
    pub path: String,
    pub file_name: String,
    pub ext: Option<String>,
    /// `raw` / `image`。
    pub kind: String,
    pub size_bytes: u64,
    pub mtime_ms: Option<i64>,
    /// 快速兜底拍摄时间（Unix 毫秒）。
    pub taken_at_ms: Option<i64>,
    /// `exif` / `filename` / `file_mtime`。
    pub taken_at_source: Option<String>,
}

/// 一次目录列取的结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceScanView {
    pub root: String,
    pub items: Vec<SourceItemView>,
    pub skipped: usize,
    pub problems: Vec<String>,
    pub elapsed_ms: i64,
}

/// 一个文件的精确拍摄时间。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeEntryView {
    pub path: String,
    pub taken_at_ms: Option<i64>,
    pub taken_at_source: Option<String>,
}

/// 照片计数。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoCountView {
    pub photos: usize,
    pub skipped: usize,
    pub truncated: bool,
}

/// 一个文件的 EXIF（喂 `flowbar` 的信息区）。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileExifView {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub focal_mm: Option<f64>,
    pub f_number: Option<f64>,
    /// 快门时间（**毫秒**；界面自己换算成「1/125s」这种写法）。
    pub exposure_ms: Option<f64>,
    pub iso: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub orientation: Option<i64>,
    pub taken_at_ms: Option<i64>,
    pub taken_at_source: Option<String>,
    /// 小写扩展名（不含点）。
    pub ext: Option<String>,
    /// `raw` / `image`。
    pub kind: String,
}

/// 拍摄时间来源的稳定标识。
fn taken_source_code(source: raybend::media::exif::TakenAtSource) -> &'static str {
    use raybend::media::exif::TakenAtSource;
    match source {
        TakenAtSource::Exif => "exif",
        TakenAtSource::Filename => "filename",
        TakenAtSource::FileMtime => "file_mtime",
    }
}

/// 媒体大类的稳定标识。
fn kind_code(kind: raybend::media::kind::MediaKind) -> &'static str {
    use raybend::media::kind::MediaKind;
    match kind {
        MediaKind::Raw => "raw",
        MediaKind::Image => "image",
        MediaKind::Other => "other",
    }
}

// ---------------------------------------------------------------------------
// 命令
// ---------------------------------------------------------------------------

/// 最近的导入目录（最新的在前）。
#[tauri::command]
pub fn recent_dirs_list<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DbState>,
) -> Result<Vec<RecentDirView>, String> {
    state.with(&app, |db| {
        let limit = recent_limit(db);
        db.read(|conn| recent::list(conn, limit))
            .map(|rows| rows.into_iter().map(RecentDirView::from).collect())
            .map_err(|e| e.to_string())
    })
}

/// 记一条最近目录（勾选目录时调）—— 顺手把列表裁到上限。
#[tauri::command]
pub fn recent_dir_remember<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DbState>,
    path: String,
    include_subdirs: bool,
) -> Result<(), String> {
    state.with(&app, |db| {
        let limit = recent_limit(db);
        let now = raybend::store::time::now_millis();
        db.write_tx(move |tx| {
            recent::remember(tx, &path, include_subdirs, now)?;
            recent::prune(tx, limit)?;
            Ok(())
        })
        .map_err(|e| e.to_string())
    })
}

/// 从最近列表里移除一条（**不动磁盘上的任何东西**）。
#[tauri::command]
pub fn recent_dir_forget<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DbState>,
    path: String,
) -> Result<bool, String> {
    state.with(&app, |db| {
        db.write(move |conn| recent::forget(conn, &path))
            .map_err(|e| e.to_string())
    })
}

/// 最近目录条数上限（设置里可改）。
fn recent_limit(db: &raybend::store::db::AppDb) -> usize {
    db.get_setting(RECENT_LIMIT_KEY)
        .ok()
        .flatten()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(DEFAULT_RECENT_LIMIT)
}

/// 本机可选的来源（驱动器 / 挂载点）。
#[tauri::command]
pub async fn volumes_list() -> Result<Vec<VolumeView>, String> {
    blocking(|| {
        Ok(volumes::list()
            .into_iter()
            .map(|v| VolumeView {
                path: v.path,
                kind: v.kind.code().to_string(),
                kind_label: v.kind.label().to_string(),
            })
            .collect())
    })
    .await
}

/// 一个目录的**直接子目录**。
#[tauri::command]
pub async fn dir_list(path: String) -> Result<Vec<DirEntryView>, String> {
    blocking(move || {
        source::list_dirs(Path::new(&path))
            .map(|dirs| {
                dirs.into_iter()
                    .map(|d| DirEntryView {
                        name: d.name,
                        path: d.path.to_string_lossy().into_owned(),
                    })
                    .collect()
            })
            .map_err(|e| e.to_string())
    })
    .await
}

/// 一个目录里的照片（**不下钻子目录** —— 那是导入时的事）。
#[tauri::command]
pub async fn source_scan(path: String) -> Result<SourceScanView, String> {
    blocking(move || {
        let listing = source::scan_photos(Path::new(&path), false).map_err(|e| e.to_string())?;
        Ok(SourceScanView {
            root: path,
            items: listing.items.into_iter().map(item_view).collect(),
            skipped: listing.skipped,
            problems: listing.problems,
            elapsed_ms: listing.elapsed_ms,
        })
    })
    .await
}

fn item_view(item: source::SourceItem) -> SourceItemView {
    SourceItemView {
        path: item.path.to_string_lossy().into_owned(),
        file_name: item.file_name,
        ext: item.ext,
        kind: kind_code(item.kind).to_string(),
        size_bytes: item.size_bytes,
        mtime_ms: item.mtime_ms,
        taken_at_ms: item.taken_at.map(|t| t.millis),
        taken_at_source: item.taken_at.map(|t| taken_source_code(t.source).to_string()),
    }
}

/// 数一个目录里有多少张照片（喂「已选择 N 张照片」）。
#[tauri::command]
pub async fn source_count(path: String, recursive: bool) -> Result<PhotoCountView, String> {
    blocking(move || {
        let count =
            source::count_photos(Path::new(&path), recursive).map_err(|e| e.to_string())?;
        Ok(PhotoCountView {
            photos: count.photos,
            skipped: count.skipped,
            truncated: count.truncated,
        })
    })
    .await
}

/// 并行读一批文件的**真实**拍摄时间（用户按下「按时间」时才调）。
#[tauri::command]
pub async fn source_times(paths: Vec<String>) -> Result<Vec<TimeEntryView>, String> {
    blocking(move || {
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        Ok(source::read_times(&paths)
            .into_iter()
            .map(|entry| TimeEntryView {
                path: entry.path.to_string_lossy().into_owned(),
                taken_at_ms: entry.taken_at.map(|t| t.millis),
                taken_at_source: entry.taken_at.map(|t| taken_source_code(t.source).to_string()),
            })
            .collect())
    })
    .await
}

/// 一个文件的 EXIF（喂 `flowbar`）。
#[tauri::command]
pub async fn file_exif(path: String) -> Result<FileExifView, String> {
    blocking(move || {
        let abs = Path::new(&path);
        let data = raybend::media::exif::read_file(abs);
        let file_name = abs
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let mtime_ms = std::fs::metadata(abs)
            .ok()
            .and_then(|m| m.modified().ok())
            .map(raybend::store::time::from_system_time);
        let taken = raybend::media::exif::resolve_taken_at(Some(&data), &file_name, mtime_ms);
        let kind = raybend::media::kind::kind_of_file(&file_name);

        Ok(FileExifView {
            camera_make: data.camera_make,
            camera_model: data.camera_model,
            lens: data.lens,
            focal_mm: data.focal_mm,
            f_number: data.f_number,
            exposure_ms: data.exposure_ms,
            iso: data.iso,
            width: data.width,
            height: data.height,
            orientation: data.orientation,
            taken_at_ms: taken.map(|t| t.millis),
            taken_at_source: taken.map(|t| taken_source_code(t.source).to_string()),
            ext: raybend::media::kind::extension(&file_name),
            kind: kind_code(kind).to_string(),
        })
    })
    .await
}
