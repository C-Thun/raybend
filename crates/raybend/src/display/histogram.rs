//! 亮度/通道**直方图**（看图态右栏那条 24 柱的图）。
//!
//! # 为什么在 Rust 侧
//!
//! `AGENTS.md` §6.1 的红线：**前端不碰像素**。直方图是从像素统计出来的东西，
//! 所以它和缩放、色彩一样属于 Rust；前端只拿到 24 个整数去画柱子。
//!
//! # 从哪张图统计
//!
//! 走**统一取图口**（[`super::display_image`]）拿 `grid` 档（长边 384）的字节再统计：
//!
//! * 比全尺寸解码快一两个数量级（看图时每换一张都要用）；
//! * 24 个桶对 384px（≈15 万像素）的样本足够稳 —— 直方图看的是分布形状，不是精确计数；
//! * 缩略图已经过 EXIF 方向与 3:1 夹取，统计的是**用户实际看到的那张图**。
//!
//! # 口径
//!
//! * **RGB 三通道叠加**（人类 2026-09-18 定：本期先做合成直方图，通道分离进 `FUTURE.md`）；
//! * 桶边界按 0..=255 均分（24 桶 ⇒ 每桶 ≈10.67 个色阶），最后一个桶含 255；
//! * 统计时**忽略 alpha**（我们显示的都是不透明照片，半透明占位图没有统计意义）；
//! * 返回每个桶的计数与**最大值**（前端按它归一化柱高；让 Rust 给最大值，
//!   免得前端自己再扫一遍数组）。

use std::path::Path;

use image::RgbImage;

use crate::error::Result;

use super::{display_image, ImagePurpose, ImageRequest};

/// 默认柱数（画布上画的就是 24 根）。
pub const DEFAULT_BINS: usize = 24;

/// 一条直方图：三个通道各自的计数。
///
/// 三个数组长度都等于 `bins`；`max` = 三个通道里最大的那个计数（前端归一化用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Histogram {
    pub bins: usize,
    pub r: Vec<u32>,
    pub g: Vec<u32>,
    pub b: Vec<u32>,
    pub max: u32,
}

impl Histogram {
    /// 空直方图（认不出/解不开时为它 —— 界面画一条平线，而不是报错）。
    #[must_use]
    pub fn empty(bins: usize) -> Self {
        Self {
            bins,
            r: vec![0; bins],
            g: vec![0; bins],
            b: vec![0; bins],
            max: 0,
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
    let mut out = Histogram::empty(bins);
    // 色阶 → 桶：`level * bins / 256`，于是 255 落在最后一桶（写色阶 255 不许越界）
    let bucket = |level: u8| -> usize { (usize::from(level) * bins) / 256 };
    for pixel in img.pixels() {
        out.r[bucket(pixel[0])] += 1;
        out.g[bucket(pixel[1])] += 1;
        out.b[bucket(pixel[2])] += 1;
    }
    out.max = out
        .r
        .iter()
        .chain(out.g.iter())
        .chain(out.b.iter())
        .copied()
        .max()
        .unwrap_or(0);
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
        let h = histogram_of_image(&img(&[[0, 0, 0], [255, 255, 255], [128, 128, 128]]), 24);
        assert_eq!(h.bins, 24);
        assert_eq!(h.r[0], 1, "0 落在第一桶");
        assert_eq!(h.r[23], 1, "255 落在最后一桶");
        assert_eq!(h.r[12], 1, "128 落在正中间（128*24/256 = 12）");
        assert_eq!(h.g[0], 1);
        assert_eq!(h.b[23], 1);
        assert_eq!(h.max, 1);
    }

    #[test]
    fn channels_are_counted_separately() {
        let h = histogram_of_image(&img(&[[255, 0, 0], [255, 0, 0], [0, 255, 0]]), 4);
        assert_eq!(h.r[3], 2, "两张红：R 在最亮桶");
        assert_eq!(h.g[3], 1, "一张绿：G 在最亮桶");
        assert_eq!(h.r[0], 1, "绿的 R 是 0");
        // 最大值来自 B 通道的 0 桶：三张图的蓝都是 0 ⇒ 计数 3（测试期望值写错过一次，
        // 这行留着提醒：max 是**三通道合并**后的峰值，不是某个通道的）
        assert_eq!(h.b[0], 3);
        assert_eq!(h.max, 3);
    }

    #[test]
    fn every_level_up_to_255_stays_inside_bounds() {
        // 边界：0 与 255 是最容易越界的两个值（桶 = level * bins / 256）
        let h = histogram_of_image(&img(&[[0, 255, 0], [255, 0, 255]]), 7);
        assert_eq!(h.r.len(), 7);
        assert_eq!(h.r[0], 1);
        assert_eq!(h.r[6], 1);
        assert_eq!(h.b[0], 1);
        assert_eq!(h.b[6], 1);
    }

    #[test]
    fn empty_input_and_zero_bins_do_not_panic() {
        let empty = RgbImage::new(0, 0);
        let h = histogram_of_image(&empty, DEFAULT_BINS);
        assert_eq!(h.max, 0);
        assert_eq!(h.r.iter().sum::<u32>(), 0);

        let zero = histogram_of_image(&img(&[[1, 2, 3]]), 0);
        assert_eq!(zero.bins, 0);
        assert!(zero.r.is_empty());
        assert_eq!(zero.max, 0);
    }

    #[test]
    fn a_single_bin_swallows_everything() {
        let h = histogram_of_image(&img(&[[0, 0, 0], [255, 255, 255], [77, 200, 13]]), 1);
        assert_eq!(h.r, vec![3]);
        assert_eq!(h.g, vec![3]);
        assert_eq!(h.b, vec![3]);
        assert_eq!(h.max, 3);
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
