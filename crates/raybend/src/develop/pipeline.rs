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
//!   → 16bit（显示时最后量化成 8bit） 量化
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
use super::local_tone::{LocalToneRows, LocalToneState};
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
        let expected = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(3)?;
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
            && (self.width as usize)
                .checked_mul(self.height as usize)
                .and_then(|n| n.checked_mul(3))
                == Some(self.rgb.len())
    }

    /// 8bit **sRGB 编码**的像素 → 线性（JPEG / RAW 内嵌预览那条路）。
    ///
    /// 这一步是有损的（8bit 显示数据里暗部本来就没有更多信息），
    /// 所以这条路的输入只能算「准线性」—— 实施记录与 `FUTURE.md` C7 都记着这件事。
    #[must_use]
    pub fn from_srgb8(width: u32, height: u32, rgb8: &[u8]) -> Option<Self> {
        let expected = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(3)?;
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

    /// High precision sRGB source (PNG/TIFF), retaining 16-bit samples before linearization.
    #[must_use]
    pub fn from_srgb16(width: u32, height: u32, samples: &[u16]) -> Option<Self> {
        let expected = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(3)?;
        if width == 0 || height == 0 || samples.len() != expected {
            return None;
        }
        static TABLE: std::sync::OnceLock<Vec<u16>> = std::sync::OnceLock::new();
        let table = TABLE.get_or_init(|| {
            (0..=u16::MAX)
                .map(|v| {
                    (super::color::srgb_to_linear(f32::from(v) / 65535.0) * 65535.0).round() as u16
                })
                .collect()
        });
        Self::new(
            width,
            height,
            samples.iter().map(|v| table[*v as usize]).collect(),
        )
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
                    out[(y * width as usize + x) * 3 + channel] =
                        ((sum + count / 2) / count) as u16;
                }
            }
        }

        Self {
            width,
            height,
            rgb: out,
        }
    }

    /// 从 **RAW 线性解码结果**建一张线性图（**按 EXIF 方向摆正**）。
    ///
    /// 位图那条路的等价物是 `display::pixels`（它已经摆正）→ [`Self::from_srgb8`]；
    /// RAW 这条路的解码结果**没摆正**（方向只是元数据），所以规则落在这里 ——
    /// 编辑侧曾经漏掉这一步（竖拍 RAW 进编辑就是横的，2026-09-24 人类报的）。
    ///
    /// 形状对不上返回 `None`（不猜、不补零）。
    #[must_use]
    pub fn from_raw16(source: crate::raw::backend::RawImage16) -> Option<Self> {
        let image = Self::new(source.width, source.height, source.rgb)?;
        Some(match source.orientation {
            Some(value) if value != 1 => image.oriented(value),
            _ => image,
        })
    }

    /// 按 **EXIF 方向**（1–8）摆正像素；`1` / 未知值原样返回。
    ///
    /// # 为什么要烤进像素，而不是在视口里转
    ///
    /// 方向是**像素事实**，不是显示选项：显影管线、裁剪、覆盖层、`1:1` 看到的都应该是
    /// 「用户实际看到的那张图」。烤进像素之后，下游（视口/覆盖层/命中测试）一份代码不用改。
    ///
    /// 编辑侧曾经**完全没做这一步**（缩略图/看图那条路有）——
    /// 症状是竖拍 RAW 进编辑就是横的（2026-09-24 人类报的）。
    ///
    /// # 规则与缩略图那条路**逐字一致**
    ///
    /// 与 `thumbnail::render::apply_orientation`（`image` crate 的 `fliph` / `rotate90`…）
    /// 同一张表；`pipeline.rs` 的测试拿 8bit 图**交叉验证**两份实现 —— 不许各走各的。
    #[must_use]
    pub fn oriented(&self, orientation: u16) -> Self {
        let (w, h) = (self.width, self.height);
        // 90°/270° 类的四个方向宽高互换
        let swap = matches!(orientation, 5..=8);
        let (out_w, out_h) = if swap { (h, w) } else { (w, h) };
        let mut out = vec![0u16; self.rgb.len()];
        // 按**目标**坐标遍历（旋转类方向的映射是「目标取源」，反过来算会越界）
        for dy in 0..out_h {
            for dx in 0..out_w {
                // 目标像素 (dx,dy) 取源像素 (sx,sy)；表与 `apply_orientation` 同源
                let (sx, sy) = match orientation {
                    2 => (w - 1 - dx, dy),         // 水平镜像
                    3 => (w - 1 - dx, h - 1 - dy), // 180°
                    4 => (dx, h - 1 - dy),         // 垂直镜像
                    5 => (dy, dx),                 // 转置
                    6 => (dy, h - 1 - dx),         // 顺时针 90°
                    7 => (w - 1 - dy, h - 1 - dx), // 反转置
                    8 => (w - 1 - dy, dx),         // 逆时针 90°
                    _ => (dx, dy),
                };
                let src = (sy as usize * w as usize + sx as usize) * 3;
                let dst = (dy as usize * out_w as usize + dx as usize) * 3;
                out[dst..dst + 3].copy_from_slice(&self.rgb[src..src + 3]);
            }
        }
        Self {
            width: out_w,
            height: out_h,
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

    /// **线性链的指纹**：这几项变了才需要重算局部色调映射的分析。
    ///
    /// 色度（饱和 / 自然饱和）与曲线**不在**里面 —— 它们作用在显示参考域，
    /// 不影响分析所看到的那张图（那张图是「链完、显示变换之前」的线性图）。
    #[must_use]
    pub fn chain_key(&self) -> [f32; 6] {
        [
            self.gains[0],
            self.gains[1],
            self.gains[2],
            self.contrast,
            self.highlights,
            self.blacks,
        ]
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
    if amount.abs() < 1e-6 || !(0.0..=1.0).contains(&value) {
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
    if amount.abs() < 1e-6 || !(0.0..=1.0).contains(&value) {
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
    let based = curves.base.eval(encoded);
    let composite = curves.rgb.eval(based);
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
pub fn map_pixel_exact_with(
    linear_rgb: [f32; 3],
    resolved: &Resolved,
    curves: &CurveSet,
) -> [f32; 3] {
    let mut display = [0.0f32; 3];
    for channel in 0..3 {
        let linear = chain_linear(linear_rgb[channel], resolved.gains[channel], resolved);
        display[channel] = encode_and_curve(linear, channel, curves);
    }
    if resolved.highlights > 0.0 {
        let baseline = Resolved {
            highlights: 0.0,
            ..*resolved
        };
        let unadjusted = std::array::from_fn(|channel| {
            encode_and_curve(
                chain_linear(linear_rgb[channel], baseline.gains[channel], &baseline),
                channel,
                curves,
            )
        });
        display = neutralize_positive_highlights(display, unadjusted);
    }
    apply_chroma(display, resolved.saturation, resolved.vibrance)
}

/// 正向高光只改变亮度，避免亮蓝通道因逐通道曲线比红绿多增益而泛蓝。
/// 已到显示白点的像素没有可恢复的亮度空间，保持原色，避免裁剪后偏色。
fn neutralize_positive_highlights(adjusted: [f32; 3], baseline: [f32; 3]) -> [f32; 3] {
    if baseline.iter().any(|value| *value >= 1.0 - 1e-5) {
        return baseline;
    }
    let delta = (luma_of(adjusted) - luma_of(baseline)).max(0.0);
    baseline.map(|value| (value + delta).clamp(0.0, 1.0))
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
        let current = if max > 1e-6 {
            ((max - min) / max).clamp(0.0, 1.0)
        } else {
            0.0
        };
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

/// **把线性域那半条链作用到整张图**（分析用）。
///
/// 局部色调映射要在「用户已经放好的画面」上估分位数（曝光 / 反差 / 高光 / 黑区都算完了），
/// 而分析图通常只有全图的 1/4 —— 所以这里不做按行并行（1.5MP 单线程就够），
/// 而且**与 `render_rgb8` 共用同一组 `chain_linear`/`Resolved`**，不是第二份实现。
#[must_use]
pub fn chain_image(source: &LinearImage, params: &DevelopParams) -> LinearImage {
    let resolved = Resolved::new(params);
    let mut out = source.clone();
    for pixel in out.rgb.as_chunks_mut::<3>().0 {
        for (channel, value) in pixel.iter_mut().enumerate() {
            let linear = f32::from(*value) / 65535.0;
            let chained = chain_linear(linear, resolved.gains[channel], &resolved);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let encoded = (chained.clamp(0.0, 1.0) * 65535.0 + 0.5) as u32;
            *value = u16::try_from(encoded.min(65535)).unwrap_or(u16::MAX);
        }
    }
    out
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
        let mut tables: [Vec<u16>; 3] = [vec![0; LUT_LEN], vec![0; LUT_LEN], vec![0; LUT_LEN]];
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

/// **生产路径**：线性源 → 16bit sRGB（行主序 RGB）。
///
/// 多线程按行切块（`std::thread::scope`，不引第三方依赖）：
/// 24MP 单线程要几百毫秒，切 8 块之后就落在「拖一下能跟得上」的范围里
/// （实测数字见实施记录）。
#[must_use]
pub fn render_rgb16(source: &LinearImage, params: &DevelopParams, curves: &CurveSet) -> Vec<u16> {
    let resolved = Resolved::new(params);
    let luts = ChannelLuts::build(&resolved, curves);
    let baseline_luts = (resolved.highlights > 0.0).then(|| {
        ChannelLuts::build(
            &Resolved {
                highlights: 0.0,
                ..resolved
            },
            curves,
        )
    });
    let mut out = vec![0u16; source.rgb.len()];
    let width = source.width as usize;

    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(16);
    if threads <= 1 || source.height < 32 {
        map_rows(
            &source.rgb,
            &mut out,
            &luts,
            baseline_luts.as_ref(),
            &resolved,
        );
        return out;
    }

    let rows_per_chunk = (source.height as usize).div_ceil(threads);
    std::thread::scope(|scope| {
        let mut remaining_in = source.rgb.as_slice();
        let mut remaining_out = out.as_mut_slice();
        while !remaining_out.is_empty() {
            let rows = (remaining_out.len() / (width * 3))
                .min(rows_per_chunk)
                .max(1);
            let take = rows * width * 3;
            let (chunk_out, rest_out) = remaining_out.split_at_mut(take);
            let (chunk_in, rest_in) = remaining_in.split_at(take);
            remaining_in = rest_in;
            remaining_out = rest_out;
            let luts_ref = &luts;
            let baseline_ref = baseline_luts.as_ref();
            let resolved_ref = &resolved;
            scope
                .spawn(move || map_rows(chunk_in, chunk_out, luts_ref, baseline_ref, resolved_ref));
        }
    });
    out
}

/// 一段像素的逐像素映射（**唯一**的像素循环）。
fn map_rows(
    input: &[u16],
    output: &mut [u16],
    luts: &ChannelLuts,
    baseline_luts: Option<&ChannelLuts>,
    resolved: &Resolved,
) {
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
        if let Some(baseline) = baseline_luts {
            let original = [
                baseline.lookup(0, pixel_in[0]),
                baseline.lookup(1, pixel_in[1]),
                baseline.lookup(2, pixel_in[2]),
            ];
            display = neutralize_positive_highlights(display, original);
        }
        if chroma {
            display = apply_chroma(display, resolved.saturation, resolved.vibrance);
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            pixel_out[0] = (display[0].clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
            pixel_out[1] = (display[1].clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
            pixel_out[2] = (display[2].clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
        }
    }
}

/// 线性链的 LUT（**融合路径用**）：线性 u16 → 链过之后的线性 f32。
///
/// 输出**不夹**：曝光 +2EV 会把高光推到 1.0 以上，而局部色调映射要在夹取之前看到它。
/// 存 f32 而不是 u16：这里的输出还要过 `log2`，定点量化会在暗部变成可见的色带。
struct LinearChainLuts {
    tables: [Vec<f32>; 3],
}

impl LinearChainLuts {
    fn build(resolved: &Resolved) -> Self {
        let mut tables: [Vec<f32>; 3] = [
            vec![0f32; LUT_LEN],
            vec![0f32; LUT_LEN],
            vec![0f32; LUT_LEN],
        ];
        #[allow(clippy::cast_precision_loss)]
        let denominator = (LUT_LEN - 1) as f32;
        for (channel, table) in tables.iter_mut().enumerate() {
            for (index, slot) in table.iter_mut().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                let linear = index as f32 / denominator;
                *slot = chain_linear(linear, resolved.gains[channel], resolved);
            }
        }
        Self { tables }
    }

    /// 查表（线性插值）。
    #[inline]
    fn lookup(&self, channel: usize, linear: u16) -> f32 {
        let table = &self.tables[channel];
        #[allow(clippy::cast_precision_loss)]
        let scaled = f32::from(linear) / 65535.0 * (LUT_LEN - 1) as f32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let index = scaled.floor() as usize;
        if index + 1 >= LUT_LEN {
            return table[LUT_LEN - 1];
        }
        let fraction = scaled - index as f32;
        table[index] + (table[index + 1] - table[index]) * fraction
    }
}

/// 显示 LUT（**融合路径用**）：线性（0..1，越界夹）→ 显示值 0..1（含 sRGB OETF 与曲线）。
///
/// 存 f32 而不是 u8：融合路径的色度步骤（饱和度 / 自然饱和度）在这之后做，
/// 用 u8 会在缩放时露出量化台阶（与 `ChannelLuts` 存 u16 同一个理由）。
struct DisplayLuts {
    tables: [Vec<f32>; 3],
}

impl DisplayLuts {
    fn build(curves: &CurveSet) -> Self {
        let mut tables: [Vec<f32>; 3] = [
            vec![0f32; LUT_LEN],
            vec![0f32; LUT_LEN],
            vec![0f32; LUT_LEN],
        ];
        #[allow(clippy::cast_precision_loss)]
        let denominator = (LUT_LEN - 1) as f32;
        for (channel, table) in tables.iter_mut().enumerate() {
            for (index, slot) in table.iter_mut().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                let linear = index as f32 / denominator;
                *slot = encode_and_curve(linear, channel, curves);
            }
        }
        Self { tables }
    }

    /// 查表（线性插值）；输入越界时夹到两端。
    #[inline]
    fn lookup(&self, channel: usize, linear: f32) -> f32 {
        let table = &self.tables[channel];
        if !linear.is_finite() || linear <= 0.0 {
            return table[0];
        }
        if linear >= 1.0 {
            return table[LUT_LEN - 1];
        }
        let scaled = linear * (LUT_LEN - 1) as f32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let index = scaled.floor() as usize;
        if index + 1 >= LUT_LEN {
            return table[LUT_LEN - 1];
        }
        let fraction = scaled - index as f32;
        table[index] + (table[index + 1] - table[index]) * fraction
    }
}

/// **生产路径（带动态反差）**：线性源 → 局部色调映射 → 16bit sRGB。
///
/// `local = None` 或强度 ≤ 0 时**原样走 [`render_rgb8`]** —— 拉杆在 0 位时这个功能
/// 等于不存在（逐位一致，且不多花一分钱）。
///
/// 为什么是**融合的一趟**而不是「先算中间图再渲染」：24MP 的中间线性图是 144MB 的
/// 内存往返，而这条链本来就是逐像素的 —— 合成一趟就只需读一次源、写一次显示像素。
#[must_use]
pub fn render_rgb16_with_local_tone(
    source: &LinearImage,
    params: &DevelopParams,
    curves: &CurveSet,
    local: Option<(&LocalToneState, f32)>,
) -> Vec<u16> {
    let Some((state, strength)) = local else {
        return render_rgb16(source, params, curves);
    };
    if strength <= 0.0 || strength.is_nan() {
        return render_rgb16(source, params, curves);
    }
    let resolved = Resolved::new(params);
    let baseline_chain = (resolved.highlights > 0.0).then(|| {
        LinearChainLuts::build(&Resolved {
            highlights: 0.0,
            ..resolved
        })
    });
    let pass = LocalPass {
        state,
        strength,
        chain: &LinearChainLuts::build(&resolved),
        baseline_chain: baseline_chain.as_ref(),
        display: &DisplayLuts::build(curves),
        resolved: &resolved,
        chroma: resolved.needs_chroma(),
        width: source.width as usize,
        height: source.height as usize,
    };
    let mut out = vec![0u16; source.rgb.len()];
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(16);
    if threads <= 1 || pass.height < 32 {
        pass.run(&source.rgb, &mut out, 0);
        return out;
    }
    let rows_per_chunk = pass.height.div_ceil(threads);
    std::thread::scope(|scope| {
        let mut remaining_in = source.rgb.as_slice();
        let mut remaining_out = out.as_mut_slice();
        let mut first_row = 0usize;
        while !remaining_out.is_empty() {
            let rows = (remaining_out.len() / (pass.width * 3))
                .min(rows_per_chunk)
                .max(1);
            let take = rows * pass.width * 3;
            let (chunk_out, rest_out) = remaining_out.split_at_mut(take);
            let (chunk_in, rest_in) = remaining_in.split_at(take);
            remaining_in = rest_in;
            remaining_out = rest_out;
            let pass_ref = &pass;
            let start = first_row;
            first_row += rows;
            scope.spawn(move || pass_ref.run(chunk_in, chunk_out, start));
        }
    });
    out
}

/// **显影的全部可选阶段**（M3-W4）：镜头校正 / 降噪 / 动态反差 / 锐化。
///
/// 顺序**不能随便改**（改顺序 = 改画质，见 `specs/M3-W4.md` §1.3）：
///
/// ```text
/// 线性源 → ① 镜头校正（暗角 + 畸变 + TCA，**一次重采样**）
///         → ② 降噪（亮度 / 色度）
///         → ③ 逐像素链（WB → 曝光 → 反差 → 高光 → 黑区 → 显示变换 → 曲线 → 色度 → 动态反差）
///         → ④ 锐化（显示域，就地）
///         → 16bit（显示时最后量化成 8bit）
/// ```
///
/// 每一项都是 `Option`，且**每一项关掉时逐位恒等**（各阶段的单测钉着这一条）；
/// 全部关掉时 [`render_develop`] 与 [`render_rgb8`] **逐位一致**。
#[derive(Default)]
pub struct DevelopStages<'a> {
    /// 镜头校正（映射是恒等时整趟跳过）
    pub lens: Option<&'a super::lens::LensMap>,
    /// 同步调用的降噪方式；编辑器高质量帧由后台先算好再进入后续阶段。
    pub nr_method: super::denoise::NrMethod,
    /// 降噪
    pub denoise: Option<&'a super::denoise::DenoisePlan>,
    /// 动态反差（W3 已有的阶段）
    pub local_tone: Option<(&'a LocalToneState, f32)>,
    /// 锐化（显示域，**就地**改 16bit 输出）
    pub sharpen: Option<&'a super::sharpen::SharpenPlan>,
    /// 选中的创意 LUT；在显示域最后应用。
    pub lut: Option<&'a super::lut::Lut>,
}

/// **从编辑参数里解析出来的可选阶段**（显影线程与缩略图**共用一份**，免得两处各拼一遍 ——
/// 两处拼两遍的话，以后加一个参数就会漏一处）。
///
/// 镜头那一块只有**手动微调**：配置文件（lensfun）由 `crate::lens` 解析后补进来，
/// 不在本函数职责内（本模块不认识数据库，见 `develop/lens.rs` 的分层说明）。
#[derive(Debug, Clone, PartialEq)]
pub struct DevelopPlans {
    pub denoise: super::denoise::DenoisePlan,
    pub sharpen: super::sharpen::SharpenPlan,
    pub lens: super::lens::LensCorrection,
    /// 动态反差强度（`0..1`）
    pub local_tone: f32,
}

impl DevelopPlans {
    /// 从参数表 + 图像尺寸建。
    #[must_use]
    pub fn from_params(width: u32, height: u32, params: &DevelopParams) -> Self {
        Self {
            denoise: super::denoise::DenoisePlan::from_sliders(
                params.value("lumaNr"),
                params.value("colorNr"),
            ),
            sharpen: super::sharpen::SharpenPlan::from_sliders(
                params.value("sharpenAmount"),
                params.value("sharpenRadius"),
            ),
            lens: super::lens::LensCorrection::manual_only(
                width,
                height,
                super::lens::ManualLens::from_sliders(
                    params.value("distortion"),
                    params.value("vignette"),
                    params.value("chromatic"),
                    params.value("vignetteRange"),
                    params.value("chromaticBlue"),
                ),
            ),
            #[allow(clippy::cast_possible_truncation)]
            local_tone: (params.value("dynamicContrast") / 100.0).clamp(0.0, 1.0) as f32,
        }
    }
}

/// **生产入口**：线性源 → 16bit sRGB（含全部可选阶段）。
///
/// 逐像素数学仍然只有 [`render_rgb8_with_local_tone`] 那一份 —— 这里只负责把
/// **空间阶段**（镜头 / 降噪 / 锐化）按固定顺序串起来，**不重复实现链上的任何数学**。
#[must_use]
pub fn render_develop16(
    source: &LinearImage,
    params: &DevelopParams,
    curves: &CurveSet,
    stages: &DevelopStages<'_>,
) -> Vec<u16> {
    let warped = stages
        .lens
        .filter(|map| !map.is_identity())
        .map(|map| super::lens::warp_lens(source, map));
    let source = warped.as_ref().unwrap_or(source);
    let denoised = stages
        .denoise
        .filter(|plan| !plan.is_identity())
        .map(|plan| match stages.nr_method {
            super::denoise::NrMethod::Fast => super::denoise::denoise_fast(source, plan),
            super::denoise::NrMethod::High => {
                super::bm3d::denoise_high(source, plan, &std::sync::atomic::AtomicBool::new(false))
                    .expect("not cancelled")
            }
        });
    let source = denoised.as_ref().unwrap_or(source);
    let mut out = render_rgb16_with_local_tone(source, params, curves, stages.local_tone);
    if let Some(plan) = stages.sharpen.filter(|plan| !plan.is_identity()) {
        super::sharpen::sharpen_display(&mut out, source.width, source.height, plan);
    }
    if let Some(lut) = stages.lut {
        lut.apply_rgb(&mut out, 1.0)
            .expect("管线输出必为 RGB 三元组");
    }
    out
}

/// 融合路径的一趟像素循环（与 `map_rows` 并列，**不是**它的替代）。
struct LocalPass<'a> {
    state: &'a LocalToneState,
    strength: f32,
    chain: &'a LinearChainLuts,
    baseline_chain: Option<&'a LinearChainLuts>,
    display: &'a DisplayLuts,
    resolved: &'a Resolved,
    chroma: bool,
    width: usize,
    height: usize,
}

impl LocalPass<'_> {
    fn run(&self, input: &[u16], output: &mut [u16], first_row: usize) {
        debug_assert_eq!(input.len(), output.len());
        let mut rows = LocalToneRows::new(
            self.state,
            self.strength,
            self.width as u32,
            self.height as u32,
        );
        let input_rows = input.as_chunks::<3>().0.chunks(self.width);
        let output_rows = output.as_chunks_mut::<3>().0.chunks_mut(self.width);
        for (local_row, (line_in, line_out)) in input_rows.zip(output_rows).enumerate() {
            rows.set_row(first_row + local_row);
            for (x, (pixel_in, pixel_out)) in line_in.iter().zip(line_out.iter_mut()).enumerate() {
                let chained = [
                    self.chain.lookup(0, pixel_in[0]),
                    self.chain.lookup(1, pixel_in[1]),
                    self.chain.lookup(2, pixel_in[2]),
                ];
                let ratio = rows.ratio_for_rgb(x, chained);
                let mut display = [
                    self.display.lookup(0, chained[0] * ratio),
                    self.display.lookup(1, chained[1] * ratio),
                    self.display.lookup(2, chained[2] * ratio),
                ];
                if let Some(baseline) = self.baseline_chain {
                    let original = [
                        self.display
                            .lookup(0, baseline.lookup(0, pixel_in[0]) * ratio),
                        self.display
                            .lookup(1, baseline.lookup(1, pixel_in[1]) * ratio),
                        self.display
                            .lookup(2, baseline.lookup(2, pixel_in[2]) * ratio),
                    ];
                    display = neutralize_positive_highlights(display, original);
                }
                if self.chroma {
                    display =
                        apply_chroma(display, self.resolved.saturation, self.resolved.vibrance);
                }
                for (channel, slot) in pixel_out.iter_mut().enumerate() {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    {
                        *slot = (display[channel].clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
                    }
                }
            }
        }
    }
}

/// Final 16-to-8 quantization; never used between develop stages.
#[must_use]
pub fn quantize_rgb8(samples: &[u16]) -> Vec<u8> {
    samples
        .iter()
        .map(|v| ((u32::from(*v) + 128) / 257) as u8)
        .collect()
}

#[must_use]
pub fn render_rgb8(source: &LinearImage, params: &DevelopParams, curves: &CurveSet) -> Vec<u8> {
    quantize_rgb8(&render_rgb16(source, params, curves))
}

#[must_use]
pub fn render_rgb8_with_local_tone(
    source: &LinearImage,
    params: &DevelopParams,
    curves: &CurveSet,
    local: Option<(&LocalToneState, f32)>,
) -> Vec<u8> {
    quantize_rgb8(&render_rgb16_with_local_tone(source, params, curves, local))
}

#[must_use]
pub fn render_develop(
    source: &LinearImage,
    params: &DevelopParams,
    curves: &CurveSet,
    stages: &DevelopStages<'_>,
) -> Vec<u8> {
    quantize_rgb8(&render_develop16(source, params, curves, stages))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::develop::local_tone::LocalToneOpts;

    fn params(values: &[(&str, f64)]) -> DevelopParams {
        let mut params = DevelopParams::new(None);
        for (id, value) in values {
            params.set(id, *value).expect("测试参数要合法");
        }
        params
    }

    #[test]
    fn positive_highlights_preserve_bright_pixel_hue_and_clipped_white() {
        let none = params(&[]);
        let raised = params(&[("highlights", 100.0)]);
        let pixel = [0.85, 0.8, 0.45];
        let before = map_pixel_exact(pixel, &none);
        let after = map_pixel_exact(pixel, &raised);
        assert!(luma_of(after) > luma_of(before));
        for channel in 1..3 {
            assert!(
                ((after[0] - after[channel]) - (before[0] - before[channel])).abs() < 1e-5,
                "正向高光应等量提亮，不把暖色高光推成蓝色"
            );
        }
        let clipped = [1.0, 1.0, 0.7];
        assert_eq!(
            map_pixel_exact(clipped, &raised),
            map_pixel_exact(clipped, &none)
        );
        let source = LinearImage::new(
            2,
            1,
            vec![
                linear(0.85),
                linear(0.8),
                linear(0.45),
                linear(1.0),
                linear(1.0),
                linear(0.7),
            ],
        )
        .expect("有效像素");
        let rendered = render_rgb8(&source, &raised, &CurveSet::identity());
        let baseline = render_rgb8(&source, &none, &CurveSet::identity());
        assert_eq!(
            &rendered[3..6],
            &baseline[3..6],
            "截断的亮部不能因蓝通道单独增亮而发蓝"
        );
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
    fn highlight_and_black_curves_leave_out_of_gamut_channels_unchanged() {
        assert_eq!(highlights_curve(1.25, 1.0), 1.25);
        assert_eq!(blacks_curve(1.25, 1.0), 1.25);
        assert_eq!(highlights_curve(-0.2, 1.0), -0.2);
        assert_eq!(blacks_curve(-0.2, -1.0), -0.2);
        assert!(highlights_curve(0.8, 1.0) > 0.8);
        assert!(blacks_curve(0.2, 1.0) > 0.2);
    }

    #[test]
    fn render_develop_without_stages_matches_the_plain_path() {
        // 全部阶段关掉（或给了恒等映射）⇒ 与 W3 那条路**逐位一致**
        let mut rgb = Vec::new();
        for y in 0..24u32 {
            for x in 0..32u32 {
                let value = (x as f32 / 32.0).min(1.0);
                let other = (y as f32 / 24.0).min(1.0);
                rgb.extend([linear(value), linear(other), linear(0.5)]);
            }
        }
        let source = LinearImage::new(32, 24, rgb).expect("形状对得上");
        let params = params(&[("exposure", 0.4), ("contrast", 25.0)]);
        let curves = CurveSet::identity();
        let plain = render_rgb8(&source, &params, &curves);

        let stages = DevelopStages::default();
        assert_eq!(render_develop(&source, &params, &curves, &stages), plain);

        // 恒等映射 / 强度 0 的计划同样不许改动任何像素
        let identity_map = super::super::lens::LensMap::new(&super::super::lens::LensCorrection {
            norm: super::super::lens::Norm::new(32, 24, 1.0, 35.0),
            distortion: None,
            tca: None,
            vignetting: None,
            manual: super::super::lens::ManualLens::default(),
        });
        let zero_denoise = super::super::denoise::DenoisePlan::default();
        let zero_sharpen = super::super::sharpen::SharpenPlan::default();
        let stages = DevelopStages {
            nr_method: Default::default(),
            lens: Some(&identity_map),
            denoise: Some(&zero_denoise),
            local_tone: None,
            sharpen: Some(&zero_sharpen),
            lut: None,
        };
        assert_eq!(render_develop(&source, &params, &curves, &stages), plain);
    }

    #[test]
    fn render_develop_stages_change_the_picture_in_the_expected_direction() {
        // 一块平坦灰：三件阶段各自都要真的动到画面（否则「接线了但没生效」看不出来）
        let source = LinearImage::new(48, 32, (0..48 * 32 * 3).map(|_| linear(0.35)).collect())
            .expect("形状对得上");
        let params = DevelopParams::new(None);
        let curves = CurveSet::identity();
        let plain = render_develop(&source, &params, &curves, &DevelopStages::default());

        // 暗角补偿（系数为负 ⇒ 角上变暗）
        let map = super::super::lens::LensMap::new(&super::super::lens::LensCorrection {
            norm: super::super::lens::Norm::new(48, 32, 1.0, 35.0),
            distortion: None,
            tca: None,
            vignetting: Some(super::super::lens::Vignetting {
                k1: -0.5,
                k2: 0.0,
                k3: 0.0,
            }),
            manual: super::super::lens::ManualLens::default(),
        });
        let stages = DevelopStages {
            lens: Some(&map),
            ..DevelopStages::default()
        };
        let with_lens = render_develop(&source, &params, &curves, &stages);
        assert_ne!(with_lens, plain, "镜头校正必须真的改到像素");
        assert!(with_lens[0] > plain[0], "角上应当补亮");

        // 锐化：平坦图上没有细节可加，所以换一张有边缘的图来验
        let mut rgb = Vec::new();
        for _y in 0..16u32 {
            for x in 0..48u32 {
                let value = if x < 24 { 0.2 } else { 0.8 };
                rgb.extend([linear(value), linear(value), linear(value)]);
            }
        }
        let edge = LinearImage::new(48, 16, rgb).expect("形状对得上");
        let plan = super::super::sharpen::SharpenPlan {
            amount: 1.0,
            radius: 2.0,
        };
        let stages = DevelopStages {
            sharpen: Some(&plan),
            ..DevelopStages::default()
        };
        assert_ne!(
            render_develop(&edge, &params, &curves, &stages),
            render_develop(&edge, &params, &curves, &DevelopStages::default()),
            "锐化必须真的改到像素"
        );

        // 降噪：给带噪图，必须改到像素
        let mut rgb = Vec::new();
        let mut seed = 9u32;
        for _ in 0..16 * 16 * 3 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            rgb.push(u16::try_from(20000 + i32::try_from(seed >> 24).unwrap_or(0)).unwrap_or(0));
        }
        let noisy = LinearImage::new(16, 16, rgb).expect("形状对得上");
        let nr = super::super::denoise::DenoisePlan {
            luma: 1.0,
            chroma: 1.0,
        };
        let stages = DevelopStages {
            denoise: Some(&nr),
            ..DevelopStages::default()
        };
        assert_ne!(
            render_develop(&noisy, &params, &curves, &stages),
            render_develop(&noisy, &params, &curves, &DevelopStages::default()),
            "降噪必须真的改到像素"
        );
    }

    #[test]
    fn raw_linear_source_is_oriented_on_the_way_in() {
        use crate::raw::backend::{PixelSource, RawImage16};

        let raw = |orientation: Option<u16>| RawImage16 {
            width: 3,
            height: 2,
            rgb: (0..3 * 2 * 3).map(|i| i as u16).collect(),
            source: PixelSource::Decoded,
            orientation,
            as_shot_temperature: None,
        };
        let pixel_at = |image: &LinearImage, x: u32, y: u32| -> [u16; 3] {
            let index = ((y * image.width + x) * 3) as usize;
            [image.rgb[index], image.rgb[index + 1], image.rgb[index + 2]]
        };

        let upright = LinearImage::from_raw16(raw(Some(1))).expect("形状对得上");
        assert_eq!((upright.width, upright.height), (3, 2), "方向 1 不动");
        assert_eq!(pixel_at(&upright, 0, 0), [0, 1, 2]);

        let rotated = LinearImage::from_raw16(raw(Some(6))).expect("形状对得上");
        assert_eq!((rotated.width, rotated.height), (2, 3), "方向 6 宽高互换");
        // 顺时针 90°：源左上 (0,0) 落到目标右上 (1,0)
        assert_eq!(pixel_at(&rotated, 1, 0), [0, 1, 2], "源左上 → 目标右上");

        let missing = LinearImage::from_raw16(raw(None)).expect("形状对得上");
        assert_eq!(
            (missing.width, missing.height),
            (3, 2),
            "没有方向就按 1 处理"
        );

        // 形状对不上 → None（不猜、不补零）
        let broken = RawImage16 {
            width: 3,
            height: 2,
            rgb: vec![0; 5],
            source: PixelSource::Decoded,
            orientation: None,
            as_shot_temperature: None,
        };
        assert!(LinearImage::from_raw16(broken).is_none());
    }

    #[test]
    fn orientation_permutation_matches_the_thumbnail_path() {
        /*
         * 显影这条路的「按方向摆正」必须与缩略图/看图那条路（`image` crate 的
         * `fliph` / `rotate90`…）**逐像素一致** —— 两份实现各走各的，用户就会看到
         * 「网格里正着、编辑里躺着」那种很难查的错（§2.12）。
         *
         * 用 3×2 的非对称图：每个像素写一个能认出位置的值（8bit 范围，两边都能精确表示）。
         */
        use crate::thumbnail::render::apply_orientation;

        let (w, h) = (3u32, 2u32);
        let value = |x: u32, y: u32| ((y * w + x) * 10 + 1) as u8;
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                let v = value(x, y);
                rgb.extend([u16::from(v), u16::from(v) + 100, u16::from(v) + 200]);
            }
        }
        let ours = LinearImage::new(w, h, rgb).expect("形状对得上");

        let mut reference = image::RgbImage::new(w, h);
        for (x, y, pixel) in reference.enumerate_pixels_mut() {
            let v = value(x, y);
            *pixel = image::Rgb([v, v + 100, v + 200]);
        }

        for orientation in 1u16..=8 {
            let ours = ours.oriented(orientation);
            let expected = apply_orientation(
                &image::DynamicImage::ImageRgb8(reference.clone()),
                orientation,
            )
            .to_rgb8();
            assert_eq!(
                (ours.width, ours.height),
                (expected.width(), expected.height()),
                "方向 {orientation}：宽高对不上"
            );
            let ours8: Vec<u8> = ours.rgb.iter().map(|v| *v as u8).collect();
            assert_eq!(
                ours8,
                expected.into_raw(),
                "方向 {orientation}：像素排布与缩略图那条路不一致"
            );
        }
    }

    #[test]
    fn orientation_keeps_identity_and_unknown_untouched() {
        let rgb: Vec<u16> = (0..3 * 2 * 3).map(|i| i as u16).collect();
        let image = LinearImage::new(3, 2, rgb).expect("形状对得上");
        for orientation in [1u16, 0, 9, u16::MAX] {
            let same = image.oriented(orientation);
            assert_eq!(same, image, "方向 {orientation} 应当原样返回");
        }
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

    /// 一张带渐变与纹理的小图（局部色调映射需要二维邻域，单行图测不了）。
    ///
    /// 亮度上限刻意压在 1.0 以下（−7…−1 stops）：`chain_image` 是 u16，会把超过 1.0 的
    /// 高光**夹掉**，而融合路径不夹 —— 拿一张会溢出的图做参照，比出来的差异
    /// 是「参照自己丢了信息」，不是融合路径算错了。
    fn textured(width: u32, height: u32) -> LinearImage {
        let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);
        for y in 0..height as usize {
            for x in 0..width as usize {
                #[allow(clippy::cast_precision_loss)]
                let t = x as f32 / (width as f32 - 1.0).max(1.0);
                let checker: f32 = if (x + y) % 2 == 0 { 1.0 } else { -1.0 };
                let value = (-7.0 + 6.0 * t).exp2() * (0.2 * checker).exp2();
                let encoded = linear(value);
                rgb.extend([encoded, encoded, encoded]);
            }
        }
        LinearImage::new(width, height, rgb).expect("形状对得上")
    }

    #[test]
    fn local_tone_at_zero_strength_is_bit_identical_to_the_plain_pipeline() {
        // 拉杆在 0 位时这个功能必须**等于不存在**（不多花一分钱，也不许改一个像素）
        let source = textured(64, 48);
        let params = params(&[("exposure", 0.5), ("contrast", 30.0), ("saturation", 20.0)]);
        let curves = CurveSet::identity();
        let state =
            LocalToneState::analyze(&chain_image(&source, &params), &LocalToneOpts::default());
        let plain = render_rgb8(&source, &params, &curves);
        assert_eq!(
            render_rgb8_with_local_tone(&source, &params, &curves, None),
            plain
        );
        for strength in [0.0f32, -1.0, f32::NAN] {
            assert_eq!(
                render_rgb8_with_local_tone(&source, &params, &curves, Some((&state, strength))),
                plain,
                "强度 {strength} 时融合路径不等于原路径"
            );
        }
    }

    #[test]
    fn fused_path_matches_the_two_step_path() {
        // 「两条路一套数学」（`mod.rs` 的口径）：融合的一趟 vs（链 → apply_inplace → 显示）。
        // 两边差得超过 2 级 8bit 就说明有一边自己长了一套算法。
        let source = textured(64, 48);
        let params = params(&[("exposure", 0.3), ("contrast", 20.0), ("saturation", 15.0)]);
        let curves = CurveSet::identity();
        let chained = chain_image(&source, &params);
        let state = LocalToneState::analyze(&chained, &LocalToneOpts::default());
        let strength = 0.7f32;

        let fused =
            render_rgb8_with_local_tone(&source, &params, &curves, Some((&state, strength)));

        let mut reference_image = chained.clone();
        state.apply_inplace(&mut reference_image, strength);
        let mut reference = Vec::with_capacity(reference_image.rgb.len());
        for pixel in reference_image.rgb.as_chunks::<3>().0 {
            for (channel, value) in pixel.iter().enumerate() {
                let display = encode_and_curve(f32::from(*value) / 65535.0, channel, &curves);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                reference.push((display.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            }
        }

        let mut worst = 0i16;
        for (a, b) in fused.iter().zip(reference.iter()) {
            worst = worst.max((i16::from(*a) - i16::from(*b)).abs());
        }
        assert!(worst <= 2, "融合路径与两步路径差了 {worst} 级 8bit");
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
        assert!(
            (i16::from(out[0]) - 162).abs() <= 1,
            "曝光 +1EV 实测 {}",
            out[0]
        );

        // ② 曝光 −1 EV：0.18 → 0.09 → sRGB = 0.331830 → 85
        let out = render_rgb8(
            &image(&[[0.18, 0.18, 0.18]]),
            &params(&[("exposure", -1.0)]),
            &CurveSet::identity(),
        );
        assert!(
            (i16::from(out[0]) - 85).abs() <= 1,
            "曝光 −1EV 实测 {}",
            out[0]
        );

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
            i16::from(boosted[0]) - i16::from(boosted[1]) > i16::from(base[0]) - i16::from(base[1]),
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
            super::super::curve::Curve::identity()
                .with_point(0.5, 0.65)
                .unwrap(),
        );
        curved.set_channel(
            super::super::curve::CurveChannel::Blue,
            super::super::curve::Curve::identity()
                .with_point(0.3, 0.2)
                .unwrap(),
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
        let mut single = vec![0u16; source.rgb.len()];
        map_rows(&source.rgb, &mut single, &luts, None, &resolved);
        assert_eq!(
            parallel,
            quantize_rgb8(&single),
            "多线程与单线程的输出必须一致"
        );
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
        let rgb = vec![
            1000u16, 1000, 1000, 2000, 2000, 2000, 3000, 3000, 3000, 4000, 4000, 4000,
        ];
        let source = LinearImage::new(2, 2, rgb).expect("形状对");
        let small = source.downscaled_to(1);
        assert_eq!((small.width, small.height), (1, 1));
        assert_eq!(small.rgb, vec![2500, 2500, 2500]);

        // 4×1 → 2×1：左半平均 (0+65535)/2 = 32768（四舍五入），右半同理
        let rgb = vec![
            0u16, 0, 0, 65535, 65535, 65535, 65535, 65535, 65535, 0, 0, 0,
        ];
        let source = LinearImage::new(4, 1, rgb).expect("形状对");
        let small = source.downscaled_to(2);
        assert_eq!((small.width, small.height), (2, 1));
        assert_eq!(small.rgb[0], 32768);
        assert_eq!(small.rgb[3], 32768);
    }
    #[test]
    fn vibrance_does_not_reverse_after_saturation_pushes_a_channel_below_zero() {
        let rgb = [0.9, 0.1, 0.05];
        let saturated = apply_chroma(rgb, 1.0, 0.0);
        let vibrant = apply_chroma(rgb, 1.0, 1.0);
        assert_eq!(
            vibrant, saturated,
            "fully saturated colors should not lose saturation with positive vibrance"
        );
    }

    #[test]
    fn base_curve_precedes_user_curve_without_changing_user_points() {
        let mut curves = CurveSet::identity();
        curves.base =
            crate::develop::Curve::from_points(vec![[0.0, 0.0], [0.5, 0.7], [1.0, 1.0]]).unwrap();
        curves.rgb =
            crate::develop::Curve::from_points(vec![[0.0, 0.0], [0.5, 0.3], [1.0, 1.0]]).unwrap();
        let input = 0.214;
        let encoded = linear_to_srgb(input);
        let expected = curves.rgb.eval(curves.base.eval(encoded));
        let reversed = curves.base.eval(curves.rgb.eval(encoded));
        let output = encode_and_curve(input, 0, &curves);
        assert!((output - expected).abs() < 1e-5);
        assert!((output - reversed).abs() > 0.02);
        assert_eq!(curves.rgb.points(), &[[0.0, 0.0], [0.5, 0.3], [1.0, 1.0]]);
    }
}
