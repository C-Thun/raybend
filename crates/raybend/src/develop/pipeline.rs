//! **显影管线**：线性 sRGB 像素 → 参数 → 显示用的 8bit sRGB。
//!
//! # 链的顺序（改顺序 = 改画质，写在这里当唯一口径）
//!
//! ```text
//! 线性 sRGB (u16)
//!   → 白平衡增益（色温，见 color::temperature_gain_ratio）
//!   → 曝光（× 2^EV）
//!   → 反差（线性域，绕中灰 0.18 的分段幂，两端固定）
//!   → 高光（线性域，作用在亮部：y + h·y⁴(1−y)）
//!   → 黑区（线性域，作用在暗部：y + b·(1−y)⁴y）
//!   → 显示变换（sRGB OETF）
//!   → 曲线（先 RGB 合成、再各通道）
//!   → 饱和度 / 自然饱和度（**显示参考域**，见下）
//!   → 8bit 量化
//! ```
//!
//! ## 为什么色度（饱和/自然饱和）放在显示参考域
//!
//! 调性与白平衡在**线性域**做（scene-referred，暗部拉伸不会出色带，这是 `FUTURE.md` D1 的要求）；
//! 但色度缩放放在显示参考域，理由有两条，都很实际：
//!
//! 1. **整条链只剩一层 LUT**（线性域那半条 + 显示变换 + 曲线可以合成 3×4096 张表）——
//!    每像素只有 3 次查表 + 一次色度缩放，1:1 拖动才可能跟得上手；
//! 2. 主流编辑器（Lightroom / ACR）的饱和度也是在 gamma 空间做的，观感一致。
//!
//! 这两条写在实现记录里，将来若要做「线性域色度」，改的是这里 + 测试向量，不是三处。
//!
//! # 两条路径，一套数学
//!
//! * [`map_pixel_exact`]：逐像素调 [`chain_linear`] / [`encode_and_curve`]（f32，无离散化）；
//! * [`render_rgb8`]：把同两条函数**采样成 LUT** 再逐像素查表（快路径，生产用）。
//!
//! 单测用外部给定的测试向量钉住 `map_pixel_exact`，再逐点比对「LUT 路径 vs 精确路径」
//! （差不超过 1 级 8bit）—— 所以「快」不会把「对」甩掉。

use super::color::{linear_to_srgb, srgb8_to_linear_table, temperature_gain_ratio};
use super::curve::CurveSet;
use super::params::DevelopParams;

/// **线性 sRGB** 图像（u16 编码：`0..=65535` 对应 `0.0..=1.0`）。
///
/// 为什么用 u16 而不是 f32：24MP 的 f32 三通道是 288MB，u16 只要 144MB；
/// 而 u16 的相对精度（1.5e-5）已经远好于 8bit 显示（1/255）与 sRGB 的暗部量化。
/// 为什么不是 8bit：线性 8bit 在暗部只有几级（线性 1/255 对应 sRGB 的 ~0.05），
/// 曝光一拉就出色带 —— 那正是「不要 8bit 显示空间」这条纪律要防的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinearImage {
    pub width: u32,
    pub height: u32,
    /// 行主序紧密排列，长度 = `width × height × 3`。
    pub rgb: Vec<u16>,
}

impl LinearImage {
    /// 从字节建（长度对不上返回 `None`，**不猜、不补零**）。
    #[must_use]
    pub fn new(width: u32, height: u32, rgb: Vec<u16>) -> Option<Self> {
        let expected = (width as usize).checked_mul(height as usize)?.checked_mul(3)?;
        if width == 0 || height == 0 || rgb.len() != expected {
            return None;
        }
        Some(Self { width, height, rgb })
    }

    /// 长度与尺寸对得上吗（跨进程/跨线程序列化之后要防一手）。
    #[must_use]
    pub fn is_consistent(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.rgb.len() == (self.width as usize) * (self.height as usize) * 3
    }

    /// 8bit **sRGB 编码**的像素 → 线性（JPEG / RAW 内嵌预览那条路）。
    ///
    /// 这一步是有损的（8bit 显示数据里暗部本来就没有更多信息），
    /// 所以这条路的输入只能算「准线性」—— 实施记录与 `FUTURE.md` C7 都记着这件事。
    #[must_use]
    pub fn from_srgb8(width: u32, height: u32, rgb8: &[u8]) -> Option<Self> {
        let expected = (width as usize).checked_mul(height as usize)?.checked_mul(3)?;
        if width == 0 || height == 0 || rgb8.len() != expected {
            return None;
        }
        let table = srgb8_to_linear_table();
        let mut rgb = Vec::with_capacity(expected);
        for value in rgb8 {
            let linear = table[*value as usize];
            // 0..1 → 0..65535（四舍五入；线性值本来就在 [0,1]）
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let encoded = (linear * 65535.0 + 0.5) as u32;
            rgb.push(u16::try_from(encoded.min(65535)).unwrap_or(u16::MAX));
        }
        Self::new(width, height, rgb)
    }

    /// 缩到长边 `long_edge`（不放大）。
    ///
    /// 用在「预览档」：视口缩小看的时候要的是**不锯齿**的小图，而不是 24MP 纹理。
    ///
    /// 用**两遍整数箱式平均**（先横后竖），不用 `image` 的 resize / thumbnail：
    /// 实测 6000×4000 → 1920 用 `imageops::thumbnail` 要 **302ms**、Triangle 要 **845ms**，
    /// 比整条管线还贵；箱式平均自己写只要几十毫秒（见 `develop-probe` 的输出）。
    /// 而预览档的缩放比常在 3–8 倍，箱式平均（区域平均）正是这个比例下的理想低通。
    #[must_use]
    pub fn downscaled_to(&self, long_edge: u32) -> Self {
        let long = self.width.max(self.height);
        if long_edge == 0 || long <= long_edge {
            return self.clone();
        }
        let scale = f64::from(long_edge) / f64::from(long);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let width = ((f64::from(self.width) * scale).round() as u32).max(1);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let height = ((f64::from(self.height) * scale).round() as u32).max(1);

        // ① 横着缩（每一行的输出 = 对应输入列区间的平均）
        let mut horizontal = vec![0u16; (width as usize) * (self.height as usize) * 3];
        for y in 0..self.height as usize {
            let row = &self.rgb[y * (self.width as usize) * 3..(y + 1) * (self.width as usize) * 3];
            let out_row = &mut horizontal[y * (width as usize) * 3..(y + 1) * (width as usize) * 3];
            for x in 0..width as usize {
                let start = x * (self.width as usize) / (width as usize);
                let end = (((x + 1) * (self.width as usize)) / (width as usize)).max(start + 1);
                for channel in 0..3 {
                    let mut sum = 0u32;
                    for column in start..end {
                        sum += u32::from(row[column * 3 + channel]);
                    }
                    #[allow(clippy::cast_possible_truncation)]
                    let count = (end - start) as u32;
                    out_row[x * 3 + channel] = ((sum + count / 2) / count) as u16;
                }
            }
        }

        // ② 竖着缩（同一套算法，作用在中间结果上）
        let mut out = vec![0u16; (width as usize) * (height as usize) * 3];
        let source_height = self.height as usize;
        for y in 0..height as usize {
            let start = y * source_height / (height as usize);
            let end = (((y + 1) * source_height) / (height as usize)).max(start + 1);
            for x in 0..width as usize {
                for channel in 0..3 {
                    let mut sum = 0u32;
                    for row in start..end {
                        sum += u32::from(horizontal[(row * width as usize + x) * 3 + channel]);
                    }
                    #[allow(clippy::cast_possible_truncation)]
                    let count = (end - start) as u32;
                    out[(y * width as usize + x) * 3 + channel] = ((sum + count / 2) / count) as u16;
                }
            }
        }

        Self {
            width,
            height,
            rgb: out,
        }
    }
}

/// 管线里被解析出来的参数（一条参数查一次，别在像素循环里查表）。
///
/// 公开是给**参考路径**（[`map_pixel_exact_with`]）与测试用的：
/// 快路径自己也会 `Resolved::new` 一次，两边看的是同一份解析结果。
#[derive(Debug, Clone, Copy)]
pub struct Resolved {
    /// 白平衡 × 曝光的**通道增益**（线性域直接乘）。
    gains: [f32; 3],
    /// 反差（−1..1）
    contrast: f32,
    /// 高光（−1..1）
    highlights: f32,
    /// 黑区（−1..1）
    blacks: f32,
    /// 饱和度（−1..1）
    saturation: f32,
    /// 自然饱和度（−1..1）
    vibrance: f32,
}

impl Resolved {
    /// 从参数集解析（只认 `wired = true` 的项；色温走「目标相对基准」的增益比）。
    #[must_use]
    pub fn new(params: &DevelopParams) -> Self {
        let exposure = params.value("exposure");
        let baseline_k = params.baseline("temperature");
        let target_k = params.value("temperature");
        #[allow(clippy::cast_possible_truncation)]
        let wb = temperature_gain_ratio(target_k as f32, baseline_k as f32);
        #[allow(clippy::cast_possible_truncation)]
        let ev = 2f32.powf(exposure as f32);
        Self {
            gains: [wb[0] * ev, wb[1] * ev, wb[2] * ev],
            #[allow(clippy::cast_possible_truncation)]
            contrast: (params.value("contrast") / 100.0) as f32,
            #[allow(clippy::cast_possible_truncation)]
            highlights: (params.value("highlights") / 100.0) as f32,
            #[allow(clippy::cast_possible_truncation)]
            blacks: (params.value("blacks") / 100.0) as f32,
            #[allow(clippy::cast_possible_truncation)]
            saturation: (params.value("saturation") / 100.0) as f32,
            #[allow(clippy::cast_possible_truncation)]
            vibrance: (params.value("vibrance") / 100.0) as f32,
        }
    }

    /// 这一组参数需要跨通道的色度步骤吗（决定逐像素循环里要不要跑 [`apply_chroma`]）。
    #[must_use]
    pub fn needs_chroma(&self) -> bool {
        self.saturation.abs() > 1e-6 || self.vibrance.abs() > 1e-6
    }
}

/// **反差**：绕中灰 0.18 的分段幂，两端（0 与 1）固定。
///
/// `amount ∈ [−1, 1]` → 指数 `k = 2^amount ∈ [0.5, 2]`。
/// 分段保证 `0 → 0`、`1 → 1`（否则「反差 −100」会把整张图压成中灰）。
#[must_use]
pub fn contrast_curve(value: f32, amount: f32) -> f32 {
    if amount.abs() < 1e-6 || value <= 0.0 || value >= 1.0 {
        return value;
    }
    let k = 2f32.powf(amount);
    const PIVOT: f32 = 0.18;
    if value <= PIVOT {
        PIVOT * (value / PIVOT).powf(k)
    } else {
        1.0 - (1.0 - PIVOT) * ((1.0 - value) / (1.0 - PIVOT)).powf(k)
    }
}

/// **高光**：亮部的软调整，白点固定。
///
/// `y + h·y⁴(1−y)`：`h < 0` 压高光（找回细节），`h > 0` 提亮高光。
/// 导数 = `1 + h(4y³ − 5y⁴) ≥ 1 − |h|` ⇒ `|h| ≤ 1` 时**严格单调**（不会出现亮暗反转）。
#[must_use]
pub fn highlights_curve(value: f32, amount: f32) -> f32 {
    if amount.abs() < 1e-6 {
        return value;
    }
    let y4 = value * value * value * value;
    value + amount * y4 * (1.0 - value)
}

/// **黑区**：暗部的软调整，黑点固定。
///
/// `y + b·(1−y)⁴y`：`b < 0` 压暗黑区（更实的黑），`b > 0` 抬亮暗部。
/// 导数 = `1 + b(1−y)³(1−5y)`，`|b| ≤ 1` 时最小值 ≈ `0.78 > 0` ⇒ 单调。
#[must_use]
pub fn blacks_curve(value: f32, amount: f32) -> f32 {
    if amount.abs() < 1e-6 {
        return value;
    }
    let one_minus = 1.0 - value;
    let w = one_minus * one_minus * one_minus * one_minus * value;
    value + amount * w
}

/// 线性域那半条链：增益 → 反差 → 高光 → 黑区（**逐通道**，不含色度与显示变换）。
#[must_use]
pub fn chain_linear(linear: f32, gain: f32, resolved: &Resolved) -> f32 {
    let mut y = linear * gain;
    y = contrast_curve(y, resolved.contrast);
    y = highlights_curve(y, resolved.highlights);
    y = blacks_curve(y, resolved.blacks);
    y
}

/// 显示域那半条链：夹取 → sRGB OETF → RGB 曲线 → 通道曲线。
#[must_use]
pub fn encode_and_curve(linear: f32, channel: usize, curves: &CurveSet) -> f32 {
    let clamped = linear.clamp(0.0, 1.0);
    let encoded = linear_to_srgb(clamped);
    let composite = curves.rgb.eval(encoded);
    let per_channel = match channel {
        0 => curves.r.eval(composite),
        1 => curves.g.eval(composite),
        _ => curves.b.eval(composite),
    };
    per_channel.clamp(0.0, 1.0)
}

/// **参考实现**：逐像素走完整条链（f32，无 LUT 离散化）。
///
/// 生产路径不调它（慢），但它是**语义的定义**：测试向量对着它写，
/// 快路径对着它比。别把它删掉。
#[must_use]
pub fn map_pixel_exact(linear_rgb: [f32; 3], params: &DevelopParams) -> [f32; 3] {
    let resolved = Resolved::new(params);
    let curves = CurveSet::identity();
    map_pixel_exact_with(linear_rgb, &resolved, &curves)
}

/// 带曲线版本的参考实现（`map_pixel_exact` 的内部形态）。
#[must_use]
pub fn map_pixel_exact_with(linear_rgb: [f32; 3], resolved: &Resolved, curves: &CurveSet) -> [f32; 3] {
    let mut display = [0.0f32; 3];
    for channel in 0..3 {
        let linear = chain_linear(linear_rgb[channel], resolved.gains[channel], resolved);
        display[channel] = encode_and_curve(linear, channel, curves);
    }
    apply_chroma(display, resolved.saturation, resolved.vibrance)
}

/// **饱和度 / 自然饱和度**（显示参考域，绕亮度缩放色度）。
///
/// * 饱和度：`f = 1 + s`（−100 ⇒ 0 = 灰度，+100 ⇒ 2 倍）；
/// * 自然饱和度：低饱和的加得多 —— `f = 1 + v·(1 − 当前饱和度)`。
#[must_use]
pub fn apply_chroma(rgb: [f32; 3], saturation: f32, vibrance: f32) -> [f32; 3] {
    let mut out = rgb;
    if saturation.abs() > 1e-6 {
        let luma = luma_of(out);
        let factor = (1.0 + saturation).max(0.0);
        out = [
            luma + (out[0] - luma) * factor,
            luma + (out[1] - luma) * factor,
            luma + (out[2] - luma) * factor,
        ];
    }
    if vibrance.abs() > 1e-6 {
        let luma = luma_of(out);
        let max = out[0].max(out[1]).max(out[2]);
        let min = out[0].min(out[1]).min(out[2]);
        let current = if max > 1e-6 { (max - min) / max } else { 0.0 };
        let factor = (1.0 + vibrance * (1.0 - current)).max(0.0);
        out = [
            luma + (out[0] - luma) * factor,
            luma + (out[1] - luma) * factor,
            luma + (out[2] - luma) * factor,
        ];
    }
    [
        out[0].clamp(0.0, 1.0),
        out[1].clamp(0.0, 1.0),
        out[2].clamp(0.0, 1.0),
    ]
}

/// Rec.709 亮度（与 sRGB 的原色一致）。
#[must_use]
pub fn luma_of(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

/// LUT 的采样点数（输入域 0..1 上的等距采样，线性插值）。
///
/// 4096 段：曲线与调性都是低频函数，插值误差远小于 1/65535；
/// 而建表只要 3×4096 次标量求值（约 0.5ms），拖动时每帧重建得起。
const LUT_LEN: usize = 4097;

/// 三张通道 LUT：**线性 u16 输入 → 显示 u16 输出**（已含显示变换与曲线）。
///
/// 输出存 u16 而不是 u8：色度步骤要在这之后做，用 u8 会在饱和缩放时露出量化台阶。
struct ChannelLuts {
    tables: [Vec<u16>; 3],
}

impl ChannelLuts {
    fn build(resolved: &Resolved, curves: &CurveSet) -> Self {
        let mut tables: [Vec<u16>; 3] = [
            vec![0; LUT_LEN],
            vec![0; LUT_LEN],
            vec![0; LUT_LEN],
        ];
        #[allow(clippy::cast_precision_loss)]
        let denominator = (LUT_LEN - 1) as f32;
        for (channel, table) in tables.iter_mut().enumerate() {
            for (index, slot) in table.iter_mut().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                let linear = index as f32 / denominator;
                let y = chain_linear(linear, resolved.gains[channel], resolved);
                let display = encode_and_curve(y, channel, curves);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let encoded = (display * 65535.0 + 0.5) as u32;
                *slot = u16::try_from(encoded.min(65535)).unwrap_or(u16::MAX);
            }
        }
        Self { tables }
    }

    /// 查表（线性插值）→ 显示值 0..1。
    #[inline]
    fn lookup(&self, channel: usize, linear: u16) -> f32 {
        let table = &self.tables[channel];
        #[allow(clippy::cast_precision_loss)]
        let scaled = f32::from(linear) / 65535.0 * (LUT_LEN - 1) as f32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let index = scaled.floor() as usize;
        if index + 1 >= LUT_LEN {
            return f32::from(table[LUT_LEN - 1]) / 65535.0;
        }
        let fraction = scaled - index as f32;
        let a = f32::from(table[index]);
        let b = f32::from(table[index + 1]);
        (a + (b - a) * fraction) / 65535.0
    }
}

/// **生产路径**：线性源 → 8bit sRGB（行主序 RGB）。
///
/// 多线程按行切块（`std::thread::scope`，不引第三方依赖）：
/// 24MP 单线程要几百毫秒，切 8 块之后就落在「拖一下能跟得上」的范围里
/// （实测数字见实施记录）。
#[must_use]
pub fn render_rgb8(source: &LinearImage, params: &DevelopParams, curves: &CurveSet) -> Vec<u8> {
    let resolved = Resolved::new(params);
    let luts = ChannelLuts::build(&resolved, curves);
    let mut out = vec![0u8; source.rgb.len()];
    let width = source.width as usize;

    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(16);
    if threads <= 1 || source.height < 32 {
        map_rows(&source.rgb, &mut out, &luts, &resolved);
        return out;
    }

    let rows_per_chunk = (source.height as usize).div_ceil(threads);
    std::thread::scope(|scope| {
        let mut remaining_in = source.rgb.as_slice();
        let mut remaining_out = out.as_mut_slice();
        while !remaining_out.is_empty() {
            let rows = (remaining_out.len() / (width * 3)).min(rows_per_chunk).max(1);
            let take = rows * width * 3;
            let (chunk_out, rest_out) = remaining_out.split_at_mut(take);
            let (chunk_in, rest_in) = remaining_in.split_at(take);
            remaining_in = rest_in;
            remaining_out = rest_out;
            let luts_ref = &luts;
            let resolved_ref = &resolved;
            scope.spawn(move || map_rows(chunk_in, chunk_out, luts_ref, resolved_ref));
        }
    });
    out
}

/// 一段像素的逐像素映射（**唯一**的像素循环）。
fn map_rows(input: &[u16], output: &mut [u8], luts: &ChannelLuts, resolved: &Resolved) {
    debug_assert_eq!(input.len(), output.len());
    let chroma = resolved.needs_chroma();
    for (pixel_in, pixel_out) in input
        .as_chunks::<3>()
        .0
        .iter()
        .zip(output.as_chunks_mut::<3>().0.iter_mut())
    {
        let mut display = [
            luts.lookup(0, pixel_in[0]),
            luts.lookup(1, pixel_in[1]),
            luts.lookup(2, pixel_in[2]),
        ];
        if chroma {
            display = apply_chroma(display, resolved.saturation, resolved.vibrance);
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            pixel_out[0] = (display[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            pixel_out[1] = (display[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            pixel_out[2] = (display[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(values: &[(&str, f64)]) -> DevelopParams {
        let mut params = DevelopParams::new(None);
        for (id, value) in values {
            params.set(id, *value).expect("测试参数要合法");
        }
        params
    }

    fn linear(value: f32) -> u16 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let encoded = (value.clamp(0.0, 1.0) * 65535.0 + 0.5) as u32;
        u16::try_from(encoded.min(65535)).unwrap_or(u16::MAX)
    }

    fn image(pixels: &[[f32; 3]]) -> LinearImage {
        let mut rgb = Vec::new();
        for pixel in pixels {
            rgb.extend(pixel.iter().map(|v| linear(*v)));
        }
        LinearImage::new(pixels.len() as u32, 1, rgb).expect("形状对得上")
    }

    #[test]
    fn linear_image_rejects_bad_shapes() {
        assert!(LinearImage::new(2, 1, vec![0; 6]).is_some());
        assert!(LinearImage::new(2, 1, vec![0; 5]).is_none());
        assert!(LinearImage::new(0, 1, vec![]).is_none());
        assert!(LinearImage::new(1, 0, vec![]).is_none());
        assert!(LinearImage::new(2, 1, vec![0; 7]).is_none());
    }

    #[test]
    fn srgb8_round_trips_through_linear() {
        // 8bit sRGB → 线性 → （默认参数）→ 8bit sRGB 应当回到原值（±1）
        let source: Vec<u8> = (0..=255).collect();
        let rgb8: Vec<u8> = source.iter().flat_map(|v| [*v, *v, *v]).collect();
        let linear_image = LinearImage::from_srgb8(256, 1, &rgb8).expect("形状对");
        let params = DevelopParams::new(None);
        let out = render_rgb8(&linear_image, &params, &CurveSet::identity());
        for (index, value) in source.iter().enumerate() {
            let got = out[index * 3];
            assert!(
                i16::from(got) - i16::from(*value) <= 1 && i16::from(*value) - i16::from(got) <= 1,
                "8bit {value} 往返成了 {got}"
            );
        }
    }

    #[test]
    fn default_params_are_a_no_op_on_linear_values() {
        // 线性 0.18（中灰）在默认参数下应当输出 sRGB 0.4613（= 118/255）
        let source = image(&[[0.18, 0.18, 0.18]]);
        let params = DevelopParams::new(None);
        let out = render_rgb8(&source, &params, &CurveSet::identity());
        assert!(
            (i16::from(out[0]) - 118).abs() <= 1,
            "中灰应当落在 118，实测 {}",
            out[0]
        );
    }

    /// **外部给定的测试向量**：一组输入 + 参数 → 期望输出（手算/独立推理得出）。
    ///
    /// 这些数字不是「跑一遍记下来」，而是按链的定义算出来的：
    /// 曝光 ±1 EV 是 ×2 / ÷2，白平衡在中性色上不动，反差绕 0.18，等等。
    #[test]
    fn external_vectors_pin_the_math() {
        // ① 曝光 +1 EV：线性 0.18 → 0.36 → sRGB = 1.055·0.36^(1/2.4) − 0.055 = 0.634253 → 162
        //    （外部给定：这三个数是拿定义式手算出来的，不是跑一遍记下来的）
        let out = render_rgb8(
            &image(&[[0.18, 0.18, 0.18]]),
            &params(&[("exposure", 1.0)]),
            &CurveSet::identity(),
        );
        assert!((i16::from(out[0]) - 162).abs() <= 1, "曝光 +1EV 实测 {}", out[0]);

        // ② 曝光 −1 EV：0.18 → 0.09 → sRGB = 0.331830 → 85
        let out = render_rgb8(
            &image(&[[0.18, 0.18, 0.18]]),
            &params(&[("exposure", -1.0)]),
            &CurveSet::identity(),
        );
        assert!((i16::from(out[0]) - 85).abs() <= 1, "曝光 −1EV 实测 {}", out[0]);

        // ③ 色温停在**基线**（元数据读不到时 = 表里的 6250K）时中性色不许被染色
        //    （这是「不动就不变」那条保证在管线上的样子；基线不是 6500K，别弄错）
        let baseline = DevelopParams::new(None).baseline("temperature");
        let out = render_rgb8(
            &image(&[[0.18, 0.18, 0.18]]),
            &params(&[("temperature", baseline)]),
            &CurveSet::identity(),
        );
        assert_eq!(out[0], out[1], "停在基线上中性色不该染色（R≠G）");
        assert_eq!(out[1], out[2], "停在基线上中性色不该染色（G≠B）");

        // ③b 拖到高 K（说「当时的光很蓝」）⇒ 画面被暖回来：R > B
        let out = render_rgb8(
            &image(&[[0.18, 0.18, 0.18]]),
            &params(&[("temperature", 10000.0)]),
            &CurveSet::identity(),
        );
        assert!(out[0] > out[2], "高 K 应当 R > B（画面变暖），实测 {out:?}");

        // ④ 拖到低 K（说「当时的光是暖的」）会把中性灰染成偏蓝：B > R
        let out = render_rgb8(
            &image(&[[0.18, 0.18, 0.18]]),
            &params(&[("temperature", 3000.0)]),
            &CurveSet::identity(),
        );
        assert!(out[2] > out[0], "低 K 应当 B > R（画面变冷），实测 {out:?}");

        // ⑤ 饱和度 −100 ⇒ 三通道相等（灰度）
        let out = render_rgb8(
            &image(&[[0.2, 0.1, 0.05]]),
            &params(&[("saturation", -100.0)]),
            &CurveSet::identity(),
        );
        assert_eq!(out[0], out[1]);
        assert_eq!(out[1], out[2]);

        // ⑥ 饱和度 +100 ⇒ 色差翻倍（同一亮度下 R−G 应当更大）
        let base = render_rgb8(
            &image(&[[0.2, 0.1, 0.05]]),
            &DevelopParams::new(None),
            &CurveSet::identity(),
        );
        let boosted = render_rgb8(
            &image(&[[0.2, 0.1, 0.05]]),
            &params(&[("saturation", 100.0)]),
            &CurveSet::identity(),
        );
        assert!(
            i16::from(boosted[0]) - i16::from(boosted[1])
                > i16::from(base[0]) - i16::from(base[1]),
            "饱和 +100 应当拉开 R−G：base={base:?} boosted={boosted:?}"
        );

        // ⑦ 反差 +100：中灰以下压暗、中灰以上提亮（两端固定）
        let low = render_rgb8(
            &image(&[[0.05, 0.05, 0.05]]),
            &params(&[("contrast", 100.0)]),
            &CurveSet::identity(),
        );
        let base_low = render_rgb8(
            &image(&[[0.05, 0.05, 0.05]]),
            &DevelopParams::new(None),
            &CurveSet::identity(),
        );
        assert!(low[0] < base_low[0], "反差拉高应当压暗暗部");
        let high = render_rgb8(
            &image(&[[0.8, 0.8, 0.8]]),
            &params(&[("contrast", 100.0)]),
            &CurveSet::identity(),
        );
        let base_high = render_rgb8(
            &image(&[[0.8, 0.8, 0.8]]),
            &DevelopParams::new(None),
            &CurveSet::identity(),
        );
        assert!(high[0] > base_high[0], "反差拉高应当提亮亮部");
        // 两端固定
        assert_eq!(contrast_curve(0.0, 1.0), 0.0);
        assert_eq!(contrast_curve(1.0, 1.0), 1.0);

        // ⑧ 高光 −100 只压亮部、不动黑点
        assert!(highlights_curve(0.8, -1.0) < 0.8);
        assert!((highlights_curve(0.05, -1.0) - 0.05).abs() < 1e-4);
        assert_eq!(highlights_curve(0.0, -1.0), 0.0);
        // ⑨ 黑区 +100 只抬暗部、不动白点
        assert!(blacks_curve(0.05, 1.0) > 0.05);
        assert!((blacks_curve(0.8, 1.0) - 0.8).abs() < 0.02);
        assert_eq!(blacks_curve(1.0, 1.0), 1.0);
    }

    #[test]
    fn tone_curves_are_monotone() {
        for amount in [-1.0f32, -0.5, 0.5, 1.0] {
            // 三条曲线各自跟自己的上一个值比 —— 它们量级不同，混在一起比没有意义
            let mut previous = [f32::NEG_INFINITY; 3];
            for step in 0..=1000 {
                let x = step as f32 / 1000.0;
                let values = [
                    contrast_curve(x, amount),
                    highlights_curve(x, amount),
                    blacks_curve(x, amount),
                ];
                for (index, y) in values.iter().enumerate() {
                    assert!(
                        *y >= previous[index] - 1e-5,
                        "amount={amount} x={x} 第 {index} 条不单调：{y} < {}",
                        previous[index]
                    );
                    previous[index] = *y;
                }
            }
        }
    }

    #[test]
    fn lut_path_matches_the_exact_path() {
        // 快路径（LUT）与精确路径必须一致到 1 级 8bit 之内 —— 覆盖
        // 默认 / 各单项拉满 / 组合 / 曲线 几类参数
        let curve = CurveSet::identity();
        let mut curved = CurveSet::identity();
        curved.set_channel(
            super::super::curve::CurveChannel::Rgb,
            super::super::curve::Curve::identity().with_point(0.5, 0.65).unwrap(),
        );
        curved.set_channel(
            super::super::curve::CurveChannel::Blue,
            super::super::curve::Curve::identity().with_point(0.3, 0.2).unwrap(),
        );
        let cases: Vec<(DevelopParams, &CurveSet)> = vec![
            (DevelopParams::new(None), &curve),
            (params(&[("exposure", 0.7)]), &curve),
            (params(&[("contrast", 60.0)]), &curve),
            (params(&[("highlights", -80.0)]), &curve),
            (params(&[("blacks", 70.0)]), &curve),
            (params(&[("temperature", 3200.0)]), &curve),
            (params(&[("temperature", 9000.0)]), &curve),
            (params(&[("saturation", 60.0)]), &curve),
            (params(&[("vibrance", -50.0)]), &curve),
            (
                params(&[
                    ("exposure", -0.5),
                    ("contrast", 40.0),
                    ("highlights", -40.0),
                    ("blacks", 30.0),
                    ("temperature", 4000.0),
                    ("saturation", 25.0),
                    ("vibrance", 40.0),
                ]),
                &curve,
            ),
            (params(&[("exposure", 0.3)]), &curved),
        ];

        let mut samples: Vec<[f32; 3]> = Vec::new();
        for step in 0..=255 {
            let v = step as f32 / 255.0;
            samples.push([v, v, v]);
        }
        for (r, g, b) in [
            (0.0, 0.0, 0.0),
            (1.0, 1.0, 1.0),
            (0.18, 0.18, 0.18),
            (0.02, 0.05, 0.2),
            (0.9, 0.4, 0.1),
            (0.3, 0.7, 0.15),
            (0.5, 0.25, 0.75),
            (0.001, 0.002, 0.003),
            (0.999, 0.998, 1.0),
        ] {
            samples.push([r, g, b]);
        }

        for (params, curves) in &cases {
            let resolved = Resolved::new(params);
            let source = image(&samples);
            let fast = render_rgb8(&source, params, curves);
            for (index, sample) in samples.iter().enumerate() {
                let exact = map_pixel_exact_with(*sample, &resolved, curves);
                for channel in 0..3 {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let expected = (exact[channel].clamp(0.0, 1.0) * 255.0 + 0.5) as i16;
                    let got = i16::from(fast[index * 3 + channel]);
                    assert!(
                        (expected - got).abs() <= 1,
                        "参数 {params:?} 样本 {sample:?} 通道 {channel}：精确 {expected} / LUT {got}"
                    );
                }
            }
        }
    }

    #[test]
    fn parallel_and_single_thread_agree() {
        // 造一张够大的图（>32 行才会走多线程），两种路径的输出必须逐字节一致
        let width = 64u32;
        let height = 96u32;
        let mut rgb = Vec::new();
        for y in 0..height {
            for x in 0..width {
                #[allow(clippy::cast_precision_loss)]
                let r = (x as f32) / 64.0;
                #[allow(clippy::cast_precision_loss)]
                let g = (y as f32) / 96.0;
                rgb.extend([linear(r), linear(g), linear(0.5)]);
            }
        }
        let source = LinearImage::new(width, height, rgb).expect("形状对");
        let params = params(&[("exposure", 0.25), ("saturation", 30.0)]);
        let curves = CurveSet::identity();
        let parallel = render_rgb8(&source, &params, &curves);
        let resolved = Resolved::new(&params);
        let luts = ChannelLuts::build(&resolved, &curves);
        let mut single = vec![0u8; source.rgb.len()];
        map_rows(&source.rgb, &mut single, &luts, &resolved);
        assert_eq!(parallel, single, "多线程与单线程的输出必须一致");
    }

    #[test]
    fn downscale_keeps_aspect_and_never_zero() {
        let rgb = vec![30000u16; 4000 * 1000 * 3];
        let source = LinearImage::new(4000, 1000, rgb).expect("形状对");
        let small = source.downscaled_to(1000);
        assert_eq!((small.width, small.height), (1000, 250));
        assert!(small.is_consistent());
        assert!(small.rgb.iter().all(|v| *v == 30000), "纯色缩完还是纯色");
        // 小图不放大
        let tiny = source.downscaled_to(9999);
        assert_eq!((tiny.width, tiny.height), (4000, 1000));
        // 极端窄图：短边至少 1 像素
        let thin_rgb = vec![0u16; 6000 * 10 * 3];
        let thin = LinearImage::new(6000, 10, thin_rgb).expect("形状对");
        let scaled = thin.downscaled_to(1000);
        assert!(scaled.height >= 1);
    }

    #[test]
    fn downscale_is_an_area_average() {
        // 2×2 → 1×1：四个像素求平均（1000+2000+3000+4000）/4 = 2500
        let rgb = vec![1000u16, 1000, 1000, 2000, 2000, 2000, 3000, 3000, 3000, 4000, 4000, 4000];
        let source = LinearImage::new(2, 2, rgb).expect("形状对");
        let small = source.downscaled_to(1);
        assert_eq!((small.width, small.height), (1, 1));
        assert_eq!(small.rgb, vec![2500, 2500, 2500]);

        // 4×1 → 2×1：左半平均 (0+65535)/2 = 32768（四舍五入），右半同理
        let rgb = vec![0u16, 0, 0, 65535, 65535, 65535, 65535, 65535, 65535, 0, 0, 0];
        let source = LinearImage::new(4, 1, rgb).expect("形状对");
        let small = source.downscaled_to(2);
        assert_eq!((small.width, small.height), (2, 1));
        assert_eq!(small.rgb[0], 32768);
        assert_eq!(small.rgb[3], 32768);
    }
}
