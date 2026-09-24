//! 缩略图命令：**导入工作区里还没入库的源文件**也要能出图（`plans/M1-5.md` §3.1）。
//!
//! # 为什么单独一个缓存库
//!
//! M1-3 的缓存是「每库一个 `thumbs.db`」，而源目录还不属于任何库。这里的做法是
//! 给源文件留一个固定的小库：`<app data>/cache/_sources/thumbs.db`。
//! 键仍然优先用**文件身份**（改名/移动后仍命中），读不到身份才退回**绝对路径** ——
//! 所以它将来跟库内的缓存不会互相打架（同一个文件在两处的键是一样的）。
//!
//! # 为什么返回裸字节
//!
//! `tauri::ipc::Response` 走的是**原始字节**通道，不经过 JSON/base64。
//! 一张 384px 的缩略图约 30–60KB，base64 会白白多三分之一的传输量；
//! 前端拿到 `ArrayBuffer` 后转 `Blob` → `URL.createObjectURL`，交给浏览器自己缓存。
//!
//! # 并发
//!
//! 渲染一张要几十到两百毫秒。**限流放在前端**（同时最多 4 个在飞），
//! 这里不再自己排队 —— 免得两处同时限流、互相等。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use raybend::display::{self, FullCache, ImagePurpose, ImageRequest, DEFAULT_BINS};
use raybend::store::develop::IssueChoice;
use raybend::store::time;
use raybend::thumbnail::render::{PIPELINE_VERSION, render_file_with_edit};
use raybend::thumbnail::{SizeClass, ThumbsDb, render_now, render_now_with_edit};
use tauri::{AppHandle, Manager, Runtime};

use crate::develop::{self, ResolvedAsset};

/// 源文件缩略图缓存所在的子目录名。
///
/// 前面加下划线是**刻意**的：库 ID 是 base62（不会以下划线开头），
/// 所以这个名字永远不可能与某个真实库的缓存目录撞上。
pub const SOURCES_CACHE_DIR: &str = "_sources";

/// 源文件缩略图缓存库的持有者（延迟打开，全程共用一份）。
///
/// 用一个 `Arc<ThumbsDb>` 而不是直接存 `ThumbsDb`：**取出引用就放开锁**，
/// 否则所有缩略图请求会被串行化在一把锁上（渲染本来就慢，再串起来就没法滚动了）。
#[derive(Default)]
pub struct SourcesThumbs {
    inner: Mutex<Option<Arc<ThumbsDb>>>,
}

impl SourcesThumbs {
    /// 取（必要时打开）缓存库。
    fn get(&self, dir: &Path, now_ms: i64) -> Result<Arc<ThumbsDb>, String> {
        let mut guard = self.inner.lock().map_err(|_| "内部锁已损坏".to_string())?;
        if let Some(db) = guard.as_ref() {
            return Ok(Arc::clone(db));
        }
        let db = Arc::new(ThumbsDb::open(dir, now_ms).map_err(|e| e.to_string())?);
        *guard = Some(Arc::clone(&db));
        Ok(db)
    }
}

/// 源文件缩略图的存放目录（`<app data>/cache/_sources/`）。
fn sources_cache_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(crate::db::data_dir(app)?.join("cache").join(SOURCES_CACHE_DIR))
}

/// 取一张缩略图的字节。
///
/// `size`：`"grid"`（网格，长边 384）或 `"strip"`（胶片带，长边 192）；缺省 `grid`。
#[tauri::command]
pub async fn thumb_get<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, SourcesThumbs>,
    path: String,
    size: Option<String>,
) -> Result<tauri::ipc::Response, String> {
    // 缓存库要先拿出来（`State` 不能跨 await 持有）
    let cache_dir = sources_cache_dir(&app)?;
    let db = {
        let now = time::now_millis();
        state.get(&cache_dir, now)?
    };
    let size = SizeClass::parse(size.as_deref().unwrap_or("grid"))
        .ok_or_else(|| format!("未知的缩略图尺度：{}", size.as_deref().unwrap_or("")))?;

    // 编辑过的照片：缩略图也要反映编辑结果（M3-W3）。
    // 解析只在**库内**文件上命中（源文件未入库时返回 None，走老路）。
    let edit = edited_stack(&app, &path);

    let bytes = crate::source::blocking(move || {
        let bytes = match &edit {
            Some(stack) => render_now_with_edit(
                &db,
                Path::new(&path),
                size,
                time::now_millis(),
                Some(stack),
            ),
            None => render_now(&db, Path::new(&path), size, time::now_millis()),
        }
        .map_err(|e| e.to_string())?;
        Ok(bytes)
    })
    .await?;

    Ok(tauri::ipc::Response::new(bytes))
}

/// 这个路径的资产编辑过吗？编辑过就把它的编辑栈取出来（给渲染用）。
///
/// 未入库 / 没编辑过 / 库离线 —— 一律 `None`（调用方走没有编辑的那条路）。
fn edited_stack<R: Runtime>(
    app: &AppHandle<R>,
    path: &str,
) -> Option<raybend::store::develop::DevelopStack> {
    let asset = develop::resolve_asset(app, Path::new(path))?;
    let (choice, stack) = develop::issue_of(app, &asset).ok()?;
    if choice == IssueChoice::Latest && !stack.is_empty() {
        Some(stack)
    } else {
        None
    }
}

/// **大图缓存**：`<库根>/cache/full/<asset>/latest-<base>-v<pipeline>.avif`。
///
/// 命中直接给；没命中就渲染一遍再写进去（写失败只记一句 —— 缓存写不进去
/// 只意味着下次再渲染一遍，不该让看图失败）。
///
/// **基准从 `asset.rel_path` 推**（`EditBase::of_file`）：调用方拿哪个文件来渲染，
/// 缓存就落在哪一侧 —— 否则切了基准会读到另一基准的旧图（`IMAGING.md` §4.3）。
///
/// `pub(crate)`：`view_image` 与 `develop_preview_refresh`（进/出编辑那两下）共用这一份 ——
/// preview 只允许有一条生成路径。
pub(crate) fn render_latest_cached(
    asset: &ResolvedAsset,
    stack: &raybend::store::develop::DevelopStack,
) -> Result<Vec<u8>, String> {
    let cache = FullCache::open(&asset.root).map_err(|e| e.to_string())?;
    let full = asset
        .root
        .join(asset.rel_path.replace('/', std::path::MAIN_SEPARATOR_STR));
    let base = raybend::store::develop::EditBase::of_file(&full);
    if let Some(bytes) = cache.read(asset.asset_id, "latest", base, PIPELINE_VERSION) {
        return Ok(bytes);
    }
    let thumb = render_file_with_edit(&full, SizeClass::Screen, Some(stack))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("解不开这张照片：{}", full.display()))?;
    if let Err(error) = cache.write(asset.asset_id, "latest", base, PIPELINE_VERSION, &thumb.data)
    {
        eprintln!("[develop] 大图缓存写失败（不影响显示）：{error}");
    }
    Ok(thumb.data)
}

/// **统一取图口**（`crates/raybend/src/display`，口径见 `plans/M2-W2.md` §2.1）。
///
/// 与 `thumb_get` 的区别：那个是「给我一张源文件的网格/胶片带小图」的专用命令；
/// 这个是 **view 与缩略图共用的总入口** —— 调用方只说「哪张、要多大、有没有编辑」，
/// 由 `display` 决定它是 RAW 还是位图、该给原图还是该渲染。
///
/// **带磁盘缓存**（`display::cached_image`）：渲染结果写进源文件缓存库
/// （与 `thumb_get` 同一个 `_sources/thumbs.db`、同一套缓存键），所以连续看图、
/// 邻图预载都不会重复解码 —— 旧的裸 `display_image` 每次都重解，一张 RAW 要一秒多。
///
/// `purpose`：`"grid"` / `"strip"` / `"screen"` / `"original"`；缺省 `screen`（看图）。
/// 位图 + `original` + 没编辑过 ⇒ **直接给原文件字节**（不经渲染管线，也不缓存）。
#[tauri::command]
pub async fn view_image<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, SourcesThumbs>,
    path: String,
    purpose: Option<String>,
) -> Result<tauri::ipc::Response, String> {
    let text = purpose.as_deref().unwrap_or("screen");
    let purpose = ImagePurpose::parse(text)
        .ok_or_else(|| format!("未知的取图用途：{text}"))?;

    /*
     * **编辑过的照片：非编辑器里也看编辑结果**（人类 2026-09-24 定的口径）。
     *
     * 只有 `screen`（全图查看）走这条路：
     * * `grid` / `strip` 由 `thumb_get` 负责（它有自己的缓存库）；
     * * `original` 的语义是「要原文件本身」（导出、互操作），**不该**被编辑结果替换。
     *
     * 顺序也重要：先看库里的大图缓存（AVIF，命中就毫秒级返回），没有再渲染并写回。
     * 没编辑过（`SOOC` / `RAW`）时**一步都不多做** —— 直接落到下面那条老路。
     */
    if purpose == ImagePurpose::Screen
        && let Some(asset) = develop::resolve_asset(&app, Path::new(&path))
        && let Ok((IssueChoice::Latest, stack)) = develop::issue_of(&app, &asset)
    {
        let bytes = crate::source::blocking(move || render_latest_cached(&asset, &stack)).await?;
        return Ok(tauri::ipc::Response::new(bytes));
    }

    // 缓存库要先拿出来（`State` 不能跨 await 持有）——与 `thumb_get` 同一条路
    let cache_dir = sources_cache_dir(&app)?;
    let db = {
        let now = time::now_millis();
        state.get(&cache_dir, now)?
    };
    let bytes = crate::source::blocking(move || {
        let request = ImageRequest::plain(Path::new(&path), purpose);
        let image = display::cached_image(&db, &request, time::now_millis())
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "取不到这张图（类型认不出或解不开）".to_string())?;
        Ok(image.bytes)
    })
    .await?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// 看图态右栏的**直方图**（24 柱 RGB 合成；`plans/M2-W2.md` 1.6）。
///
/// 统计在 Rust 侧做（`AGENTS.md` §6.1 的红线：前端不碰像素）——
/// 前端只拿到 86 个浮点采样去画曲线。取的是**像素口**的 `Screen` 档（长边 1920）RGB 像素，
/// **不经过编码**（直方图只要像素；2026-09-24 起不再绕道字节）。
#[tauri::command]
pub async fn image_histogram(path: String, bins: Option<usize>) -> Result<HistogramDto, String> {
    // 采样口径固定为 86（0 单独，其余每 3 级平均）；保留参数只为旧前端兼容。
    let _requested_bins = bins;
    let bins = DEFAULT_BINS;
    crate::source::blocking(move || {
        let histogram = raybend::display::histogram_of_file(Path::new(&path), bins)
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| raybend::display::Histogram::empty(bins));
        Ok(HistogramDto::from(histogram))
    })
    .await
}

/// 直方图的传输形状（前端画柱子用）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistogramDto {
    pub bins: usize,
    /// 每个桶的计数（长度都等于 `bins`）
    pub r: Vec<f64>,
    pub g: Vec<f64>,
    pub b: Vec<f64>,
    /// 三通道合并后的峰值（前端按它归一化柱高）
    pub max: f64,
}

impl From<raybend::display::Histogram> for HistogramDto {
    fn from(h: raybend::display::Histogram) -> Self {
        Self {
            bins: h.bins,
            r: h.r,
            g: h.g,
            b: h.b,
            max: h.max,
        }
    }
}

/// 源文件缩略图缓存的统计（设置面板与排错用）。
#[tauri::command]
pub async fn thumb_sources_stats<R: Runtime>(
    app: AppHandle<R>,
) -> Result<raybend::thumbnail::CacheStats, String> {
    let cache_dir = sources_cache_dir(&app)?;
    let state = app.state::<SourcesThumbs>();
    let db = state.get(&cache_dir, time::now_millis())?;
    db.read(raybend::thumbnail::cache::stats)
        .map_err(|e| e.to_string())
}
