//! 亮度/通道**直方图**（看图态右栏那条曲线）。
//!
//! # 为什么在 Rust 侧
//!
//! `AGENTS.md` §6.1 的红线：**前端不碰像素**。直方图是从像素统计出来的东西，
//! 所以它和缩放、色彩一样属于 Rust；前端只拿到 86 个浮点采样去画曲线。
//!
//! # 从哪张图统计
//!
//! 走**像素口**（[`super::pixels::pixels`]）拿**屏幕档**（长边 1920）的 RGB 像素直接统计 ——
//! **不经过编码**：
//!
//! * 统计只需要像素。以前绕道 [`super::display_image`] 拿字节再 `image::load_from_memory` 解回来，
//!   多付一次「编码 + 解码」，还踩过 AVIF 回归（2026-09-24：`image` 的 avif feature 只有编码器
//!   没有解码器 → 我们自己渲染出来的 AVIF 字节读不回来 → 直方图恒为空）。
//!   现在直接用管线吐出的 RGB 字节（`DisplayPixels`），一次拷贝都不用；
//! * 比全尺寸解码快一两个数量级（看图时每换一张都要用）；
//! * 先逐色阶统计 256 桶，再按「0 单独；1..255 每 3 级取平均」压成 86 个采样点；
//! * 像素口已经过 EXIF 方向，统计的是**用户实际看到的那张图**。
//!
//! # 口径：只认「SDR、8 位、sRGB」的像素（第一阶段）
//!
//! 直方图**不按文件格式分叉**：JPEG / PNG / WebP / AVIF / RAW 走的是同一个像素口，
//! 拿到的都是 RGB8。格式的特殊性（HDR、gain map、ICC、10/16 位）属于**解码与色彩管理层**，
//! 不属于统计层 —— 将来接 HDR/色彩管理时，改的是像素口的契约与实现（上游统一变换成
//! 显示空间），直方图这一层不需要第二条路径。
//!
//! # 实时刷新也是同一条路（W4 的「编辑后直方图」）
//!
//! 调整参数时直方图要跟着变 —— 那条路**不许另起一套统计**：
//! [`histogram_of_rgb8`] 是唯一的计数实现，它只看像素、不管像素从哪来。
//! 实时那条路的像素由显影线程**当前这一帧**直接给出（RGB8 已在内存里：不落盘、不解码、不编码），
//! 文件那条路（初次载入）走像素口 —— 两条只是**取像素的来源不同**，计数器是同一个。
//!
//! # 口径
//!
//! * RGB 三通道各自统计；前端可叠加或把一个通道提到前景；
//! * 0 单独保留；1..=255 每三个色阶的计数取平均，共 86 点；
//! * 统计时**忽略 alpha**（我们显示的都是不透明照片，半透明占位图没有统计意义）；
//! * 返回每个桶的计数与**最大值**（前端按它归一化柱高；让 Rust 给最大值，
//!   免得前端自己再扫一遍数组）。

use std::path::Path;

use image::RgbImage;

use crate::error::Result;

use super::pixels::{pixels, PixelSize};

/// 0 单独 + 1..255 每 3 级一组，共 86 个采样点。
pub const DEFAULT_BINS: usize = 86;

/// 一条直方图：三个通道各自的平均计数。
///
/// 三个数组长度都等于 `bins`；`max` = 三个通道里最大的那个计数（前端归一化用）。
#[derive(Debug, Clone, PartialEq)]
pub struct Histogram {
    pub bins: usize,
    pub r: Vec<f64>,
    pub g: Vec<f64>,
    pub b: Vec<f64>,
    pub max: f64,
}

impl Histogram {
    /// 空直方图（认不出/解不开时为它 —— 界面画一条平线，而不是报错）。
    #[must_use]
    pub fn empty(bins: usize) -> Self {
        Self {
            bins,
            r: vec![0.0; bins],
            g: vec![0.0; bins],
            b: vec![0.0; bins],
            max: 0.0,
        }
    }
}

/// 从一张 RGB 图统计（纯函数，好测）。
///
/// `bins == 0` 时返回空直方图（调用方不该这么传，但也不该崩）。
#[must_use]
pub fn histogram_of_image(img: &RgbImage, bins: usize) -> Histogram {
    histogram_of_rgb8(img.as_raw(), bins)
}

/// 从**紧密排列的 RGB8 字节**统计（[`DisplayPixels`] 的口径 —— 直方图的常规入口）。
///
/// 与 [`histogram_of_image`] 同一条实现，只是不要求调用方先造一个 `RgbImage`。
/// 尾部不足 3 字节的残余直接忽略（正常输入不会有）。
///
/// [`DisplayPixels`]: super::DisplayPixels
#[must_use]
pub fn histogram_of_rgb8(rgb: &[u8], bins: usize) -> Histogram {
    if bins == 0 {
        return Histogram::empty(0);
    }
    let mut raw = [[0_u32; 256]; 3];
    for pixel in rgb.as_chunks::<3>().0 {
        raw[0][usize::from(pixel[0])] += 1;
        raw[1][usize::from(pixel[1])] += 1;
        raw[2][usize::from(pixel[2])] += 1;
    }
    let collapse = |channel: &[u32; 256]| -> Vec<f64> {
        let mut out = Vec::with_capacity(DEFAULT_BINS);
        out.push(f64::from(channel[0]));
        for start in (1..=253).step_by(3) {
            let sum = channel[start] + channel[start + 1] + channel[start + 2];
            // 必须除 3：否则每个普通采样覆盖 3 个色阶，0 桶只覆盖 1 个色阶，
            // 黑位会被系统性压低到三分之一。
            out.push(f64::from(sum) / 3.0);
        }
        out
    };
    let mut out = Histogram {
        bins: DEFAULT_BINS,
        r: collapse(&raw[0]),
        g: collapse(&raw[1]),
        b: collapse(&raw[2]),
        max: 0.0,
    };
    out.max = out
        .r
        .iter()
        .chain(out.g.iter())
        .chain(out.b.iter())
        .copied()
        .fold(0.0_f64, f64::max);
    out
}

/// 从文件统计（走像素口 → `Screen` 档 RGB 像素 → 统计，**不经过编码**）。
///
/// 认不出类型 / 解不开 → `Ok(None)`（界面画空直方图），IO 出错 → `Err`。
pub fn histogram_of_file(path: &Path, bins: usize) -> Result<Option<Histogram>> {
    let Some(image) = pixels(path, PixelSize::Screen)? else {
        return Ok(None);
    };
    Ok(Some(histogram_of_rgb8(&image.rgb, bins)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(pixels: &[[u8; 3]]) -> RgbImage {
        RgbImage::from_fn(pixels.len() as u32, 1, |x, _| {
            image::Rgb(pixels[x as usize])
        })
    }

    #[test]
    fn black_white_and_midtones_land_in_the_right_buckets() {
        let h = histogram_of_image(
            &img(&[[0, 0, 0], [255, 255, 255], [128, 128, 128]]),
            DEFAULT_BINS,
        );
        assert_eq!(h.bins, DEFAULT_BINS);
        assert_eq!(h.r[0], 1.0, "0 单独落在第一点");
        assert_eq!(h.r[85], 1.0 / 3.0, "255 在 253..255 的平均点");
        assert_eq!(h.r[43], 1.0 / 3.0, "128 在 127..129 的平均点");
        assert_eq!(h.g[0], 1.0);
        assert_eq!(h.b[85], 1.0 / 3.0);
        assert_eq!(h.max, 1.0);
    }

    #[test]
    fn channels_are_counted_separately() {
        let h = histogram_of_image(&img(&[[255, 0, 0], [255, 0, 0], [0, 255, 0]]), DEFAULT_BINS);
        assert_eq!(h.r[85], 2.0 / 3.0, "两张红：R 在最亮平均点");
        assert_eq!(h.g[85], 1.0 / 3.0, "一张绿：G 在最亮平均点");
        assert_eq!(h.r[0], 1.0, "绿的 R 是 0");
        // 最大值来自 B 通道的 0 桶：三张图的蓝都是 0 ⇒ 计数 3（测试期望值写错过一次，
        // 这行留着提醒：max 是**三通道合并**后的峰值，不是某个通道的）
        assert_eq!(h.b[0], 3.0);
        assert_eq!(h.max, 3.0);
    }

    #[test]
    fn every_level_up_to_255_stays_inside_bounds() {
        let h = histogram_of_image(&img(&[[0, 255, 0], [255, 0, 255]]), DEFAULT_BINS);
        assert_eq!(h.r.len(), DEFAULT_BINS);
        assert_eq!(h.r[0], 1.0);
        assert_eq!(h.r[85], 1.0 / 3.0);
        assert_eq!(h.b[0], 1.0);
        assert_eq!(h.b[85], 1.0 / 3.0);
    }

    #[test]
    fn empty_input_and_zero_bins_do_not_panic() {
        let empty = RgbImage::new(0, 0);
        let h = histogram_of_image(&empty, DEFAULT_BINS);
        assert_eq!(h.max, 0.0);
        assert_eq!(h.r.iter().sum::<f64>(), 0.0);

        let zero = histogram_of_image(&img(&[[1, 2, 3]]), 0);
        assert_eq!(zero.bins, 0);
        assert!(zero.r.is_empty());
        assert_eq!(zero.max, 0.0);
    }

    #[test]
    fn nonzero_requested_bins_always_use_the_stable_86_point_contract() {
        let h = histogram_of_image(&img(&[[0, 0, 0], [255, 255, 255]]), 1);
        assert_eq!(h.bins, DEFAULT_BINS);
        assert_eq!(h.r.len(), DEFAULT_BINS);
    }

    #[test]
    fn averaging_three_levels_keeps_zero_comparable() {
        let h = histogram_of_image(
            &img(&[[0, 0, 0], [1, 1, 1], [2, 2, 2], [3, 3, 3]]),
            DEFAULT_BINS,
        );
        assert_eq!(h.r[0], 1.0, "一个 0 像素占比为 1");
        assert_eq!(h.r[1], 1.0, "1/2/3 三个计数取平均后也为 1");
        assert_eq!(h.max, 1.0);
    }

    #[test]
    fn rgb8_bytes_and_rgb_image_agree() {
        let img = img(&[[0, 0, 0], [255, 128, 3], [7, 200, 64]]);
        assert_eq!(
            histogram_of_rgb8(img.as_raw(), DEFAULT_BINS),
            histogram_of_image(&img, DEFAULT_BINS),
            "同一份统计，两种入口必须完全一致"
        );
        // 尾部残余（不足 3 字节）忽略，不 panic
        assert_eq!(
            histogram_of_rgb8(&[1, 2, 3, 4], DEFAULT_BINS),
            histogram_of_rgb8(&[1, 2, 3], DEFAULT_BINS)
        );
        assert_eq!(histogram_of_rgb8(&[], DEFAULT_BINS).max, 0.0);
        assert_eq!(histogram_of_rgb8(&[1, 2, 3], 0).bins, 0);
    }

    #[test]
    fn a_real_bitmap_file_yields_a_histogram() {
        // 这条是 2026-09-24 AVIF 回归的回归测试：旧实现拿渲染管线吐出的 AVIF 字节
        // 再用 `image::load_from_memory` 解 —— avif 只有编码器没有解码器 ⇒ 恒为 None。
        // 走像素口之后，任何能解码的位图都必须统计得出来。
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("photo.png");
        let img = image::RgbImage::from_fn(4, 1, |x, _| match x {
            0 => image::Rgb([0, 0, 0]),
            1 => image::Rgb([255, 255, 255]),
            _ => image::Rgb([128, 128, 128]),
        });
        img.save(&path).expect("写 PNG");
        let h = histogram_of_file(&path, DEFAULT_BINS)
            .expect("不该报错")
            .expect("能解码的位图必须给出直方图");
        assert_eq!(h.r[0], 1.0, "一个 0");
        assert_eq!(h.r[85], 1.0 / 3.0, "一个 255");
        assert_eq!(h.r[43], 2.0 / 3.0, "两个 128");
        assert_eq!(h.max, 1.0, "峰值是 0 桶（一个纯黑像素）");
    }

    #[test]
    fn a_missing_file_is_not_an_error() {
        let dir = tempfile::tempdir().expect("临时目录");
        let missing = dir.path().join("没有这张.jpg");
        // 位图走「原图」那条路会 IO 失败 → Err；RAW 走渲染 → None。两者都不许 panic。
        let outcome = histogram_of_file(&missing, DEFAULT_BINS);
        assert!(matches!(outcome, Ok(None) | Err(_)), "缺文件只能是「空」或「错」");
    }
}
