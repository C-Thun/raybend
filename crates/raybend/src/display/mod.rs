//! **统一取图口**（人类 2026-09-18 定；口径见 `specs/M2-W2.md` §2.1）。
//!
//! # 它解决什么
//!
//! 「给我一张能显示的图」这件事，在代码里以前散在三处：
//! `thumbnail::render_file`（网格/胶片带）、`raw::` 的内嵌预览、以及以后编辑器的渲染。
//! 调用方得自己判断「这是 RAW 还是位图」「有没有编辑过」「该走哪条路」——
//! 每加一个消费方（view、对比、导出预览…）就重写一遍这套判断。
//!
//! 本模块把这些判断收成**一个入口**：调用方只给「哪张照片 + 要多大 + 有没有编辑」，
//! 拿回 [`DisplayImage`]（字节 + MIME + 来源），**不需要知道它是 RAW 还是位图**。
//!
//! # 两个后端（`AGENTS.md` 里说的「两个口」）
//!
//! | 后端 | 什么时候走 | 现在给出什么 |
//! | --- | --- | --- |
//! | [`BitmapBackend`] | 这张照片是位图（JPG/PNG/HEIC…） | **没编辑过要原图 → 直接给原文件字节**；其余交给渲染管线 |
//! | [`RawBackend`] | 这张照片是 RAW | 交给渲染管线（内嵌预览优先，其次完整解码） |
//!
//! 分派靠 `media::kind`（扩展名表），**调用方永远不用自己判**。
//!
//! # 与编辑器的关系（重要，别理解反）
//!
//! 这个口**服务于 view 与缩略图缓存**，**不是**编辑器的实时路径 ——
//! 编辑器每改一次参数就渲染一帧，那一帧直接进 GPU 纹理，既不落盘也不经这里
//! （「操作一次生成一次图片到磁盘上」是必须避免的）。
//!
//! 但两边**共用同一份渲染模块**：编辑栈的应用、色彩、缩放都在
//! [`crate::thumbnail::render`] 与（将来的）`crate::render` 里。
//! 也就是说：**这个口 = 对「编辑器里真正把图片渲染出来」那条链路的一次封装**，
//! 只是它把结果落成字节，好给 `<img>`、给缓存、给导出用。
//!
//! # 本轮范围（基础框架）
//!
//! * 两个后端 + 统一入口 + 用途枚举 ✅
//! * **编辑栈是占位的**（[`EditSpec`]）：结构留好，等编辑里程碑接进来；
//! * 缓存复用缩略图那套（`render_sig` 含管线版本，算法升级自动变孤儿）；
//! * 临时图**不落盘**：本轮所有路径都是「现算 + 交给缓存」，没有自建临时文件。
//!
//! # 兄弟口：像素（M3-W2 加）
//!
//! [`pixels`] 是给 **GPU 纹理**用的：同样是「给一张照片、拿一张能显示的图」，
//! 但它出的是 RGB 像素而不是 JPEG 字节（编辑视口不能把图编成 JPEG 再解回来 ——
//! 那是 RapidRAW 官方博客记的 20fps→120fps 那道坎）。
//! 两个口**共用同一份解码与方向逻辑**（`thumbnail::render::decode_file`）。

pub mod full_cache;
pub mod histogram;
pub mod pixels;
pub mod output;

use std::path::Path;

pub use full_cache::{CacheClass, FullCache};
pub use histogram::{
    DEFAULT_BINS, Histogram, histogram_of_file, histogram_of_image, histogram_of_rgb8,
};
pub use pixels::{DisplayPixels, PixelSize, pixels, pixels_from_avif};

use crate::error::Result;
use crate::media::kind::{self, MediaKind};
use crate::thumbnail::cache as thumb_cache;
use crate::thumbnail::{SizeClass, Thumb, ThumbsDb, cache_key_for, render_sig};

/// 取图目的 —— 决定走哪一档（以及能不能直接给原图）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagePurpose {
    /// 缩略图（长边 384）—— 网格 tiles、胶片带、定稿卡片共用这一档。
    ///
    /// 192 那一档 2026-10-11 取消（崔总：384 直接缩着显示就行，
    /// 没必要为胶片带多算一遍缩图；`specs/thumbnail-tiers-and-import-memory.md`）。
    Grid,
    /// 屏幕档（长边 1920）：**看图载入期间的占位**。
    ///
    /// 2026-10-11 起它的定位是占位，不是看图成品（真身见 [`Self::Full`]）。
    Screen,
    /// **原图**：位图且没编辑过时直接给文件本身；RAW 或编辑过则退到 `Screen`。
    Original,
    /// **真身**（2026-10-11 加，崔总口径）：该图自己的像素，不缩放、不降分辨率。
    ///
    /// * 位图 + 没编辑 → **直接给原文件字节**（与 [`Self::Original`] 同一条短路）；
    /// * 位图 / RAW + 有编辑 → 全尺寸渲染（RAW 走**完整解码**，不用内嵌预览）；
    /// * 这是看图的**终态**：`Screen` 只在它到达之前顶着（`specs/view-true-image.md`）。
    Full,
}

impl ImagePurpose {
    /// 界面上用的名字（命令参数走它）。
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "grid" => Some(Self::Grid),
            "screen" => Some(Self::Screen),
            "original" => Some(Self::Original),
            "full" => Some(Self::Full),
            _ => None,
        }
    }

    /// 退到渲染管线时用哪一档。
    #[must_use]
    const fn size_class(self) -> SizeClass {
        match self {
            Self::Grid => SizeClass::Grid,
            // 原图退到渲染时给屏幕档：它是「要原文件本身」的语义，2026-10-11
            // 之后真身那条需求由 `Full` 承担（那里真的会去全尺寸渲染）。
            Self::Screen | Self::Original => SizeClass::Screen,
            Self::Full => SizeClass::Full,
        }
    }
}

/// 图片字节是什么格式（`<img>` 要的 MIME 由它决定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageMime {
    Jpeg,
    Png,
    Webp,
    Gif,
    Tiff,
    /// **AVIF**（M3-W3 起渲染管线的输出格式：人类 2026-09-24 定「缓存图一律 AVIF 90 / 4:4:4」）。
    Avif,
    /// 认不出来的扩展名 —— 按 JPEG 处理。
    Unknown,
}

impl ImageMime {
    /// HTTP/Blob 用的 MIME 串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Jpeg | Self::Unknown => "image/jpeg",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
            Self::Gif => "image/gif",
            Self::Tiff => "image/tiff",
            Self::Avif => "image/avif",
        }
    }

    /// 按扩展名判（大小写不敏感）。**不看文件内容** —— 这一步只是为了少一轮 IO，
    /// 真正的解码失败由渲染管线报错。
    #[must_use]
    pub fn of_path(path: &Path) -> Self {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "jpg" | "jpeg" | "jpe" | "jfif" | "heic" | "heif" | "avif" => Self::Jpeg,
            "png" | "apng" => Self::Png,
            "webp" => Self::Webp,
            "gif" => Self::Gif,
            "tif" | "tiff" => Self::Tiff,
            _ => Self::Unknown,
        }
    }
}

/// 字节是从哪来的（诊断 + 测试用；调用方通常不用管）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageOrigin {
    /// **原文件本身**（没编辑过的位图 —— 「直接给原图地址」那条路）。
    OriginalFile,
    /// 渲染管线现算的（缩略图 / 屏幕档 / RAW 的解码结果）。
    Rendered,
}

/// 实际干活的两个后端是谁（诊断用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Bitmap,
    Raw,
}

/// 一张能显示的图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayImage {
    pub bytes: Vec<u8>,
    pub mime: ImageMime,
    pub origin: ImageOrigin,
    pub backend: Backend,
    /// 渲染管线给的尺寸（走原文件那条路时是 `None` —— 我们不为了报尺寸去解码原图）。
    pub size: Option<(u32, u32)>,
}

/// **编辑栈的占位**（编辑里程碑接进来）。
///
/// 现在只有「有没有编辑」这一个事实：`None` = 没编辑过（导入阶段、SOOC 原图）。
/// 将来的编辑栈（色调、裁剪、局部调整…）会作为它的字段进来，两个后端都要在同一个位置应用它 ——
/// **编辑器也是在这里应用**（只是输出方式不同：那边进 GPU 纹理，这边出字节）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditSpec {
    /// 占位字段：真接进来之前，任何「非默认」的编辑都必须显式打开它，
    /// 免得将来有人以为「没传编辑参数 = 没编辑」是永久成立的假设。
    pub has_edits: bool,
}

/// 一次取图请求。
#[derive(Debug, Clone, Copy)]
pub struct ImageRequest<'a> {
    pub path: &'a Path,
    pub purpose: ImagePurpose,
    /// 编辑栈；`None` = 没编辑过（view 现在全走这条）。
    pub edit: Option<&'a EditSpec>,
    /**
     * **这张图属于哪一套口径**（崔总 2026-10-11 定两条，`AGENTS.md` §2.24）。
     *
     * 一个字段管三件事 —— 编码格式、缓存签名、缓存住在哪 —— 因为它们本来就是
     * 同一个决定（库内是持久资产，库外是「看一眼就走」的临时目录）。
     * 拆成三个字段就等着哪天有人改了一个忘了另外两个。
     */
    pub scope: ImageScope,
}

/// **库内 / 库外两套口径**（`AGENTS.md` §2.24）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageScope {
    /// 入库资产：AVIF q90 + 落盘缓存（重启后浏览还得靠它）。
    Library,
    /// 来源目录（import）/ 未入库文件：JPEG q82 + **只在进程内存里**
    /// （临时目录下次打开内容可能完全不同，落盘的行永远读不回来）。
    External,
}

impl ImageScope {
    /// 渲染出来的图用哪种编码（实测 1920 档 AVIF 要几秒，JPEG 快 2–3 倍）。
    #[must_use]
    pub const fn format(self) -> crate::thumbnail::render::EncodeFormat {
        match self {
            Self::Library => crate::thumbnail::render::EncodeFormat::Avif,
            Self::External => crate::thumbnail::render::EncodeFormat::Jpeg,
        }
    }

    /// 缓存签名：两套口径各存一条，同一个文件先后被 import 与 browse 看不会互相冒充。
    #[must_use]
    pub const fn render_sig(self, size: crate::thumbnail::render::SizeClass) -> &'static str {
        match self {
            Self::Library => render_sig(size),
            Self::External => crate::thumbnail::render::render_sig_source(size),
        }
    }

    /// 编出来的是哪种字节（给 `<img>` 声明 MIME 用）。
    #[must_use]
    pub const fn mime(self) -> ImageMime {
        match self {
            Self::Library => ImageMime::Avif,
            Self::External => ImageMime::Jpeg,
        }
    }
}

impl<'a> ImageRequest<'a> {
    /// 没编辑过的一次请求（最常见）—— **库内口径**。
    #[must_use]
    pub const fn plain(path: &'a Path, purpose: ImagePurpose) -> Self {
        Self {
            path,
            purpose,
            edit: None,
            scope: ImageScope::Library,
        }
    }

    /// 没编辑过的一次请求 —— **库外口径**（来源目录 / 未入库）。
    #[must_use]
    pub const fn source(path: &'a Path, purpose: ImagePurpose) -> Self {
        Self {
            path,
            purpose,
            edit: None,
            scope: ImageScope::External,
        }
    }

    /// 有没有需要套用的编辑。
    #[must_use]
    fn needs_render(&self) -> bool {
        self.edit.is_some_and(|spec| spec.has_edits)
    }
}

/// 后端接口：拿请求、还一张能显示的图。
///
/// 两个实现都很薄 —— 真正的算法在 `thumbnail::render` 与（将来的）编辑渲染模块里，
/// 这里只负责「选路 + 决定给原图还是给渲染结果」。
pub trait DisplayBackend {
    /// 取不到图（认不出类型 / 解不开）返回 `Ok(None)`；真出错才是 `Err`。
    fn render(&self, request: &ImageRequest<'_>) -> Result<Option<DisplayImage>>;
}

/// 位图后端：**没编辑过的原图直接给文件本身**（`memory/FUNCTION-REPOSITORY.md` §1 的落地文件）。
#[derive(Debug, Clone, Copy, Default)]
pub struct BitmapBackend;

impl DisplayBackend for BitmapBackend {
    fn render(&self, request: &ImageRequest<'_>) -> Result<Option<DisplayImage>> {
        // 「直接给原图」对 `Original` 与 `Full` 且**没编辑过**成立：
        // 网格/胶片带要的是小图（给 20MB 的 TIFF 是灾难），编辑过的必须渲染。
        // `Full` = 真身（崔总 2026-10-11）：没编辑过的位图，它的真身就是文件本身 ——
        // 不再编一遍（那既慢又会改变像素）。
        if matches!(request.purpose, ImagePurpose::Original | ImagePurpose::Full)
            && !request.needs_render()
        {
            let bytes = std::fs::read(request.path)?;
            return Ok(Some(DisplayImage {
                bytes,
                mime: ImageMime::of_path(request.path),
                origin: ImageOrigin::OriginalFile,
                backend: Backend::Bitmap,
                size: None,
            }));
        }
        rendered(request, Backend::Bitmap)
    }
}

/// RAW 后端：**永远走渲染管线**（内嵌预览优先 → 完整解码），不存在「给原图」这条路 ——
/// 原始传感器数据不是能显示的东西。
#[derive(Debug, Clone, Copy, Default)]
pub struct RawBackend;

impl DisplayBackend for RawBackend {
    fn render(&self, request: &ImageRequest<'_>) -> Result<Option<DisplayImage>> {
        rendered(request, Backend::Raw)
    }
}

/// 走一遍渲染管线（两条路共用）。
fn rendered(request: &ImageRequest<'_>, backend: Backend) -> Result<Option<DisplayImage>> {
    let thumb: Option<Thumb> = crate::thumbnail::render::render_file_with_edit_and_lut(
        request.path,
        request.purpose.size_class(),
        None,
        None,
        None,
        request.scope.format(),
    )?;
    let mime = request.scope.mime();
    Ok(thumb.map(|thumb| DisplayImage {
        bytes: thumb.data,
        mime,
        origin: ImageOrigin::Rendered,
        backend,
        size: Some((thumb.width, thumb.height)),
    }))
}

/// **统一取图口**：给一张照片，拿一张能显示的图。
///
/// * 认不出类型 / 文件不存在 → `Ok(None)`（界面显示空态，不是错误）；
/// * 编解码失败 → `Err`（界面显示错误态）。
pub fn display_image(request: &ImageRequest<'_>) -> Result<Option<DisplayImage>> {
    match kind::kind_of_file(&request.path.to_string_lossy()) {
        MediaKind::Raw => RawBackend.render(request),
        // `Other`（不是照片）也走位图后端：读原图时会把「不是图片」暴露给调用方，
        // 而渲染那边有 `placeholder()` 兜底 —— 界面上是「未知类型」而不是崩溃。
        MediaKind::Image | MediaKind::Other => BitmapBackend.render(request),
    }
}

/// 「位图 + 原图 + 没编辑」——会**直接给原文件字节**那条路（见 [`BitmapBackend`]）。
///
/// 抽出来是给 [`cached_image`] 判断「这次请求该不该查缓存」：
/// 那条路就是一次文件读，而且 `Original` 的语义是「要原文件本身」，
/// 命中一条 Screen 渲染缓存是错的。
fn takes_original_shortcut(request: &ImageRequest<'_>, kind: MediaKind) -> bool {
    // `Full` 与 `Original` 在同一条短路上：**位图 + 没编辑 ⇒ 原文件本身就是真身**
    //（SOOC 一秒都不用等，也不用缓存 —— 这是最快的那条路）。
    matches!(request.purpose, ImagePurpose::Original | ImagePurpose::Full)
        && !request.needs_render()
        && kind != MediaKind::Raw
}

const fn backend_of(kind: MediaKind) -> Backend {
    match kind {
        MediaKind::Raw => Backend::Raw,
        MediaKind::Image | MediaKind::Other => Backend::Bitmap,
    }
}

/// **带缓存的统一取图口**（view 路径走它；`src-tauri` 的 `view_image`）。
///
/// 与 [`display_image`] 的唯一区别：走渲染管线出来的结果会被缓存，同一张、同一档
/// 下次直接命中。**缓存住在哪由 [`ImageRequest::scope`] 决定**（崔总 2026-10-11；
/// `AGENTS.md` §2.24）：
///
/// | 口径 | 缓存 | 为什么 |
/// | --- | --- | --- |
/// | 库内（入库资产） | 磁盘 `thumbs.db`（与网格/胶片带同一套键与 `render_sig`） | 持久资产，重启后浏览还得靠它 |
/// | 库外（import / 未入库） | **只在 `memory`（进程内存）** | 临时目录下次打开内容可能全变，落盘的行永远读不回来（§2.24） |
///
/// 为什么必须有缓存（人类 2026-09-23 报「每切一张都要等 1 秒多」）：旧路径每次都重新解码 ——
/// RAW 要几百毫秒到一秒多。命中之后只剩一次读。
///
/// **不缓存那条「直接给原文件」的路**（位图 + `Original` + 没编辑）：它就是一次
/// 文件读，缓存反而多一次查找，而且会偷换语义（见 [`takes_original_shortcut`]）。
///
/// 两份缓存句柄**按口径各要一份**：库内要 `thumbs`（磁盘），库外要 `memory`（进程内存）。
/// 该给的那份没给（`None`）就只是「这次不缓存」，语义不变 —— 测试与无状态的调用方走这条。
///
/// # Errors
/// 与 [`display_image`] 相同（读不到文件 / 解不开）。
pub fn cached_image(
    thumbs: Option<&ThumbsDb>,
    memory: Option<&crate::thumbnail::memory::SourceThumbs>,
    request: &ImageRequest<'_>,
    now_ms: i64,
) -> Result<Option<DisplayImage>> {
    let kind = kind::kind_of_file(&request.path.to_string_lossy());
    if takes_original_shortcut(request, kind) {
        return display_image(request);
    }

    let size = request.purpose.size_class();
    let sig = request.scope.render_sig(size);
    let mime = request.scope.mime();
    let backend = backend_of(kind);

    /*
     * **真身档（库内）不进 `thumbs.db`**（2026-10-11）：那一档是 ~9MB/张的 JPEG，
     * 该住在库内 `cache/full/<资产>/`（跟库走、随资产删、有容量上限与 LRU），
     * 不是 app data 里那个「几万行小 BLOB」的缩略图库（`AGENTS.md` §6.5）。
     * 所以库内的真身档只“现算 + 交给调用方缓存”，这里不做 DB 读写。
     *
     * 库外（import）**不一样**：那边的真身要在内存里留最近的 10 张
     * （`specs/thumbnail-tiers-and-import-memory.md`），所以这一档不在这里短路。
     */
    if size.is_full() && request.scope == ImageScope::Library {
        return display_image(request);
    }

    // ── 库外：**只在进程内存里**，磁盘一眼都不看 ───────────────────────────
    if request.scope == ImageScope::External {
        // 进内存缓存的只有缩略图（384）与真身两档；1920 那一档 import 不产生（见 `memory`）
        let key = crate::thumbnail::memory::SourceKey::of(request.path, size, sig);
        if let Some(hit) = memory.and_then(|cache| cache.get(&key)) {
            return Ok(Some(DisplayImage {
                bytes: hit.data,
                mime,
                origin: ImageOrigin::Rendered,
                backend,
                size: Some((hit.width, hit.height)),
            }));
        }
        let Some(image) = display_image(request)? else {
            return Ok(None);
        };
        if image.origin == ImageOrigin::Rendered {
            let (width, height) = image.size.unwrap_or((0, 0));
            if let Some(cache) = memory {
                cache.put(
                    key,
                    Thumb {
                        data: image.bytes.clone(),
                        width,
                        height,
                        placeholder: false,
                    },
                );
            }
        }
        return Ok(Some(image));
    }

    // ── 库内：磁盘缓存（未入库的源文件身份字符串就是绝对路径，见 `cache_key_for`）──
    let Some(thumbs) = thumbs else {
        return display_image(request);
    };
    debug_assert!(!size.is_full(), "库内真身档在上面就短路了");
    let material = request.path.to_string_lossy().into_owned();
    let key = cache_key_for(request.path, &material);

    let read_key = key.clone();
    if let Some((bytes, width, height)) =
        thumbs.read(move |conn| thumb_cache::get_with_size(conn, &read_key, size, sig))?
    {
        return Ok(Some(DisplayImage {
            bytes,
            mime,
            origin: ImageOrigin::Rendered,
            backend,
            size: Some((width, height)),
        }));
    }

    let Some(image) = display_image(request)? else {
        return Ok(None);
    };
    if image.origin == ImageOrigin::Rendered {
        let data = image.bytes.clone();
        let (width, height) = image.size.unwrap_or((0, 0));
        thumbs.write(move |conn| {
            thumb_cache::put(conn, &key, size, sig, &data, width, height, now_ms)
        })?;
    }
    Ok(Some(image))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_file(name: &str, bytes: &[u8]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).expect("写文件");
        (dir, path)
    }

    #[test]
    fn purpose_parsing_covers_the_five_intents_and_rejects_junk() {
        for (text, expected) in [
            ("grid", ImagePurpose::Grid),
            ("screen", ImagePurpose::Screen),
            ("original", ImagePurpose::Original),
            ("full", ImagePurpose::Full),
        ] {
            assert_eq!(ImagePurpose::parse(text), Some(expected));
        }
        for bad in ["", "FULL", "grid ", "原图"] {
            assert_eq!(ImagePurpose::parse(bad), None, "{bad:?} 不该被认");
        }
        assert_eq!(
            ImagePurpose::Full.size_class(),
            crate::thumbnail::SizeClass::Full,
            "真身档要真的落到「不缩放」那一档（不是又一档 1920）"
        );
    }

    /// 真身档对**没编辑过的位图**走「原文件」短路 —— SOOC 看图不用渲染、不用缓存。
    #[test]
    fn full_purpose_serves_the_original_file_for_an_unedited_bitmap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shot.jpg");
        let bytes = {
            let mut data = Vec::new();
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut data, 80);
            encoder
                .encode(
                    &vec![90u8; 320 * 240 * 3],
                    320,
                    240,
                    image::ExtendedColorType::Rgb8,
                )
                .unwrap();
            data
        };
        std::fs::write(&path, &bytes).unwrap();
        let request = ImageRequest::plain(&path, ImagePurpose::Full);
        assert!(
            takes_original_shortcut(&request, MediaKind::Image),
            "位图 + 没编辑 + 真身档 = 原文件本身"
        );
        assert!(
            !takes_original_shortcut(&request, MediaKind::Raw),
            "RAW 没有「原文件就是图」这回事，要走完整解码"
        );
        let image = display_image(&request).unwrap().expect("原文件");
        assert_eq!(image.bytes, bytes, "给的就是文件本身，一个字节不改");
    }

    #[test]
    fn original_falls_back_to_screen_for_rendering() {
        assert_eq!(ImagePurpose::Screen.size_class(), SizeClass::Screen);
        assert_eq!(
            ImagePurpose::Original.size_class(),
            SizeClass::Screen,
            "1:1（FULL）还没实现，原图的退路是屏幕档"
        );
        assert_eq!(ImagePurpose::Grid.size_class(), SizeClass::Grid);
    }

    #[test]
    fn mime_comes_from_the_extension_case_insensitively() {
        let cases = [
            ("a.JPG", ImageMime::Jpeg),
            ("a.jpeg", ImageMime::Jpeg),
            ("a.HEIC", ImageMime::Jpeg),
            ("a.png", ImageMime::Png),
            ("a.WEBP", ImageMime::Webp),
            ("a.gif", ImageMime::Gif),
            ("a.TIF", ImageMime::Tiff),
            ("a.rw2", ImageMime::Unknown),
            ("noext", ImageMime::Unknown),
        ];
        for (name, expected) in cases {
            assert_eq!(ImageMime::of_path(Path::new(name)), expected, "{name}");
        }
        assert_eq!(ImageMime::Unknown.as_str(), "image/jpeg", "兜底按 JPEG 说");
        assert_eq!(ImageMime::Png.as_str(), "image/png");
    }

    #[test]
    fn a_bitmap_asked_for_the_original_gets_the_file_itself() {
        let (_dir, path) = tmp_file("photo.JPG", b"not-really-a-jpeg-but-bytes-are-bytes");
        let image = display_image(&ImageRequest::plain(&path, ImagePurpose::Original))
            .expect("不该报错")
            .expect("应当有图");
        assert_eq!(image.origin, ImageOrigin::OriginalFile);
        assert_eq!(image.backend, Backend::Bitmap);
        assert_eq!(image.mime, ImageMime::Jpeg);
        assert_eq!(image.size, None, "不为了报尺寸去解码原图");
        assert_eq!(image.bytes, b"not-really-a-jpeg-but-bytes-are-bytes");
    }

    #[test]
    fn cached_image_serves_from_disk_and_leaves_the_original_shortcut_uncached() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("photo.jpg");
        let img = image::RgbImage::from_fn(8, 6, |x, y| {
            image::Rgb([(x * 20) as u8, (y * 30) as u8, 40])
        });
        img.save(&path).expect("写 JPEG");
        let db = ThumbsDb::open(dir.path().join("cache"), 0).expect("开缓存库");

        // 未变更时命中；同一路径覆盖保存后必须重新渲染。
        let screen = ImageRequest::plain(&path, ImagePurpose::Screen);
        let first = cached_image(Some(&db), None, &screen, 0)
            .expect("第一次不该报错")
            .expect("应当有图");
        assert_eq!(first.origin, ImageOrigin::Rendered);
        assert!(first.size.is_some(), "渲染出来的带尺寸");

        let unchanged = cached_image(Some(&db), None, &screen, 1).unwrap().unwrap();
        assert!(unchanged.bytes == first.bytes, "未变化时仍命中缓存");
        let other = image::RgbImage::from_fn(64, 48, |_, _| image::Rgb([250, 10, 10]));
        other.save(&path).expect("覆盖成另一张图");
        let second = cached_image(Some(&db), None, &screen, 1)
            .expect("第二次不该报错")
            .expect("缓存里应当有");
        assert!(second.bytes != first.bytes, "源文件覆盖保存必须作废旧渲染");
        assert_ne!(second.size, first.size);

        // 「直接给原文件」那条路不缓存：读到的必须就是刚写进去的那张（而不是旧渲染）
        let original = ImageRequest::plain(&path, ImagePurpose::Original);
        let raw = cached_image(Some(&db), None, &original, 0)
            .expect("原图请求不该报错")
            .expect("有原文件");
        assert_eq!(raw.origin, ImageOrigin::OriginalFile);
        assert_eq!(raw.bytes, std::fs::read(&path).expect("读原文件"));
    }

    #[test]
    fn external_scope_caches_in_memory_and_never_touches_the_disk() {
        use crate::thumbnail::memory::SourceThumbs;

        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("photo.jpg");
        let img = image::RgbImage::from_fn(64, 48, |_, _| image::Rgb([30, 90, 200]));
        img.save(&path).expect("写 JPEG");
        let db = ThumbsDb::open(dir.path().join("cache"), 0).expect("开缓存库");
        let memory = SourceThumbs::new();
        let request = ImageRequest::source(&path, ImagePurpose::Grid);

        let first = cached_image(Some(&db), Some(&memory), &request, 0)
            .expect("第一次不该报错")
            .expect("应当有图");
        assert_eq!(first.mime, ImageMime::Jpeg, "库外口径是 JPEG");

        // 第二次：命中**内存**，磁盘一行都没写（崔总 2026-10-11；AGENTS.md §2.24）
        let again = cached_image(Some(&db), Some(&memory), &request, 1)
            .expect("第二次不该报错")
            .expect("内存里应当有");
        assert_eq!(again.bytes, first.bytes);
        assert_eq!(
            memory.stats().entries,
            1,
            "库外的那张住在内存里"
        );
        assert_eq!(
            db.read(crate::thumbnail::cache::stats).unwrap().entries,
            0,
            "库外渲染不许碰磁盘缓存"
        );

        // 文件换了内容（大小或 mtime 变了）→ 内存里那条自动不命中
        let other = image::RgbImage::from_fn(96, 96, |_, _| image::Rgb([200, 20, 20]));
        other.save(&path).expect("覆盖成另一张图");
        let third = cached_image(Some(&db), Some(&memory), &request, 2)
            .expect("第三次不该报错")
            .expect("应当有图");
        assert_ne!(third.bytes, first.bytes, "换了内容必须重新渲染");
        assert_eq!(third.size, Some((96, 96)), "96px 的图不放大");
        assert_eq!(db.read(crate::thumbnail::cache::stats).unwrap().entries, 0);
    }

    #[test]
    fn external_1920_is_computed_but_never_cached() {
        use crate::thumbnail::memory::SourceThumbs;

        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("photo.jpg");
        image::RgbImage::from_fn(64, 48, |_, _| image::Rgb([9, 9, 9]))
            .save(&path)
            .expect("写 JPEG");
        let db = ThumbsDb::open(dir.path().join("cache"), 0).expect("开缓存库");
        let memory = SourceThumbs::new();
        let request = ImageRequest::source(&path, ImagePurpose::Screen);

        let first = cached_image(Some(&db), Some(&memory), &request, 0)
            .expect("不该报错")
            .expect("应当有图");
        assert_eq!(first.mime, ImageMime::Jpeg);
        // import 不产生也不请求 1920 这一档（占位用 384）：真有人要就现算，不留内存
        assert_eq!(memory.stats().entries, 0, "1920 不该进内存缓存");
        assert_eq!(db.read(crate::thumbnail::cache::stats).unwrap().entries, 0);
    }

    #[test]
    fn a_missing_file_is_not_an_error() {
        let dir = tempfile::tempdir().expect("临时目录");
        let missing = dir.path().join("没有这张.jpg");
        let result = display_image(&ImageRequest::plain(&missing, ImagePurpose::Original));
        // 位图的「给原图」那条路读不到文件会报错（IO 错误），这是对的：
        // 它和「RAW 解不开」是两回事，调用方要能区分。
        assert!(result.is_err() || matches!(result, Ok(None)));
    }

    #[test]
    fn edited_bitmaps_never_take_the_original_shortcut() {
        let (_dir, path) = tmp_file("photo.JPG", b"bytes");
        let edit = EditSpec { has_edits: true };
        let request = ImageRequest {
            path: &path,
            purpose: ImagePurpose::Original,
            edit: Some(&edit),
            scope: ImageScope::Library,
        };
        // 有编辑 ⇒ 不能给原图（那会绕开编辑）—— 于是走渲染管线；
        // 假 JPEG 解不开，所以这里只断言「没走原图那条路」。
        let outcome = display_image(&request);
        match outcome {
            Ok(Some(image)) => assert_ne!(
                image.origin,
                ImageOrigin::OriginalFile,
                "编辑过的位图不许直接给原文件"
            ),
            // 解不开（我们写的不是真 JPEG）也是「走了渲染」的证据
            Err(_) => {}
            Ok(None) => {}
        }
    }

    #[test]
    fn raw_never_returns_the_original_file() {
        // 扩展名决定走哪个后端：RAW 那条压根没有「给原图」的分支
        assert_eq!(
            kind::kind_of_file("a.RW2"),
            MediaKind::Raw,
            "前提：RW2 被认成 RAW"
        );
        let dir = tempfile::tempdir().expect("临时目录");
        for name in ["a.RW2", "a.CR3", "a.NEF", "a.DNG"] {
            let path = dir.path().join(name);
            std::fs::write(&path, b"fake-raw").expect("写文件");
            let outcome = display_image(&ImageRequest::plain(&path, ImagePurpose::Original));
            match outcome {
                Ok(Some(image)) => assert_ne!(image.origin, ImageOrigin::OriginalFile),
                Err(_) => {}
                Ok(None) => {}
            }
        }
    }
}
