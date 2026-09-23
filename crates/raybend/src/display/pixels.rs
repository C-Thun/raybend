//! **像素取图口**（编辑器 GPU 视口用）—— `display_image` 的兄弟，不是它的复制品。
//!
//! # 两个口的分工
//!
//! | 口 | 给什么 | 谁在用 |
//! | --- | --- | --- |
//! | [`super::display_image`] | **JPEG 字节**（给 `<img>`、给缓存、给导出） | 网格 / 胶片带 / 看图 / 直方图 |
//! | [`pixels`] | **RGB 像素**（直接进 GPU 纹理） | 编辑器的 wgpu 视口（M3-W2） |
//!
//! 两条路**共用同一份解码与方向逻辑**（`thumbnail::render::decode_file`）——
//! 这是本文件存在的意义：编辑器**不许**另写一套「RAW 内嵌预览优先 / 方向要摆正」的判断，
//! 那套东西已经踩过三次坑（见 `thumbnail/render.rs` 的 `PIPELINE_VERSION` 记录）。
//!
//! # 两档（`ImageTier` 在渲染侧决定什么时候要哪一档）
//!
//! * [`PixelSize::Screen`]：长边 ≤1920（RAW 走内嵌预览快路径）—— 进视口、适合窗口、缩小看；
//! * [`PixelSize::Full`]：原尺寸完整解码（RAW 走传感器数据）—— `1:1` 看真实像素。
//!
//! 与缩略图那条路一样：**按展示宽高比截取（3:1）只作用于小图**，
//! 编辑视口必须看到完整照片，所以这里两档都不截取。

use std::path::Path;

use crate::error::Result;
use crate::media::kind::{MediaKind, kind_of_file};
use crate::thumbnail::render::{
    DecodeSpec, DecodedSource, PixelOrigin, SCREEN_LONG_EDGE, apply_orientation, decode_file,
    resize_for_thumb,
};

/// 编辑器要的像素档。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelSize {
    /// 屏幕档（长边 ≤1920；RAW 优先用内嵌预览）。
    Screen,
    /// 原尺寸（1:1 看像素）。
    Full,
}

impl PixelSize {
    /// 屏幕档的长边（与缩略图体系同一个常量，别另立一个数）。
    pub const SCREEN_EDGE: u32 = SCREEN_LONG_EDGE;

    /// 界面上/日志里的稳定名字。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Screen => "screen",
            Self::Full => "full",
        }
    }

    /// 对应的一次解码请求。
    #[must_use]
    const fn spec(self) -> DecodeSpec {
        match self {
            Self::Screen => DecodeSpec::thumb(Self::SCREEN_EDGE),
            Self::Full => DecodeSpec::full(),
        }
    }
}

/// 解好的一张图（**RGB8，行主序紧密排列** —— 与 `raw::RawImage8` 同一个口径，
/// 扩成 RGBA8 是渲染那一侧的事，见 `render::RenderImage::from_rgb8`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayPixels {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
    /// 像素从哪来（RAW 的两条路在这一项上分得开 —— 界面与诊断要看它）。
    pub origin: PixelOrigin,
    /// 实际给到的档位（`Screen` 档遇到小图时可能没缩，`Full` 一定是原尺寸）。
    pub size: PixelSize,
}

impl DisplayPixels {
    /// 长度与尺寸对不对得上（跨线程序列化之后要防一手）。
    #[must_use]
    pub fn is_consistent(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.rgb.len() == (self.width as usize) * (self.height as usize) * 3
    }
}

/// 认得出这张照片是什么类型吗（RAW / 位图）。认不出就走位图那条路（与 `display_image` 一致）。
fn is_raw(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| kind_of_file(&name.to_string_lossy()) == MediaKind::Raw)
}

/// **取像素**：给一张照片 + 一个档位，拿回能直接进纹理的 RGB 像素。
///
/// * 文件读不到 / RAW worker 出问题 → `Err`（真错误，界面要报）；
/// * 文件解不开（不是图 / 相机不支持 / 损坏）→ `Ok(None)`（界面显示空态或占位）。
///
/// # Errors
/// 见上。
pub fn pixels(path: &Path, size: PixelSize) -> Result<Option<DisplayPixels>> {
    // RAW 的档位由 worker 执行（跨进程搬 72MB 再缩很亏）；位图这一层的 `max_edge`
    // 只影响 RAW 那条路，位图统一是「先解出来，再在本地缩」。
    let Some(DecodedSource {
        image,
        orientation,
        source,
    }) = decode_file(path, size.spec())?
    else {
        return Ok(None);
    };

    // 方向先摆正（1..8；`None` / 1 原样）——与缩略图那条路同一份实现
    let oriented = match orientation {
        Some(value) if value != 1 => apply_orientation(&image, value),
        _ => image,
    };

    // 缩到档位（不放大）；`Full` 档不缩
    let limited = match size {
        PixelSize::Screen => resize_for_thumb(oriented, PixelSize::SCREEN_EDGE),
        PixelSize::Full => oriented,
    };

    let rgb = limited.to_rgb8();
    let (width, height) = (rgb.width(), rgb.height());
    let pixels = DisplayPixels {
        width,
        height,
        rgb: rgb.into_raw(),
        origin: source,
        size,
    };
    debug_assert!(
        pixels.is_consistent(),
        "解码结果的尺寸与字节数对不上：{}×{} / {} 字节（{}）",
        width,
        height,
        pixels.rgb.len(),
        path.display()
    );
    Ok(Some(pixels))
}

/// 这张照片是 RAW 吗（编辑器据此决定日志与「哪条路」的说法）。
#[must_use]
pub fn is_raw_photo(path: &Path) -> bool {
    is_raw(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// 写一张真 PNG（用 `image` 自己编，免得仓库里塞测试素材）。
    fn png_file(name: &str, width: u32, height: u32) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join(name);
        let mut img = image::RgbImage::new(width, height);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            // 每张图都给一个能认出来的图案（左上红、右下蓝）
            *pixel = image::Rgb([
                (x % 256) as u8,
                (y % 256) as u8,
                ((x + y) % 256) as u8,
            ]);
        }
        img.save(&path).expect("写 PNG");
        (dir, path)
    }

    #[test]
    fn pixel_size_names_are_stable_and_share_the_thumbnail_edge() {
        assert_eq!(PixelSize::Screen.as_str(), "screen");
        assert_eq!(PixelSize::Full.as_str(), "full");
        assert_eq!(
            PixelSize::SCREEN_EDGE,
            SCREEN_LONG_EDGE,
            "屏幕档必须与缩略图体系同一个常量（两处各写一个数就会漂）"
        );
        assert_eq!(PixelSize::Screen.spec().max_edge, Some(SCREEN_LONG_EDGE));
        assert!(PixelSize::Full.spec().max_edge.is_none(), "全尺寸档不设上限");
        assert!(!PixelSize::Full.spec().allow_preview, "全尺寸要真解码");
    }

    #[test]
    fn screen_size_keeps_small_images_untouched() {
        let (_dir, path) = png_file("small.png", 64, 32);
        let image = pixels(&path, PixelSize::Screen)
            .expect("不该报错")
            .expect("有图");
        assert_eq!((image.width, image.height), (64, 32), "小图不放大");
        assert_eq!(image.rgb.len(), 64 * 32 * 3);
        assert!(image.is_consistent());
        assert_eq!(image.origin, PixelOrigin::Bitmap);
        assert_eq!(image.size, PixelSize::Screen);
    }

    #[test]
    fn screen_size_caps_the_long_edge_and_keeps_aspect() {
        let (_dir, path) = png_file("wide.png", 4000, 1000);
        let image = pixels(&path, PixelSize::Screen)
            .expect("不该报错")
            .expect("有图");
        assert_eq!(image.width, PixelSize::SCREEN_EDGE, "长边压到屏幕档");
        assert_eq!(image.height, PixelSize::SCREEN_EDGE / 4, "比例保持");
        assert!(image.is_consistent());
    }

    #[test]
    fn full_size_does_not_downscale() {
        let (_dir, path) = png_file("big.png", 2400, 1200);
        let image = pixels(&path, PixelSize::Full)
            .expect("不该报错")
            .expect("有图");
        assert_eq!((image.width, image.height), (2400, 1200));
        assert_eq!(image.size, PixelSize::Full);
        // 与屏幕档比：屏幕档应当更小（说明两档真的不同）
        let small = pixels(&path, PixelSize::Screen)
            .expect("不该报错")
            .expect("有图");
        assert!(
            small.width < image.width,
            "屏幕档 {} 应当小于全尺寸档 {}",
            small.width,
            image.width
        );
    }

    #[test]
    fn portrait_images_stay_portrait_after_scaling() {
        // 1000×4000 缩到长边 1920 → 480×1920（比例 1:4 不变）
        let (_dir, path) = png_file("portrait.png", 1000, 4000);
        let image = pixels(&path, PixelSize::Screen)
            .expect("不该报错")
            .expect("有图");
        assert_eq!((image.width, image.height), (480, 1920));
        assert!(image.height > image.width, "竖图缩放后必须还是竖的");
        assert!(image.is_consistent());
    }

    #[test]
    fn undecodable_and_missing_files_stay_distinguishable() {
        let dir = tempfile::tempdir().expect("临时目录");
        let junk = dir.path().join("junk.png");
        std::fs::write(&junk, b"not really a png").expect("写文件");
        assert!(
            pixels(&junk, PixelSize::Screen).expect("解不开不是错").is_none(),
            "解不开 → None（界面用占位图）"
        );

        let missing = dir.path().join("没有这张.png");
        assert!(
            pixels(&missing, PixelSize::Screen).is_err(),
            "读不到文件 → Err（与「解不开」分开）"
        );
    }

    #[test]
    fn raw_files_are_recognised_for_diagnostics() {
        assert!(is_raw_photo(Path::new("/x/IMG.RW2")));
        assert!(is_raw_photo(Path::new("/x/IMG.CR3")));
        assert!(!is_raw_photo(Path::new("/x/IMG.JPG")));
        assert!(!is_raw_photo(Path::new("/x/noext")));
    }
}
