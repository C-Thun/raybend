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

use raybend::media::meta_cache::{MetaCache, MetaInput};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use raybend::media::source;
use raybend::store::{recent, volumes};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

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
// 目录元信息（宽高 + 方向）：会话级内存缓存
// ---------------------------------------------------------------------------

/// 前端传进来的一条「这个文件现在是什么样」——直接来自它已经拿到的目录清单，
/// 因此**不需要再 read_dir 一次**（也不会与界面上的条目错位）。
// 入参也要 ：契约测试靠它把**真实键名**取出来对 JSON 断言
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaFileView {
    /// 目录内的相对名（非递归扫描下就是文件名）
    pub relative: String,
    pub file_size: u64,
    pub mtime_ms: i64,
}

/// 一条照片元信息给前端的样子。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoMetaView {
    pub relative: String,
    /// **已应用方向**的宽（竖图就是 width < height）
    pub width: u32,
    pub height: u32,
    /// 原始 EXIF 方向（1..8）
    pub orientation: u16,
    /*
     * 拍摄时间跟着**同一次读头**一起回来（规格 `specs/source-time-inline.md`）——
     * 以前它是一条单独的命令（`source_times`）：一次读整目录、真机上要 6 秒，
     * 把整条 IPC 与界面都按住了。现在读多宽高就顺路拿它，且随文件身份进缓存。
     */
    pub taken_at_ms: Option<i64>,
    pub taken_at_source: Option<String>,
    pub taken_at_offset_min: Option<i32>,
}

/// 「浏览过的目录」的元信息缓存本体（会话级，进程退出即丢）。
///
/// 用 `Arc<Mutex<...>>` 而不是直接放 `Mutex<MetaCache>`：命令是异步的，
/// 要把锁搬进 `spawn_blocking` 的闭包里 —— 那样闭包必须是 `'static`，
/// 拿 `State` 借来的引用过不去（与 `SourcesThumbs` 用 `Arc` 的理由相同）。
#[derive(Clone, Default)]
pub struct SourcesMetaCache(pub Arc<Mutex<MetaCache>>);

/// 确保某个目录下这批文件的元信息是新鲜的（宽高 + 方向）。
///
/// 读头是 I/O（慢卡上可能几百毫秒），所以走 `spawn_blocking`，**不占 UI 线程**。
/// 缓存命中时几乎不花时间（一次哈希查找）。
///
/// 这个目录如果落在某个**库根**里（浏览网格给老库补比例就是这条路），
/// 顺手把刚读到的展示尺寸写回 `asset_files`（懒回填，只补空值；
/// `specs/display-size-source.md` §3.2）—— 写失败只记日志，不影响这次读数。
#[tauri::command]
pub async fn dir_meta_ensure<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, SourcesMetaCache>,
    path: String,
    files: Vec<MetaFileView>,
    cache_only: Option<bool>,
) -> Result<Vec<PhotoMetaView>, String> {
    let cache = Arc::clone(&state.0);
    blocking(move || {
        let dir = PathBuf::from(&path);
        let inputs: Vec<MetaInput> = files
            .iter()
            .map(|file| MetaInput {
                relative: file.relative.clone(),
                file_size: file.file_size,
                mtime_ms: file.mtime_ms,
            })
            .collect();

        let mut guard = cache
            .lock()
            .map_err(|_| "目录元信息缓存被污染（锁中毒）".to_string())?;
        /*
         * `cache_only`：**只查缓存，一个文件都不读**（崔总 2026-10-11，`specs/source-time-inline.md`）。
         *
         * 拍摄时间的真相是**出图那一次读**顺手放进缓存的（`thumbs.rs` 的 `remember`；
         * 宽高/方向也随它落在同一处）—— 界面任何时候要时间都只走这一支，
         * 所以**不存在「为了补时间再读一遍文件」的第二条线路**：
         * 缓存里没有的那些，界面先用兜底时间，等它们真的出图时自然补上。
         */
        let cached_only = cache_only.unwrap_or(false);
        let batch = if cached_only {
            guard.cached_only(&dir, &inputs, Instant::now())
        } else {
            guard
                .ensure_dir(&dir, &inputs, Instant::now(), |abs| {
                    raybend::media::meta::read_photo_meta(abs)
                })
                .map_err(|error| error.to_string())?
        };
        drop(guard);

        /*
         * 配对继承（**唯一实现在 `media::pairing`**）：RAW 自己没读到拍摄时间时，
         * 借同名位图的。少了这一步，按时间分组会把同一张照片的 JPG 与 RAW
         * 分进不同的片（人类 2026-09-17/18 两次上报「同一天两个分组标题」）。
         *
         * 以前它住在 `read_times`（那条一次读整目录的命令）；2026-10-11 那条命令被并进这里
         * （`specs/source-time-inline.md`），配对也跟着搬过来。
         * 前提：**一次调用里不许把一对拆开**（前端分块时按主名切边界）。
         */
        let mut metas = batch.metas;
        let names: Vec<String> = files.iter().map(|file| file.relative.clone()).collect();
        raybend::media::pairing::inherit_times_in_metas(&names, &mut metas);

        let mut views: Vec<PhotoMetaView> = files
            .iter()
            .zip(metas.iter())
            .map(|(file, meta)| PhotoMetaView {
                relative: file.relative.clone(),
                width: meta.width,
                height: meta.height,
                orientation: meta.orientation,
                taken_at_ms: meta.taken_at_ms,
                taken_at_source: meta
                    .taken_at_source
                    .map(|source| taken_source_code(source).to_string()),
                taken_at_offset_min: meta.taken_at_offset_min,
            })
            .collect();

        if let Some((_repository_id, catalog, rel)) = crate::develop::catalog_for_dir(&app, &dir)
        {
            /*
             * RAW 的**像素数只认 record**（= rawler 出图尺寸）—— 崔总 2026-10-11：
             * 「对 RAW 来说不能用私自解算像素数来替代正式 develop 输出的真实位图像素数」。
             * 文件头那份是整帧、比出图大一圈，所以不进 record、也不进这里的读数：
             * 记录还没建好（老库）就报 0×0，界面按默认占位比例摆，等懒补落地自然变准。
             * 方向仍用文件头读到的（那是 EXIF 语义，不是像素数）。
             */
            let raw: Vec<usize> = files
                .iter()
                .enumerate()
                .filter(|(_, file)| {
                    raybend::media::kind::kind_of_file(&file.relative)
                        == raybend::media::kind::MediaKind::Raw
                })
                .map(|(index, _)| index)
                .collect();
            if !raw.is_empty() {
                let rels: Vec<String> = raw
                    .iter()
                    .map(|index| {
                        let name = &files[*index].relative;
                        if rel.is_empty() {
                            name.clone()
                        } else {
                            format!("{rel}/{name}")
                        }
                    })
                    .collect();
                let sizes = catalog
                    .read(|conn| {
                        rels.iter()
                            .map(|rel_path| {
                                raybend::store::sizes::file_display_size(conn, rel_path)
                            })
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .unwrap_or_else(|error| {
                        // i18n-exempt: 终端诊断（读不到就退回占位比例，不拦这次读数）
                        eprintln!("[source] RAW 尺寸 record 读失败：{error}");
                        vec![None; raw.len()]
                    });
                for (index, size) in raw.iter().zip(sizes) {
                    let orientation = views[*index].orientation;
                    // 拍摄时间照抄读头那一次的（这一支只改尺寸：RAW 的像素数认 record）
                    let taken = &views[*index];
                    let (taken_at_ms, taken_at_source, taken_at_offset_min) = (
                        taken.taken_at_ms,
                        taken.taken_at_source.clone(),
                        taken.taken_at_offset_min,
                    );
                    views[*index] = match size {
                        Some((width, height, orientation)) => PhotoMetaView {
                            relative: views[*index].relative.clone(),
                            width,
                            height,
                            orientation,
                            taken_at_ms,
                            taken_at_source,
                            taken_at_offset_min,
                        },
                        None => PhotoMetaView {
                            relative: views[*index].relative.clone(),
                            width: 0,
                            height: 0,
                            orientation,
                            taken_at_ms,
                            taken_at_source,
                            taken_at_offset_min,
                        },
                    };
                }
            }

            let entries: Vec<(String, u32, u32, u16)> = views
                .iter()
                .map(|view| {
                    (
                        view.relative.clone(),
                        view.width,
                        view.height,
                        view.orientation,
                    )
                })
                .collect();
            if let Err(error) =
                raybend::store::sizes::backfill_dir_display_sizes(&catalog, &dir, &entries)
            {
                // i18n-exempt: 终端诊断（懒回填失败不拦这次读数，下次再来）
                eprintln!("[source] 老库尺寸回填跳过：{error}");
            }
        }

        Ok(views)
    })
    .await
}

// ---------------------------------------------------------------------------
// 视图类型（`src/api/types.ts` 是它们的镜像）
// ---------------------------------------------------------------------------

/// 最近导入过的一个目录。
#[derive(Debug, Default, Serialize)]
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
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeView {
    pub path: String,
    /// `local` / `removable` / `optical` / `network` / `cloud` / `unknown`。
    pub kind: String,
    /// 中文名（提示文案用）。
    pub kind_label: String,
}

/// 目录树里的一个子目录。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntryView {
    pub name: String,
    pub path: String,
    /// 里面还有没有会显示出来的子目录（树上的箭头）—— 人类 2026-09-19：
    /// 不允许多扫一层之前那种「没子目录也画箭头」的误导。
    pub has_children: bool,
}

/// 中列里的一张照片。
#[derive(Debug, Default, Serialize)]
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
    /// 拍摄时间的时区偏移（分钟，东八区 = 480）。
    /// `None` = 相机没写时区，毫秒是「墙上时间当 UTC」（见 M1-3 的口径），
    /// 界面上要按 UTC 显示才对得上相机里的数字。
    pub taken_at_offset_min: Option<i32>,
}

/// 一次目录列取的结果。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceScanView {
    pub root: String,
    pub items: Vec<SourceItemView>,
    pub skipped: usize,
    pub problems: Vec<String>,
    pub elapsed_ms: i64,
}

/// 照片计数。
#[derive(Debug, Default, Serialize)]
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
    pub tags: Vec<raybend::media::exif::ExifField>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub software: Option<String>,
    pub gps_lat: Option<f64>,
    pub gps_lon: Option<f64>,
    pub datetime_raw: Option<String>,
    pub focal_mm: Option<f64>,
    pub f_number: Option<f64>,
    /// 快门时间（**毫秒**；界面自己换算成「1/125s」这种写法）。
    pub exposure_ms: Option<f64>,
    /// 曝光补偿（EV，负值 = 减光）。编辑右栏「与调节相关」那组要用。
    pub exposure_bias_ev: Option<f64>,
    pub iso: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub orientation: Option<i64>,
    pub taken_at_ms: Option<i64>,
    pub taken_at_source: Option<String>,
    pub taken_at_offset_min: Option<i32>,
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
        TakenAtSource::FileCreated => "file_created",
        TakenAtSource::FileMtime => "file_mtime",
        TakenAtSource::Sibling => "sibling",
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
pub async fn recent_dirs_list<R: Runtime>(app: AppHandle<R>) -> Result<Vec<RecentDirView>, String> {
    blocking(move || {
        app.state::<DbState>().with(&app, |db| {
            let limit = recent_limit(db);
            db.read(|conn| recent::list(conn, limit))
                .map(|rows| rows.into_iter().map(RecentDirView::from).collect())
                .map_err(|e| e.to_string())
        })
    })
    .await
}

/// 记一条最近目录，仍复用既有写者和裁剪逻辑。
#[tauri::command]
pub async fn recent_dir_remember<R: Runtime>(
    app: AppHandle<R>,
    path: String,
    include_subdirs: bool,
) -> Result<(), String> {
    blocking(move || {
        app.state::<DbState>().with(&app, |db| {
            let limit = recent_limit(db);
            let now = raybend::store::time::now_millis();
            db.write_tx(move |tx| {
                recent::remember(tx, &path, include_subdirs, now)?;
                recent::prune(tx, limit)?;
                Ok(())
            })
            .map_err(|e| e.to_string())
        })
    })
    .await
}

/// 从最近列表移除，不动磁盘；startup 等待也不占窗口线程。
#[tauri::command]
pub async fn recent_dir_forget<R: Runtime>(
    app: AppHandle<R>,
    path: String,
) -> Result<bool, String> {
    blocking(move || {
        app.state::<DbState>().with(&app, |db| {
            db.write(move |conn| recent::forget(conn, &path))
                .map_err(|e| e.to_string())
        })
    })
    .await
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
                        has_children: d.has_children,
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
        taken_at_source: item
            .taken_at
            .map(|t| taken_source_code(t.source).to_string()),
        taken_at_offset_min: item.taken_at.and_then(|t| t.offset_min),
    }
}

/// 数一个目录里有多少张照片（喂「已选择 N 张照片」）。
#[tauri::command]
pub async fn source_count(path: String, recursive: bool) -> Result<PhotoCountView, String> {
    blocking(move || {
        let count = source::count_photos(Path::new(&path), recursive).map_err(|e| e.to_string())?;
        Ok(PhotoCountView {
            photos: count.photos,
            skipped: count.skipped,
            truncated: count.truncated,
        })
    })
    .await
}

/// 一批路径现在还是不是目录（给「最近目录」标灰用；判据轻、可批量）。
#[tauri::command]
pub async fn source_paths_status(paths: Vec<String>) -> Result<Vec<bool>, String> {
    blocking(move || Ok(raybend::media::source::paths_are_dirs(&paths))).await
}

/*
 * 这里以前有一条 `source_times`（一次读整目录的拍摄时间）：真机实测它在 306 张的目录上
 * 要 **6 秒**，而这 6 秒里整条 IPC 与界面都被它按住（缩略图全部排队）——
 * 2026-10-11 崔总拍板把它并进「读文件信息」（`dir_meta_ensure`：同一次 EXIF 读顺路带回时间），
 * 从此**不再有第二条专读时间的命令**。规格：`specs/source-time-inline.md`。
 */

/// 一个文件的 EXIF（喂 `flowbar`）。
///
/// **缓存优先**（崔总 2026-10-11，`specs/source-time-inline.md`）：出图那一次读
///（`thumbs.rs`）已经把**整块 EXIF** 留在这张图的缓存里 —— 鼠标点/停到一张图上时
/// flowinfo 立刻有内容，**不读盘**。缓存里没有（还没出过图、或文件被换过）才真读一次，
/// 顺带写回缓存。RAW 的镜头名仍旧**懒取**（`Low` 档，选中才拿，不顶掉出图）。
#[tauri::command]
pub async fn file_exif(
    state: tauri::State<'_, SourcesMetaCache>,
    path: String,
) -> Result<FileExifView, String> {
    // 把锁搬进 `spawn_blocking` 的闭包：闭包必须 `'static`，借来的 `State` 过不去
    let cache = Arc::clone(&state.0);
    blocking(move || {
        let abs = Path::new(&path);
        let file_name = abs
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let kind = raybend::media::kind::kind_of_file(&file_name);
        let metadata = std::fs::metadata(abs).ok();
        let file_size = metadata.as_ref().map(std::fs::Metadata::len).unwrap_or(0);
        let mtime_ms = metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .map(raybend::store::time::from_system_time)
            .unwrap_or(0);
        let dir = abs.parent().unwrap_or_else(|| Path::new(""));

        // ① 缓存优先（零 I/O）
        let cached = cache
            .lock()
            .map(|mut guard| {
                guard
                    .exif_for(dir, &file_name, file_size, mtime_ms)
                    .map(|cached| (cached.data, cached.fields))
            })
            .ok()
            .flatten();

        let (mut data, tags) = match cached {
            Some((data, fields)) => (data, fields),
            None => {
                // ② 没有就真读一次（同一个文件一段头，两份结果一起解），并写回缓存
                let (data, fields) = raybend::media::exif::read_bundle_for(abs);
                let taken = raybend::media::exif::resolve_taken_at(
                    Some(&data),
                    &file_name,
                    (mtime_ms != 0).then_some(mtime_ms),
                );
                let orientation = raybend::media::meta::normalize_orientation(data.orientation);
                let (width, height) = match (data.width, data.height) {
                    (Some(w), Some(h)) if w > 0 && h > 0 => raybend::media::meta::oriented_size(
                        u32::try_from(w).unwrap_or(0),
                        u32::try_from(h).unwrap_or(0),
                        orientation,
                    ),
                    _ => (0, 0),
                };
                if let Ok(mut guard) = cache.lock() {
                    guard.remember(
                        dir,
                        &file_name,
                        raybend::media::meta::PhotoMeta {
                            width,
                            height,
                            orientation,
                            taken_at_ms: taken.map(|t| t.millis),
                            taken_at_source: taken.map(|t| t.source),
                            taken_at_offset_min: taken.and_then(|t| t.offset_min),
                            file_size,
                            mtime_ms,
                        },
                        Some(raybend::media::meta_cache::CachedExif {
                            data: data.clone(),
                            fields: fields.clone(),
                        }),
                        std::time::Instant::now(),
                    );
                }
                (data, fields)
            }
        };

        // RAW 的镜头名懒取（Low 档；缓存里那份没带就补一次）
        if kind == raybend::media::kind::MediaKind::Raw
            && data.lens.as_ref().is_none_or(|name| name.trim().is_empty())
        {
            let _permit = raybend::raw::worker::acquire(raybend::raw::DecodePriority::Low);
            data.lens = raybend::raw::worker::shared()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .lens_name(abs)
                .ok()
                .flatten();
        }

        let taken = raybend::media::exif::resolve_taken_at(
            Some(&data),
            &file_name,
            (mtime_ms != 0).then_some(mtime_ms),
        );
        Ok(FileExifView {
            tags,
            camera_make: data.camera_make,
            camera_model: data.camera_model,
            lens: data.lens,
            software: data.software,
            gps_lat: data.gps.as_ref().map(|gps| gps.lat),
            gps_lon: data.gps.as_ref().map(|gps| gps.lon),
            datetime_raw: data.datetime_raw,
            focal_mm: data.focal_mm,
            f_number: data.f_number,
            exposure_ms: data.exposure_ms,
            exposure_bias_ev: data.exposure_bias_ev,
            iso: data.iso,
            width: data.width,
            height: data.height,
            orientation: data.orientation,
            taken_at_ms: taken.map(|t| t.millis),
            taken_at_source: taken.map(|t| taken_source_code(t.source).to_string()),
            taken_at_offset_min: taken.and_then(|t| t.offset_min),
            ext: raybend::media::kind::extension(&file_name),
            kind: kind_code(kind).to_string(),
        })
    })
    .await
}

/// 忘掉一个目录的元信息缓存（拔盘再插回来时用：缓存里的未必还是盘上的）。
#[tauri::command]
pub async fn dir_meta_forget(
    state: tauri::State<'_, SourcesMetaCache>,
    path: String,
) -> Result<(), String> {
    if let Ok(mut guard) = state.0.lock() {
        guard.forget_dir(Path::new(&path));
    }
    Ok(())
}
