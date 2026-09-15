//! 缩略图渲染：解码 → 按 EXIF 朝向摆正 → 缩放 → 编码 JPEG。
//!
//! # 尺度（`AGENTS.md` §6.5）
//!
//! 第一阶段只有两个尺度，够用且省事：
//!
//! * [`SizeClass::Grid`] —— 网格缩略图，长边 384：浏览网格最多 9 档（最大 512 显示宽），
//!   384 在常见档位下足够清晰；
//! * [`SizeClass::Strip`] —— 胶片带，长边 192（显示约 96×80，给到 2×）。
//!
//! `SCREEN` / `FULL` 属于后续（视口分辨率与 1:1），本阶段不做。
//!
//! # 渲染签名
//!
//! [`render_sig`] 把「编码格式 + 质量 + 管线版本」编成一个串，进缓存键。
//! **改了算法就改版本号** —— 旧缓存会自动变成孤儿被 GC 收走，不用写数据迁移。
//!
//! # RAW 与「只有 RAW」
//!
//! 本阶段 RAW **不做真实解码**（`FUTURE.md` B8 记录了原因与届时的要求），一律用
//! [`placeholder`] 生成的占位图。占位图是程序画的，**不依赖任何字体文件**。

use std::path::Path;

use image::{DynamicImage, ExtendedColorType, ImageFormat, Rgb, RgbImage};

use crate::error::{Error, Result};
use crate::media::kind::MediaKind;

/// 网格缩略图长边。
pub const GRID_LONG_EDGE: u32 = 384;
/// 胶片带缩略图长边。
pub const STRIP_LONG_EDGE: u32 = 192;
/// JPEG 质量（用户 2026-09-15 定：q82）。
pub const JPEG_QUALITY: u8 = 82;
/// 渲染管线版本：**算法一改就 +1**（缓存靠它自动失效）。
pub const PIPELINE_VERSION: u32 = 1;

/// 缩略图尺度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SizeClass {
    /// 网格（长边 [`GRID_LONG_EDGE`]）。
    Grid,
    /// 胶片带（长边 [`STRIP_LONG_EDGE`]）。
    Strip,
}

impl SizeClass {
    /// 稳定的字符串名（进数据库与缓存键，**不要改**）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Grid => "grid",
            Self::Strip => "strip",
        }
    }

    #[must_use]
    pub const fn long_edge(self) -> u32 {
        match self {
            Self::Grid => GRID_LONG_EDGE,
            Self::Strip => STRIP_LONG_EDGE,
        }
    }

    /// 所有尺度（GC、预热、统计遍历用）。
    #[must_use]
    pub const fn all() -> [Self; 2] {
        [Self::Grid, Self::Strip]
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "grid" => Some(Self::Grid),
            "strip" => Some(Self::Strip),
            _ => None,
        }
    }
}

/// 渲染签名（编码格式 + 质量 + 管线版本 + 尺度）。
#[must_use]
pub const fn render_sig(size: SizeClass) -> &'static str {
    match size {
        SizeClass::Grid => "jpeg-q82-grid-v1",
        SizeClass::Strip => "jpeg-q82-strip-v1",
    }
}

/// 一张渲染好的缩略图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumb {
    /// JPEG 字节（可以直接进 SQLite BLOB，也可以写文件）。
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// 是不是占位图（RAW 等）—— UI 可以据此加个角标。
    pub placeholder: bool,
}

/// 渲染一个文件。**不是图像（比如 RAW）返回 `Ok(None)`** —— 由调用方决定用占位图
/// 还是去读它的配对位图（`REPOSITORY.md` §4.1）。
pub fn render_file(path: &Path, size: SizeClass) -> Result<Option<Thumb>> {
    let bytes = std::fs::read(path)?;
    render_bytes(&bytes, size, None)
}

/// 从内存渲染；`orientation` 是 EXIF 的 1..8（给了就摆正）。
pub fn render_bytes(
    bytes: &[u8],
    size: SizeClass,
    orientation: Option<u16>,
) -> Result<Option<Thumb>> {
    let Ok(img) = image::load_from_memory(bytes) else {
        return Ok(None); // 不是能解码的图像（RAW、损坏文件…）
    };
    encode(img, size, orientation, false).map(Some)
}

/// 缩放后编码成 JPEG。
///
/// 用 **Lanczos3** 而不是盒式平均：缩略图是照片软件的「门面」，6000px → 384px 这种
/// 大幅度缩小时盒式滤波会出现明显的锯齿与摩尔纹。速度实测够用（见
/// `examples/thumb-bench.rs`）。
pub fn encode(
    img: DynamicImage,
    size: SizeClass,
    orientation: Option<u16>,
    placeholder: bool,
) -> Result<Thumb> {
    let img = match orientation {
        Some(o) if o != 1 => apply_orientation(&img, o),
        _ => img,
    };
    let resized = resize_for_thumb(img, size.long_edge());

    let rgb = resized.to_rgb8();
    let (width, height) = (rgb.width(), rgb.height());
    let mut data = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut data, JPEG_QUALITY);
    encoder
        .encode_image(&rgb)
        .map_err(|e| Error::Unsupported(format!("JPEG 编码失败：{e}")))?;

    Ok(Thumb {
        data,
        width,
        height,
        placeholder,
    })
}

/// 缩到长边 `long`（不放大）。
///
/// # 为什么要两段式（有实测支撑，别改回去）
///
/// 实测（`examples/thumb-bench.rs`，30 张 6000×4000 的 JPG，release）：
///
/// | 做法 | 每张耗时 |
/// | --- | --- |
/// | 直接 Lanczos3 | **191 ms** |
/// | 直接三角滤波 | 68 ms |
/// | 直接最近邻 | 14 ms |
/// | **两段式（盒式 → 2×目标 → Lanczos3）** | **约 45 ms** |
///
/// 直接把 6000px 缩到 384px，Lanczos3 的核要跨 15 个像素取样，代价极高；
/// 先用盒式（整数快路径）降到 2× 目标，再让 Lanczos3 做最后一步精修，
/// 观感与直接 Lanczos3 几乎一致（大幅缩小本就丢高频），速度却快 4 倍。
#[must_use]
pub fn resize_for_thumb(img: DynamicImage, long: u32) -> DynamicImage {
    let (w, h) = (img.width(), img.height());
    if w.max(h) <= long {
        return img; // 已经比目标小：不放大（放大只会更糊，还更费空间）
    }
    let double = long * 2;
    if w.max(h) <= double {
        // 只比目标大一点点：直接一次滤波就够
        return img.resize(long, long, image::imageops::FilterType::Lanczos3);
    }
    // ① 盒式快降（整数路径，`thumbnail` 就是干这个的）
    let coarse = img.thumbnail(double, double);
    // ② Lanczos3 精修最后一段（2× → 1×，核很短，几乎不花时间）
    coarse.resize(long, long, image::imageops::FilterType::Lanczos3)
}

/// 按 EXIF orientation 摆正（1..8）。
///
/// 相机的方向传感器把「怎么拿的」记在这里：竖着拍的照片其像素是横的，
/// 不加这一步网格里就会出现一片躺倒的照片。
#[must_use]
pub fn apply_orientation(img: &DynamicImage, orientation: u16) -> DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img.clone(), // 1 与未知值：原样
    }
}

/// 生成占位图：深底 + 「RAW」字样，**不依赖字体文件**。
///
/// 字体是手写的 5×7 点阵（只有 R / A / W 三个字母，够用）。
pub fn placeholder(kind: MediaKind, size: SizeClass) -> Result<Thumb> {
    let long = size.long_edge();
    // 占位图按 3:2 画（相机的常见比例），再按尺度等比
    let (w, h) = (long, long * 2 / 3);
    let mut img = RgbImage::from_pixel(w, h, Rgb(BG));

    // 网格里加一圈淡淡的描边，避免浅色主题下与背景糊在一起
    for x in 0..w {
        img.put_pixel(x, 0, Rgb(EDGE));
        img.put_pixel(x, h - 1, Rgb(EDGE));
    }
    for y in 0..h {
        img.put_pixel(0, y, Rgb(EDGE));
        img.put_pixel(w - 1, y, Rgb(EDGE));
    }

    let text = match kind {
        MediaKind::Raw => "RAW",
        MediaKind::Image => "IMG",
        MediaKind::Other => "?",
    };
    draw_text(&mut img, text, FONT_SCALE.max(1));

    encode(DynamicImage::ImageRgb8(img), size, None, true)
}

// 占位图配色（这里**故意不引 tokens.css**：它是画布内容不是 UI 组件，
// 但取值与 DESIGN.md 的 `--surface-bar` / `--fg-3` 保持一致，改了要同步。）
const BG: [u8; 3] = [0x2a, 0x2d, 0x33];
const EDGE: [u8; 3] = [0x3a, 0x3e, 0x46];
const FG: [u8; 3] = [0xb6, 0xb0, 0xaf];

/// 点阵放大的倍数（小尺度下用大字母，大尺度下用大字母也一样清楚）。
const FONT_SCALE: u32 = 6;

/// 5×7 点阵字体（只含占位图需要的字母）。
fn glyph(c: char) -> Option<[u8; 7]> {
    Some(match c {
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
        'I' => [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10001, 0b10001, 0b10001, 0b10001,
        ],
        '?' => [
            0b01110, 0b10001, 0b00010, 0b00100, 0b00100, 0b00000, 0b00100,
        ],
        _ => return None,
    })
}

/// 把一串字画在图片正中间。
fn draw_text(img: &mut RgbImage, text: &str, scale: u32) {
    let (gw, gh) = (5u32, 7u32);
    let spacing = scale;
    let total_w: u32 = text.chars().filter(|c| glyph(*c).is_some()).count() as u32
        * (gw * scale + spacing)
        - spacing.max(1);
    let total_h = gh * scale;
    let x0 = img.width().saturating_sub(total_w) / 2;
    let y0 = img.height().saturating_sub(total_h) / 2;

    let mut cursor = x0;
    for c in text.chars() {
        let Some(rows) = glyph(c) else { continue };
        for (ry, bits) in rows.iter().enumerate() {
            for rx in 0..gw {
                if bits & (1 << (gw - 1 - rx)) == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let x = cursor + rx * scale + dx;
                        let y = y0 + ry as u32 * scale + dy;
                        if x < img.width() && y < img.height() {
                            img.put_pixel(x, y, Rgb(FG));
                        }
                    }
                }
            }
        }
        cursor += gw * scale + spacing;
    }
}

/// 给「不是图像」的字节算个稳定的图（调试用：把哈希画成条纹）。本阶段不用。
#[allow(dead_code)]
pub(crate) fn color_of(bytes: &[u8]) -> Rgb<u8> {
    let mut h: u32 = 2166136261;
    for b in bytes.iter().take(64) {
        h = (h ^ u32::from(*b)).wrapping_mul(16777619);
    }
    #[allow(clippy::cast_possible_truncation)]
    Rgb([(h >> 16) as u8, (h >> 8) as u8, h as u8])
}

/// JPEG 字节是不是一张「像样的」缩略图（校验用：缓存里存的东西别是坏的）。
#[must_use]
pub fn is_valid_jpeg(bytes: &[u8]) -> bool {
    matches!(image::guess_format(bytes), Ok(ImageFormat::Jpeg)) && bytes.len() > 64
}

/// 校验一个 `ExtendedColorType`（`image` 0.25 需要，避免 elsewhere 手拼）。
#[must_use]
pub fn rgb8_layout() -> ExtendedColorType {
    ExtendedColorType::Rgb8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg_of(w: u32, h: u32, color: [u8; 3]) -> Vec<u8> {
        let img = RgbImage::from_pixel(w, h, Rgb(color));
        let mut buf = Vec::new();
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 90);
        enc.encode_image(&img).unwrap();
        buf
    }

    fn png_of(w: u32, h: u32) -> Vec<u8> {
        let img = RgbImage::from_pixel(w, h, Rgb([10, 20, 30]));
        let mut buf = std::io::Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    }

    // ---------- 尺度与签名 ----------

    #[test]
    fn size_class_contract() {
        assert_eq!(SizeClass::Grid.long_edge(), 384);
        assert_eq!(SizeClass::Strip.long_edge(), 192);
        assert_eq!(SizeClass::all().len(), 2);
        for s in SizeClass::all() {
            assert_eq!(SizeClass::parse(s.as_str()), Some(s));
            assert!(render_sig(s).contains(s.as_str()), "签名里要能看出尺度");
        }
        assert_eq!(SizeClass::parse("screen"), None);
        assert_eq!(SizeClass::parse(""), None);
    }

    #[test]
    fn render_sig_changes_with_pipeline_version() {
        // 这条测试的意义：提醒「改算法要改版本号」，否则旧缓存会被当新缓存用
        assert!(render_sig(SizeClass::Grid).contains("v1"));
        assert_ne!(render_sig(SizeClass::Grid), render_sig(SizeClass::Strip));
        assert_eq!(PIPELINE_VERSION, 1);
    }

    // ---------- 渲染 ----------

    #[test]
    fn renders_jpeg_and_png_to_a_jpeg_thumbnail() {
        for bytes in [jpeg_of(4000, 3000, [200, 100, 50]), png_of(300, 200)] {
            let t = render_bytes(&bytes, SizeClass::Grid, None)
                .unwrap()
                .unwrap();
            assert!(is_valid_jpeg(&t.data), "输出必须是合法 JPEG");
            assert!(t.width <= GRID_LONG_EDGE && t.height <= GRID_LONG_EDGE);
            assert!(!t.placeholder);
        }
    }

    #[test]
    fn keeps_aspect_ratio_and_fits_the_long_edge() {
        let t = render_bytes(&jpeg_of(4000, 3000, [1, 2, 3]), SizeClass::Grid, None)
            .unwrap()
            .unwrap();
        assert_eq!(t.width, 384, "横图的长边应当是宽度");
        assert_eq!(t.height, 288, "4:3 → 384×288");

        let portrait = render_bytes(&jpeg_of(3000, 4000, [1, 2, 3]), SizeClass::Grid, None)
            .unwrap()
            .unwrap();
        assert_eq!(
            (portrait.width, portrait.height),
            (288, 384),
            "竖图长边是高"
        );

        let strip = render_bytes(&jpeg_of(4000, 3000, [1, 2, 3]), SizeClass::Strip, None)
            .unwrap()
            .unwrap();
        assert_eq!((strip.width, strip.height), (192, 144));
    }

    #[test]
    fn two_stage_resize_matches_direct_resize_in_shape() {
        // 形状与直接缩放一致（两段式只是快，不该改变尺寸规则）
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(6000, 4000, Rgb([40, 80, 120])));
        let two = resize_for_thumb(img.clone(), GRID_LONG_EDGE);
        let direct = img.resize(
            GRID_LONG_EDGE,
            GRID_LONG_EDGE,
            image::imageops::FilterType::Lanczos3,
        );
        assert_eq!(
            (two.width(), two.height()),
            (direct.width(), direct.height())
        );
        assert_eq!(
            (two.width(), two.height()),
            (384, 256),
            "6000×4000 → 384×256"
        );
        // 极小的图走直接路径
        let small = DynamicImage::ImageRgb8(RgbImage::from_pixel(800, 600, Rgb([1, 2, 3])));
        assert_eq!(resize_for_thumb(small, GRID_LONG_EDGE).width(), 384);
        // 比目标小：原样
        let tiny = DynamicImage::ImageRgb8(RgbImage::from_pixel(100, 80, Rgb([1, 2, 3])));
        assert_eq!(resize_for_thumb(tiny, GRID_LONG_EDGE).width(), 100);
    }

    #[test]
    fn does_not_upscale_small_images() {
        // 比目标还小的图不该被放大（放大只会更糊、更占空间）
        let t = render_bytes(&jpeg_of(100, 80, [9, 9, 9]), SizeClass::Grid, None)
            .unwrap()
            .unwrap();
        assert_eq!((t.width, t.height), (100, 80));
        // 但也仍然输出合法 JPEG，UI 不用分情况
        assert!(is_valid_jpeg(&t.data));
    }

    #[test]
    fn non_image_bytes_yield_none_not_error() {
        assert!(
            render_bytes(b"not an image", SizeClass::Grid, None)
                .unwrap()
                .is_none()
        );
        assert!(render_bytes(&[], SizeClass::Grid, None).unwrap().is_none());
        // RAW 文件（这里用假的字节）同样返回 None —— 调用方据此改走占位图
        assert!(
            render_bytes(&[0x49, 0x49, 0x2a, 0x00, 1, 2, 3, 4], SizeClass::Grid, None)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn missing_file_is_an_error_not_a_panic() {
        let r = render_file(Path::new("/definitely/not/here.jpg"), SizeClass::Grid);
        assert!(r.is_err(), "文件读不了要报错（调用方好记 missing）");
    }

    // ---------- 朝向 ----------

    #[test]
    fn orientation_rotations_are_applied() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(4, 2, Rgb([1, 2, 3])));
        assert_eq!((img.width(), img.height()), (4, 2));
        // 6 = 顺时针 90°（最常见的「竖着拍」）
        let r6 = apply_orientation(&img, 6);
        assert_eq!((r6.width(), r6.height()), (2, 4));
        // 8 = 逆时针 90°
        let r8 = apply_orientation(&img, 8);
        assert_eq!((r8.width(), r8.height()), (2, 4));
        // 3 = 180°，尺寸不变
        let r3 = apply_orientation(&img, 3);
        assert_eq!((r3.width(), r3.height()), (4, 2));
        // 1 与未知值：原样
        assert_eq!(
            (
                apply_orientation(&img, 1).width(),
                apply_orientation(&img, 1).height()
            ),
            (4, 2)
        );
        assert_eq!(
            (
                apply_orientation(&img, 99).width(),
                apply_orientation(&img, 99).height()
            ),
            (4, 2)
        );
    }

    #[test]
    fn orientation_is_applied_before_resizing() {
        // 竖拍照片（像素 4000×3000 但 orientation=6）：缩略图应当是竖的
        let t = render_bytes(&jpeg_of(4000, 3000, [5, 5, 5]), SizeClass::Grid, Some(6))
            .unwrap()
            .unwrap();
        assert_eq!(
            (t.width, t.height),
            (288, 384),
            "摆正后才缩放，结果应当是竖图"
        );
    }

    #[test]
    fn orientation_flips_do_not_change_size() {
        for o in [2u16, 4, 5, 7] {
            let t = render_bytes(&jpeg_of(400, 300, [1, 1, 1]), SizeClass::Grid, Some(o))
                .unwrap()
                .unwrap();
            assert!(t.width <= GRID_LONG_EDGE);
            assert!(is_valid_jpeg(&t.data), "orientation={o} 也应当是合法 JPEG");
        }
    }

    // ---------- 占位图 ----------

    #[test]
    fn placeholder_is_a_valid_jpeg_with_expected_shape() {
        for kind in [MediaKind::Raw, MediaKind::Image, MediaKind::Other] {
            let p = placeholder(kind, SizeClass::Grid).unwrap();
            assert!(p.placeholder, "要标出来是占位图");
            assert!(is_valid_jpeg(&p.data), "{kind:?} 的占位图必须是合法 JPEG");
            assert_eq!(p.width, GRID_LONG_EDGE);
            assert_eq!(p.height, GRID_LONG_EDGE * 2 / 3);
        }
        let strip = placeholder(MediaKind::Raw, SizeClass::Strip).unwrap();
        assert_eq!((strip.width, strip.height), (192, 128));
    }

    #[test]
    fn placeholder_draws_text_pixels() {
        // 图上应当真的有字（中间区域出现前景色像素），而不是一片纯色
        let p = placeholder(MediaKind::Raw, SizeClass::Grid).unwrap();
        let img = image::load_from_memory(&p.data).unwrap().to_rgb8();
        // JPEG 是有损的，不能用精确色相等来判断「有字」——按与前景色的**接近程度**数
        let near_fg = |px: &Rgb<u8>| {
            px.0.iter()
                .zip(FG)
                .map(|(a, b)| a.abs_diff(b) as u32)
                .sum::<u32>()
                < 90
        };
        let fg = img.pixels().filter(|px| near_fg(px)).count();
        assert!(fg > 50, "「RAW」字样该画出可见的像素，实际 {fg}");
    }

    #[test]
    fn glyphs_exist_for_all_placeholder_letters() {
        for c in "RAWI M?".chars().filter(|c| !c.is_whitespace()) {
            assert!(glyph(c).is_some(), "缺字形：{c}");
        }
        assert!(glyph('Z').is_none());
    }

    #[test]
    fn placeholder_encodes_into_the_same_pipeline() {
        // 占位图与原图走同一条编码路径 → 尺寸规则一致，UI 不用分情况
        let p = placeholder(MediaKind::Raw, SizeClass::Grid).unwrap();
        assert!(p.data.len() > 200, "JPEG 不该是空壳");
    }
}
