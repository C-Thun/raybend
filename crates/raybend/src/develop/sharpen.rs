//! **锐化（显示域）**：亮度域的 unsharp mask + 软限幅防光晕。
//!
//! # 为什么在显示域、为什么只动亮度
//!
//! * **显示域**：锐化的观感与「相邻像素差多少」成正比，而人眼看到的是显示编码之后的差。
//!   在线性域做同样幅度的高频提升，暗部会被放大得过头（线性域暗部一个台阶在显示域是好几级）。
//!   这也是主流编辑器的做法（Lightroom 的输出锐化、RapidRAW 的 WGSL 锐化都在显示域）。
//! * **只动亮度**：逐通道锐化会在高对比边缘上把三个通道推向不同方向，于是边缘长出彩色描边
//!   （典型是紫边）。做法是：算出亮度的细节量 `d`，把**同一个增量**加到 R/G/B 上 ——
//!   通道差不变 ⇒ 色相与饱和度不变。
//!
//! # 为什么在 8bit 上做（**与计划里那句「量化之前」的偏离，理由在这**）
//!
//! 管线（`pipeline::render_rgb8`）本来就在最后一步量化成 8bit 输出。要「量化前锐化」
//! 就得让整条链改成输出 f32/u16 显示值，多出来的**全尺寸缓冲**在 24MP 上是 144–288 MB
//! （显影线程里已经躺着线性源 144 MB + 输出 72 MB）。
//! 实测代价：8bit 数据算出来的细节信号带约 `0.5/255 ≈ 0.2%` 的量化底噪，
//! 乘上增益后仍低于 8bit 输出本身的量化台阶 —— **看不出来**。
//! 换来的是这一趟只需要一张 48 MB 的 u16 中间图（横向模糊结果）。
//! 真要在 f32 显示域做，改的是 [`sharpen_display`] 的输入类型，不是算法。
//!
//! # 软限幅（复用 `local_tone` 那一份）
//!
//! `added_detail(d, gain, limit)`：`|gain·d| ≤ limit` 时**完全按斜率生效**，
//! 超过才平滑收住。硬增益会把强边缘推成白边/黑边（halo），软限幅是**自限制**的：
//! 细纹理照常放大，大跳变自动收住。这就是 [`super::filters::soft_saturate`] 存在的理由，
//! 两个模块共用一份，不各写一套。

use super::filters::added_detail;

/// 增益上限（`1.0` 拉杆 = 这个值）。1.5 在强边缘上约等于「细节量翻 1.5 倍」。
const GAIN_MAX: f32 = 1.5;
/// 软限幅的拐点（0..255 亮度单位）：光晕被压在约 `±2×16 = ±32` 级以内。
const DETAIL_LIMIT: f32 = 16.0;
/// u16 定点与 0..255 之间的换算（`257 = 65535/255`）。
const FIXED_POINT: f32 = 257.0;

/// 锐化计划：`amount` 0..1（0 = 不动），`radius` 单位是像素。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SharpenPlan {
    pub amount: f32,
    pub radius: f32,
}

impl Default for SharpenPlan {
    fn default() -> Self {
        Self {
            amount: 0.0,
            radius: 1.0,
        }
    }
}

impl SharpenPlan {
    /// 从界面拉杆（0..100）建计划。
    ///
    /// **半径映射**：`1.0 + v/100 × 3.0` px ⇒ 默认 0 就是 **1.0 px**（Lightroom 的默认半径），
    /// 拉到底 4 px。契约里的默认值不动（`只存非默认值` 的语义靠它）。
    #[must_use]
    pub fn from_sliders(amount: f64, radius: f64) -> Self {
        Self {
            amount: (amount / 100.0).clamp(0.0, 1.0) as f32,
            radius: 1.0 + (radius / 100.0).clamp(0.0, 1.0) as f32 * 3.0,
        }
    }

    /// 什么都不用做（强度 0；NaN 也当「没开」）。
    #[must_use]
    pub fn is_identity(&self) -> bool {
        !self.amount.is_finite() || self.amount <= 0.0
    }

    /// 增益（软限幅前的倍数）。
    #[must_use]
    pub fn gain(&self) -> f32 {
        self.amount.clamp(0.0, 1.0) * GAIN_MAX
    }
}

/// **锐化**：8bit 显示域 RGB，**就地**改。
///
/// 两趟：横向箱式模糊亮度（写进 u16 中间图）→ 纵向模糊 + 加回细节。
/// 计划为空、尺寸对不上、像素长度对不上时**原样返回**（不 panic、不改动）。
pub fn sharpen_display(rgb: &mut [u8], width: u32, height: u32, plan: &SharpenPlan) {
    if plan.is_identity() {
        return;
    }
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || rgb.len() != w * h * 3 {
        return;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let radius = plan.radius.round().clamp(1.0, 8.0) as usize;
    let gain = plan.gain();

    // ① 横向箱式模糊（亮度，u16 定点）
    let mut horizontal = vec![0u16; w * h];
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(16);
    let rows_per_chunk = h.div_ceil(threads.max(1));
    {
        // 借一份不可变的像素视图：闭包里用的是 `&[u8]`（Copy），不会把 `rgb` 移进去
        let source: &[u8] = rgb;
        let mut remaining = horizontal.as_mut_slice();
        std::thread::scope(|scope| {
            let mut first_row = 0usize;
            while !remaining.is_empty() {
                let rows = (remaining.len() / w).min(rows_per_chunk).max(1);
                let (chunk, rest) = remaining.split_at_mut(rows * w);
                remaining = rest;
                let start = first_row;
                first_row += rows;
                scope.spawn(move || {
                    for (local, line) in chunk.chunks_mut(w).enumerate() {
                        blur_row_luma(source, line, start + local, w, radius);
                    }
                });
            }
        });
    }

    // ② 纵向模糊 + 加回细节
    {
        let blurred: &[u16] = &horizontal;
        let mut remaining: &mut [u8] = rgb;
        std::thread::scope(|scope| {
            let mut first_row = 0usize;
            while !remaining.is_empty() {
                let rows = (remaining.len() / (w * 3)).min(rows_per_chunk).max(1);
                let take = rows * w * 3;
                let (chunk, rest) = remaining.split_at_mut(take);
                remaining = rest;
                let start = first_row;
                first_row += rows;
                scope.spawn(move || {
                    let mut accumulator = vec![0f32; w];
                    for (local, line) in chunk.as_chunks_mut::<3>().0.chunks_mut(w).enumerate() {
                        let y = start + local;
                        let lo = y.saturating_sub(radius);
                        let hi = (y + radius).min(h - 1);
                        accumulator.fill(0.0);
                        for row in lo..=hi {
                            let source = &blurred[row * w..(row + 1) * w];
                            for (slot, value) in accumulator.iter_mut().zip(source.iter()) {
                                *slot += f32::from(*value);
                            }
                        }
                        #[allow(clippy::cast_precision_loss)]
                        let count = (hi - lo + 1) as f32;
                        for (x, pixel) in line.iter_mut().enumerate() {
                            let original = luma_of_rgb8(pixel);
                            let blur = accumulator[x] / count / FIXED_POINT;
                            let delta = added_detail(original - blur, gain, DETAIL_LIMIT);
                            for channel in pixel.iter_mut() {
                                let value = f32::from(*channel) + delta;
                                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                                {
                                    *channel = (value + 0.5).clamp(0.0, 255.0) as u8;
                                }
                            }
                        }
                    }
                });
            }
        });
    }
}

/// 一行亮度的横向箱式模糊（滑动窗，O(宽)，与半径无关）。
fn blur_row_luma(rgb: &[u8], out: &mut [u16], y: usize, width: usize, radius: usize) {
    let row = &rgb[y * width * 3..(y + 1) * width * 3];
    let luma = |x: usize| luma_of_rgb8(&row[x * 3..x * 3 + 3]);
    let first_hi = radius.min(width - 1);
    let mut sum = 0f32;
    for x in 0..=first_hi {
        sum += luma(x);
    }
    let mut previous_lo = 0usize;
    let mut previous_hi = first_hi;
    for (x, slot) in out.iter_mut().enumerate() {
        let lo = x.saturating_sub(radius);
        let hi = (x + radius).min(width - 1);
        for index in (previous_hi + 1)..=hi {
            sum += luma(index);
        }
        for index in previous_lo..lo {
            sum -= luma(index);
        }
        previous_lo = lo;
        previous_hi = hi;
        #[allow(clippy::cast_precision_loss)]
        let count = (hi - lo + 1) as f32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            *slot = ((sum / count) * FIXED_POINT + 0.5).clamp(0.0, 65535.0) as u16;
        }
    }
}

/// Rec.709 亮度（8bit 显示值，0..255）。
#[inline]
fn luma_of_rgb8(pixel: &[u8]) -> f32 {
    0.2126 * f32::from(pixel[0]) + 0.7152 * f32::from(pixel[1]) + 0.0722 * f32::from(pixel[2])
}

/* ══════════════════════════════════════════════════════════════
 * 单测
 * ══════════════════════════════════════════════════════════════ */

#[cfg(test)]
mod tests {
    use super::*;

    /// 中间一条竖边、两侧平坦的灰图。
    fn edge_image(width: u32, height: u32, left: u8, right: u8) -> Vec<u8> {
        let mut rgb = vec![0u8; (width as usize) * (height as usize) * 3];
        for y in 0..height as usize {
            for x in 0..width as usize {
                let value = if x < (width as usize) / 2 { left } else { right };
                let index = (y * width as usize + x) * 3;
                rgb[index] = value;
                rgb[index + 1] = value;
                rgb[index + 2] = value;
            }
        }
        rgb
    }

    #[test]
    fn zero_amount_is_bit_identical() {
        let mut rgb = edge_image(32, 16, 80, 160);
        let original = rgb.clone();
        sharpen_display(&mut rgb, 32, 16, &SharpenPlan::default());
        assert_eq!(rgb, original);
    }

    #[test]
    fn sliders_map_to_expected_plan() {
        assert_eq!(SharpenPlan::from_sliders(0.0, 0.0), SharpenPlan::default());
        let plan = SharpenPlan::from_sliders(100.0, 100.0);
        assert!((plan.amount - 1.0).abs() < 1e-6);
        assert!((plan.radius - 4.0).abs() < 1e-6, "拉到底是 4 px");
        // 半径 0 = 1.0 px（默认半径），不是「不锐化」
        assert!((SharpenPlan::from_sliders(50.0, 0.0).radius - 1.0).abs() < 1e-6);
        // 越界输入不 panic
        let clamped = SharpenPlan::from_sliders(-5.0, 200.0);
        assert_eq!(clamped.amount, 0.0);
        assert!((clamped.radius - 4.0).abs() < 1e-6);
    }

    #[test]
    fn flat_area_is_untouched() {
        // 没有细节就没有可加的东西（±1 级量化）
        let mut rgb = edge_image(32, 32, 120, 120);
        let original = rgb.clone();
        sharpen_display(
            &mut rgb,
            32,
            32,
            &SharpenPlan {
                amount: 1.0,
                radius: 1.0,
            },
        );
        for (a, b) in rgb.iter().zip(original.iter()) {
            assert!(a.abs_diff(*b) <= 1, "{a} vs {b}");
        }
    }

    #[test]
    fn edge_contrast_increases() {
        // 平滑过渡的边缘（有曲率）：锐化会把暗侧压得更暗、亮侧抬得更亮 ⇒ 行内动态范围变大。
        // （注：**纯线性**斜坡锐化不动它 —— 对称模糊对线性函数恒等，这是数学事实，不是 bug。）
        let width = 64u32;
        let height = 16u32;
        let mut rgb = vec![0u8; (width as usize) * (height as usize) * 3];
        for y in 0..height as usize {
            for x in 0..width as usize {
                let t = ((x as f32 - 28.0) / 8.0).clamp(0.0, 1.0);
                let smooth = t * t * (3.0 - 2.0 * t);
                let value = (80.0 + smooth * 100.0) as u8;
                let index = (y * width as usize + x) * 3;
                rgb[index] = value;
                rgb[index + 1] = value;
                rgb[index + 2] = value;
            }
        }
        let range = |data: &[u8]| -> i32 {
            let y = 8usize;
            let row = &data[y * width as usize * 3..(y + 1) * width as usize * 3];
            let values: Vec<i32> = row.chunks(3).map(|p| i32::from(p[0])).collect();
            values.iter().max().unwrap_or(&0) - values.iter().min().unwrap_or(&0)
        };
        let before = range(&rgb);
        sharpen_display(
            &mut rgb,
            width,
            height,
            &SharpenPlan {
                amount: 1.0,
                radius: 2.0,
            },
        );
        let after = range(&rgb);
        assert!(after > before, "边缘动态范围应当变大：{before} → {after}");
    }

    #[test]
    fn overshoot_stays_within_the_soft_limit() {
        // 强边缘（0 → 255）：细节量很大，软限幅必须把它压住（不出现夸张的白边/黑边）
        let mut rgb = edge_image(64, 16, 0, 255);
        sharpen_display(
            &mut rgb,
            64,
            16,
            &SharpenPlan {
                amount: 1.0,
                radius: 1.0,
            },
        );
        // 增量被限在约 ±2×16 级以内（给量化留 2 级余量）
        for (a, b) in rgb.iter().zip(edge_image(64, 16, 0, 255).iter()) {
            assert!(
                i32::from(*a) - i32::from(*b) <= 2 * 16 + 2,
                "增量过大：{a} vs {b}"
            );
        }
    }

    #[test]
    fn sharpening_preserves_channel_differences() {
        // 只动亮度 ⇒ 彩色像素的 R−B 差不变（色相/饱和不被推歪）。
        // 取值故意选在离 0/255 较远的地方：**夹取**本身会破坏通道差（那是合理的边界行为）。
        let width = 32u32;
        let height = 8u32;
        let mut rgb = vec![0u8; (width as usize) * (height as usize) * 3];
        for y in 0..height as usize {
            for x in 0..width as usize {
                let index = (y * width as usize + x) * 3;
                let value = if x < 16 { 110u8 } else { 190u8 };
                rgb[index] = value;
                rgb[index + 1] = value.saturating_sub(20);
                rgb[index + 2] = value.saturating_sub(40);
            }
        }
        let differences = |data: &[u8]| -> Vec<i32> {
            data.as_chunks::<3>()
                .0
                .iter()
                .map(|p| i32::from(p[0]) - i32::from(p[2]))
                .collect()
        };
        let before = differences(&rgb);
        sharpen_display(
            &mut rgb,
            width,
            height,
            &SharpenPlan {
                amount: 1.0,
                radius: 1.0,
            },
        );
        for (a, b) in differences(&rgb).iter().zip(before.iter()) {
            assert_eq!(a, b, "通道差必须原样保留");
        }
    }

    #[test]
    fn mismatched_buffer_is_left_alone() {
        let mut short = vec![0u8; 5];
        sharpen_display(
            &mut short,
            32,
            32,
            &SharpenPlan {
                amount: 1.0,
                radius: 1.0,
            },
        );
        assert_eq!(short, vec![0u8; 5]);
        // 尺寸为 0 也不 panic
        let mut empty: Vec<u8> = Vec::new();
        sharpen_display(
            &mut empty,
            0,
            0,
            &SharpenPlan {
                amount: 1.0,
                radius: 1.0,
            },
        );
    }

    #[test]
    fn tiny_images_do_not_panic() {
        for (w, h) in [(1u32, 1u32), (2, 1), (1, 2), (3, 3), (5, 4)] {
            let mut rgb = edge_image(w, h, 10, 200);
            sharpen_display(
                &mut rgb,
                w,
                h,
                &SharpenPlan {
                    amount: 1.0,
                    radius: 4.0,
                },
            );
            assert_eq!(rgb.len(), (w as usize) * (h as usize) * 3);
        }
    }
}
