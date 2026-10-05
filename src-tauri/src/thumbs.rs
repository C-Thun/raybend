//! 缩略图命令：**导入工作区里还没入库的源文件**也要能出图（`specs/M1-5.md` §3.1）。
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

use raybend::display::{self, DEFAULT_BINS, FullCache, ImagePurpose, ImageRequest};
use raybend::store::develop::IssueChoice;
use raybend::store::time;
use raybend::thumbnail::render::PIPELINE_VERSION;
use raybend::thumbnail::worker::render_cached_with;
use raybend::thumbnail::{SizeClass, ThumbsDb, render_now};
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
    pub(crate) fn get(&self, dir: &Path, now_ms: i64) -> Result<Arc<ThumbsDb>, String> {
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
pub(crate) fn sources_cache_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(crate::db::data_dir(app)?
        .join("cache")
        .join(SOURCES_CACHE_DIR))
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

    // 来源查库、lensfun 解析与渲染同在后台线程；缩略图多张并发时
    // 不能让这些同步工作占住 IPC executor，拖慢编辑换图与状态轮询。
    let handle = app.clone();
    let bytes = crate::source::blocking(move || {
        let _permit=handle.state::<crate::browse::BrowseState>().path_task(&handle,Path::new(&path))?;
        let edit = edited_source(&handle, &path)?;
        let bytes = match &edit {
            Some((asset, stack, source)) => render_profile_thumb(
                &handle, &db, asset, source, stack, size, Some("latest")),
            None => render_now(&db, Path::new(&path), size, time::now_millis()),
        }
        .map_err(|e| e.to_string())?;
        Ok(bytes)
    })
    .await?;

    Ok(tauri::ipc::Response::new(bytes))
}

/// 所有 profile 小图共用：thumb → 已有 preview 缩小 → 按请求尺寸重建。
/// 不因读取小图而创建一份新的 1920 缓存。
pub(crate) fn render_profile_thumb<R: Runtime>(
    app: &AppHandle<R>, db: &ThumbsDb, asset: &ResolvedAsset, source: &Path,
    stack: &raybend::store::develop::DevelopStack, size: SizeClass,
    preview_variant: Option<&str>,
) -> raybend::Result<Vec<u8>> {
    render_cached_with(db, source, size, time::now_millis(), Some(stack), || {
        let signature = raybend::media::source::source_signature(source)?;
        let preview = preview_variant.and_then(|variant| FullCache::open(&asset.root).ok()
            .and_then(|cache| cache.read(asset.asset_id, &FullCache::source_name(variant, &signature),
                stack.source_base, PIPELINE_VERSION)));
        raybend::thumbnail::render::render_preview_or_else(preview.as_deref(), size, || {
            let lens = crate::lens::render_correction(app, &asset.repository_id, asset.asset_id,
                stack.lens_profile.as_deref(), stack.lens_enabled);
            let lut = stack.lut_id.as_deref().filter(|_| stack.lut_enabled == Some(true))
                .map(|id| crate::lut::resolve(app, id)).transpose()
                .map_err(raybend::Error::Unsupported)?;
            raybend::thumbnail::render::render_file_with_profiles(
                source, size, Some(stack), lens.as_ref(), lut.as_deref(), |id, role| crate::color_profiles::resolve(app, id, role))
        })
    })
}

/// 这个路径的资产编辑过吗？编辑过就把它的编辑栈取出来（给渲染用）。
///
/// 未入库 / 没编辑过 / 库离线 —— 一律 `None`（调用方走没有编辑的那条路）。
fn edited_source<R: Runtime>(
    app: &AppHandle<R>,
    path: &str,
) -> Result<
    Option<(
        ResolvedAsset,
        raybend::store::develop::DevelopStack,
        PathBuf,
    )>,
    String,
> {
    let Some(asset) = develop::resolve_asset(app, Path::new(path)) else {
        return Ok(None);
    };
    let (choice, stack) = develop::issue_of(app, &asset)?;
    if choice != IssueChoice::Latest || stack.is_empty() {
        return Ok(None);
    }
    let source = develop::source_path_of(app, &asset, stack.source_base)
        .ok_or_else(|| format!("latest 的 {} 源文件不存在", stack.source_base.as_str()))?;
    if raybend::store::develop::EditBase::of_file(&source) != stack.source_base {
        return Err(format!(
            "latest 的 {} 源文件不存在",
            stack.source_base.as_str()
        ));
    }
    Ok(Some((asset, stack, source)))
}

/// **大图缓存**：`<库根>/cache/full/<asset>/latest-<base>-v<pipeline>.avif`。
///
/// 命中直接给；没命中就渲染一遍再写进去（写失败只记一句 —— 缓存写不进去
/// 只意味着下次再渲染一遍，不该让看图失败）。
///
/// **基准从 `asset.rel_path` 推**（`EditBase::of_file`）：调用方拿哪个文件来渲染，
/// 缓存就落在哪一侧 —— 否则切了基准会读到另一基准的旧图（`memory/FUNCTION-IMAGING.md` §4.3）。
///
/// `pub(crate)`：`view_image` 与 `develop_preview_refresh`（进/出编辑那两下）共用这一份 ——
/// preview 只允许有一条生成路径。
pub(crate) fn render_latest_cached<R: Runtime>(
    app: &AppHandle<R>,
    asset: &ResolvedAsset,
    stack: &raybend::store::develop::DevelopStack,
) -> Result<Vec<u8>, String> {
    render_profile_cached(app, asset, stack, "latest")
}

/// 明确指定稿的显示缓存；导出画廊不改浏览/编辑的 latest 选择。
pub(crate) fn render_profile_cached<R: Runtime>(
    app: &AppHandle<R>, asset: &ResolvedAsset,
    stack: &raybend::store::develop::DevelopStack, variant: &str,
) -> Result<Vec<u8>, String> {
    let _permit=app.state::<crate::browse::BrowseState>().sessions.begin_task(&asset.repository_id).map_err(|e|e.to_string())?;
    let catalog=app.state::<crate::browse::BrowseState>().lease(app,&asset.repository_id)?;
    if catalog.root()!=asset.root { return Err(raybend::Error::SessionExpired.to_string()); }
    let cache = FullCache::open(&asset.root).map_err(|e| e.to_string())?;
    let full = develop::source_path_of(app, asset, stack.source_base)
        .ok_or_else(|| format!("latest 的 {} 源文件不存在", stack.source_base.as_str()))?;
    let base = stack.source_base;
    if raybend::store::develop::EditBase::of_file(&full) != base {
        return Err(format!("latest 的 {} 源文件不存在", base.as_str()));
    }
    let signature = raybend::media::source::source_signature(&full).map_err(|e| e.to_string())?;
    let name = FullCache::source_name(variant, &signature);
    if let Some(bytes) = cache.read(asset.asset_id, &name, base, PIPELINE_VERSION) {
        return Ok(bytes);
    }
    // 镜头配置文件（调用方解析 —— 渲染层不认识数据库）
    let lens = crate::lens::render_correction(
        app,
        &asset.repository_id,
        asset.asset_id,
        stack.lens_profile.as_deref(),
        stack.lens_enabled,
    );
    let lut = stack
        .lut_id
        .as_deref()
        .filter(|_| stack.lut_enabled == Some(true))
        .map(|id| crate::lut::resolve(app, id)).transpose()?;
    let thumb = raybend::thumbnail::render::render_file_with_profiles(
        &full,
        SizeClass::Screen,
        Some(stack),
        lens.as_ref(),
        lut.as_deref(),
        |id, role| crate::color_profiles::resolve(app, id, role),
    )
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("解不开这张照片：{}", full.display()))?;
    catalog.ensure_current().map_err(|e|e.to_string())?;
    if let Err(error) = cache.write(asset.asset_id, &name, base, PIPELINE_VERSION, &thumb.data) {
        eprintln!("[develop] 大图缓存写失败（不影响显示）：{error}");
    }
    Ok(thumb.data)
}

/// **统一取图口**（`crates/raybend/src/display`，口径见 `specs/M2-W2.md` §2.1）。
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
    let handle=app.clone(); let task_path=path.clone();
    let _permit = crate::source::blocking(move || handle.state::<crate::browse::BrowseState>().path_task(&handle,Path::new(&task_path))).await?;
    let purpose = ImagePurpose::parse(text).ok_or_else(|| format!("未知的取图用途：{text}"))?;

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
        let handle = app.clone();
        let bytes =
            crate::source::blocking(move || render_latest_cached(&handle, &asset, &stack)).await?;
        return Ok(tauri::ipc::Response::new(bytes));
    }

    // 缓存库要先拿出来（`State` 不能跨 await 持有）——与 `thumb_get` 同一条路
    let cache_dir = sources_cache_dir(&app)?;
    let db = {
        let now = time::now_millis();
        state.get(&cache_dir, now)?
    };
    let bytes = crate::source::blocking(move || {
        let _permit=app.state::<crate::browse::BrowseState>().path_task(&app,Path::new(&path))?;
        let request = ImageRequest::plain(Path::new(&path), purpose);
        let image = display::cached_image(&db, &request, time::now_millis())
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "取不到这张图（类型认不出或解不开）".to_string())?;
        Ok(image.bytes)
    })
    .await?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// 看图态右栏的**直方图**（24 柱 RGB 合成；`specs/M2-W2.md` 1.6）。
///
/// 统计在 Rust 侧做（`AGENTS.md` §6.1 的红线：前端不碰像素）——
/// 前端只拿到 86 个浮点采样去画曲线。取的是**像素口**的 `Screen` 档（长边 1920）RGB 像素，
/// **不经过编码**（直方图只要像素；2026-09-24 起不再绕道字节）。
#[tauri::command]
pub async fn image_histogram<R:Runtime>(app:AppHandle<R>, path: String, bins: Option<usize>) -> Result<HistogramDto, String> {
    // 采样口径固定为 86（0 单独，其余每 3 级平均）；保留参数只为旧前端兼容。
    let _requested_bins = bins;
    let bins = DEFAULT_BINS;
    crate::source::blocking(move || {
        let _permit=app.state::<crate::browse::BrowseState>().path_task(&app,Path::new(&path))?;
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
    pub luma: Vec<f64>,
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
            luma: h.luma,
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

/// 原始 RAW 真显影与内嵌模拟小图使用不同缓存键，不能相互冒充。
fn raw_thumb_key(asset: &ResolvedAsset) -> Vec<u8> {
    format!("raw-original:{}:{}", asset.repository_id, asset.asset_id).into_bytes()
}
fn raw_thumb_sig(signature: &str) -> String {
    format!("raw-original-v{PIPELINE_VERSION}-src{signature}")
}
pub(crate) fn raw_original_thumb<R: Runtime>(
    app: &AppHandle<R>, asset: &ResolvedAsset, size: SizeClass,
) -> Result<Option<Vec<u8>>, String> {
    let Some(source) = develop::source_path_of(app, asset, raybend::store::develop::EditBase::Raw) else { return Ok(None) };
    let signature = raybend::media::source::source_signature(&source).map_err(|e| e.to_string())?;
    let db = app.state::<SourcesThumbs>().get(&sources_cache_dir(app)?, time::now_millis())?;
    let key = raw_thumb_key(asset); let sig = raw_thumb_sig(&signature);
    let read_key = key.clone(); let read_sig = sig.clone();
    if let Some(bytes) = db.read(move |conn| raybend::thumbnail::cache::get(conn, &read_key, size, &read_sig)).map_err(|e| e.to_string())? {
        return Ok(Some(bytes));
    }
    let Some(preview) = FullCache::open(&asset.root).ok().and_then(|cache|
        cache.read(asset.asset_id, &FullCache::source_name("raw-original", &signature),
            raybend::store::develop::EditBase::Raw, PIPELINE_VERSION)) else { return Ok(None) };
    let Some(thumb) = raybend::thumbnail::render::render_bytes(&preview, size, None).map_err(|e| e.to_string())? else { return Ok(None) };
    let bytes = thumb.data.clone();
    db.write(move |conn| raybend::thumbnail::cache::put(conn, &key, size, &sig, &thumb.data,
        thumb.width, thumb.height, time::now_millis())).map_err(|e| e.to_string())?;
    Ok(Some(bytes))
}

/// SOOC 生成 1920 + 384/192；RAW 仅补 384/192。只在导入收尾/当前编辑照片后台调用。
/// SOOC 不重复存 initial-latest：未编辑 latest 的显示复用同一份源快照。
pub(crate) fn prepare_source_images<R: Runtime>(
    app: &AppHandle<R>, asset: &ResolvedAsset, base: raybend::store::develop::EditBase,
) -> Result<(), String> {
    use raybend::store::develop::EditBase;
    let browse = app.state::<crate::browse::BrowseState>();
    let _permit = browse.sessions.begin_task(&asset.repository_id).map_err(|e| e.to_string())?;
    let catalog = browse.lease(app, &asset.repository_id)?;
    if catalog.root() != asset.root { return Err(raybend::Error::SessionExpired.to_string()); }
    let reference = raybend::export::VariantRef { asset_id: asset.asset_id, variant: base.as_str().into() };
    let snapshot = catalog.read(|conn| raybend::export::snapshot(conn, &asset.root, &reference)).map_err(|e| e.to_string())?;
    let source = develop::source_path_of(app, asset, base).ok_or("原始源不存在")?;
    let signature = raybend::media::source::source_signature(&source).map_err(|e| e.to_string())?;
    let db = app.state::<SourcesThumbs>().get(&sources_cache_dir(app)?, time::now_millis())?;
    if base == EditBase::Raw {
        let mut missing = Vec::new();
        for size in [SizeClass::Grid, SizeClass::Strip] {
            if raw_original_thumb(app, asset, size)?.is_none() { missing.push(size); }
        }
        if missing.is_empty() { return Ok(()); }
        // 当前照片后台只需要两档真小图，不另编码一份没人用的 1920 RAW 预览。
        let rgb = raybend::export::render_captured(&source, &snapshot, None, None,
            SizeClass::Grid.long_edge()).map_err(|e| e.to_string())?;
        catalog.ensure_current().map_err(|e| e.to_string())?;
        if raybend::media::source::source_signature(&source).map_err(|e| e.to_string())? != signature {
            return Err("源文件在生成缩略图时发生变化".into());
        }
        for size in missing {
            let thumb = raybend::thumbnail::render::encode(image::DynamicImage::ImageRgb16(rgb.clone()),
                size, None, false, None, None).map_err(|e| e.to_string())?;
            let key = raw_thumb_key(asset); let sig = raw_thumb_sig(&signature);
            db.write(move |conn| raybend::thumbnail::cache::put(conn, &key, size, &sig,
                &thumb.data, thumb.width, thumb.height, time::now_millis())).map_err(|e| e.to_string())?;
        }
    } else {
        let full = FullCache::open(&asset.root).map_err(|e| e.to_string())?;
        let name = FullCache::source_name("sooc", &signature);
        let preview = match full.read(asset.asset_id, &name, base, PIPELINE_VERSION) {
            Some(bytes) if image::load_from_memory(&bytes).is_ok() => bytes,
            _ => {
                let image = raybend::thumbnail::render::render_file(&source, SizeClass::Screen)
                    .map_err(|e| e.to_string())?.ok_or("无法生成 SOOC 预览")?;
                catalog.ensure_current().map_err(|e| e.to_string())?;
                if raybend::media::source::source_signature(&source).map_err(|e| e.to_string())? != signature {
                    return Err("源文件在生成预览时发生变化".into());
                }
                full.write(asset.asset_id, &name, base, PIPELINE_VERSION, &image.data).map_err(|e| e.to_string())?;
                image.data
            }
        };
        for size in [SizeClass::Grid, SizeClass::Strip] {
            render_cached_with(&db, &source, size, time::now_millis(), None, ||
                raybend::thumbnail::render::render_bytes(&preview, size, None)).map_err(|e| e.to_string())?;
        }
    }
    catalog.ensure_current().map_err(|e| e.to_string())
}

/// 仅当前选中的编辑照片：首帧就绪后后台补原始源快照，不阻塞首帧。
#[tauri::command]
pub async fn issue_sources_prepare<R: Runtime>(app: AppHandle<R>, path: String) -> Result<(), String> {
    crate::source::blocking(move || {
        let Some(asset) = develop::resolve_asset(&app, Path::new(&path)) else { return Ok(()) };
        for base in [raybend::store::develop::EditBase::Sooc, raybend::store::develop::EditBase::Raw] {
            if develop::source_path_of(&app, &asset, base).is_some() {
                prepare_source_images(&app, &asset, base)?;
            }
        }
        Ok(())
    }).await
}
