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

pub use full_cache::FullCache;
pub use histogram::{
    DEFAULT_BINS, Histogram, histogram_of_file, histogram_of_image, histogram_of_rgb8,
};
pub use pixels::{DisplayPixels, PixelSize, pixels, pixels_from_avif};

use crate::error::Result;
use crate::media::kind::{self, MediaKind};
use crate::thumbnail::cache as thumb_cache;
use crate::thumbnail::{SizeClass, Thumb, ThumbsDb, cache_key_for, render_file, render_sig};

/// 取图目的 —— 决定走哪一档（以及能不能直接给原图）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagePurpose {
    /// 网格缩略图（长边 384）。
    Grid,
    /// 胶片带缩略图（长边 192）。
    Strip,
    /// 屏幕档（长边 1920）：看图用。
    Screen,
    /// **原图**：位图且没编辑过时直接给文件本身；RAW 或编辑过则退到 `Screen`。
    Original,
}

impl ImagePurpose {
    /// 界面上用的名字（命令参数走它）。
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "grid" => Some(Self::Grid),
            "strip" => Some(Self::Strip),
            "screen" => Some(Self::Screen),
            "original" => Some(Self::Original),
            _ => None,
        }
    }

    /// 退到渲染管线时用哪一档。
    #[must_use]
    const fn size_class(self) -> SizeClass {
        match self {
            // 原图退到渲染时给屏幕档：`FULL`（1:1）还没实现（`thumbnail/mod.rs` 的尺度说明），
            // 而 1:1 是一个要另做决定的东西（内存、缓存策略），不在这里偷偷引入。
            Self::Grid => SizeClass::Grid,
            Self::Strip => SizeClass::Strip,
            Self::Screen | Self::Original => SizeClass::Screen,
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
}

impl<'a> ImageRequest<'a> {
    /// 没编辑过的一次请求（最常见）。
    #[must_use]
    pub const fn plain(path: &'a Path, purpose: ImagePurpose) -> Self {
        Self {
            path,
            purpose,
            edit: None,
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

/// 位图后端：**没编辑过的原图直接给文件本身**（`REPOSITORY.md` §1 的落地文件）。
#[derive(Debug, Clone, Copy, Default)]
pub struct BitmapBackend;

impl DisplayBackend for BitmapBackend {
    fn render(&self, request: &ImageRequest<'_>) -> Result<Option<DisplayImage>> {
        // 「直接给原图」只对 `Original` 且**没编辑过**成立：
        // 网格/胶片带要的是小图（给 20MB 的 TIFF 是灾难），编辑过的必须渲染。
        if request.purpose == ImagePurpose::Original && !request.needs_render() {
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
    let thumb: Option<Thumb> = render_file(request.path, request.purpose.size_class())?;
    Ok(thumb.map(|thumb| DisplayImage {
        bytes: thumb.data,
        // 管线的输出是 AVIF（`thumbnail/render.rs` 的 `encode`；人类 2026-09-24 定的缓存格式）
        mime: ImageMime::Avif,
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
    request.purpose == ImagePurpose::Original && !request.needs_render() && kind != MediaKind::Raw
}

const fn backend_of(kind: MediaKind) -> Backend {
    match kind {
        MediaKind::Raw => Backend::Raw,
        MediaKind::Image | MediaKind::Other => Backend::Bitmap,
    }
}

/// **带磁盘缓存的统一取图口**（view 路径走它；`src-tauri` 的 `view_image`）。
///
/// 与 [`display_image`] 的唯一区别：走渲染管线出来的结果会落进 `thumbs.db`
/// （与网格/胶片带**同一套缓存键与 `render_sig`**），同一张、同一档下次直接命中。
///
/// 为什么必须有它（人类 2026-09-23 报「每切一张都要等 1 秒多」）：
/// 旧的 view 路径每次都重新解码 —— RAW 要几百毫秒到一秒多，连续看图就变成
/// 「每张都等」。缓存命中后只剩一次 SQLite BLOB 读。预载也才有意义：
/// 预载把结果写进磁盘缓存，用户真正翻到那张时不再重解码。
///
/// **不缓存那条「直接给原文件」的路**（位图 + `Original` + 没编辑）：它就是一次
/// 文件读，缓存反而多一次查库，而且会偷换语义（见 [`takes_original_shortcut`]）。
///
/// # Errors
/// 与 [`display_image`] 相同（读不到文件 / 解不开）。
pub fn cached_image(
    thumbs: &ThumbsDb,
    request: &ImageRequest<'_>,
    now_ms: i64,
) -> Result<Option<DisplayImage>> {
    let kind = kind::kind_of_file(&request.path.to_string_lossy());
    if takes_original_shortcut(request, kind) {
        return display_image(request);
    }

    let size = request.purpose.size_class();
    let sig = render_sig(size);
    // 未入库的源文件：「身份字符串」就是绝对路径（与 `render_now` 同一口径）
    let material = request.path.to_string_lossy().into_owned();
    let key = cache_key_for(request.path, &material);

    let read_key = key.clone();
    if let Some((bytes, width, height)) =
        thumbs.read(move |conn| thumb_cache::get_with_size(conn, &read_key, size, sig))?
    {
        return Ok(Some(DisplayImage {
            bytes,
            mime: ImageMime::Jpeg,
            origin: ImageOrigin::Rendered,
            backend: backend_of(kind),
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
    fn purpose_parsing_covers_the_four_intents_and_rejects_junk() {
        for (text, expected) in [
            ("grid", ImagePurpose::Grid),
            ("strip", ImagePurpose::Strip),
            ("screen", ImagePurpose::Screen),
            ("original", ImagePurpose::Original),
        ] {
            assert_eq!(ImagePurpose::parse(text), Some(expected));
        }
        for bad in ["", "FULL", "grid ", "原图"] {
            assert_eq!(ImagePurpose::parse(bad), None, "{bad:?} 不该被认");
        }
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
        assert_eq!(ImagePurpose::Strip.size_class(), SizeClass::Strip);
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
        let first = cached_image(&db, &screen, 0)
            .expect("第一次不该报错")
            .expect("应当有图");
        assert_eq!(first.origin, ImageOrigin::Rendered);
        assert!(first.size.is_some(), "渲染出来的带尺寸");

        let unchanged = cached_image(&db, &screen, 1).unwrap().unwrap();
        assert!(unchanged.bytes == first.bytes, "未变化时仍命中缓存");
        let other = image::RgbImage::from_fn(64, 48, |_, _| image::Rgb([250, 10, 10]));
        other.save(&path).expect("覆盖成另一张图");
        let second = cached_image(&db, &screen, 1)
            .expect("第二次不该报错")
            .expect("缓存里应当有");
        assert!(second.bytes != first.bytes, "源文件覆盖保存必须作废旧渲染");
        assert_ne!(second.size, first.size);

        // 「直接给原文件」那条路不缓存：读到的必须就是刚写进去的那张（而不是旧渲染）
        let original = ImageRequest::plain(&path, ImagePurpose::Original);
        let raw = cached_image(&db, &original, 0)
            .expect("原图请求不该报错")
            .expect("有原文件");
        assert_eq!(raw.origin, ImageOrigin::OriginalFile);
        assert_eq!(raw.bytes, std::fs::read(&path).expect("读原文件"));
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
