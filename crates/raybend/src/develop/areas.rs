//! **编辑区（area）的渲染侧形态** —— 层列表 + 控制点权重（`specs/editor-area.md` §4.2）。
//!
//! # 为什么单独一个模块
//!
//! 本模块只装「一层是什么」与「控制点的权重怎么算」这两件**纯数学**的事：
//!
//! * [`AreaSpec`] / [`AreaKind`]：与存储无关的一层（`store::develop::Area` 由
//!   `store` 那边转成它 —— 本模块**不依赖 store**，`develop` 仍然是纯数学）；
//! * [`LayerStack`]：层顺序 + 默认层的位置，参数与曲线已经在这里解析成管线形态；
//! * [`feature_vector`] / [`point_weight`]：控制点的全图向量匹配（**没有空间项**）。
//!
//! 逐像素的求值在 `pipeline` 里（它要用查表、色度、LUT 那些东西）；这里只给
//! 「这一层长什么样」与「这个像素给这层多少权重」。
//!
//! # 口径（`specs/editor-area.md` §4.2，违反任何一条都要返工）
//!
//! 1. **控制点不是蒙版**：`seed` 是创建时冻结的特征向量，`tolerance` 是**向量距离**的
//!    尺度；`x` / `y` 只是采样位（取种子、画标记），**不参与权重**。
//! 2. 权重 `α = V(d / tolerance)`，`V(x) = 0.5^(x·k)`，`k` 取到 `V(1) ≈ 0.02`。
//! 3. 权重 = **亮度轴 × 色相轴**两个独立的两段式区间（见 [`PointWeightConfig`]）：
//!    亮度轴放得开、色相轴收得紧 —— 点住一件绿衣服，选中的是「这一亮度范围里、
//!    色相接近绿色」的像素，而不是「所有亮度相近的像素」。

use std::collections::BTreeMap;

use super::curve::{Curve, CurveChannel, CurveSet};
use super::params::{DevelopParams, ParamTarget, target as param_target};

/// 编辑区层数上限（**不含默认层**；`memory/LIMITS.md`）。
///
/// 与 `store::develop::MAX_AREAS` 同一个数 —— 那条是数据侧的真相（DB / 校验都用它），
/// 这里只在渲染侧做一次防御（越界的列表直接截断，不让一次坏数据拖垮整张图）。
pub const MAX_AREAS: usize = 32;

/// 一层的类型（默认层不在枚举里：它就是栈本身）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AreaKind {
    /// 用户建的全幅调整层：权重恒 1。
    Full,
    /// 控制点：冻结的种子向量 + 容差（**全图向量匹配，没有空间项**）。
    Point {
        /// 采样位（源图归一化坐标）。**只用来取种子、画标记**，不进权重。
        ///
        /// 渲染侧本来不需要它，但**种子取样需要**：落点之后要按
        /// 「该层输入 = 在它之前的所有层」重新算一次，位置得留着。
        x: f32,
        y: f32,
        /// 创建时冻结的特征向量 `[luma, c1, c2]`（见 [`feature_vector`]）。
        seed: [f32; 3],
        /// 归一化向量距离的尺度（`0..=1`）；`0` = 只认完全一致的像素。
        tolerance: f32,
    },
}

/// 一层（与存储无关的形态）。
#[derive(Debug, Clone, PartialEq)]
pub struct AreaSpec {
    pub kind: AreaKind,
    /// 层启停：`false` = 整层旁路（不删）
    pub enabled: bool,
    /// 0..=1（近期没有界面）
    pub opacity: f32,
    /// 只装 `Layered` 参数（`Global` 的由调用方挡在门外）
    pub params: BTreeMap<String, f64>,
    /// 通道 → 控制点
    pub curves: BTreeMap<String, Vec<[f32; 2]>>,
}

impl AreaSpec {
    /// 一层空的全图调整层。
    #[must_use]
    pub fn full() -> Self {
        Self {
            kind: AreaKind::Full,
            enabled: true,
            opacity: 1.0,
            params: BTreeMap::new(),
            curves: BTreeMap::new(),
        }
    }
}

/// 已经解析成管线形态的一层（参数 / 曲线都过了校验）。
#[derive(Debug, Clone, PartialEq)]
pub struct AreaLayer {
    pub kind: AreaKind,
    pub enabled: bool,
    pub opacity: f32,
    pub params: DevelopParams,
    pub curves: CurveSet,
}

/// 层列表（默认层 + 额外层），`default_index` 是默认层在**计算顺序**里的位置。
#[derive(Debug, Clone, PartialEq)]
pub struct LayerStack {
    pub areas: Vec<AreaLayer>,
    /// 默认层的位置（0-based，`0..=areas.len()`）。
    pub default_index: usize,
}

impl LayerStack {
    /// 从存储侧的层列表建（**唯一入口**）。
    ///
    /// `as_shot` 是这张照片的拍摄色温：非默认层的色温参数也是**相对 as-shot** 的
    /// （`DevelopParams` 的基线规则全栈一致，否则「同一条色温拉杆」在两层里颜色不同）。
    ///
    /// # Errors
    /// 参数非法 / 曲线非法 / 层数超限 / 默认层位置越界。
    pub fn from_specs(
        specs: &[AreaSpec],
        default_index: Option<u8>,
        as_shot: Option<f32>,
    ) -> Result<Self, String> {
        if specs.len() > MAX_AREAS {
            return Err(format!("编辑区最多 {MAX_AREAS} 层"));
        }
        let mut areas = Vec::with_capacity(specs.len());
        for spec in specs {
            areas.push(AreaLayer::from_spec(spec, as_shot)?);
        }
        let layers = areas.len() + 1;
        let default_index = default_index.map_or(0, usize::from);
        if default_index >= layers {
            return Err("默认层的位置超出了层列表".into());
        }
        Ok(Self {
            areas,
            default_index,
        })
    }

    /// 只有默认层（退化形状）吗。
    #[must_use]
    pub fn is_degenerate(&self) -> bool {
        self.areas.is_empty()
    }

    /// 层总数（含默认层）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.areas.len() + 1
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl AreaLayer {
    fn from_spec(spec: &AreaSpec, as_shot: Option<f32>) -> Result<Self, String> {
        let kind = match spec.kind {
            AreaKind::Full => AreaKind::Full,
            AreaKind::Point {
                x,
                y,
                seed,
                tolerance,
            } => {
                // 坏数据也不能在权重里算出一个 NaN 扫掉整张图（§4.2）
                let clamp = |value: f32, low: f32, high: f32| {
                    if value.is_finite() {
                        value.clamp(low, high)
                    } else {
                        low.max(0.0)
                    }
                };
                AreaKind::Point {
                    x: clamp(x, 0.0, 1.0),
                    y: clamp(y, 0.0, 1.0),
                    // 种子是 `[Y′, Cb, Cr]`：亮度 0..1，两个色差 ±0.5
                    seed: [
                        clamp(seed[0], 0.0, 1.0),
                        clamp(seed[1], -0.5, 0.5),
                        clamp(seed[2], -0.5, 0.5),
                    ],
                    // 容差是 100..1000 的数值域（不是 0..1）
                    tolerance: clamp(tolerance, TOLERANCE_MIN, TOLERANCE_MAX),
                }
            }
        };
        // 与 `Area::valid` 同一条阶段边界：全局 / 掩膜参数不能进层
        //（DB / DTO 之外的调用方也走这里，不能把全局阶段偷偷算进某一层）。
        for id in spec.params.keys() {
            if param_target(id) != Some(ParamTarget::Layered) {
                return Err(format!("{id} 不是逐像素层参数，不能写进编辑区层"));
            }
        }
        let params = DevelopParams::from_values(
            spec.params.iter().map(|(id, value)| (id.clone(), *value)),
            as_shot,
        )?;
        // 色温的基线随照片：`DevelopParams::from_values` 已按 `as_shot` 解释它 ——
        // 没传 as_shot 时退回表里的默认值（6250），与默认层同一条规则。
        let mut curves = CurveSet::identity();
        for (channel, points) in &spec.curves {
            let Some(channel) = CurveChannel::parse(channel) else {
                return Err(format!("未知的曲线通道：{channel}"));
            };
            curves.set_channel(channel, Curve::from_points(points.clone())?);
        }
        Ok(Self {
            kind,
            enabled: spec.enabled,
            opacity: spec.opacity.clamp(0.0, 1.0),
            params,
            curves,
        })
    }
}

/* ══════════════════════════════════════════════════════════════
 * 控制点权重：**亮度 × 色相** 两个独立的两段式区间（崔总 2026-11-01 定）
 * ══════════════════════════════════════════════════════════════
 *
 * ```text
 *   α
 *   1 ┤━━━━━━━━━━┓                       每个轴各自这样：
 *     │ 100% 应用  ┗━━╲                     [0, core] = 1
 *   0 ┴──────────────╲──┴────→ Δ            [core, end] = smoothstep 1 → 0
 *                    core end               [end, ∞) = 0
 *
 *   α = band_luma(|ΔY′|) × band_chroma(‖Δ(Cb,Cr)‖)
 * ```
 *
 * 为什么不是「三个分量合成一个距离再比一个容差」（旧模型）：
 * 那样两个轴被同一个标量绑死 —— 想把一件衣服的受光面与阴影都选上，就必须把
 * 「同亮度但别的颜色」一起拉进来。分成两个轴相乘之后，亮度可以放得很宽而色相仍然收着。
 */

/// 容差值的**数值域**（界面上的 100..1000；存 `f32`，连续、没有台阶）。
///
/// 它是「容差」的唯一单位：`Area.tolerance`、手柄拖动、调试面板都按这套数字说话。
/// 映射到实际的完全容差 / 过渡容差由 [`PointWeightConfig`] 决定。
pub const TOLERANCE_MIN: f32 = 100.0;
pub const TOLERANCE_MAX: f32 = 1000.0;
pub const TOLERANCE_DEFAULT: f32 = 300.0;

/// 一个轴上的两段式区间。
///
/// * `core`（**完全容差**）：到这儿都是 100% 应用；
/// * `end`（**过渡容差**）：到这儿完全不处理 —— 中间那一段平滑降到 0。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub core: f32,
    pub end: f32,
}

impl Band {
    /// 夹成合法形状（`end` 不能小于 `core`，否则没有渐变段）。
    #[must_use]
    pub fn sanitized(self) -> Self {
        let core = if self.core.is_finite() { self.core.max(0.0) } else { 0.0 };
        let end = if self.end.is_finite() { self.end.max(core) } else { core };
        Self { core, end }
    }

    /// 这个值落在这个轴上多少权重：`core` 内 1、`end` 外 0、中间 smoothstep。
    #[must_use]
    pub fn weight(self, value: f32) -> f32 {
        if !value.is_finite() {
            return 0.0;
        }
        let band = self.sanitized();
        if value <= band.core {
            return 1.0;
        }
        if value >= band.end {
            return 0.0;
        }
        let span = band.end - band.core;
        if !(span > 1e-6) {
            return 0.0;
        }
        let t = ((value - band.core) / span).clamp(0.0, 1.0);
        // smoothstep：`1 − t²(3−2t)`，两端斜率为 0 —— 边界上看不到一圈硬边
        1.0 - t * t * (3.0 - 2.0 * t)
    }
}

/// 控制点权重的**唯一配置**（改手感改这里；开发期的调试面板可以临时覆盖它）。
///
/// 每个字段都是 `[容差 = 100 时的值, 容差 = 1000 时的值]`，中间**线性插值**。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointWeightConfig {
    /// 亮度轴：完全容差（`Y′`，0..1）
    pub luma_core: [f32; 2],
    /// 亮度轴：过渡终点（`Y′`，可 > 1 = 永远走不完）
    pub luma_end: [f32; 2],
    /// 色相轴：完全容差（`‖Δ(Cb,Cr)‖`，约 0..0.7）
    pub chroma_core: [f32; 2],
    /// 色相轴：过渡终点
    pub chroma_end: [f32; 2],
}

/// 控制点权重的**默认配置**（调试面板的「恢复默认」也读它）。
///
/// 取值来自一组实测样本：绿色衣服的明暗跨度 `ΔY′ ≈ 0.22`、同一件衣服的
/// `ΔC ≈ 0.05`、同亮度的红色 `ΔC ≈ 0.25` —— 容差 300 时亮度放得开、色相收得紧。
pub const POINT_WEIGHT_CONFIG: PointWeightConfig = PointWeightConfig {
    luma_core: [0.05, 0.95],
    luma_end: [0.12, 1.40],
    chroma_core: [0.015, 0.10],
    chroma_end: [0.05, 0.28],
};

impl Default for PointWeightConfig {
    fn default() -> Self {
        POINT_WEIGHT_CONFIG
    }
}

impl PointWeightConfig {
    /// 容差值 → `0..1` 的插值参数（夹在 `TOLERANCE_MIN..=TOLERANCE_MAX`）。
    #[must_use]
    pub fn position(tolerance: f32) -> f32 {
        if !tolerance.is_finite() {
            return 0.0;
        }
        ((tolerance - TOLERANCE_MIN) / (TOLERANCE_MAX - TOLERANCE_MIN)).clamp(0.0, 1.0)
    }

    fn lerp(pair: [f32; 2], t: f32) -> f32 {
        pair[0] + (pair[1] - pair[0]) * t
    }

    /// 这个容差值下，亮度轴的（完全容差, 过渡终点）。
    #[must_use]
    pub fn luma_band(&self, tolerance: f32) -> Band {
        let t = Self::position(tolerance);
        Band {
            core: Self::lerp(self.luma_core, t),
            end: Self::lerp(self.luma_end, t),
        }
        .sanitized()
    }

    /// 这个容差值下，色相轴的（完全容差, 过渡终点）。
    #[must_use]
    pub fn chroma_band(&self, tolerance: f32) -> Band {
        let t = Self::position(tolerance);
        Band {
            core: Self::lerp(self.chroma_core, t),
            end: Self::lerp(self.chroma_end, t),
        }
        .sanitized()
    }
}

/// sRGB 编码表的长度（每个像素要算 3 次，`powf` 太贵）。
const ENCODE_LEN: usize = 4097;

fn encode_table() -> &'static [f32; ENCODE_LEN] {
    static TABLE: std::sync::OnceLock<[f32; ENCODE_LEN]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0.0f32; ENCODE_LEN];
        let denominator = (ENCODE_LEN - 1) as f32;
        for (index, slot) in table.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let linear = index as f32 / denominator;
            *slot = super::color::linear_to_srgb(linear);
        }
        table
    })
}

/// 线性光 → sRGB 显示值（0..1），查表 + 线性插值；域外按端点夹取。
#[inline]
fn encode_linear(value: f32) -> f32 {
    if !value.is_finite() || value <= 0.0 {
        return 0.0;
    }
    if value >= 1.0 {
        return 1.0;
    }
    let table = encode_table();
    let scaled = value * (ENCODE_LEN - 1) as f32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = scaled.floor() as usize;
    if index + 1 >= ENCODE_LEN {
        return table[ENCODE_LEN - 1];
    }
    let fraction = scaled - index as f32;
    table[index] + (table[index + 1] - table[index]) * fraction
}

/// 特征向量：`[Y′, Cb, Cr]`（**感知域**：先 sRGB 编码，再按 BT.709 求亮度 / 色差）。
///
/// * `Y′` 在 `0..=1`；中性灰的 `Cb = Cr = 0`；
/// * 用**感知域**而不是线性：线性下暗部挤在 0 附近，同一个容差在阴影里很宽、
///   在高光里很窄 —— 手感必然「一格一格」。编码之后再量，容差在整条影调上近似等感；
/// * 用**色度平面距离**而不是色相角：近中性像素没有稳定的色相角（角度会跳变），
///   平面距离天然把「很淡的颜色」放在原点附近 —— 中性种子只命中中性像素，
///   彩色种子不会命中灰区，语义自洽。
///
/// 这个函数就是「种子怎么来」的唯一口径：落点取样与求值必须用同一个。
#[must_use]
pub fn feature_vector(linear: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = linear.map(encode_linear);
    let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    /*
     * 夹回文档承诺的范围（`Y′ ∈ 0..=1`，`Cb/Cr ∈ -0.5..=0.5`）。
     * 数学上极值正好落在 ±0.5，但 f32 的乘加会把它抬到 0.5000001 ——
     * 存储层 `Area::valid` 拿 `..=0.5` 卡种子时就会把一颗**刚取出来的**合法种子
     * 判成非法（2026-11-01 崔总报的「调节容差时报错」）。这里夹一下，
     * 取样、求值、落库三处从此同一个范围。
     */
    [
        luma.clamp(0.0, 1.0),
        ((b - luma) / 1.8556).clamp(-0.5, 0.5),
        ((r - luma) / 1.5748).clamp(-0.5, 0.5),
    ]
}

/// 两个特征向量在**色相轴**上的距离（`Cb`/`Cr` 平面上的欧氏距离）。
#[must_use]
pub fn chroma_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dc1 = a[1] - b[1];
    let dc2 = a[2] - b[2];
    (dc1 * dc1 + dc2 * dc2).sqrt()
}

/// 控制点的权重（默认配置；给测试与老调用方的入口）。
#[must_use]
pub fn point_weight(seed: [f32; 3], tolerance: f32, linear: [f32; 3]) -> f32 {
    point_weight_with(seed, tolerance, linear, &POINT_WEIGHT_CONFIG)
}

/// 控制点的权重：**亮度 × 色相**两个独立的两段式区间相乘（§4.2）。
///
/// * `ΔY′ ≤ luma.core` **且** `ΔC ≤ chroma.core` ⇒ 1（满强度）；
/// * 任一轴走到它的 `end` ⇒ 0；
/// * 中间各自平滑下降，乘起来就是「必须同时接近」—— 不会出现「一个轴很远、
///   另一个很近把总分拉回来」的补偿（旧的三分量欧氏距离就有这个毛病）。
///
/// 权重只依赖**颜色**，与像素的空间位置无关（§4.2 的硬口径）。
#[must_use]
pub fn point_weight_with(
    seed: [f32; 3],
    tolerance: f32,
    linear: [f32; 3],
    config: &PointWeightConfig,
) -> f32 {
    let vector = feature_vector(linear);
    let luma = config.luma_band(tolerance).weight((vector[0] - seed[0]).abs());
    if luma <= 0.0 {
        return 0.0;
    }
    let chroma = config
        .chroma_band(tolerance)
        .weight(chroma_distance(vector, seed));
    luma * chroma
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec_full(params: &[(&str, f64)]) -> AreaSpec {
        let mut area = AreaSpec::full();
        for (id, value) in params {
            area.params.insert((*id).to_string(), *value);
        }
        area
    }

    #[test]
    fn feature_vector_is_perceptual_and_neutrals_have_no_chroma() {
        // 中性灰：编码后仍是中性 ⇒ Cb = Cr = 0（这是「灰的色相没有意义」的正确表达）
        for level in [0.05f32, 0.2, 0.5, 0.9] {
            let grey = feature_vector([level, level, level]);
            assert!(grey[1].abs() < 1e-6, "中性灰的 Cb 必须是 0");
            assert!(grey[2].abs() < 1e-6, "中性灰的 Cr 必须是 0");
            assert!((0.0..=1.0).contains(&grey[0]));
        }
        // 感知域：中灰 0.2 线性 → Y′ ≈ 0.48（而不是线性那个 0.2）
        let grey = feature_vector([0.2, 0.2, 0.2]);
        assert!((grey[0] - 0.484).abs() < 0.01, "Y′ 必须在感知域（实际 {}）", grey[0]);
        // 纯色落在色度平面上，且和灰的色度距离明显
        let red = feature_vector([1.0, 0.0, 0.0]);
        let blue = feature_vector([0.0, 0.0, 1.0]);
        assert!(red[2] > 0.4, "纯红的 Cr 应当明显（实际 {}）", red[2]);
        assert!(blue[1] > 0.4, "纯蓝的 Cb 应当明显（实际 {}）", blue[1]);
        assert!(chroma_distance(red, blue) > 0.4);
        assert!(chroma_distance(grey, grey) < 1e-6);
        // 域外值不会把向量推出去
        for value in feature_vector([-3.0, 2.0, 9.0]) {
            assert!(value.is_finite() && (-2.0..=2.0).contains(&value));
        }
    }

    /// 取样结果必须**总是**落在存储层承诺的范围里（`Y′ 0..=1`、`Cb/Cr ±0.5`）。
    ///
    /// 数学上极值正好卡在边界，f32 的乘加会把纯蓝/纯红抬到 0.5000001；
    /// 不夹的话，存储层 `Area::valid` 会把刚取出来的种子判成非法。
    #[test]
    fn feature_vector_always_stays_inside_the_stored_ranges() {
        let extremes = [
            [1.0f32, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0],
        ];
        for linear in extremes {
            let seed = feature_vector(linear);
            assert!(
                (0.0..=1.0).contains(&seed[0]),
                "Y′ 越界：{linear:?} → {seed:?}"
            );
            for channel in &seed[1..] {
                assert!(
                    (-0.5..=0.5).contains(channel),
                    "Cb/Cr 越界：{linear:?} → {seed:?}"
                );
            }
        }
    }

    #[test]
    fn band_is_flat_inside_the_core_and_smooth_down_to_the_end() {
        let band = Band { core: 0.2, end: 0.4 };
        assert_eq!(band.weight(0.0), 1.0);
        assert_eq!(band.weight(0.2), 1.0, "core 边界上仍然是 100%");
        assert_eq!(band.weight(0.4), 0.0, "end 上必须完全是 0");
        assert_eq!(band.weight(0.5), 0.0);
        let middle = band.weight(0.3);
        assert!((middle - 0.5).abs() < 1e-3, "渐变区中点 ≈ 0.5（实际 {middle}）");
        // 单调 + 无阶跃
        let mut previous = 1.0;
        for step in 0..=64 {
            let value = 0.2 + 0.2 * step as f32 / 64.0;
            let weight = band.weight(value);
            assert!(weight <= previous + 1e-6, "渐变区必须单调不增");
            assert!(previous - weight < 0.05, "渐变区不能有阶跃");
            previous = weight;
        }
        // 坏区间（end < core）不产生 NaN，也不产生「无过渡的硬边」
        let broken = Band { core: 0.4, end: 0.2 }.sanitized();
        assert_eq!(broken.core, 0.4);
        assert_eq!(broken.end, 0.4);
        assert_eq!(broken.weight(0.3), 1.0);
        assert_eq!(broken.weight(0.5), 0.0);
    }

    #[test]
    fn point_weight_is_the_product_of_the_two_bands() {
        let seed_linear = [0.4f32, 0.4, 0.4];
        let seed = feature_vector(seed_linear);
        assert!(
            (point_weight(seed, TOLERANCE_DEFAULT, seed_linear) - 1.0).abs() < 1e-5,
            "种子像素必须满权重"
        );
        let luma_band = POINT_WEIGHT_CONFIG.luma_band(TOLERANCE_DEFAULT);
        let chroma_band = POINT_WEIGHT_CONFIG.chroma_band(TOLERANCE_DEFAULT);
        // 中性灰到中性灰：色相距离恒为 0 ⇒ 权重 == 亮度轴的权重（可直接对表）
        for level in [0.02f32, 0.1, 0.4, 0.8, 0.99] {
            let pixel = [level, level, level];
            let vector = feature_vector(pixel);
            let dl = (vector[0] - seed[0]).abs();
            assert!(chroma_distance(vector, seed) < 1e-6, "灰对灰的色相距离必须是 0");
            let expected = luma_band.weight(dl) * chroma_band.weight(0.0);
            let actual = point_weight(seed, TOLERANCE_DEFAULT, pixel);
            assert!(
                (actual - expected).abs() < 1e-5,
                "权重必须是两个区间的乘积（level {level}：{actual} vs {expected}）"
            );
        }
        // 极端容差（最小 / 最大 / 越界）都不出 NaN，且落在 0..1
        for tolerance in [TOLERANCE_MIN, TOLERANCE_MAX, -50.0, 5000.0] {
            let weight = point_weight(seed, tolerance, [0.5, 0.5, 0.5]);
            assert!(weight.is_finite() && (0.0..=1.0).contains(&weight));
        }
        // 容差越大，同一个像素的权重不会变小
        let pixel = [0.9f32, 0.9, 0.9];
        let mut previous = -1.0;
        for tolerance in [100.0f32, 250.0, 400.0, 600.0, 800.0, 1000.0] {
            let weight = point_weight(seed, tolerance, pixel);
            assert!(
                weight >= previous - 1e-6,
                "容差变大不能让权重变小（{tolerance}：{weight} < {previous}）"
            );
            previous = weight;
        }
    }

    /// **「亮度 × 色相」的核心断言**（崔总 2026-11-01 真机问题的回归）：
    /// 同亮度、不同色相必须被排除；同色相、亮度差在容差内必须被选中。
    #[test]
    fn same_luma_different_hue_is_rejected() {
        // 绿色衣服 vs 同亮度的红色（实测样本：ΔY′ ≈ 0.03，ΔC ≈ 0.25）
        let green = [0.06f32, 0.20, 0.08];
        let seed = feature_vector(green);
        let red = [0.36f32, 0.09, 0.08];
        let red_vector = feature_vector(red);
        assert!(
            (red_vector[0] - seed[0]).abs() < 0.06,
            "这一对样本的亮度必须接近，否则测不到「同亮度不同色相」"
        );
        assert!(
            chroma_distance(red_vector, seed) > 0.2,
            "这一对样本的色相必须明显不同"
        );
        for tolerance in [TOLERANCE_MIN, 300.0, 600.0] {
            assert_eq!(
                point_weight(seed, tolerance, red),
                0.0,
                "同亮度、不同色相的像素在常用容差下不该被选中（容差 {tolerance}）"
            );
        }
        // 容差拉到最大时色相闸门也开到最宽 —— 允许一点点串色，但必须很小
        assert!(
            point_weight(seed, TOLERANCE_MAX, red) < 0.1,
            "即使容差拉满，同亮度的别的色相也只能有极小的权重"
        );

        // 同一件衣服的受光面：亮度与色相都在容差内 ⇒ 必须被选中
        let lit = [0.15f32, 0.50, 0.20];
        let lit_vector = feature_vector(lit);
        assert!(
            (lit_vector[0] - seed[0]).abs() < POINT_WEIGHT_CONFIG.luma_band(600.0).core,
            "受光面的亮度差应当落在完全容差内"
        );
        assert!(
            point_weight(seed, 600.0, lit) > 0.99,
            "同一件衣服的受光面在容差 600 时必须是满强度（实际 {}）",
            point_weight(seed, 600.0, lit)
        );
        assert!(
            point_weight(seed, TOLERANCE_DEFAULT, lit) > 0.7,
            "默认容差下，同一件衣服的受光面也该是主要命中（实际 {}）",
            point_weight(seed, TOLERANCE_DEFAULT, lit)
        );
    }

    #[test]
    fn point_weight_grows_with_tolerance_and_ignores_position() {
        let seed = feature_vector([0.3f32, 0.3, 0.3]);
        // 一个离种子足够远的亮像素（ΔY′ ≈ 0.37）：容差从小往大扫时它的权重必须单调上升
        let pixel = [0.9f32, 0.9, 0.9];
        let mut last = -1.0;
        let mut steps = 0;
        for tolerance in [300.0f32, 315.0, 330.0, 345.0, 360.0] {
            let weight = point_weight(seed, tolerance, pixel);
            assert!(
                weight > last,
                "容差变大 ⇒ 同一像素的权重必须变大（{tolerance} → {weight}）"
            );
            last = weight;
            steps += 1;
        }
        assert_eq!(steps, 5);
        // 入参只有颜色与容差：函数签名本身就钉着「没有位置」
        assert_eq!(
            point_weight(seed, 400.0, pixel),
            point_weight(seed, 400.0, pixel)
        );
    }

    #[test]
    fn layer_stack_keeps_the_default_position_and_parses_layers() {
        let stack = LayerStack::from_specs(
            &[
                spec_full(&[("exposure", 0.5)]),
                AreaSpec {
                    kind: AreaKind::Point {
                        x: 0.3,
                        y: 0.7,
                        seed: [0.5, 0.5, 0.5],
                        tolerance: TOLERANCE_DEFAULT,
                    },
                    ..AreaSpec::full()
                },
            ],
            Some(1),
            Some(5200.0),
        )
        .expect("合法层列表");
        assert_eq!(stack.len(), 3);
        assert_eq!(stack.default_index, 1);
        assert!((stack.areas[0].params.value("exposure") - 0.5).abs() < 1e-9);
        assert!((stack.areas[0].params.value("temperature") - 5200.0).abs() < 1e-3);

        // 默认层位置越界 / 未知参数都要当场报错（不静默纠正）
        assert!(LayerStack::from_specs(&[], Some(1), None).is_err());
        assert!(
            LayerStack::from_specs(&[spec_full(&[("nope", 1.0)])], None, None).is_err()
        );
        // 退化形状：没有额外层时只有一个默认层
        let degenerate = LayerStack::from_specs(&[], None, None).expect("退化形状");
        assert!(degenerate.is_degenerate());
        assert_eq!(degenerate.len(), 1);
        assert_eq!(degenerate.default_index, 0);
    }
}
