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
//! 调性与白平衡在**线性域**做（scene-referred，暗部拉伸不会出色带，这是 `memory/FUTURE.md` D1 的要求）；
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

use super::areas::{AreaKind, LayerStack, PointWeightConfig, point_weight_with};
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
    /// 所以这条路的输入只能算「准线性」—— 实施记录与 `memory/FUTURE.md` C7 都记着这件事。
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
        let view = super::sample::RgbView { width: self.width, height: self.height, rgb: &self.rgb };
        let (width, height, rgb) = super::sample::orient_rgb(view, orientation);
        Self { width, height, rgb }
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

    pub(crate) fn gains(&self) -> [f32; 3] { self.gains }
    pub(crate) fn chroma(&self) -> (f32, f32) { (self.saturation, self.vibrance) }

    /// 这一组参数需要跨通道的色度步骤吗（决定逐像素循环里要不要跑 [`apply_chroma`]）。
    #[must_use]
    pub fn needs_chroma(&self) -> bool {
        self.saturation.abs() > 1e-6 || self.vibrance.abs() > 1e-6
    }

    /// 线性那半条链是**恒等**吗（没有白平衡 / 曝光 / 反差 / 高光 / 黑区）。
    ///
    /// 层循环用它决定「这一层要不要建线性查表」：新建的空层是恒等的，
    /// 33 层全恒等时一次查表都不该发生。
    #[must_use]
    fn is_linear_identity(&self) -> bool {
        self.gains
            .iter()
            .all(|gain| (*gain - 1.0).abs() < 1e-6)
            && self.contrast.abs() < 1e-6
            && self.highlights.abs() < 1e-6
            && self.blacks.abs() < 1e-6
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

/// 影调窗口的**感知域指数**：`t = y^(1/2.2)`。
///
/// 为什么窗口定义在感知域：高光与黑区是**视觉**概念（亮部 / 暗部），而线性值差得远 ——
/// 线性 0.8 已是显示域 0.91 的一小块、线性 0.2 是显示域 0.48 的**中间调**。
/// 旧实现把窗口钉在这两个线性值上，实测症状就是「高光拉不动、黑区像在调中间调曝光」
/// （崔总 2026-10-09 反馈；对照 RapidRAW 的 `apply_highlights_adjustment` /
/// `apply_tonal_adjustments`，它同样在感知域 `pow(luma, 0.4545)` 上定义高光/黑区窗口）。
const TONE_PERCEPTUAL_GAMMA: f32 = 1.0 / 2.2;
const TONE_LINEAR_GAMMA: f32 = 2.2;

/// 高光的**基础**跨度（感知域≈显示域）：amount≈0 时只动**最亮 30%**（≥177 级）。
///
/// 演进（崔总 2026-10-09 反馈循环）：初版①（最亮 17%）完全没效果；初版②（最亮 55% 起、±38 级）
/// 「有效果但很弱」；线性动态区间版（最亮 50% → 75%）「按我说的方式动作了」，但中途就开始扩范围。
/// **v4（本版）**：跨度律改成 `base + E·a²` —— 开头精细（a=0.1 只动最亮 20%），越拉张得越快。
/// 拉满仍是 0.75，与上一版**完全一致**，不动已被接受的拉满观感。
/// （崔总 2026-10-09 追加：起始区再收窄一档，便于精确调整。）
const HIGHLIGHTS_SPAN_BASE: f32 = 0.20;
/// 高光跨度加速度：`span(a) = 0.20 + 0.55·a²` ⇒ 拉满 0.75（最亮 75%，= 上一版）。
const HIGHLIGHTS_SPAN_ACCEL: f32 = 0.55;
/// 高光幅度（感知域）：±1 时窗口峰值处约 ±0.15（≈ 38 个 8bit 级）。
///
/// **单调性**：`max_a a/span(a) = 1.508`（在 a≈0.603 处）⇒ 最大斜率 `0.15π×1.508 ≈ 0.71 < 1`。
const HIGHLIGHTS_GAIN: f32 = 0.15;

/// 黑区的**基础**跨度：amount≈0 时只动**最暗 12%**（≤31 级）—— 小幅拉动是纯「救死黑」。
const BLACKS_SPAN_BASE: f32 = 0.12;
/// 黑区跨度加速度：`span(a) = 0.12 + 0.36·a²` ⇒ 拉满 0.48（最暗 48%；2026-10-09 崔总真机反馈后由 0.60 降 20%）。
const BLACKS_SPAN_ACCEL: f32 = 0.36;
/// 黑区幅度（感知域）：±1 时约 ±0.072（2026-10-09 崔总真机反馈「20→30 太快」后降 40%）。
/// 最大斜率 `0.072π×2.406 ≈ 0.54 < 1`（a≈0.577 处最紧）。
const BLACKS_GAIN: f32 = 0.072;

/// 抬黑时的**对比补偿**（对照 RapidRAW `apply_tonal_adjustments` 的做法）：
/// 纯加性抬黑会压低暗部反差（观感「发灰」），把抬起的量围绕 `pivot` 拉伸一下再按 `mix` 混回。
/// 只在 `amount > 0`（抬黑）时生效；压暗一侧不动。三个数直接沿用 RR。
const BLACKS_COMP_PIVOT: f32 = 0.20;
const BLACKS_COMP_STRETCH: f32 = 1.3;
const BLACKS_COMP_MIX: f32 = 0.85;

/// 高光的作用下界：跨度 `span(a) = base + E·a²`，区间 `[1 − span, 1]`。
fn highlights_floor(amount: f32) -> f32 {
    let a = amount.abs();
    1.0 - (HIGHLIGHTS_SPAN_BASE + HIGHLIGHTS_SPAN_ACCEL * a * a)
}

/// 黑区的作用上界：跨度 `span(a) = base + E·a²`，区间 `[0, span]`。
fn blacks_ceil(amount: f32) -> f32 {
    let a = amount.abs();
    BLACKS_SPAN_BASE + BLACKS_SPAN_ACCEL * a * a
}

/// 感知域窗口 `sin²(πu)`：`u = 0 / 1` 两端为 0、中点 1，**两端导数也是 0**。
///
/// 端点取 0 是「窗口外恒等」的来源（黑点 / 白点固定）；导数连续则保证衔接处没有折角
/// ——折角在平滑渐变上会显成一条淡淡的带。
/// 最大斜率 `|w′| = π`，因此「幅度 × π / 窗口跨度 < 1」就能证明合成的映射仍然单调。
fn tone_window(u: f32) -> f32 {
    let s = (std::f32::consts::PI * u).sin();
    s * s
}

/// **高光**：亮部的软调整，白点固定。
///
/// * `amount < 0`：压高光、找回细节（「救高光」的主用法）；
/// * `amount > 0`：提亮高光 —— 允许把高光推爆（想炸就炸得对，不做人为回卷）。
///
/// 作用区间是**动态**的（`HIGHLIGHTS_SPAN_BASE/ACCEL`）：小幅只动最亮 30%，拉满扩到最亮 75%。
/// 单调性：`幅度 × π / span(a)` 的全程最大值 ≈ 0.64 < 1（推导见常量注释）。
#[must_use]
pub fn highlights_curve(value: f32, amount: f32) -> f32 {
    if amount.abs() < 1e-6 || !(0.0..=1.0).contains(&value) || value <= 0.0 || value >= 1.0 {
        return value;
    }
    let t = value.powf(TONE_PERCEPTUAL_GAMMA);
    let floor = highlights_floor(amount);
    if t <= floor {
        return value;
    }
    let u = (t - floor) / (1.0 - floor);
    let lifted = t + amount * HIGHLIGHTS_GAIN * tone_window(u);
    lifted.clamp(0.0, 1.0).powf(TONE_LINEAR_GAMMA)
}

/// **黑区**：暗部的软调整，黑点固定。
///
/// * `amount > 0`：抬死黑、托起暗部（「救死黑」的主用法）；
/// * `amount < 0`：压暗部、让黑更实。
///
/// 作用区间同样随 `|amount|` 以 `a²` 加速扩展（`BLACKS_SPAN_BASE/ACCEL`）。
///
/// 抬黑（`amount > 0`）时额外做**对比补偿**（`BLACKS_COMP_*`）：把抬起的量围绕 pivot 拉伸后按 85% 混回，
/// 抵掉「抬黑发灰」；压暗（`amount < 0`）不走补偿。
/// 实测（削弱后·拉满）：峰值 64 级处 **+21 级**（补偿贡献 +2；小幅度时补偿反而把抬升压低 ~0.7 级）。
#[must_use]
pub fn blacks_curve(value: f32, amount: f32) -> f32 {
    if amount.abs() < 1e-6 || !(0.0..=1.0).contains(&value) || value <= 0.0 {
        return value;
    }
    let t = value.powf(TONE_PERCEPTUAL_GAMMA);
    let ceil = blacks_ceil(amount);
    if t >= ceil {
        return value;
    }
    let u = t / ceil;
    let lift = amount * BLACKS_GAIN * tone_window(u);
    let lifted = (t + lift).clamp(0.0, 1.0);
    let compensated = if lift > 0.0 {
        let stretched =
            BLACKS_COMP_PIVOT + (lifted - BLACKS_COMP_PIVOT) * (1.0 + BLACKS_COMP_STRETCH * lift);
        lifted + (stretched - lifted) * BLACKS_COMP_MIX
    } else {
        lifted
    };
    compensated.clamp(0.0, 1.0).powf(TONE_LINEAR_GAMMA)
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
    apply_chroma_unbounded(rgb, saturation, vibrance).map(|v| v.clamp(0.0, 1.0))
}

/// Identical chroma math without clipping the float reference domain.
pub fn apply_chroma_unbounded(rgb: [f32; 3], saturation: f32, vibrance: f32) -> [f32; 3] {
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
    out
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

    /// 查表（**f32 输入**，层循环用）。
    ///
    /// 表只在 `0..=1` 上采样（与快路径同一个域）；越界的通道**当场退回标量**，
    /// 而不是夹到表的两端 —— 层间是「f32 线性、不夹取」（`specs/editor-area.md` §4.6），
    /// 夹一次就会把上一层的高光吃掉。
    #[inline]
    fn lookup_f32(&self, channel: usize, linear: f32, resolved: &Resolved) -> f32 {
        if !(0.0..=1.0).contains(&linear) {
            #[allow(clippy::cast_possible_truncation)]
            let gain = resolved.gains[channel];
            return chain_linear(linear, gain, resolved);
        }
        let table = &self.tables[channel];
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

/// 显示域 → 线性（**带符号扩展**：±1 以上按同一份 sRGB 曲线外推）。
///
/// 层间穿过显示域（曲线 / 色度 / LUT）之后要回到线性给下一层用；
/// 夹到 `0..1` 会把上一层已经推出的高光吃掉，所以走带符号的那一套
/// （与 `develop::working` 的 `signed_encode / decode` 同一个口径）。
#[inline]
fn decode_signed(value: f32) -> f32 {
    if value.abs() <= 1e-6 {
        return 0.0;
    }
    value.signum() * super::color::srgb_to_linear(value.abs())
}

/// 线性 → 显示（带符号扩展，`decode_signed` 的逆）。
#[inline]
fn encode_signed(value: f32) -> f32 {
    if value.abs() <= 1e-6 {
        return 0.0;
    }
    value.signum() * linear_to_srgb(value.abs())
}

/// 层间「线性 → 线性」的预折叠表：显示值 `0..1` → 线性（带符号）。
///
/// 每像素每层错开一次 `powf` 太贵；表外的值（±1 以上）当场退回标量 ——
/// 那是被上一层推出去的少量高光，不影响主流。
struct DecodeLut(Vec<f32>);

impl DecodeLut {
    fn build() -> Self {
        #[allow(clippy::cast_precision_loss)]
        let denominator = (LUT_LEN - 1) as f32;
        Self(
            (0..LUT_LEN)
                .map(|index| {
                    #[allow(clippy::cast_precision_loss)]
                    let display = index as f32 / denominator;
                    super::color::srgb_to_linear(display)
                })
                .collect(),
        )
    }

    #[inline]
    fn decode(&self, value: f32) -> f32 {
        if value.abs() > 1.0 {
            return decode_signed(value);
        }
        let scaled = value.abs() * (LUT_LEN - 1) as f32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let index = scaled.floor() as usize;
        let a = self.0[index.min(LUT_LEN - 1)];
        let b = self.0[(index + 1).min(LUT_LEN - 1)];
        let linear = a + (b - a) * (scaled - index as f32);
        if value < 0.0 { -linear } else { linear }
    }
}

/// 一层**准备好求值**的形态（`specs/editor-area.md` §4.5 的层内固定管线）。
///
/// ```text
/// 线性 → WB×EV → 反差 → 高光/黑区 →（默认层：基础曲线）→ rgb 曲线 → 通道曲线
///      → 正向高光中性化 → 色度 →（默认层：LUT）
/// ```
///
/// 既无曲线也无色度（也不是默认层带 LUT / 本地色调）⇒ **全程留在线性域**，
/// 不跨域、不做 OETF。
struct PreparedArea<'a> {
    /// 权重（全图恒 1；控制点按向量距离平滑衰减）
    weight: AreaWeight,
    /// 层不透明度
    opacity: f32,
    resolved: Resolved,
    /// 线性半条链的查表（`None` = 本层没有线性调整）
    linear: Option<LinearChainLuts>,
    /// 显示半条链的查表（`None` = 本层不跨到显示域）
    display: Option<DisplayLuts>,
    /// 本层的曲线：**表外**的值用它精确求值（signed 扩展，不能夹到 0..1）
    curves: &'a CurveSet,
    /// **额外地层**才走 signed 扩展；默认层必须保持改动前的夹取语义（单层等价）。
    extended_display: bool,
    /// `highlights > 0` 时的基线（正向高光只提亮度，不推色相）
    baseline: Option<DisplayLuts>,
    chroma: bool,
    /// 默认层独有的两件：LUT 链与局部色调（§4.3）
    lut: Option<&'a super::lut::LutChain>,
    local: Option<(&'a LocalToneState, f32)>,
}

/// 一层拿多少权重。
#[derive(Debug, Clone, Copy)]
enum AreaWeight {
    /// 全图（含默认层）：恒 1
    Full,
    /// 控制点：冻结的种子向量 + 容差（**没有空间项**）
    Point {
        seed: [f32; 3],
        tolerance: f32,
        /// 容差 → 两个轴的映射（默认 = 固化常量；开发期可被调试面板覆盖）
        config: PointWeightConfig,
    },
}

impl AreaWeight {
    #[inline]
    fn at(&self, linear: [f32; 3]) -> f32 {
        match *self {
            Self::Full => 1.0,
            Self::Point { seed, tolerance, config } => {
                point_weight_with(seed, tolerance, linear, &config)
            }
        }
    }
}

impl<'a> PreparedArea<'a> {
    fn prepare(
        resolved: Resolved,
        curves: &'a CurveSet,
        weight: AreaWeight,
        opacity: f32,
        lut: Option<&'a super::lut::LutChain>,
        local: Option<(&'a LocalToneState, f32)>,
    ) -> Self {
        let chroma = resolved.needs_chroma();
        let baseline_resolved = (resolved.highlights > 0.0).then_some(Resolved {
            highlights: 0.0,
            ..resolved
        });
        let needs_display = !curves.is_identity() || chroma || lut.is_some();
        Self {
            weight,
            opacity: opacity.clamp(0.0, 1.0),
            resolved,
            linear: (!resolved.is_linear_identity()).then(|| LinearChainLuts::build(&resolved)),
            display: needs_display.then(|| DisplayLuts::build(curves)),
            curves,
            extended_display: false,
            baseline: baseline_resolved.map(|_| DisplayLuts::build(curves)),
            chroma,
            lut,
            local,
        }
    }

    fn from_area(area: &'a super::areas::AreaLayer, config: PointWeightConfig) -> Self {
        let weight = match area.kind {
            AreaKind::Full => AreaWeight::Full,
            AreaKind::Point { seed, tolerance, .. } => AreaWeight::Point {
                seed,
                tolerance,
                config,
            },
        };
        let mut prepared = Self::prepare(
            Resolved::new(&area.params),
            &area.curves,
            weight,
            // 关掉的层 = 权重恒 0（整层旁路，但**留在链里**：顺序 / 名字 / 参数都不动）
            if area.enabled { area.opacity } else { 0.0 },
            None,
            None,
        );
        prepared.extended_display = true;
        prepared
    }

    /// 线性半条链（本层无调整时原样返回）。
    #[inline]
    fn apply_linear(&self, prev: [f32; 3]) -> [f32; 3] {
        match &self.linear {
            None => prev,
            Some(tables) => [
                tables.lookup_f32(0, prev[0], &self.resolved),
                tables.lookup_f32(1, prev[1], &self.resolved),
                tables.lookup_f32(2, prev[2], &self.resolved),
            ],
        }
    }

    /// 显示半条链（`None` = 本层不跨域）。
    #[inline]
    fn apply_display(&self, linear: [f32; 3]) -> Option<[f32; 3]> {
        let display = self.display.as_ref()?;
        let mut out = [
            self.display_value(display, 0, linear[0]),
            self.display_value(display, 1, linear[1]),
            self.display_value(display, 2, linear[2]),
        ];
        if let Some(baseline) = &self.baseline {
            let original = [
                self.display_value(baseline, 0, linear[0]),
                self.display_value(baseline, 1, linear[1]),
                self.display_value(baseline, 2, linear[2]),
            ];
            out = neutralize_positive_highlights(out, original);
        }
        if self.chroma {
            out = apply_chroma_unbounded(out, self.resolved.saturation, self.resolved.vibrance);
        }
        if let Some(lut) = self.lut {
            out = lut.eval_extended(out);
        }
        Some(out)
    }

    /// 表内查表；**表外**（上一层推出的 >1 / <0）用 signed 的精确求值。
    ///
    /// 这是 §4.6「层间不夹取」的关键：`DisplayLuts` 的域只有 `0..1`，
    /// 直接查它会把上一层的高光夹到白点，下一层再也拉不回来。
    #[inline]
    fn display_value(&self, table: &DisplayLuts, channel: usize, linear: f32) -> f32 {
        if self.extended_display && linear.is_finite() && !(0.0..=1.0).contains(&linear) {
            encode_and_curve_extended(linear, channel, self.curves)
        } else {
            table.lookup(channel, linear)
        }
    }
}

/// `encode_and_curve` 的**带符号扩展**版本（层间用，不进快路径）。
///
/// 与 `develop::working` 的 `curve_at` 同一条口径：OETF 按符号外推，
/// 曲线用 `eval_extended`（保留域外残差），于是「上一层推出去的 >1」
/// 在下一层仍然是一個可以继续运算的数，而不是被削成 1。
fn encode_and_curve_extended(linear: f32, channel: usize, curves: &CurveSet) -> f32 {
    let encoded = encode_signed(linear);
    let based = curves.base.eval_extended(encoded);
    let composite = curves.rgb.eval_extended(based);
    match channel {
        0 => curves.r.eval_extended(composite),
        1 => curves.g.eval_extended(composite),
        _ => curves.b.eval_extended(composite),
    }
}

/// **链循环**（`specs/editor-area.md` §4.1）：默认层插在 `default_index` 的位置。
struct LayerChain<'a> {
    layers: Vec<PreparedArea<'a>>,
}

impl<'a> LayerChain<'a> {
    fn build(
        default: PreparedArea<'a>,
        stack: &'a LayerStack,
        config: PointWeightConfig,
    ) -> Self {
        let mut layers = Vec::with_capacity(stack.len());
        let mut areas = stack.areas.iter();
        let mut default = Some(default);
        for position in 0..stack.len() {
            if position == stack.default_index {
                if let Some(layer) = default.take() {
                    layers.push(layer);
                }
            } else if let Some(area) = areas.next() {
                layers.push(PreparedArea::from_area(area, config));
            }
        }
        Self { layers }
    }

    /// 默认层带局部色调时，每个线程要自己建一份行推进器（`strength` 已在里面）。
    fn local_tone(&self) -> Option<(&'a LocalToneState, f32)> {
        self.layers.iter().find_map(|layer| layer.local)
    }

    /// 一个像素：线性输入 → 显示输出（`0..1`，未夹取）。
    ///
    /// 红线：**权重读本层输入 `prev`，绝不读本层输出**（§4.1 红线一）。
    #[inline]
    fn eval(
        &self,
        linear: [f32; 3],
        x: usize,
        rows: &mut Option<LocalToneRows<'_>>,
        decode: &DecodeLut,
    ) -> [f32; 3] {
        match self.run(linear, x, rows, decode) {
            ChainValue::Display(value) => value,
            ChainValue::Linear(value) => [
                encode_signed(value[0]),
                encode_signed(value[1]),
                encode_signed(value[2]),
            ],
        }
    }

    /// 合成到第 `layer` 层**之前**为止的**线性**值（穿过显示域的那一段解回线性）——
    /// 控制点落点取种子用（`layer` 在**行序**里的位置；0 = 原始源）。
    fn compose_linear_upto(&self, linear: [f32; 3], layer: usize, decode: &DecodeLut) -> [f32; 3] {
        let mut current = linear;
        let mut display: Option<[f32; 3]> = None;
        for (index, area) in self.layers.iter().enumerate() {
            if index == layer {
                break;
            }
            let prev = current;
            let alpha = area.weight.at(prev) * area.opacity;
            if !(alpha > 1e-6) {
                continue;
            }
            let lin = area.apply_linear(prev);
            if let Some(mapped) = area.apply_display(lin) {
                let previous_display = display.unwrap_or_else(|| {
                    [
                        encode_signed(prev[0]),
                        encode_signed(prev[1]),
                        encode_signed(prev[2]),
                    ]
                });
                let blended = std::array::from_fn(|channel| {
                    previous_display[channel] + (mapped[channel] - previous_display[channel]) * alpha
                });
                display = Some(blended);
                current = [
                    decode.decode(blended[0]),
                    decode.decode(blended[1]),
                    decode.decode(blended[2]),
                ];
            } else {
                current = std::array::from_fn(|channel| {
                    prev[channel] + (lin[channel] - prev[channel]) * alpha
                });
                display = None;
            }
        }
        match display {
            Some(value) => [
                decode.decode(value[0]),
                decode.decode(value[1]),
                decode.decode(value[2]),
            ],
            None => current,
        }
    }

    fn run(
        &self,
        linear: [f32; 3],
        x: usize,
        rows: &mut Option<LocalToneRows<'_>>,
        decode: &DecodeLut,
    ) -> ChainValue {
        let mut current = linear;
        let mut display: Option<[f32; 3]> = None;
        for layer in &self.layers {
            let prev = current;
            let alpha = layer.weight.at(prev) * layer.opacity;
            if !(alpha > 1e-6) {
                continue;
            }
            let mut lin = layer.apply_linear(prev);
            if layer.local.is_some()
                && let Some(tone) = rows.as_mut()
            {
                // `ratio_for_rgb` 的 `log2` 在 NaN 上不会短路：luma NaN ⇒ ratio NaN。
                // 调不出 NaN 的链不该让它有出口 —— 别把一个 NaN 带进输出。
                let ratio = tone.ratio_for_rgb(x, lin);
                let ratio = if ratio.is_finite() { ratio } else { 1.0 };
                lin = [lin[0] * ratio, lin[1] * ratio, lin[2] * ratio];
            }
            if let Some(mapped) = layer.apply_display(lin) {
                let previous_display = display.unwrap_or_else(|| {
                    [
                        encode_signed(prev[0]),
                        encode_signed(prev[1]),
                        encode_signed(prev[2]),
                    ]
                });
                let blended = std::array::from_fn(|channel| {
                    previous_display[channel] + (mapped[channel] - previous_display[channel]) * alpha
                });
                display = Some(blended);
                current = [
                    decode.decode(blended[0]),
                    decode.decode(blended[1]),
                    decode.decode(blended[2]),
                ];
            } else {
                current = std::array::from_fn(|channel| {
                    prev[channel] + (lin[channel] - prev[channel]) * alpha
                });
                display = None;
            }
        }
        match display {
            Some(value) => ChainValue::Display(value),
            None => ChainValue::Linear(current),
        }
    }
}

/// 链跑完时值在哪一个域。
enum ChainValue {
    Linear([f32; 3]),
    Display([f32; 3]),
}

/// **层链的生产路径**（有额外层时才走这里；单层仍走 [`map_rows`] / [`LocalPass`]）。
///
/// 与快路径共用同一套数学（`Resolved` / `DisplayLuts` / `apply_chroma` / `neutralize_positive_highlights`），
/// 只是把「一个像素里内循环走层」写在了一处 —— 合成不需要任何中间图（§4.6）。
fn render_layered16(
    source: &LinearImage,
    params: &DevelopParams,
    curves: &CurveSet,
    stages: &DevelopStages<'_>,
    stack: &LayerStack,
) -> Vec<u16> {
    let resolved = Resolved::new(params);
    let default = PreparedArea::prepare(
        resolved,
        curves,
        AreaWeight::Full,
        1.0,
        stages.lut.filter(|chain| !chain.is_empty()),
        stages.local_tone.filter(|(_, strength)| *strength > 0.0),
    );
    let chain = LayerChain::build(default, stack, stages.weight);
    // **退化的层不跑**：空层 / 关掉的层不产生任何像素改动，整张图走快路径即可
    //（33 层全空时不能多跑一遍全画幅）。
    if chain
        .layers
        .iter()
        .all(|layer| layer.opacity <= 1e-6 || (layer.linear.is_none() && layer.display.is_none()))
    {
        // 退化的**层**省掉了，但 LUT 仍要套（它是默认层那一段的末尾）
        let mut out = render_rgb16_with_local_tone(source, params, curves, stages.local_tone);
        if let Some(chain) = stages.lut.filter(|chain| !chain.is_empty()) {
            chain
                .apply_rgb(&mut out)
                .expect("管线输出必为 RGB 三元组");
        }
        return out;
    }
    let decode = DecodeLut::build();
    let width = source.width as usize;
    let mut out = vec![0u16; source.rgb.len()];
    let run = |input: &[u16], output: &mut [u16], first_row: usize| {
        let mut rows = chain
            .local_tone()
            .map(|(state, strength)| LocalToneRows::new(state, strength, source.width, source.height));
        for (row, (line_in, line_out)) in input
            .as_chunks::<3>()
            .0
            .chunks(width)
            .zip(output.as_chunks_mut::<3>().0.chunks_mut(width))
            .enumerate()
        {
            if let Some(tone) = rows.as_mut() {
                tone.set_row(first_row + row);
            }
            for (x, (pixel_in, pixel_out)) in line_in.iter().zip(line_out.iter_mut()).enumerate() {
                let linear = [
                    f32::from(pixel_in[0]) / 65535.0,
                    f32::from(pixel_in[1]) / 65535.0,
                    f32::from(pixel_in[2]) / 65535.0,
                ];
                let display = chain.eval(linear, x, &mut rows, &decode);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                for (channel, slot) in pixel_out.iter_mut().enumerate() {
                    *slot = (display[channel].clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
                }
            }
        }
    };

    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(16);
    if threads <= 1 || source.height < 32 {
        run(&source.rgb, &mut out, 0);
        return out;
    }
    let rows_per_chunk = (source.height as usize).div_ceil(threads);
    std::thread::scope(|scope| {
        let mut remaining_in = source.rgb.as_slice();
        let mut remaining_out = out.as_mut_slice();
        let mut first_row = 0usize;
        while !remaining_out.is_empty() {
            let rows = (remaining_out.len() / (width * 3)).min(rows_per_chunk).max(1);
            let take = rows * width * 3;
            let (chunk_out, rest_out) = remaining_out.split_at_mut(take);
            let (chunk_in, rest_in) = remaining_in.split_at(take);
            remaining_in = rest_in;
            remaining_out = rest_out;
            let start = first_row;
            first_row += rows;
            let run_ref = &run;
            scope.spawn(move || run_ref(chunk_in, chunk_out, start));
        }
    });
    out
}

/// **单个像素的合成结果（线性）** —— 控制点落点取种子用（`specs/editor-area.md` §3.3）。
///
/// 种子是「**该层输入**」：把链合成到 `layer` 那一层**之前**为止（第 `layer` 行在
/// 行序中的位置；0 = 原始线性源，`layer == layer_count()` = 整链跑完）。
/// 穿过显示域的那一段再解回线性（§4.6 的 signed 可逆编码）。
///
/// `x` / `y` 是**像素坐标**（采样位本身不参与权重，§4.2）。
/// 镜头 / 降噪 / 局部色调**不在这里**：种子取的是「这张照片的像素」，
/// 而镜头重采样会移动像素位置（落点是一次点击，不值得为它跑一遍镜头）。
#[must_use]
pub fn compose_linear_at(
    source: &LinearImage,
    params: &DevelopParams,
    curves: &CurveSet,
    stages: &DevelopStages<'_>,
    layer: usize,
    x: u32,
    y: u32,
) -> Option<[f32; 3]> {
    compose_linear_median(source, params, curves, stages, layer, x, y, 0)
}

/// 控制点落点取样的**小窗半径**（1024 长边解码上的像素）。
///
/// 7×7 = 49 个像素；对 3712 宽的原图约 25×25 像素。单像素会采到热噪点、
/// 压缩块边界或刚好跨在物体边缘的一颗异色点 —— 一次点击就永久冻结错的种子。
/// 小窗中位数对少量坏值免疫，又不会像大窗那样把邻近物体混进来。
pub const SEED_SAMPLE_RADIUS: u32 = 3;

/// **小窗取样的合成结果**（控制点种子用）：在 `radius` 像素的方形窗内，
/// 对「该层输入」的线性 RGB 逐通道取**中位数**。
///
/// 为什么是中位数而不是均值：窗外缘可能压到物体边界，窗内也可能有热噪点；
/// 均值会被这些少数坏值拉走，中位数不会。窗是**方形**而不是圆：实现简单、
/// 半径语义一眼看得懂（`3` = 7×7）。`radius = 0` 时退化为单点，
/// 与旧的 [`compose_linear_at`] 逐位一致。
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn compose_linear_median(
    source: &LinearImage,
    params: &DevelopParams,
    curves: &CurveSet,
    stages: &DevelopStages<'_>,
    layer: usize,
    x: u32,
    y: u32,
    radius: u32,
) -> Option<[f32; 3]> {
    if source.width == 0 || source.height == 0 || x >= source.width || y >= source.height {
        return None;
    }
    let mut default = Some(PreparedArea::prepare(
        Resolved::new(params),
        curves,
        AreaWeight::Full,
        1.0,
        // 默认层挂着的 LUT 也是「该层输入」的一部分（默认层在目标层之前时）
        stages.lut.filter(|chain| !chain.is_empty()),
        None,
    ));
    let decode = DecodeLut::build();
    let chain = if let Some(stack) = stages.areas {
        let default = default.take()?;
        Some(LayerChain::build(default, stack, stages.weight))
    } else {
        None
    };
    let sample = |pixel: [f32; 3]| -> Option<[f32; 3]> {
        if let Some(chain) = &chain {
            // 行号在层数之后 = 整链跑完（与 `LayerStack::len()` 同口径；超出的按整链算）
            Some(chain.compose_linear_upto(pixel, layer.min(chain.layers.len()), &decode))
        } else if layer == 0 {
            // 没有额外层：layer 0 = 原始源，layer 1（默认层之后）= 默认层的线性段
            Some(pixel)
        } else {
            default
                .as_ref()
                .map(|default| default.apply_linear(pixel))
        }
    };
    let x0 = x.saturating_sub(radius);
    let x1 = x.saturating_add(radius).min(source.width - 1);
    let y0 = y.saturating_sub(radius);
    let y1 = y.saturating_add(radius).min(source.height - 1);
    let mut channels: [Vec<f32>; 3] = std::array::from_fn(|_| Vec::new());
    for yy in y0..=y1 {
        let row = (yy as usize) * (source.width as usize);
        for xx in x0..=x1 {
            let offset = (row + xx as usize) * 3;
            let pixel = [
                f32::from(source.rgb[offset]) / 65535.0,
                f32::from(source.rgb[offset + 1]) / 65535.0,
                f32::from(source.rgb[offset + 2]) / 65535.0,
            ];
            let value = sample(pixel);
            // 坏数据不能把中位数带成 NaN（管线正常不会给，但这是进程边界）
            if let Some(value) = value.filter(|value| value.iter().all(|channel| channel.is_finite())) {
                for (channel, slot) in channels.iter_mut().enumerate() {
                    slot.push(value[channel]);
                }
            }
        }
    }
    if channels[0].is_empty() {
        return None;
    }
    let mut out = [0.0; 3];
    for (channel, values) in channels.iter_mut().enumerate() {
        values.sort_by(f32::total_cmp);
        out[channel] = values[values.len() / 2];
    }
    Some(out)
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
    /// 选中的创意 LUT；作用在**默认层那一段的末尾**（§4.7，2026-10-31 起在锐化之前）。
    pub lut: Option<&'a super::lut::LutChain>,
    /// **默认层之外的编辑区层 + 默认层的位置**（`specs/editor-area.md`）。
    ///
    /// `None` / 退化（只有默认层）⇒ 走原来的快路径，与改动前**逐位一致**；
    /// 有额外层 ⇒ 逐像素链循环（`render_layered16`）。
    pub areas: Option<&'a super::areas::LayerStack>,
    /// **控制点容差 → 权重**的映射（`develop/areas.rs::PointWeightConfig`）。
    ///
    /// 默认就是固化常量；开发期的调试面板可以逐次覆盖它（只在编辑器真帧那条路上生效）。
    pub weight: PointWeightConfig,
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
    let layered = stages.areas.filter(|stack| !stack.is_degenerate());
    let mut out = match layered {
        // 有额外层：一条链一次算完（LUT 在默认层那一段的末尾，已在链里）
        Some(stack) => render_layered16(source, params, curves, stages, stack),
        None => {
            let mut out =
                render_rgb16_with_local_tone(source, params, curves, stages.local_tone);
            // LUT = **默认层那一段的末尾**（§4.7）：默认层在第 1 位时它就落在锐化之前。
            if let Some(chain) = stages.lut.filter(|chain| !chain.is_empty()) {
                chain
                    .apply_rgb(&mut out)
                    .expect("管线输出必为 RGB 三元组");
            }
            out
        }
    };
    if let Some(plan) = stages.sharpen.filter(|plan| !plan.is_identity()) {
        super::sharpen::sharpen_display(&mut out, source.width, source.height, plan);
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
    fn tone_change_profile_diagnostic() {
        // 诊断（2026-10-09 崔总真机：高光只让直方图低部动、峰不动）：
        // 打印中性灰在各显示亮度的实际变化量 —— 含 `map_pixel_exact` 的全部逻辑（含高光中和）。
        let none = params(&[]);
        for (key, amount) in [
            ("highlights", 100.0f64),
            ("highlights", -100.0),
            ("blacks", 100.0),
            ("blacks", -100.0),
            ("blacks", 20.0),
            ("blacks", 30.0),
            ("blacks", 40.0),
            ("highlights", 50.0),
            ("highlights", -50.0),
        ] {
            let adjusted = params(&[(key, amount)]);
            println!("=== {key} {amount:+.0}");
            for step in 0..=16 {
                let target = step as f32 / 16.0;
                let linear = target.powf(TONE_LINEAR_GAMMA);
                let base = map_pixel_exact([linear, linear, linear], &none)[1];
                let out = map_pixel_exact([linear, linear, linear], &adjusted)[1];
                println!(
                    "  显示 {target:5.3} → 基础 {base:5.3} / 调整 {out:5.3}   Δ {:+.1} 级",
                    (out - base) * 255.0
                );
            }
        }
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
        // 黑区作用在最暗 40%（感知域 ≤ 0.60 ⇒ 线性 ≤ 0.1 的极暗端）；线性 0.4（显示域 0.66）
        // 这类中高调不该被它影响
        assert!((blacks_curve(0.4, 1.0) - 0.4).abs() < 1e-6);
        assert!(blacks_curve(0.005, 1.0) > 0.005);
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
            areas: None,
            ..DevelopStages::default()
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
            camera_matrix_id: crate::color::ProfileId::of_bytes(b"test-matrix"),
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
            camera_matrix_id: crate::color::ProfileId::of_bytes(b"test-matrix"),
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

        // ⑧ 高光 −100 只压亮部、不动黑点与极暗端
        assert!(highlights_curve(0.8, -1.0) < 0.8);
        // 作用带在显示域高段（拉满从 0.374 起）；显示域 0.1 以下的极暗端始终不动
        assert!((highlights_curve(0.004, -1.0) - 0.004).abs() < 1e-4);
        assert_eq!(highlights_curve(0.0, -1.0), 0.0);
        // ⑨ 黑区 +100 只抬暗部、不动白点
        assert!(blacks_curve(0.005, 1.0) > 0.005);
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
    fn tone_curve_windows_land_on_the_perceptual_ends() {
        // 在**显示域**（感知域 `t = y^(1/2.2)`）网格上量「拉满时的效果」：
        // 高光峰值在显示域 ≈ 0.66（159 级）±38 级、黑区峰值 ≈ 0.25（64 级）±21 级 ——
        // 峰值落在有内容的暗部/中高调，而不是空的高光/黑端点。
        let sweep = |curve: fn(f32, f32) -> f32| {
            let mut peak = (0.0f32, 0.0f32); // (显示域位置, 显示域增量)
            for step in 0..=1_000 {
                let display = step as f32 / 1_000.0;
                let value = display.powf(TONE_LINEAR_GAMMA);
                let positive = curve(value, 1.0).powf(TONE_PERCEPTUAL_GAMMA) - display;
                let negative = display - curve(value, -1.0).powf(TONE_PERCEPTUAL_GAMMA);
                let delta = positive.max(negative);
                if delta > peak.1 {
                    peak = (display, delta);
                }
            }
            peak
        };
        let (highlight_at, highlight_gain) = sweep(highlights_curve);
        let (black_at, black_gain) = sweep(blacks_curve);
        assert!(
            (highlight_at - 0.66).abs() < 0.08,
            "高光峰值应落在中高调（显示域 0.65 附近），实测 {highlight_at}"
        );
        assert!(highlight_gain > 0.10, "高光拉满至少 10 个显示点（实测 ≈0.15 = 38 个 8bit 级）");
        assert!(
            (black_at - 0.25).abs() < 0.07,
            "黑区峰值应在显示域 0.25 附近（最暗 48% 带·拉满时），实测 {black_at}"
        );
        assert!(black_gain > 0.06, "黑区拉满至少 6 个显示点（削弱后实测 ≈0.081 = 21 个 8bit 级）");
    }

    #[test]
    fn tone_windows_expand_with_amount() {
        // 动态区间 v4（崔总 2026-10-09）：`span(a) = base + E·a²` —— 开头精细、越拉张得越快。
        //
        // a≈0：高光只动最亮 20%（≥204 级）、黑区只动最暗 12%（≤31 级）。
        assert!((highlights_floor(0.0) - 0.80).abs() < 1e-6);
        assert!((blacks_ceil(0.0) - 0.12).abs() < 1e-6);
        // a=0.5（前半程终点）：只吃掉 1/4 的扩张量 —— 这正是「前半程精修」的来源。
        assert!((1.0 - highlights_floor(0.5) - 0.3375).abs() < 1e-5);
        assert!((blacks_ceil(0.5) - 0.21).abs() < 1e-5);
        // a=1（拉满）：高光 span 0.75（floor 0.25）、黑区 span 0.48 —— 黑区比 v4 初版窄 20%。
        assert!((1.0 - highlights_floor(1.0) - 0.75).abs() < 1e-5);
        assert!((blacks_ceil(1.0) - 0.48).abs() < 1e-5);
        // 与方向无关：±100 的区间一样大（只看 |amount|）。
        assert!((highlights_floor(-1.0) - highlights_floor(1.0)).abs() < 1e-6);
        assert!((blacks_ceil(-1.0) - blacks_ceil(1.0)).abs() < 1e-6);
        // 随 |amount| 只扩不回缩。
        let mut previous = 0.0f32;
        for step in 0..=20 {
            let amount = step as f32 / 20.0;
            let span = 1.0 - highlights_floor(amount);
            assert!(span >= previous - 1e-6, "高光区间不许回缩（amount={amount}）");
            previous = span;
        }
    }

    #[test]
    fn blacks_lift_compensation_keeps_the_curve_monotone() {
        // 对比补偿不许破坏单调性（全程斜率 > 0），正负两侧都要查。
        for step in 0..=20 {
            let amount = step as f32 / 10.0 - 1.0;
            let mut previous = f32::NEG_INFINITY;
            for i in 0..=2_000 {
                let t = i as f32 / 2_000.0;
                let out = blacks_curve(t.powf(TONE_LINEAR_GAMMA), amount).powf(TONE_PERCEPTUAL_GAMMA);
                assert!(out >= previous - 1e-5, "黑区曲线在 amount={amount} t={t} 处回退");
                previous = out;
            }
        }
    }

    #[test]
    fn blacks_lift_compensation_restores_mid_contrast() {
        let value = |display: f32| display.powf(TONE_LINEAR_GAMMA);
        let lift = |display: f32, amount: f32| {
            blacks_curve(value(display), amount).powf(TONE_PERCEPTUAL_GAMMA) - display
        };
        // 64 级处：补偿把它从 ≈+18 抬到 ≈+21 级（拉满；小幅度时补偿反而压低，见函数注释）。
        assert!(
            lift(0.25, 1.0) > 0.062,
            "补偿后峰值（64 级附近）应明显超过无补偿的 ≈0.072，实测 {}",
            lift(0.25, 1.0)
        );
        // 中段（≈64 级）明显高于暗端（≈26 级）—— 拉回反差，而不是整体平移。
        assert!(lift(0.25, 1.0) > lift(0.10, 1.0) + 0.03);
        // 压暗一侧不走补偿：仍是对称的原始幅度（≈ −0.072）。
        assert!((-0.08..-0.06).contains(&lift(0.25, -1.0)));
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

    /// **种子取「该层输入」**（`compose_linear_at`）：只算到那一层之前，
    /// 在那一层之后的调整**不能**影响种子（2026-10-31 评审发现的真实 bug）。
    #[test]
    fn seed_sampling_only_uses_layers_before_the_target() {
        let source = image(&[[0.2, 0.2, 0.2]]);
        let params = DevelopParams::new(None);
        let curves = CurveSet::identity();
        // 默认层压暗一半、之后一层拉亮 +1EV：默认层之后取到的应当是**压暗过**的
        let stack = layer_stack(vec![full_area(&[("exposure", 1.0)])], None);
        let at_default = {
            let stages = DevelopStages {
                areas: Some(&stack),
                ..DevelopStages::default()
            };
            compose_linear_at(&source, &params, &curves, &stages, 1, 0, 0)
                .expect("合法像素")
        };
        assert!(
            (at_default[0] - 0.2).abs() < 1e-6,
            "默认层（无调整）之后应当还是原值（{at_default:?}）"
        );
        let after_first = {
            let stages = DevelopStages {
                areas: Some(&stack),
                ..DevelopStages::default()
            };
            compose_linear_at(&source, &params, &curves, &stages, 2, 0, 0).expect("合法像素")
        };
        assert!(
            (after_first[0] - 0.4).abs() < 1e-4,
            "+1EV 之后应当是 0.4（{after_first:?}）"
        );
        // 行号在层数之后 = 整链跑完（同一份链），再大也只是"合成全部"
        let stages = DevelopStages {
            areas: Some(&stack),
            ..DevelopStages::default()
        };
        assert!(compose_linear_at(&source, &params, &curves, &stages, 3, 0, 0).is_some());
        assert!(compose_linear_at(&source, &params, &curves, &stages, 9, 0, 0).is_some());
        assert!(compose_linear_at(&source, &params, &curves, &stages, 0, 5, 0).is_none());
        // 没有额外层时：0 = 原始源，1 = 默认层的线性段
        let plain = DevelopStages::default();
        let raw_pixel = compose_linear_at(&source, &params, &curves, &plain, 0, 0, 0)
            .expect("原始源");
        assert!((raw_pixel[0] - 0.2).abs() < 1e-6);
        assert!(
            (compose_linear_at(&source, &params, &curves, &plain, 1, 0, 0)
                .expect("默认层之后")[0]
                - 0.2)
                .abs()
                < 1e-6
        );
    }

    /* ── 编辑区（area）层链（W2）────────────────────────────── */

    /// **种子取样是“小窗中位数”，不是单像素**（2026-11-02 真实照片修复的配套）。
    ///
    /// 单像素会采到热噪点 / 压缩边缘；区域中位数对少量坏值免疫。
    /// 这里用一个 5×5 全 0.2 的图、只在中心放一颗纯红热像素：
    /// 单点必须采到红，`SEED_SAMPLE_RADIUS` 的窗必须回到 0.2。
    #[test]
    fn seed_sampling_uses_a_region_median_not_one_pixel() {
        let base = linear(0.2);
        let mut rgb = vec![base; 5 * 5 * 3];
        let center = (2 * 5 + 2) * 3;
        rgb[center] = linear(1.0);
        rgb[center + 1] = 0;
        rgb[center + 2] = 0;
        let source = LinearImage::new(5, 5, rgb).expect("形状对得上");
        let params = DevelopParams::new(None);
        let curves = CurveSet::identity();
        let stages = DevelopStages::default();

        let single = compose_linear_at(&source, &params, &curves, &stages, 0, 2, 2)
            .expect("中心像素");
        assert!(
            (single[0] - 1.0).abs() < 1e-4 && single[1] < 1e-4,
            "radius 0 应当就是那颗热像素：{single:?}"
        );

        let sampled = compose_linear_median(
            &source,
            &params,
            &curves,
            &stages,
            0,
            2,
            2,
            SEED_SAMPLE_RADIUS,
        )
        .expect("小窗");
        assert!(
            (sampled[0] - 0.2).abs() < 1e-3
                && (sampled[1] - 0.2).abs() < 1e-3
                && (sampled[2] - 0.2).abs() < 1e-3,
            "24 个底色 + 1 个热像素 ⇒ 中位数应回到底色：{sampled:?}"
        );

        // 靠边时窗口夹在图像内，不 panic、也不越界
        let edge = compose_linear_median(&source, &params, &curves, &stages, 0, 0, 0, 99)
            .expect("边缘小窗");
        assert!(
            (edge[0] - 0.2).abs() < 1e-3,
            "窗口夹到图像内后仍应是底色中位数：{edge:?}"
        );
    }

    fn layer_stack(
        areas: Vec<super::super::areas::AreaSpec>,
        default_index: Option<u8>,
    ) -> super::super::areas::LayerStack {
        super::super::areas::LayerStack::from_specs(&areas, default_index, None)
            .expect("测试用的层列表要合法")
    }

    fn full_area(params: &[(&str, f64)]) -> super::super::areas::AreaSpec {
        let mut area = super::super::areas::AreaSpec::full();
        for (id, value) in params {
            area.params.insert((*id).to_string(), *value);
        }
        area
    }

    fn point_area(
        seed: [f32; 3],
        tolerance: f32,
        params: &[(&str, f64)],
    ) -> super::super::areas::AreaSpec {
        let mut area = full_area(params);
        area.kind = super::super::areas::AreaKind::Point {
            x: 0.5,
            y: 0.5,
            seed,
            tolerance,
        };
        area
    }

    /// 一条 16×16 的渐变（亮度从左到右、色相从上到下）。
    fn gradient_image() -> LinearImage {
        let mut rgb = Vec::new();
        for y in 0..16u32 {
            for x in 0..16u32 {
                let luma = x as f32 / 15.0;
                let warm = y as f32 / 15.0;
                rgb.extend([
                    linear(luma),
                    linear(luma * (1.0 - warm * 0.5)),
                    linear(luma * (1.0 - warm)),
                ]);
            }
        }
        LinearImage::new(16, 16, rgb).expect("形状对得上")
    }

    /// **退化等价**：多一层「什么都不做」的层，逐位等于单层快路径。
    #[test]
    fn identity_area_layer_is_bit_identical_to_the_single_layer_path() {
        let source = gradient_image();
        let params = params(&[
            ("exposure", 0.4),
            ("contrast", 25.0),
            ("saturation", 30.0),
            ("blacks", 10.0),
        ]);
        let mut curves = CurveSet::identity();
        curves.rgb =
            crate::develop::Curve::from_points(vec![[0.0, 0.0], [0.5, 0.6], [1.0, 1.0]]).unwrap();
        let plain = render_develop16(&source, &params, &curves, &DevelopStages::default());

        for default_index in [None, Some(0), Some(1)] {
            let stack = layer_stack(vec![super::super::areas::AreaSpec::full()], default_index);
            let stages = DevelopStages {
                areas: Some(&stack),
                ..DevelopStages::default()
            };
            /*
             * 逐点等价到 **1 个 16bit 级**：快路径把「线性链 + 显示变换」合成一张表
             * （一次插值），层链是「线性表 → 显示表」两次插值 —— 中间那一次的
             * 表误差（≈1e-5）在个别像素上会翻成 1 级。这是**精度实现差异**，
             * 不是口径差异；8bit 输出最多差 1 级（量化时贴着舍入边界的那一格）。
             */
            let layered = render_develop16(&source, &params, &curves, &stages);
            let worst = layered
                .iter()
                .zip(&plain)
                .map(|(a, b)| usize::from(a.abs_diff(*b)))
                .max()
                .unwrap_or(0);
            assert!(
                worst <= 1,
                "空层（默认层位置 {default_index:?}）不能改变画面（最大偏差 {worst} 级）"
            );
            assert!(
                quantize_rgb8(&layered) == quantize_rgb8(&plain)
                    || quantize_rgb8(&layered)
                        .iter()
                        .zip(quantize_rgb8(&plain))
                        .all(|(a, b)| a.abs_diff(b) <= 1),
                "8bit 输出最多差 1 级（默认层位置 {default_index:?}）"
            );
        }
    }

    /// **关掉的层整层旁路**（权重 0），但留在链里（顺序 / 名字 / 参数都不动）。
    #[test]
    fn disabled_area_is_bypassed_without_being_removed() {
        let source = gradient_image();
        let params = DevelopParams::new(None);
        let curves = CurveSet::identity();
        let plain = render_develop16(&source, &params, &curves, &DevelopStages::default());

        let mut off = full_area(&[("exposure", 1.5)]);
        off.enabled = false;
        let stack = layer_stack(vec![off], None);
        let stages = DevelopStages {
            areas: Some(&stack),
            ..DevelopStages::default()
        };
        let bypassed = render_develop16(&source, &params, &curves, &stages);
        let worst = bypassed
            .iter()
            .zip(&plain)
            .map(|(a, b)| usize::from(a.abs_diff(*b)))
            .max()
            .unwrap_or(0);
        assert!(
            worst <= 1,
            "关掉的层不能改画面（最大偏差 {worst} 级；表插值差异上限 1）"
        );

        let mut on = full_area(&[("exposure", 1.5)]);
        on.enabled = true;
        let stack = layer_stack(vec![on], None);
        let stages = DevelopStages {
            areas: Some(&stack),
            ..DevelopStages::default()
        };
        assert_ne!(
            render_develop16(&source, &params, &curves, &stages),
            plain,
            "开着的同一层必须真的改到画面"
        );
    }

    /// **层间不夹取**（§4.6）：上一层把高光推出 1，下一层仍然能拉回来。
    #[test]
    fn out_of_range_values_survive_the_display_domain_between_layers() {
        // 中性灰：色度对它没有影响，方便只看「夹没夹」
        let source = image(&[[0.2, 0.2, 0.2]]);
        let params = DevelopParams::new(None);
        let curves = CurveSet::identity();
        let plain = render_develop16(&source, &params, &curves, &DevelopStages::default());

        // 第 1 层（默认层之后）+2EV 并带色度 ⇒ 这一层要跨到显示域；
        // 第 2 层 −2EV 把它拉回来。中间若被夹到 1，结果就是白的而不是原来的 0.2。
        let stack = layer_stack(
            vec![
                full_area(&[("exposure", 2.0), ("saturation", 30.0)]),
                full_area(&[("exposure", -2.0)]),
            ],
            None,
        );
        let stages = DevelopStages {
            areas: Some(&stack),
            ..DevelopStages::default()
        };
        let out = render_develop16(&source, &params, &curves, &stages);
        let back = f32::from(out[0]) / 65535.0;
        let expected = f32::from(plain[0]) / 65535.0;
        assert!(
            (back - expected).abs() < 0.01,
            "两层互相抵消后应当回到原图（实际 {back}，期望 {expected}；被夹过就会接近 1）"
        );
        assert_eq!(out.len(), plain.len());
    }

    /// **层序改变 ⇒ 画面改变**（列表自上而下 = 计算顺序）。
    #[test]
    fn layer_order_changes_the_picture() {
        let source = gradient_image();
        let params = DevelopParams::new(None);
        let curves = CurveSet::identity();
        let exposure = full_area(&[("exposure", 0.8)]);
        let contrast = full_area(&[("contrast", 70.0)]);

        let first = layer_stack(vec![exposure.clone(), contrast.clone()], None);
        let second = layer_stack(vec![contrast, exposure], None);
        let render = |stack: &super::super::areas::LayerStack| {
            let stages = DevelopStages {
                areas: Some(stack),
                ..DevelopStages::default()
            };
            render_develop16(&source, &params, &curves, &stages)
        };
        let a = render(&first);
        let b = render(&second);
        assert_ne!(a, b, "曝光→反差 与 反差→曝光 必须是两种结果");
    }

    /// **容差变大 ⇒ 受影响的像素变多**（权重单调）。
    #[test]
    fn larger_tolerance_affects_more_pixels() {
        let source = gradient_image();
        let params = DevelopParams::new(None);
        let curves = CurveSet::identity();
        let base = render_develop16(&source, &params, &curves, &DevelopStages::default());
        let seed = super::super::areas::feature_vector([0.5, 0.5, 0.5]);
        let affected = |tolerance: f32| {
            let stack = layer_stack(
                vec![point_area(seed, tolerance, &[("exposure", 0.5)])],
                None,
            );
            let stages = DevelopStages {
                areas: Some(&stack),
                ..DevelopStages::default()
            };
            render_develop16(&source, &params, &curves, &stages)
                .iter()
                .zip(&base)
                .filter(|(after, before)| after != before)
                .count()
        };
        // 容差是 100..1000 的**数值域**（映射到两个轴的区间见 `POINT_WEIGHT_CONFIG`）。
        // 这两个取值让「小」只圈住种子附近、「大」明显更宽但还不到全图。
        let small = affected(300.0);
        let large = affected(700.0);
        assert!(small > 0, "种子附近必须真的改到像素");
        assert!(
            large > small,
            "容差变大要让更多像素进入（300 → {small}，700 → {large}）"
        );
        assert!(large < base.len(), "容差再大也不该命中全部像素");
    }

    /// **没有空间项**：采样位（`x` / `y`）怎么动，画面都逐位一样。
    #[test]
    fn moving_the_point_sample_does_not_change_the_pixels() {
        use crate::store::develop::{Area, AreaKind as StoreKind};

        let source = gradient_image();
        let params = DevelopParams::new(None);
        let curves = CurveSet::identity();
        let make = |x: f32, y: f32| Area {
            id: 1,
            kind: StoreKind::Point {
                x,
                y,
                seed: [0.4, 0.55, 0.5],
                tolerance: 0.2,
            },
            name: String::new(),
            enabled: true,
            opacity: 1.0,
            color: 0,
            params: [("exposure".to_string(), 0.6)].into(),
            curves: std::collections::BTreeMap::new(),
            seed_version: crate::store::develop::SEED_VERSION,
        };
        let render = |area: &Area| {
            let stack = super::super::areas::LayerStack::from_specs(
                &[area.to_spec()],
                None,
                None,
            )
            .expect("合法层");
            let stages = DevelopStages {
                areas: Some(&stack),
                ..DevelopStages::default()
            };
            render_develop16(&source, &params, &curves, &stages)
        };
        assert_eq!(
            render(&make(0.1, 0.9)),
            render(&make(0.95, 0.05)),
            "控制点只看向量，采样位不同不该改任何像素"
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
