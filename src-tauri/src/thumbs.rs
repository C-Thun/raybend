//! 小图命令：网格 / 胶片带 / 看图的图都从这里出。
//!
//! # 两份缓存，两个口径（崔总 2026-10-11；`AGENTS.md` §2.24）
//!
//! | 口径 | 住哪 | 为什么 |
//! | --- | --- | --- |
//! | **库内**（入库资产） | 磁盘 `<库根>/cache/thumbs.db`（每库一份） | 持久资产，重启后浏览还得靠它命中；跟着库走 |
//! | **库外**（来源目录 / 未入库） | **进程内存**（[`SourcesMemory`]） | import 面对的是临时目录：同一个路径下次打开内容可能完全不同，**落盘的行永远读不回来**，只剩无限增长 |
//!
//! ✋ **别再把库外的东西写进 `thumbs.db`**（2026-10-11 之前就是那样，用户机器上攒了 148 MB 死行）。
//! 旧的全局 `_sources/thumbs.db` 在升级时一次性删除（派生数据，各库按需重建）。
//! 库内小图**只存 384**：1920 载入占位走库根 `cache/full/`（`FullCache`）。
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
//! 普通限流仍由调用方负责；这里额外做**同请求单飞**：网格、看图、对比与
//! 退出编辑的预览刷新可能同时撞同一幅输出，缓存未命中时也只真正渲染一次。
//! key 与缓存身份同源（文件身份 / 源签名 / 编辑指纹 / 档位 / 管线版本），
//! 不是只按路径去重；不同内容的正常请求仍可并行。

use std::path::{Path, PathBuf};
use std::sync::Arc;

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

/// 旧的全局小图缓存（`<app data>/cache/_sources/thumbs.db`）清理。
///
/// 2026-10-25 起小图缓存跟着库走（`<库根>/cache/thumbs.db`），这份全局文件
/// 不再有任何读者；它只是派生数据，升级后删掉即可（各库按需重建）。
/// 返回释放的字节数（日志用）。
pub(crate) fn cleanup_legacy_sources_cache<R: Runtime>(app: &AppHandle<R>) -> Result<u64, String> {
    let dir = crate::db::data_dir(app)?
        .join("cache")
        .join(SOURCES_CACHE_DIR);
    let mut freed = 0u64;
    for name in ["thumbs.db", "thumbs.db-wal", "thumbs.db-shm"] {
        let path = dir.join(name);
        if let Ok(meta) = std::fs::metadata(&path) {
            freed += meta.len();
            if let Err(error) = std::fs::remove_file(&path) {
                log::warn!("[thumbs] 旧全局小图缓存清理失败（{}）：{error}", path.display());
            }
        }
    }
    Ok(freed)
}

/// 库外图的**进程内**缓存（崔总 2026-10-11；`AGENTS.md` §2.24）。
///
/// 跟着 app 活、退出即清 —— 这正是 import 那种「临时目录」该有的临时性：
/// 同一个进程内来回滚 / 来回切目录都不用重新算，关掉 app 就什么都不剩。
/// 预算与键的算法在 `raybend::thumbnail::memory`（网格档按字节 256 MB，看图档 30 条）。
#[derive(Clone, Default)]
pub struct SourcesMemory(pub Arc<raybend::thumbnail::memory::SourceThumbs>);

/// 取一张缩略图的字节。
///
/// `size`：只支持 `"grid"`（长边 384）；1920 请走 [`view_image`]。
#[tauri::command]
pub async fn thumb_get<R: Runtime>(
    app: AppHandle<R>,
    path: String,
    size: Option<String>,
) -> Result<tauri::ipc::Response, String> {
    let size = SizeClass::parse(size.as_deref().unwrap_or("grid"))
        .ok_or_else(|| format!("未知的缩略图尺度：{}", size.as_deref().unwrap_or("")))?;
    if size != SizeClass::Grid {
        return Err("缩略图只支持 grid（384）；1920 请走 view_image".to_string());
    }

    // 来源查库、lensfun 解析与渲染同在后台线程；缩略图多张并发时
    // 不能让这些同步工作占住 IPC executor，拖慢编辑换图与状态轮询。
    // 计时只给**慢请求**记一行（debug）：排障看「tiles 半天不出来」时谁在拖。
    let started = std::time::Instant::now();
    let label = path.clone(); // 日志用（path 要 move 进后台闭包）
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
                /*
                 * 磁盘缓存句柄**只在库内这条路上取** —— 库外看一眼就走，连库都不该开。
                 * 打不开（库已释放 / 权限 / 磁盘）只降级为「这次不缓存」：
                 * 一张缩略图不该让整个网格变成错误。
                 */
                let library = handle
                    .state::<crate::browse::BrowseState>()
                    .library_thumbs(&handle, &asset.repository_id);
                let db = match library {
                    Ok(db) => Some(db),
                    Err(error) => {
                        log::warn!("[thumbs] 库小图缓存不可用，本次直接渲染：{error}");
                        None
                    }
                };
                match (edited_source(&handle, asset)?, db) {
                    (Some((stack, source)), Some(db)) => {
                        let variant = crate::develop::latest_variant(&stack)?;
                        render_profile_thumb(
                            &handle, &db, asset, &source, &stack, size, Some(&variant),
                        )
                    }
                    (Some((stack, source)), None) => {
                        let variant = crate::develop::latest_variant(&stack)?;
                        render_profile_uncached(&handle, asset, &source, &stack, size, &variant)
                    }
                    (None, Some(db)) => render_now(&db, Path::new(&path), size, now),
                    (None, None) => render_source(Path::new(&path), size).map(|thumb| thumb.data),
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
    let elapsed = started.elapsed();
    if elapsed.as_millis() >= 200 {
        log::debug!(
            "[view] thumb 慢 {} {}ms（{}B）",
            label,
            elapsed.as_millis(),
            bytes.len()
        );
    }

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
        render_profile_pixels(app, asset, source, stack, size, preview_variant)
    })
}

/// [`render_profile_thumb`] 的**像素那一半**：已有 preview 缩小 → 否则按尺寸重建。
///
/// 抽出来是给「缓存库打不开」的降级路径用：那条路要的是**带编辑结果的图**，
/// 不能退回去渲染源文件（会显示成未编辑的样子）。
pub(crate) fn render_profile_pixels<R: Runtime>(
    app: &AppHandle<R>, asset: &ResolvedAsset, source: &Path,
    stack: &raybend::store::develop::DevelopStack, size: SizeClass,
    preview_variant: Option<&str>,
) -> raybend::Result<Option<raybend::thumbnail::render::Thumb>> {
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
}

/// 缓存不可用时的兜底：像素照常出，`None` 用占位图接住（与队列那条路同一取舍）。
fn render_profile_uncached<R: Runtime>(
    app: &AppHandle<R>, asset: &ResolvedAsset, source: &Path,
    stack: &raybend::store::develop::DevelopStack, size: SizeClass, variant: &str,
) -> raybend::Result<Vec<u8>> {
    let thumb = render_profile_pixels(app, asset, source, stack, size, Some(variant))?;
    let thumb = match thumb {
        Some(thumb) => thumb,
        None => {
            let kind = raybend::media::kind::kind_of_file(&source.to_string_lossy());
            raybend::thumbnail::render::placeholder(kind, size)?
        }
    };
    Ok(thumb.data)
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

/// 1920 / 真身渲染的执行中身份。
/// `name` 已含变体指纹与源文件签名；尺寸档区分 Preview 与 Full。
/// 路径只进日志（Debug 走脱敏层），身份判断不依赖路径文本。
#[derive(Clone, PartialEq, Eq, Hash)]
struct FullFlightKey {
    repository_id: String,
    asset_id: i64,
    name: String,
    base: String,
    size: SizeClass,
    pipeline: u32,
    path: PathBuf,
}

impl std::fmt::Debug for FullFlightKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("full")
            .field("path", &raybend::thumbnail::flight::LogPath(&self.path))
            .field("repository", &self.repository_id)
            .field("asset", &self.asset_id)
            .field("variant", &self.name)
            .field("base", &self.base)
            .field("size", &self.size)
            .field("pipeline", &self.pipeline)
            .finish()
    }
}

fn full_flights() -> &'static raybend::thumbnail::flight::FlightGate<FullFlightKey, Vec<u8>> {
    static FLIGHTS: std::sync::OnceLock<
        raybend::thumbnail::flight::FlightGate<FullFlightKey, Vec<u8>>,
    > = std::sync::OnceLock::new();
    FLIGHTS.get_or_init(Default::default)
}

fn full_flight_key(
    asset: &ResolvedAsset,
    path: &Path,
    name: &str,
    base: raybend::store::develop::EditBase,
    size: SizeClass,
) -> FullFlightKey {
    FullFlightKey {
        repository_id: asset.repository_id.clone(),
        asset_id: asset.asset_id,
        name: name.to_string(),
        base: base.as_str().to_string(),
        size,
        pipeline: PIPELINE_VERSION,
        path: path.to_path_buf(),
    }
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
    let flight_key = full_flight_key(asset, &full, &name, base, SizeClass::Screen);
    match full_flights().begin(flight_key) {
        raybend::thumbnail::flight::FlightClaim::Wait(wait) => {
            wait.wait().map(raybend::thumbnail::flight::into_owned)
        }
        raybend::thumbnail::flight::FlightClaim::Leader(leader) => {
            let result = (|| {
    if let Some(bytes) = cache.read(asset.asset_id, &name, base, PIPELINE_VERSION) {
        log::debug!("[view] 占位缓存命中 {}（{}B）", full.display(), bytes.len());
        return Ok(bytes);
    }
    log::debug!("[view] 占位缓存未命中，开始渲染 {}（变体 {variant}）", full.display());
    let started = std::time::Instant::now();
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
        // 1920 也走同一套容量清扫（未编辑 SOOC/RAW 的 Screen 现在也落这里）。
        let _ = cache.maybe_enforce_capacity();
    }
    log::debug!(
        "[view] 占位渲染完成 {} {}ms（{}B）",
        full.display(),
        started.elapsed().as_millis(),
        thumb.data.len()
    );
    Ok(thumb.data)
            })();
            match result {
                // 共享后的字节由最后一个持者直接拿走（无等待者时零拷贝）。
                Ok(bytes) => Ok(raybend::thumbnail::flight::into_owned(leader.publish(bytes))),
                Err(error) => {
                    leader.publish_error(&error);
                    Err(error)
                }
            }
        }
    }
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
    let flight_key = full_flight_key(asset, &full, &name, base, SizeClass::Full);
    match full_flights().begin(flight_key) {
        raybend::thumbnail::flight::FlightClaim::Wait(wait) => {
            wait.wait().map(raybend::thumbnail::flight::into_owned)
        }
        raybend::thumbnail::flight::FlightClaim::Leader(leader) => {
            let result = (|| {
    if let Some(bytes) = cache.read_class(
        asset.asset_id,
        &name,
        base,
        CacheClass::Full,
        PIPELINE_VERSION,
    ) {
        log::debug!("[view] 真身缓存命中 {}（{}B）", full.display(), bytes.len());
        return Ok(bytes);
    }
    log::debug!("[view] 真身缓存未命中，开始渲染 {}（变体 {variant}）", full.display());
    let started = std::time::Instant::now();
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
    log::debug!(
        "[view] 真身渲染完成 {} {}ms（{}B）",
        full.display(),
        started.elapsed().as_millis(),
        thumb.data.len()
    );
    Ok(thumb.data)
            })();
            match result {
                // 共享后的字节由最后一个持者直接拿走（无等待者时零拷贝）。
                Ok(bytes) => Ok(raybend::thumbnail::flight::into_owned(leader.publish(bytes))),
                Err(error) => {
                    leader.publish_error(&error);
                    Err(error)
                }
            }
        }
    }
}

/// 看图/占位该渲染**哪个变体**（`Screen` / `Full` 的 1920/真身分派）。
///
/// 关键一条：`IssueChoice::Sooc` 必须把基准改成 [`EditBase::Sooc`] ——
/// 未编辑照片的编辑栈是空栈，`source_base` 落的是**默认值 `Raw`**；
/// 直接拿它去渲染会去解 RAW（JPG 只有位图时干脆找不到源文件），
/// 看到的就不是「相机直出」，而是 RAW 解码结果（或直接报错）。
fn profile_target(
    purpose: ImagePurpose,
    choice: IssueChoice,
    stack: &raybend::store::develop::DevelopStack,
) -> Result<Option<(String, raybend::store::develop::DevelopStack)>, String> {
    use raybend::store::develop::EditBase;
    let mut stack = stack.clone();
    let target = match (purpose, choice) {
        (ImagePurpose::Screen | ImagePurpose::Full, IssueChoice::Latest) => {
            let variant = develop::latest_variant(&stack)?;
            Some((variant, stack))
        }
        // 1920 一律走 `cache/full/`（FullCache），**不进 thumbs.db**。
        // 未编辑的 SOOC / RAW 也要落一份 1920，否则每次看图都要重解。
        (ImagePurpose::Screen, IssueChoice::Sooc) => {
            stack.source_base = EditBase::Sooc;
            Some(("sooc".to_string(), stack))
        }
        (ImagePurpose::Screen, IssueChoice::Raw) => {
            stack.source_base = EditBase::Raw;
            Some(("raw".to_string(), stack))
        }
        (ImagePurpose::Full, IssueChoice::Raw) => {
            stack.source_base = EditBase::Raw;
            Some(("raw".to_string(), stack))
        }
        // Full + SOOC（位图）走下面的原文件短路，不渲染；grid/original 不走这里。
        _ => None,
    };
    Ok(target)
}

/// **统一取图口**（`crates/raybend/src/display`，口径见 `specs/M2-W2.md` §2.1）。
///
/// 与 `thumb_get` 的区别：那个是「给我一张源文件的网格/胶片带小图」的专用命令；
/// 这个是 **view 与缩略图共用的总入口** —— 调用方只说「哪张、要多大、有没有编辑」，
/// 由 `display` 决定它是 RAW 还是位图、该给原图还是该渲染。
///
/// **带缓存**（`display::cached_image`）：所以连续看图、邻图预载都不会重复解码 ——
/// 旧的裸 `display_image` 每次都重解，一张 RAW 要一秒多。
/// 缓存跟着口径走：库内 → 库根 `cache/thumbs.db`（384）与 `cache/full/`（1920/真身）；
/// 库外 → **进程内存**（`AGENTS.md` §2.24）。
///
/// `purpose`：`"grid"` / `"screen"` / `"original"` / `"full"`；缺省 `screen`（看图）。
/// 位图 + `original` + 没编辑过 ⇒ **直接给原文件字节**（不经渲染管线，也不缓存）。
#[tauri::command]
pub async fn view_image<R: Runtime>(
    app: AppHandle<R>,
    path: String,
    purpose: Option<String>,
) -> Result<tauri::ipc::Response, String> {
    let text = purpose.as_deref().unwrap_or("screen");
    /*
     * 请求进/出各一行（debug）：排障「双击看大图卡住」时，日志里要能看出
     * **谁先到、谁排了多久**。同时段内两条重叠就是排队（单例 RAW worker + 渲染）。
     */
    let started = std::time::Instant::now();
    log::debug!("[view] → {text} {path}");
    let label = path.clone(); // 日志用（path 要 move 进后面的后台闭包）
    let handle=app.clone(); let task_path=path.clone();
    let _permit = crate::source::blocking(move || handle.state::<crate::browse::BrowseState>().path_task(&handle,Path::new(&task_path))).await?;
    log::debug!(
        "[view] 库许可已取 {text} {path}（{}ms）",
        started.elapsed().as_millis()
    );
    let purpose = ImagePurpose::parse(text).ok_or_else(|| format!("未知的取图用途：{text}"))?;

    /*
     * **编辑过的照片：非编辑器里也看编辑结果**（人类 2026-09-24 定的口径）。
     *
     * 只有 `screen`（全图查看）走这条路：
     * * `grid` 由 `thumb_get` 负责（它有自己的缓存库）；
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
            let (choice, stack) = develop::issue_of(&app, asset)?;
            if let Some((variant, stack)) = profile_target(purpose, choice, &stack)? {
                let handle = app.clone();
                let want_full = purpose == ImagePurpose::Full;
                // 闭包要 `'static`：拿一份**自己的**资产，别把上面那笔借用带进去
                let owned = asset.clone();
                let bytes = crate::source::blocking(move || {
                    if want_full {
                        render_full_cached(&handle, &owned, &stack, &variant)
                    } else {
                        render_profile_cached(&handle, &owned, &stack, &variant)
                    }
                })
                .await?;
                log::debug!(
                    "[view] ← {text} {path} {}ms（{}B）",
                    started.elapsed().as_millis(),
                    bytes.len()
                );
                return Ok(tauri::ipc::Response::new(bytes));
            }
        }
    }

    let memory = {
        // 库外那份在内存里（`AGENTS.md` §2.24）；`State` 不能跨 await 持有，先取出来
        let handle = app.clone();

        handle.state::<SourcesMemory>().0.clone()
    };
    // 库内：这个资产所属库自己的 thumbs.db（跟着库走）。打不开就降级为不缓存。
    // 句柄在 blocking 线程里取（开库/迁移都可能碰磁盘，不能在 IPC executor 上做）。
    let library_repo = asset.as_ref().map(|asset| asset.repository_id.clone());
    let bytes = crate::source::blocking(move || {
        let _permit=app.state::<crate::browse::BrowseState>().path_task(&app,Path::new(&path))?;
        let library_thumbs = match &library_repo {
            Some(id) => match app
                .state::<crate::browse::BrowseState>()
                .library_thumbs(&app, id)
            {
                Ok(db) => Some(db),
                Err(error) => {
                    // 缓存不可用只降级为不缓存：图还要能看。
                    log::warn!("[thumbs] 库小图缓存不可用，本次不落缓存：{error}");
                    None
                }
            },
            None => None,
        };
        let request = if in_library {
            ImageRequest::plain(Path::new(&path), purpose)
        } else {
            // 库外（来源目录）：JPEG —— import 只看一眼就走，不为画质付秒级编码
            ImageRequest::source(Path::new(&path), purpose)
        };
        let image = display::cached_image(
            library_thumbs.as_deref(),
            Some(&memory),
            &request,
            time::now_millis(),
        )
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "取不到这张图（类型认不出或解不开）".to_string())?;
        Ok(image.bytes)
    })
    .await?;
    log::debug!(
        "[view] ← {text} {label} {}ms（{}B）",
        started.elapsed().as_millis(),
        bytes.len()
    );
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

/// 当前库的小图缓存统计（设置面板与排错用；缓存跟着库走）。
#[tauri::command]
pub async fn thumb_cache_stats<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
) -> Result<raybend::thumbnail::CacheStats, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        let db = handle
            .state::<crate::browse::BrowseState>()
            .library_thumbs(&handle, &repository_id)?;
        db.read(raybend::thumbnail::cache::stats)
            .map_err(|e| e.to_string())
    })
    .await
}

/// 清空当前库的小图缓存（派生数据，删掉后浏览时自动重建）。
#[tauri::command]
pub async fn thumb_cache_clear<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
) -> Result<usize, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        let db = handle
            .state::<crate::browse::BrowseState>()
            .library_thumbs(&handle, &repository_id)?;
        db.write(|conn| raybend::thumbnail::cache::clear(conn))
            .map_err(|e| e.to_string())
    })
    .await
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
    let db = app
        .state::<crate::browse::BrowseState>()
        .library_thumbs(app, &asset.repository_id)?;
    let key = raw_thumb_key(asset); let sig = raw_thumb_sig(&signature);
    let read_key = key.clone(); let read_sig = sig.clone();
    if let Some(bytes) = db.read(move |conn| raybend::thumbnail::cache::get(conn, &read_key, size, &read_sig)).map_err(|e| e.to_string())? {
        db.note_use(&key, size, &sig, time::now_millis());
        return Ok(Some(bytes));
    }
    let Some(preview) = FullCache::open(&asset.root).ok().and_then(|cache|
        cache.read(asset.asset_id, &FullCache::source_name("raw-original", &signature),
            raybend::store::develop::EditBase::Raw, PIPELINE_VERSION)) else { return Ok(None) };
    let Some(thumb) = raybend::thumbnail::render::render_bytes(&preview, size, None).map_err(|e| e.to_string())? else { return Ok(None) };
    let bytes = thumb.data.clone();
    db.write(move |conn| raybend::thumbnail::cache::put(conn, &key, size, &sig, &thumb.data,
        thumb.width, thumb.height, time::now_millis())).map_err(|e| e.to_string())?;
    let _ = db.maybe_maintain(raybend::thumbnail::cache::DEFAULT_MAX_BYTES, time::now_millis());
    Ok(Some(bytes))
}

/// SOOC 生成 1920 + 384；RAW 仅补 384。只在导入收尾/当前编辑照片后台调用。
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
    let db = browse.library_thumbs(app, &asset.repository_id)?;
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
        let _ = db.maybe_maintain(raybend::thumbnail::cache::DEFAULT_MAX_BYTES, time::now_millis());
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

#[cfg(test)]
mod tests {
    use super::*;
    use raybend::store::develop::{DevelopStack, EditBase};

    #[test]
    fn sooc_screen_renders_the_bitmap_not_the_raw_side() {
        // 未编辑照片的栈是空栈：`source_base` 默认 Raw。选 SOOC 时必须改成 Sooc，
        // 否则 JPG-only 资产会「找不到 raw 源文件」，JPG+RAW 会显示 RAW 解码结果。
        let stack = DevelopStack::default();
        assert_eq!(stack.source_base, EditBase::Raw, "前提：默认基准是 RAW");
        let (variant, resolved) =
            profile_target(ImagePurpose::Screen, IssueChoice::Sooc, &stack).unwrap().unwrap();
        assert_eq!(variant, "sooc");
        assert_eq!(resolved.source_base, EditBase::Sooc);
        assert!(
            profile_target(ImagePurpose::Full, IssueChoice::Sooc, &stack).unwrap().is_none(),
            "位图真身直接给原文件字节，不渲染"
        );
    }

    #[test]
    fn raw_and_latest_targets_keep_their_base() {
        let stack = DevelopStack::default();
        let (variant, resolved) =
            profile_target(ImagePurpose::Screen, IssueChoice::Raw, &stack).unwrap().unwrap();
        assert_eq!(variant, "raw");
        assert_eq!(resolved.source_base, EditBase::Raw);

        let (variant, resolved) =
            profile_target(ImagePurpose::Full, IssueChoice::Raw, &stack).unwrap().unwrap();
        assert_eq!(variant, "raw");
        assert_eq!(resolved.source_base, EditBase::Raw);

        let mut edited = DevelopStack::default();
        edited.source_base = EditBase::Sooc;
        let (variant, resolved) =
            profile_target(ImagePurpose::Screen, IssueChoice::Latest, &edited).unwrap().unwrap();
        assert!(variant.starts_with("latest-"), "编辑结果用栈指纹命名");
        assert_eq!(resolved.source_base, EditBase::Sooc, "编辑基准必须原样保留");
    }

    #[test]
    fn bitmap_full_and_grid_do_not_target_the_profile_cache() {
        let stack = DevelopStack::default();
        assert!(
            profile_target(ImagePurpose::Full, IssueChoice::Sooc, &stack).unwrap().is_none(),
            "位图真身直接给原文件字节，不渲染"
        );
        assert!(
            profile_target(ImagePurpose::Grid, IssueChoice::Raw, &stack).unwrap().is_none(),
            "384 走 thumb_get"
        );
    }
}
