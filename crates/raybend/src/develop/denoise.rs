//! **降噪（快速档）**：亮度 / 色度两支，0 = 逐位不动。
//!
//! # 它要解决的问题
//!
//! RAW 解码（去马赛克）之后有两类噪声：**亮度噪点**（细密的颗粒，放大才明显）与
//! **色度噪点**（成片的红/绿/蓝斑块，缩略图尺度就看得见）。两者在视觉上的容忍度完全不同 ——
//! 亮度噪点不能过度抹（会变成「塑料感」），色度噪点可以往死里抹（人眼对色度细节极不敏感）。
//! 所以**两支分开做**，对应界面上两根拉杆。
//!
//! # 亮度支：多尺度的保边收缩
//!
//! ```text
//! ① 低分辨率（1/4）上做多尺度分解
//!      B1 = 箱式滤波(Y, 细半径)   D1 = Y  − B1   ← 细尺度：颗粒
//!      B2 = 箱式滤波(B1, 粗半径)  D2 = B1 − B2   ← 中尺度：结构
//! ② 逐尺度维纳收缩：D' = D · D²/(D² + s²)   ← |D| ≫ s 时保留，|D| ≪ s 时抹掉
//! ③ Y' = B2 + D2' + D1'，再按 Y'/Y 缩回 RGB ← 只动亮度，保色相与饱和
//! ```
//!
//! **噪声尺度 `s` 随亮度增长**（`s = Y · σ`）：散粒噪声的绝对幅度 ∝ √信号，
//! 在**线性域**用固定阈值会「暗部糊成一片、亮部没动」；让 `s ∝ Y` 就拿到了
//! 「log 域一个阈值通吃」的同一个效果，**而不用每像素跑一次 log2/exp2**
//! （早期版本就是那样写的，实测那两个超越函数是全分辨率那一趟的主要开销）。
//! 与 `local_tone` 的分解是同一套算子，只是那边在 log 域、这边用 `s ∝ Y` 代替。
//!
//! **为什么用维纳因子而不是硬阈值**：硬阈值在阈值附近产生「要么全留要么全抹」的跳变，
//! 表现为细节时隐时现的斑点；`D²/(D²+s²)` 是平滑的，而且**大信号自动趋近 1**（强边缘原样保留）。
//! `w = 1` 时 `B2 + D2 + D1 = Y` 逐位重构，所以「不收缩」是真的不收缩。
//!
//! # 色度支：大半径保边模糊
//!
//! 色度噪声是低频的，所以做法很简单：把 `Cb/Cr`（相对亮度的差）做**大半径模糊**再混回去。
//! 唯一要防的是「跨边缘把颜色带过去」：用亮度细尺度 `|D1|` 做门控 —— 有细节（边缘）的地方
//! 少模糊。参考 RapidRAW 的 `remove_raw_artifacts_and_enhance`（同一思路，那边用 5×5 稀疏采样）。
//!
//! # 为什么分解在 1/4 分辨率上做（**这条是内存纪律**）
//!
//! 24MP 一张 f32 平面就是 96 MB。上面那条链在**全分辨率**要同时活着 6 张平面（≈580 MB），
//! 而显影线程里已经躺着线性源（u16×3，144 MB）与输出（72 MB）。所以：
//! 低分辨率平面用**一个并行单趟**直接从全分辨率源算出来（不经过中间图），
//! 分解、滤波、色度混合全部在 1/4 分辨率上做，回到全分辨率时只留**逐行上采样**
//! （[`RowUpsampler`]，一行缓冲），额外内存降到 ≈ 50 MB。
//! 代价是细尺度只到「全分辨率 4 px」这一档 —— 而那正是噪点的尺度，够用。
//!
//! （早期版本是「先 `downscaled_to` 缩图、再取亮度」：那条路要读三通道、写一张中间图、
//! 再读回来，**而且全是串行** —— 实测它把多线程加速比从 5× 拉到 2×。现已换掉。）
//!
//! # 已知边界
//!
//! * **没有相机噪声档案**（`FUTURE.md` D3）：`σ` 由拉杆直接给（拉杆 = 噪声水平 + 强度合一），
//!   同一档位在不同 ISO 的照片上效果不同 —— 这是本轮**有意**的取舍，等有档案再细化。
//! * 强度 0 时**逐位恒等**（早退，不经过任何浮点往返），单测钉住。

use super::filters::{Plane, RowUpsampler, box_blur};
use super::pipeline::{LinearImage, luma_of};

/// 分析用的降采样倍数（分解、滤波、色度混合都在这个分辨率上做）。
const ANALYSIS_SCALE: u32 = 4;

/// 细尺度最大收缩阈值（**log2 单位**；`1.0` 拉杆 = 这个值）。
///
/// 0.06 log2 ≈ 4% 亮度 —— 这是「拉到底」的强度，不是默认值（默认是 0 = 不动）。
/// 内部用 [`LN2`] 换算成「噪声尺度 ∝ 亮度」的线性域系数。
const LUMA_SIGMA_MAX: f32 = 0.06;
/// `ln 2`：log2 域的阈值 → 线性域的相对阈值（`d(ln y)/dy = 1/y`）。
const LN2: f32 = std::f32::consts::LN_2;
/// 粗尺度的阈值折减：粗尺度的噪声已被平均掉一部分，而真实结构更多。
const COARSE_SIGMA_RATIO: f32 = 0.5;
/// 色度门控的细节阈值（**log2 单位**）：相对细节超过它就算「边缘」，少模糊。
const CHROMA_EDGE_THRESHOLD: f32 = 0.06;
/// 亮度的地板（= u16 编码里的 1）。
const LUMA_FLOOR: f32 = 1.0 / 65535.0;

/// 降噪方式（**编辑栈的一级**，不是全局偏好 —— 见 `catalog_0006_lens.sql` 的注释）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NrMethod {
    /// 快速档（默认）：多尺度保边收缩，实时跟手
    #[default]
    Fast,
    /// 高质量档：BM3D（后台任务，拖动期间先显示快速档结果）
    High,
}

impl NrMethod {
    /// 存库用的字面量（默认档**不写库** ⇒ `NULL`）。
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::High => "high",
        }
    }

    /// 解析（认不出给 `None` —— 调用方决定是报错还是忽略，不静默回退）。
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "fast" => Some(Self::Fast),
            "high" => Some(Self::High),
            _ => None,
        }
    }

    /// 是默认档吗（默认档不写库）。
    #[must_use]
    pub fn is_default(self) -> bool {
        self == Self::Fast
    }
}

/// 降噪计划（快速档）：两支强度都是 `0..1`，两个都是 0 = 整趟跳过。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DenoisePlan {
    /// 亮度降噪强度
    pub luma: f32,
    /// 色度降噪强度
    pub chroma: f32,
}

impl DenoisePlan {
    /// 从界面拉杆（0..100）建计划。
    #[must_use]
    pub fn from_sliders(luma_nr: f64, color_nr: f64) -> Self {
        Self {
            luma: (luma_nr / 100.0).clamp(0.0, 1.0) as f32,
            chroma: (color_nr / 100.0).clamp(0.0, 1.0) as f32,
        }
    }

    /// 什么都不用做（两支都是 0；NaN 也当「没开」，见 `!v.is_finite()`）。
    #[must_use]
    pub fn is_identity(&self) -> bool {
        let inactive = |value: f32| !value.is_finite() || value <= 0.0;
        inactive(self.luma) && inactive(self.chroma)
    }
}

/// **快速档降噪**：线性 u16 源 → 线性 u16 结果（同尺寸）。
///
/// 计划为空时直接 `clone`（**逐位一致**）。
#[must_use]
pub fn denoise_fast(source: &LinearImage, plan: &DenoisePlan) -> LinearImage {
    if plan.is_identity() {
        return source.clone();
    }
    let luma = plan.luma.clamp(0.0, 1.0);
    let chroma = plan.chroma.clamp(0.0, 1.0);

    // ── 分析（低分辨率平面一个并行单趟算出，之后全在 1/4 分辨率上做）──
    let low = low_planes_from(source, ANALYSIS_SCALE);
    let long = source.width.max(source.height);
    let r1_full = band_radius(long, 1200, 8);
    let r1 = (r1_full / ANALYSIS_SCALE).max(1) as usize;
    let b1 = box_blur(&low.luma, r1);
    let coarse = if luma > 0.0 {
        let r2 = (r1_full * 3 / ANALYSIS_SCALE).max(1) as usize;
        Some(box_blur(&b1, r2))
    } else {
        None
    };
    let chroma_low = if chroma > 0.0 {
        let radius = (band_radius(long, 220, 40) / ANALYSIS_SCALE).max(1) as usize;
        let (cb, cr) = chroma_analysis(&low.luma, &b1, low.cb, low.cr, chroma, radius);
        Some(ChromaAnalysis { cb, cr })
    } else {
        None
    };

    // ── 回到全分辨率（逐行上采样 + 逐像素收缩）──
    let width = source.width as usize;
    let height = source.height as usize;
    let mut out = vec![0u16; source.rgb.len()];
    let luma_sigma = LUMA_SIGMA_MAX * luma * LN2;
    let stage = Stage {
        b1: &b1,
        coarse: coarse.as_ref(),
        chroma_low: chroma_low.as_ref(),
        luma_sigma,
        luma_active: luma > 0.0,
        chroma_active: chroma > 0.0,
        width: source.width,
        height: source.height,
    };
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(16);
    if threads <= 1 || height < 32 {
        stage.run(&source.rgb, &mut out, 0);
    } else {
        let rows_per_chunk = height.div_ceil(threads);
        std::thread::scope(|scope| {
            let mut remaining = out.as_mut_slice();
            let mut first_row = 0usize;
            while !remaining.is_empty() {
                let rows = (remaining.len() / (width * 3)).min(rows_per_chunk).max(1);
                let take = rows * width * 3;
                let (chunk, rest) = remaining.split_at_mut(take);
                remaining = rest;
                let start = first_row;
                first_row += rows;
                let stage_ref = &stage;
                scope.spawn(move || stage_ref.run(&source.rgb, chunk, start));
            }
        });
    }
    LinearImage {
        width: source.width,
        height: source.height,
        rgb: out,
    }
}

/// 全分辨率那一趟要用的东西（低分辨率平面 + 上采样器）。
struct Stage<'a> {
    b1: &'a Plane,
    coarse: Option<&'a Plane>,
    chroma_low: Option<&'a ChromaAnalysis>,
    /// **线性域**的细尺度噪声尺度系数（`s = Y · luma_sigma`）
    luma_sigma: f32,
    luma_active: bool,
    chroma_active: bool,
    width: u32,
    height: u32,
}

impl Stage<'_> {
    /// 一段行的像素循环（**唯一**的降噪像素循环）。
    fn run(&self, input: &[u16], output: &mut [u16], first_row: usize) {
        let width = self.width as usize;
        let mut up_b1 = RowUpsampler::new(self.b1, self.width, self.height);
        let mut up_b2 = self
            .coarse
            .map(|plane| RowUpsampler::new(plane, self.width, self.height));
        let mut up_cb = self
            .chroma_low
            .map(|c| RowUpsampler::new(&c.cb, self.width, self.height));
        let mut up_cr = self
            .chroma_low
            .map(|c| RowUpsampler::new(&c.cr, self.width, self.height));
        let input_rows = input.as_chunks::<3>().0.chunks(width);
        let output_rows = output.as_chunks_mut::<3>().0.chunks_mut(width);
        for (local_row, (line_in, line_out)) in input_rows.zip(output_rows).enumerate() {
            let y = first_row + local_row;
            up_b1.set_row(y);
            if let Some(up) = up_b2.as_mut() {
                up.set_row(y);
            }
            if let Some(up) = up_cb.as_mut() {
                up.set_row(y);
            }
            if let Some(up) = up_cr.as_mut() {
                up.set_row(y);
            }
            for (x, (pixel_in, pixel_out)) in line_in.iter().zip(line_out.iter_mut()).enumerate() {
                let mut rgb = [
                    f32::from(pixel_in[0]) / 65535.0,
                    f32::from(pixel_in[1]) / 65535.0,
                    f32::from(pixel_in[2]) / 65535.0,
                ];
                let y0 = luma_of(rgb);
                if y0 > LUMA_FLOOR {
                    // ① 色度：把 Cb/Cr 换成低分辨率上模糊过的那一份
                    //    （“原色度”已经在低分辨率那边混过了，这里只拿结果）
                    if let (Some(cb_up), Some(cr_up)) = (up_cb.as_ref(), up_cr.as_ref()) {
                        rgb[2] = y0 + cb_up.at(x);
                        rgb[0] = y0 + cr_up.at(x);
                        rgb[1] = (y0 - 0.2126 * rgb[0] - 0.0722 * rgb[2]) / 0.7152;
                    }
                    // ② 亮度：多尺度收缩（噪声尺度随亮度增长），再按比例缩回 RGB
                    if self.luma_active {
                        let base1 = up_b1.at(x);
                        let d1 = y0 - base1;
                        let noise1 = y0 * self.luma_sigma;
                        let target = if let Some(up) = up_b2.as_ref() {
                            let base2 = up.at(x);
                            let d2 = base1 - base2;
                            base2
                                + shrink(d2, noise1 * COARSE_SIGMA_RATIO)
                                + shrink(d1, noise1)
                        } else {
                            base1 + shrink(d1, noise1)
                        };
                        let scale = target / y0;
                        if scale.is_finite() && scale > 0.0 {
                            rgb = [rgb[0] * scale, rgb[1] * scale, rgb[2] * scale];
                        }
                    }
                }
                for channel in 0..3 {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    {
                        pixel_out[channel] =
                            (rgb[channel].clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
                    }
                }
            }
        }
        let _ = (self.chroma_active, self.height);
    }
}

/// 维纳收缩因子（线性域）：`D · D²/(D²+s²)`（`|D| ≫ s` ⇒ D；`|D| ≪ s` ⇒ 0）。
///
/// `s` 是**该像素处**的噪声尺度（`s = Y·σ`，随亮度增长）——
/// 这就是「log 域一个阈值通吃」的线性等价形式。
#[inline]
fn shrink(detail: f32, scale: f32) -> f32 {
    if scale <= 0.0 {
        return detail;
    }
    let d2 = detail * detail;
    let s2 = scale * scale;
    detail * (d2 / (d2 + s2))
}

/// 低分辨率上的三张平面（亮度 + 两个色度差）。
struct LowPlanes {
    luma: Plane,
    /// `B − Y`
    cb: Plane,
    /// `R − Y`
    cr: Plane,
}

/// **一个并行单趟**：全分辨率源 → 1/`factor` 分辨率的亮度与两个色度差。
///
/// 块取法与 `LinearImage::downscaled_to` 同一套口径（`x·src/dst` 的整数区间），
/// 但**只读一遍三通道、直接出三张 1/4 分辨率的平面**（而那条路要两遍扫描 + 一张三通道中间图，
/// 且无法并行）。色度那两张多花每像素 4 次加减 —— 比起省下的那一趟，值。
fn low_planes_from(source: &LinearImage, factor: u32) -> LowPlanes {
    let step = factor.max(1) as usize;
    let source_width = source.width as usize;
    let source_height = source.height as usize;
    let width = source_width.div_ceil(step).max(1);
    let height = source_height.div_ceil(step).max(1);
    let mut values = vec![[0f32; 3]; width * height];
    let rgb = &source.rgb;
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(16);
    let rows_per_chunk = height.div_ceil(threads.max(1)).max(1);
    let mut remaining = values.as_mut_slice();
    let mut first = 0usize;
    std::thread::scope(|scope| {
        while !remaining.is_empty() {
            let rows = remaining.len().div_ceil(width).min(rows_per_chunk).max(1);
            let (chunk, rest) = remaining.split_at_mut(rows * width);
            remaining = rest;
            let start = first;
            first += rows;
            scope.spawn(move || {
                for (local, line) in chunk.chunks_mut(width).enumerate() {
                    let ly = start + local;
                    let y0 = ly * step;
                    let y1 = ((ly + 1) * step).min(source_height);
                    for (lx, slot) in line.iter_mut().enumerate() {
                        let x0 = lx * step;
                        let x1 = ((lx + 1) * step).min(source_width);
                        let mut sum = [0f32; 3];
                        let mut count = 0u32;
                        for y in y0..y1 {
                            let base = y * source_width * 3;
                            for x in x0..x1 {
                                let index = base + x * 3;
                                let r = f32::from(rgb[index]) / 65535.0;
                                let g = f32::from(rgb[index + 1]) / 65535.0;
                                let b = f32::from(rgb[index + 2]) / 65535.0;
                                let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                                sum[0] += luma;
                                sum[1] += b - luma;
                                sum[2] += r - luma;
                                count += 1;
                            }
                        }
                        #[allow(clippy::cast_precision_loss)]
                        let divisor = if count == 0 { 1.0 } else { count as f32 };
                        *slot = [
                            (sum[0] / divisor).max(LUMA_FLOOR),
                            sum[1] / divisor,
                            sum[2] / divisor,
                        ];
                    }
                }
            });
        }
    });
    // 三张平面（拆开：`Plane` 是单通道的，而分解与滤波都按平面做）
    let mut luma = Vec::with_capacity(values.len());
    let mut cb = Vec::with_capacity(values.len());
    let mut cr = Vec::with_capacity(values.len());
    for value in &values {
        luma.push(value[0]);
        cb.push(value[1]);
        cr.push(value[2]);
    }
    let size = (width as u32, height as u32);
    LowPlanes {
        luma: Plane {
            width: size.0,
            height: size.1,
            values: luma,
        },
        cb: Plane {
            width: size.0,
            height: size.1,
            values: cb,
        },
        cr: Plane {
            width: size.0,
            height: size.1,
            values: cr,
        },
    }
}

/// 低分辨率上算好的色度（**已经混过、可以直接上采样**）。
struct ChromaAnalysis {
    cb: Plane,
    cr: Plane,
}

/// 色度支的分析：大半径模糊 + 细节门控混合（两张平面**原地**改）。
fn chroma_analysis(
    luma_low: &Plane,
    b1: &Plane,
    mut cb: Plane,
    mut cr: Plane,
    strength: f32,
    radius: usize,
) -> (Plane, Plane) {
    let width = luma_low.width as usize;
    let cb_blur = box_blur(&cb, radius);
    let cr_blur = box_blur(&cr, radius);
    // 门控：有细节的地方少模糊（**相对**细节大 ⇒ 边缘）
    let threshold = CHROMA_EDGE_THRESHOLD * LN2;
    for index in 0..width * luma_low.height as usize {
        let luma = luma_low.values[index];
        let relative = (luma - b1.values[index]).abs() / luma.max(LUMA_FLOOR);
        let weight = strength / (1.0 + (relative / threshold).powi(2));
        cb.values[index] += (cb_blur.values[index] - cb.values[index]) * weight;
        cr.values[index] += (cr_blur.values[index] - cr.values[index]) * weight;
    }
    (cb, cr)
}

/// 尺度半径：`长边 / divisor`，夹在 `1..=max` 里。
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn band_radius(long_edge: u32, divisor: u32, max: u32) -> u32 {
    (long_edge / divisor.max(1)).clamp(1, max)
}

/* ══════════════════════════════════════════════════════════════
 * 单测
 * ══════════════════════════════════════════════════════════════ */

#[cfg(test)]
mod tests {
    use super::*;

    /// 一块「平坦底 + 一条竖边 + 固定颗粒」的合成图（噪声用确定性伪随机，不引 rand）。
    fn synthetic(width: u32, height: u32, grain: u16) -> LinearImage {
        let mut rgb = vec![0u16; (width as usize) * (height as usize) * 3];
        let mut seed = 0x1234_5678u32;
        for y in 0..height as usize {
            for x in 0..width as usize {
                let mut next = || {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (seed >> 16) as i32 % (i32::from(grain) + 1) - i32::from(grain) / 2
                };
                let base = if x < (width as usize) / 2 { 20000 } else { 40000 };
                for channel in 0..3 {
                    let value = (base + next()).clamp(0, 65535);
                    rgb[(y * width as usize + x) * 3 + channel] = value as u16;
                }
            }
        }
        LinearImage::new(width, height, rgb).expect("尺寸与长度对得上")
    }

    fn variance(values: &[u16]) -> f64 {
        let n = values.len() as f64;
        let mean = values.iter().map(|v| f64::from(*v)).sum::<f64>() / n;
        values
            .iter()
            .map(|v| (f64::from(*v) - mean).powi(2))
            .sum::<f64>()
            / n
    }

    /// 取左半块（平坦区）的绿通道 —— 噪声应当在这里被压下去。
    fn flat_region(image: &LinearImage) -> Vec<u16> {
        let width = image.width as usize;
        let mut out = Vec::new();
        for y in 8..(image.height as usize - 8) {
            for x in 8..(width / 2 - 8) {
                out.push(image.rgb[(y * width + x) * 3 + 1]);
            }
        }
        out
    }

    #[test]
    fn zero_plan_is_bit_identical() {
        let source = synthetic(64, 48, 400);
        let out = denoise_fast(&source, &DenoisePlan::default());
        assert_eq!(out.rgb, source.rgb, "强度 0 必须逐位一致");
    }

    #[test]
    fn luma_denoise_reduces_noise_in_flat_areas() {
        let source = synthetic(128, 96, 800);
        let plan = DenoisePlan {
            luma: 1.0,
            chroma: 0.0,
        };
        let out = denoise_fast(&source, &plan);
        let before = variance(&flat_region(&source));
        let after = variance(&flat_region(&out));
        assert!(
            after < before * 0.6,
            "平坦区方差应当明显下降：{before:.0} → {after:.0}"
        );
    }

    #[test]
    fn luma_denoise_keeps_the_edge() {
        // 竖边两侧的平均值必须还在（边缘不被抹平）
        let source = synthetic(128, 96, 0);
        let plan = DenoisePlan {
            luma: 1.0,
            chroma: 0.0,
        };
        let out = denoise_fast(&source, &plan);
        let width = out.width as usize;
        let left = f64::from(out.rgb[(48 * width + 20) * 3 + 1]);
        let right = f64::from(out.rgb[(48 * width + 108) * 3 + 1]);
        assert!(
            (right - left).abs() > 15000.0,
            "边缘两侧的差应当保住：{left} vs {right}"
        );
    }

    #[test]
    fn chroma_denoise_pulls_chroma_towards_zero() {
        // 造一块色度噪声：绿通道固定、红蓝抖动
        let width = 96;
        let height = 72;
        let mut rgb = vec![0u16; (width as usize) * (height as usize) * 3];
        let mut seed = 7u32;
        for pixel in rgb.as_chunks_mut::<3>().0 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let jitter = i32::try_from((seed >> 20) % 4000).unwrap_or(0) - 2000;
            let value = 30000 + jitter;
            pixel[0] = u16::try_from(value.clamp(0, 65535)).unwrap_or(0);
            pixel[1] = 30000;
            pixel[2] = u16::try_from((60000 - value).clamp(0, 65535)).unwrap_or(0);
        }
        let source = LinearImage::new(width, height, rgb).expect("尺寸对得上");
        let plan = DenoisePlan {
            luma: 0.0,
            chroma: 1.0,
        };
        let out = denoise_fast(&source, &plan);
        let spread = |image: &LinearImage| {
            let mut values = Vec::new();
            for y in 8..(image.height as usize - 8) {
                for x in 8..(image.width as usize - 8) {
                    let index = (y * image.width as usize + x) * 3;
                    let r = f32::from(image.rgb[index]);
                    let b = f32::from(image.rgb[index + 2]);
                    values.push(r - b);
                }
            }
            variance(&values.iter().map(|v| v.round() as u16).collect::<Vec<_>>())
        };
        assert!(
            spread(&out) < spread(&source) * 0.5,
            "色度抖动应当被压下去：{:.0} → {:.0}",
            spread(&source),
            spread(&out)
        );
    }

    #[test]
    fn plan_from_sliders_clamps_and_scales() {
        assert_eq!(DenoisePlan::from_sliders(0.0, 0.0), DenoisePlan::default());
        assert_eq!(
            DenoisePlan::from_sliders(100.0, 50.0),
            DenoisePlan {
                luma: 1.0,
                chroma: 0.5
            }
        );
        // 越界输入不 panic（夹取）
        let clamped = DenoisePlan::from_sliders(-10.0, 300.0);
        assert_eq!(clamped, DenoisePlan { luma: 0.0, chroma: 1.0 });
    }

    #[test]
    fn tiny_image_does_not_panic() {
        for (w, h) in [(1, 1), (2, 3), (3, 2), (5, 5)] {
            let source = synthetic(w, h, 100);
            let out = denoise_fast(
                &source,
                &DenoisePlan {
                    luma: 1.0,
                    chroma: 1.0,
                },
            );
            assert_eq!(out.rgb.len(), source.rgb.len());
        }
    }

    #[test]
    fn pure_black_stays_black() {
        let source = LinearImage::new(32, 32, vec![0u16; 32 * 32 * 3]).expect("尺寸对得上");
        let out = denoise_fast(
            &source,
            &DenoisePlan {
                luma: 1.0,
                chroma: 1.0,
            },
        );
        assert!(out.rgb.iter().all(|v| *v == 0), "全黑图不许被抬起来");
    }

    #[test]
    fn uniform_gray_is_untouched() {
        // 没有噪声就没有可收缩的细节：整幅纯灰应当原样返回（±1 级量化）
        let source = LinearImage::new(64, 64, vec![30000u16; 64 * 64 * 3]).expect("尺寸对得上");
        let out = denoise_fast(
            &source,
            &DenoisePlan {
                luma: 1.0,
                chroma: 0.0,
            },
        );
        for (a, b) in out.rgb.iter().zip(source.rgb.iter()) {
            assert!(a.abs_diff(*b) <= 1, "{a} vs {b}");
        }
    }
}
