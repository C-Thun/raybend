//! 亮度/通道**直方图**（看图态右栏那条曲线）。
//!
//! # 为什么在 Rust 侧
//!
//! `AGENTS.md` §6.1 的红线：**前端不碰像素**。直方图是从像素统计出来的东西，
//! 所以它和缩放、色彩一样属于 Rust；前端只拿到 86 个浮点采样去画曲线。
//!
//! # 从哪张图统计
//!
//! 走**统一取图口**（[`super::display_image`]）拿 `grid` 档（长边 384）的字节再统计：
//!
//! * 比全尺寸解码快一两个数量级（看图时每换一张都要用）；
//! * 先逐色阶统计 256 桶，再按「0 单独；1..255 每 3 级取平均」压成 86 个采样点；
//! * 缩略图已经过 EXIF 方向，统计的是**用户实际看到的那张图**。
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

use super::{display_image, ImagePurpose, ImageRequest};

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
    if bins == 0 {
        return Histogram::empty(0);
    }
    let mut raw = [[0_u32; 256]; 3];
    for pixel in img.pixels() {
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

/// 从文件统计（走统一取图口 → `grid` 档字节 → 解码 → 统计）。
///
/// 认不出类型 / 解不开 → `Ok(None)`（界面画空直方图），IO 出错 → `Err`。
pub fn histogram_of_file(path: &Path, bins: usize) -> Result<Option<Histogram>> {
    let request = ImageRequest::plain(path, ImagePurpose::Grid);
    let Some(image) = display_image(&request)? else {
        return Ok(None);
    };
    let Ok(decoded) = image::load_from_memory(&image.bytes) else {
        return Ok(None);
    };
    Ok(Some(histogram_of_image(&decoded.to_rgb8(), bins)))
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
    fn a_missing_file_is_not_an_error() {
        let dir = tempfile::tempdir().expect("临时目录");
        let missing = dir.path().join("没有这张.jpg");
        // 位图走「原图」那条路会 IO 失败 → Err；RAW 走渲染 → None。两者都不许 panic。
        let outcome = histogram_of_file(&missing, DEFAULT_BINS);
        assert!(matches!(outcome, Ok(None) | Err(_)), "缺文件只能是「空」或「错」");
    }
}
