//! 小图命令：网格 / 胶片带 / 看图的图都从这里出。
//!
//! # 两份缓存，两个口径（崔总 2026-10-11；`AGENTS.md` §2.24）
//!
//! | 口径 | 住哪 | 为什么 |
//! | --- | --- | --- |
//! | **库内**（入库资产） | 磁盘 `<app data>/cache/_sources/thumbs.db` | 持久资产，重启后浏览还得靠它命中 |
//! | **库外**（来源目录 / 未入库） | **进程内存**（[`SourcesMemory`]） | import 面对的是临时目录：同一个路径下次打开内容可能完全不同，**落盘的行永远读不回来**，只剩无限增长 |
//!
//! ✋ **别再把库外的东西写进 `thumbs.db`**（2026-10-11 之前就是那样，用户机器上攒了 148 MB 死行）。
//! 目录名 `_sources` 是早年只服务源文件时的历史名字，现在装的是**库内**文件的图，不再改（改了等于清缓存）。
//!
//! 库内那条路的键优先用**文件身份**（改名/移动后仍命中），读不到身份才退回**绝对路径**。
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

use raybend::display::{self, CacheClass, DEFAULT_BINS, FullCache, ImagePurpose, ImageRequest};
use raybend::store::develop::IssueChoice;
use raybend::store::time;
use raybend::thumbnail::render::PIPELINE_VERSION;
use raybend::thumbnail::memory::SourceKey;
use raybend::thumbnail::render::render_sig_source;
use raybend::thumbnail::worker::{render_cached_with, render_source};
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

/// 库外图的**进程内**缓存（崔总 2026-10-11；`AGENTS.md` §2.24）。
///
/// 跟着 app 活、退出即清 —— 这正是 import 那种「临时目录」该有的临时性：
/// 同一个进程内来回滚 / 来回切目录都不用重新算，关掉 app 就什么都不剩。
/// 预算与键的算法在 `raybend::thumbnail::memory`（网格档按字节 256 MB，看图档 30 条）。
#[derive(Clone, Default)]
pub struct SourcesMemory(pub Arc<raybend::thumbnail::memory::SourceThumbs>);

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
    path: String,
    size: Option<String>,
) -> Result<tauri::ipc::Response, String> {
    let cache_dir = sources_cache_dir(&app)?;
    let size = SizeClass::parse(size.as_deref().unwrap_or("grid"))
        .ok_or_else(|| format!("未知的缩略图尺度：{}", size.as_deref().unwrap_or("")))?;

    // 来源查库、lensfun 解析与渲染同在后台线程；缩略图多张并发时
    // 不能让这些同步工作占住 IPC executor，拖慢编辑换图与状态轮询。
    let handle = app.clone();
    let bytes = crate::source::blocking(move || {
        let _permit=handle.state::<crate::browse::BrowseState>().path_task(&handle,Path::new(&path))?;
        let now = time::now_millis();
        /*
         * **口径分派点**（`AGENTS.md` §2.24）：
         * * 这个路径落在**库里**（`resolve_asset` 有）—— 库内口径：AVIF + 磁盘缓存；
         * * 落在**库外**（来源目录 / 未入库）—— 库外口径：JPEG q82 + **只在内存**。
         */
        let asset = develop::resolve_asset(&handle, Path::new(&path));
        let bytes = match &asset {
            Some(asset) => {
                // 磁盘缓存句柄**只在库内这条路上取** —— 库外看一眼就走，连库都不该开
                let db = handle.state::<SourcesThumbs>().get(&cache_dir, now)?;
                match edited_source(&handle, asset)? {
                    Some((stack, source)) => {
                        let variant = crate::develop::latest_variant(&stack)?;
                        render_profile_thumb(
                            &handle, &db, asset, &source, &stack, size, Some(&variant),
                        )
                    }
                    None => render_now(&db, Path::new(&path), size, now),
                }
            }
            None => {
                // 库外：键含文件身份（大小 + mtime）—— 换卡 / 覆盖保存自动不命中
                let memory = handle.state::<SourcesMemory>().0.clone();
                let key = SourceKey::of(Path::new(&path), size, render_sig_source(size));
                let mut rendered = false;
                let thumb = memory.get_or_render(&key, || {
                    rendered = true;
                    render_source(Path::new(&path), size)
                });
                /*
                 * 时间搭在这一读上（崔总 2026-10-11，`specs/source-time-inline.md`）。
                 *
                 * 只在**刚渲染出来**这一支上做（`rendered` 标志判定，命中内存缓存时一次也不多花）：
                 * 出图本来就把这个文件读了一遍，读头（宽高/方向/拍摄时间）顺手回来、
                 * 放回会话级元信息缓存 —— 界面随后**只查缓存**就能拿到，
                 * 所以系统里不存在第二条「为了时间再去读文件」的线路。
                 * 读不到无所谓：界面继续用它自己的兜底时间。
                 */
                if rendered {
                    /*
                     * **一次读、拿全**（崔总 2026-10-11，`specs/source-time-inline.md`）：
                     * 出图本来就把这个文件读了一遍，读头（宽高/方向/拍摄时间）与
                     * flowinfo 要的整块 EXIF 都在这里顺手解出来、一起放进**每张图的缓存**。
                     * 于是：
                     *   * 界面要时间 → 只查缓存（零 I/O）；
                     *   * 鼠标点/停到图上 → flowinfo 的 EXIF 也从缓存出（不读盘）；
                     *   * 这条路上**没有第二次读文件**。
                     * 读不到无所谓：界面继续用它清单层的兜底时间。
                     */
                    let file = Path::new(&path);
                    let dir = file.parent().unwrap_or_else(|| Path::new(""));
                    let relative = file
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    let (exif, fields) = raybend::media::exif::read_bundle_for(file);
                    let mtime_ms = std::fs::metadata(file)
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .map(raybend::store::time::from_system_time);
                    let taken = raybend::media::exif::resolve_taken_at(
                        Some(&exif),
                        &relative,
                        mtime_ms,
                    );
                    let orientation =
                        raybend::media::meta::normalize_orientation(exif.orientation);
                    let (width, height) = match (exif.width, exif.height) {
                        (Some(w), Some(h)) if w > 0 && h > 0 => raybend::media::meta::oriented_size(
                            u32::try_from(w).unwrap_or(0),
                            u32::try_from(h).unwrap_or(0),
                            orientation,
                        ),
                        _ => (0, 0),
                    };
                    let meta = raybend::media::meta::PhotoMeta {
                        width,
                        height,
                        orientation,
                        taken_at_ms: taken.map(|t| t.millis),
                        taken_at_source: taken.map(|t| t.source),
                        taken_at_offset_min: taken.and_then(|t| t.offset_min),
                        file_size: std::fs::metadata(file).map(|m| m.len()).unwrap_or(0),
                        mtime_ms: mtime_ms.unwrap_or(0),
                    };
                    if let Ok(mut cache) = handle.state::<crate::source::SourcesMetaCache>().0.lock()
                    {
                        cache.remember(
                            dir,
                            &relative,
                            meta,
                            Some(raybend::media::meta_cache::CachedExif { data: exif, fields }),
                            std::time::Instant::now(),
                        );
                    }
                }
                thumb.map(|thumb| thumb.data)
            }
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
                source, size, Some(stack), lens.as_ref(), lut.as_deref(), raybend::thumbnail::render::EncodeFormat::Avif,
                |id, role| crate::color_profiles::resolve(app, id, role))
        })
    })
}

/// 这个资产**编辑过吗**？编辑过就把它的编辑栈取出来（给渲染用）。
///
/// 未编辑 / 栈为空 —— 一律 `None`（调用方走没有编辑的那条路）。
/// “这个路径在不在库里”由调用方先算（`resolve_asset`）—— 那个结果还要用来分派
/// 库内 / 库外两套编码口径（`AGENTS.md` §2.24），不能在这里丢掉。
fn edited_source<R: Runtime>(
    app: &AppHandle<R>,
    asset: &ResolvedAsset,
) -> Result<Option<(raybend::store::develop::DevelopStack, PathBuf)>, String> {
    let (choice, stack) = develop::issue_of(app, asset)?;
    if choice != IssueChoice::Latest || stack.is_empty() {
        return Ok(None);
    }
    let source = develop::source_path_of(app, asset, stack.source_base)
        .ok_or_else(|| format!("latest 的 {} 源文件不存在", stack.source_base.as_str()))?;
    if raybend::store::develop::EditBase::of_file(&source) != stack.source_base {
        return Err(format!(
            "latest 的 {} 源文件不存在",
            stack.source_base.as_str()
        ));
    }
    Ok(Some((stack, source)))
}

/// **大图缓存**：`<库根>/cache/full/<asset>/latest-<hash12>-src<signature>-<base>-v<pipeline>.avif`。
///
/// 命中直接给；没命中就渲染一遍再写进去（写失败只记一句 —— 缓存写不进去
/// 只意味着下次再渲染一遍，不该让看图失败）。写完顺手回收旧快照
/// （每资产保留最新 2 份，§4.2）。
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
    let variant = crate::develop::latest_variant(stack)?;
    render_profile_cached(app, asset, stack, &variant)
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
        raybend::thumbnail::render::EncodeFormat::Avif,
        |id, role| crate::color_profiles::resolve(app, id, role),
    )
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("解不开这张照片：{}", full.display()))?;
    catalog.ensure_current().map_err(|e|e.to_string())?;
    if let Err(error) = cache.write(asset.asset_id, &name, base, PIPELINE_VERSION, &thumb.data) {
        log::warn!("[develop] 大图缓存写失败（不影响显示）：{error}");
    } else {
        // 写新快照之后顺手回收旧版本（每资产保留最新 2 份，§4.2）；
        // 不进热路径、失败不算错。
        let _ = cache.gc_latest(asset.asset_id, 2);
    }
    Ok(thumb.data)
}

/// **真身档**（2026-10-11 崔总口径）：该变体自己的像素，不缩放、不用内嵌预览充数。
///
/// 与 [`render_profile_cached`]（占位档 1920 AVIF）同一套结构，差别三条：
/// * 渲染档 `SizeClass::Full` —— RAW 走**完整解码**（内嵌预览是机内 JPEG，不是这张的显影结果）；
/// * 缓存档 `CacheClass::Full` —— `.jpg`（q92，~9MB/20MP；AVIF 编码是秒级），
///   落库内 `cache/full/<资产>/…-full-…jpg`（跟库走、随资产删、有容量上限）；
/// * 写完顺手 `gc_latest(asset, 2)`（**按档各留 2 份**：撤销/重做能直接命中）
///   与有间隔的容量清扫。
///
/// `variant`：编辑过用 `latest-<hash12>`，未编辑的 RAW 基准用 `raw` ——
/// 与占位档同一套命名（`FullCache::source_name`），只是多一个 `-full` 标记。
pub(crate) fn render_full_cached<R: Runtime>(
    app: &AppHandle<R>,
    asset: &ResolvedAsset,
    stack: &raybend::store::develop::DevelopStack,
    variant: &str,
) -> Result<Vec<u8>, String> {
    let _permit = app
        .state::<crate::browse::BrowseState>()
        .sessions
        .begin_task(&asset.repository_id)
        .map_err(|e| e.to_string())?;
    let catalog = app.state::<crate::browse::BrowseState>().lease(app, &asset.repository_id)?;
    if catalog.root() != asset.root {
        return Err(raybend::Error::SessionExpired.to_string());
    }
    let cache = FullCache::open(&asset.root).map_err(|e| e.to_string())?;
    let full = develop::source_path_of(app, asset, stack.source_base)
        .ok_or_else(|| format!("真身的 {} 源文件不存在", stack.source_base.as_str()))?;
    let base = stack.source_base;
    if raybend::store::develop::EditBase::of_file(&full) != base {
        return Err(format!("真身的 {} 源文件不存在", base.as_str()));
    }
    let signature = raybend::media::source::source_signature(&full).map_err(|e| e.to_string())?;
    let name = FullCache::source_name(variant, &signature);
    if let Some(bytes) = cache.read_class(
        asset.asset_id,
        &name,
        base,
        CacheClass::Full,
        PIPELINE_VERSION,
    ) {
        return Ok(bytes);
    }
    // 镜头配置文件与 LUT 由调用方（这里）解析 —— 渲染层不认识数据库
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
        .map(|id| crate::lut::resolve(app, id))
        .transpose()?;
    let thumb = raybend::thumbnail::render::render_file_with_profiles(
        &full,
        SizeClass::Full,
        Some(stack),
        lens.as_ref(),
        lut.as_deref(),
        raybend::thumbnail::render::EncodeFormat::Avif,
        |id, role| crate::color_profiles::resolve(app, id, role),
    )
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("解不开这张照片：{}", full.display()))?;
    catalog.ensure_current().map_err(|e| e.to_string())?;
    if let Err(error) = cache.write_class(
        asset.asset_id,
        &name,
        base,
        CacheClass::Full,
        PIPELINE_VERSION,
        &thumb.data,
    ) {
        log::warn!("[view] 真身缓存写失败（不影响显示）：{error}");
    } else {
        let _ = cache.gc_latest(asset.asset_id, 2);
        let _ = cache.maybe_enforce_capacity();
    }
    Ok(thumb.data)
}

/// **统一取图口**（`crates/raybend/src/display`，口径见 `specs/M2-W2.md` §2.1）。
///
/// 与 `thumb_get` 的区别：那个是「给我一张源文件的网格/胶片带小图」的专用命令；
/// 这个是 **view 与缩略图共用的总入口** —— 调用方只说「哪张、要多大、有没有编辑」，
/// 由 `display` 决定它是 RAW 还是位图、该给原图还是该渲染。
///
/// **带缓存**（`display::cached_image`）：所以连续看图、邻图预载都不会重复解码 ——
/// 旧的裸 `display_image` 每次都重解，一张 RAW 要一秒多。
/// 缓存跟着口径走：库内 → 磁盘 `_sources/thumbs.db`；库外 → **进程内存**（`AGENTS.md` §2.24）。
///
/// `purpose`：`"grid"` / `"strip"` / `"screen"` / `"original"`；缺省 `screen`（看图）。
/// 位图 + `original` + 没编辑过 ⇒ **直接给原文件字节**（不经渲染管线，也不缓存）。
#[tauri::command]
pub async fn view_image<R: Runtime>(
    app: AppHandle<R>,
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
    /*
     * **编辑过的照片 / 未编辑的 RAW：看图给真身**（崔总 2026-10-11 口径）。
     *
     * 与 `screen` 那一段同构，两条区别：
     * * `full` 档：渲染全尺寸（RAW 完整解码）、落库内 `cache/full/…-full-…jpg`；
     * * **未编辑的位图不进这里** —— 它的真身就是原文件本身（下面 `original` 短路），
     *   一秒不用等、也不占缓存。
     *
     * 未编辑的 RAW 走这里（变体名 `raw`）：原文件是传感器数据，必须真解码一次。
     */
    /*
     * **库内 / 库外的分派点**（崔总 2026-10-11；`AGENTS.md` §2.24）：
     * 这个路径落在库里就按库内口径（AVIF），落在库外（来源目录 / 未入库）就是 JPEG ——
     * 实测 1920 档 AVIF 要 4–5.6 s，JPEG 只要 1.3–2.2 s（同一台机、同一批图）。
     */
    let asset = develop::resolve_asset(&app, Path::new(&path));
    let in_library = asset.is_some();
    if matches!(purpose, ImagePurpose::Screen | ImagePurpose::Full) {
        // 这一段只在库里才成立（编辑只存在于库里）—— 作用域收在花括号里，
        // 免得把 `asset` 的借用带到下面的 blocking 闭包里。
        if let Some(asset) = &asset {
            let target = match develop::issue_of(&app, asset) {
                Ok((IssueChoice::Latest, stack)) => {
                    Some((develop::latest_variant(&stack)?, stack))
                }
                Ok((_, stack))
                    if purpose == ImagePurpose::Full
                        && stack.source_base == raybend::store::develop::EditBase::Raw =>
                {
                    Some((stack.source_base.as_str().to_string(), stack))
                }
                _ => None,
            };
            if let Some((variant, stack)) = target {
                let handle = app.clone();
                let want_full = purpose == ImagePurpose::Full;
                // 闭包要 `'static`：拿一份**自己的**资产，别把上面那笔借用带进去
                let owned = asset.clone();
                let bytes = crate::source::blocking(move || {
                    if want_full {
                        render_full_cached(&handle, &owned, &stack, &variant)
                    } else {
                        render_latest_cached(&handle, &owned, &stack)
                    }
                })
                .await?;
                return Ok(tauri::ipc::Response::new(bytes));
            }
        }
    }

    let cache_dir = sources_cache_dir(&app)?;
    let memory = {
        // 库外那份在内存里（`AGENTS.md` §2.24）；`State` 不能跨 await 持有，先取出来
        let handle = app.clone();
        
        handle.state::<SourcesMemory>().0.clone()
    };
    let bytes = crate::source::blocking(move || {
        let _permit=app.state::<crate::browse::BrowseState>().path_task(&app,Path::new(&path))?;
        let request = if in_library {
            ImageRequest::plain(Path::new(&path), purpose)
        } else {
            // 库外（来源目录）：JPEG —— import 只看一眼就走，不为画质付秒级编码
            ImageRequest::source(Path::new(&path), purpose)
        };
        // 磁盘缓存句柄**只在库内那条路上取**：库外连库都不该开
        let db = if in_library {
            Some(app.state::<SourcesThumbs>().get(&cache_dir, time::now_millis())?)
        } else {
            None
        };
        let image = display::cached_image(
            db.as_deref(),
            Some(&memory),
            &request,
            time::now_millis(),
        )
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
        for size in [SizeClass::Grid] {
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
        render_cached_with(&db, &source, SizeClass::Grid, time::now_millis(), None, ||
            raybend::thumbnail::render::render_bytes(&preview, SizeClass::Grid, None))
            .map_err(|e| e.to_string())?;
    }
    catalog.ensure_current().map_err(|e| e.to_string())
}

/// 仅当前选中的编辑照片：首帧就绪后后台补原始源快照，不阻塞首帧。
///
/// 走**后台队列的 P3（批量）**（W2 §4.3）：同库同资产同基准只排一次；
/// 前台重活（编辑器解码 / 预览）可以插队；失败只记日志（派生数据，下次进入再补）。
#[tauri::command]
pub async fn issue_sources_prepare<R: Runtime>(app: AppHandle<R>, path: String) -> Result<(), String> {
    crate::source::blocking(move || {
        let Some(asset) = develop::resolve_asset(&app, Path::new(&path)) else { return Ok(()) };
        for base in [raybend::store::develop::EditBase::Sooc, raybend::store::develop::EditBase::Raw] {
            if develop::source_path_of(&app, &asset, base).is_none() {
                continue;
            }
            let key = format!(
                "source-images:{}:{}:{}",
                asset.repository_id,
                asset.asset_id,
                base.as_str()
            );
            let task_app = app.clone();
            let task_asset = asset.clone();
            let _ = app.state::<crate::work::WorkQueue>().spawn(
                crate::work::Priority::Batch,
                key,
                move |_token| {
                    if let Err(error) = prepare_source_images(&task_app, &task_asset, base) {
                        // i18n-exempt: 后台诊断（终端日志，不进界面）
                        log::warn!("[thumbs] 原始源快照预热失败（{}）：{error}", base.as_str());
                    }
                },
            );
        }
        Ok(())
    })
    .await
}
