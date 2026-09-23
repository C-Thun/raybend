//! **动态反差**（dynamicContrast）：局部色调映射 —— 压整体光比、抬局部反差。
//!
//! # 它解决的是什么
//!
//! 一张逆光照片的问题从来不是「暗部不够亮」，而是**整体光比太大**：把暗部提亮就会
//! 灰掉、把亮部压住就会闷。逐像素的曲线（曝光 / 反差 / 高光 / 黑区）在这件事上有个
//! 数学上的天花板 —— 它们是**一条曲线**，整体压光比意味着曲线平均斜率 < 1，
//! 于是任意两点的差值也被一起压小，**局部反差跟着塌**。要么整张更平，要么整张更锐。
//!
//! 这一支算子多了一个**空间维度**，于是两件事可以同时做：
//!
//! ```text
//! ① L = log2(亮度)                       ← 乘性曝光差变加性
//! ② 边缘保持多尺度分解（filters::GuidedModel）
//!      B1 = 引导滤波(L, 细半径)   D1 = L  − B1   ← 细尺度（质感 / 纹理）
//!      B2 = 引导滤波(B1, 粗半径)  D2 = B1 − B2   ← 中尺度（结构 / clarity）
//! ③ base 压缩：T(B2)                     ← 「降整体光比」（绕中点的软压缩，黑场锚住）
//! ④ detail 分级增益：D_i + gate·boost(D_i)  ← 「抬局部反差」（带软限幅，防 halo）
//! ⑤ L' = T(B2) + Σ…，再按 2^(L'−L) 缩回 RGB ← 只动亮度，保色相
//! ```
//!
//! # 为什么这样切分（每一条都有反面教训）
//!
//! * **`analyze` 与 `apply` 分开**：分解（贵，要过整张图）只依赖图像与尺度参数，
//!   **与强度无关**；强度只驱动压缩量、增益与重组（便宜）。于是渲染线程可以把
//!   [`LocalToneState`] 与线性源一起常驻，拖动拉杆只跑 `apply` —— 否则 24MP 每帧
//!   重跑一遍分解，拉杆会「拖一格就断」（`AGENTS.md` §11.4 那两次教训的同一种病）。
//! * **压缩绕中点做软压缩**（有理式 `d/(1+α|d|)`）而不是线性乘一个系数：
//!   线性压缩会把中灰附近的反差一起压掉（画面「发闷」）；有理式在支点处斜率为 1，
//!   离支点越远压得越狠 —— 观感上正是「中间还留着劲儿、两头收进来」。
//! * **不做「黑场锚」**（曾经有过，已删）：它的想法是「最深处不许被抬」，但那正是这个
//!   算子要做的事，而且锚在 p02 上一盖就是一大片（平滑渐变图的 p05 与 p02 几乎重合），
//!   实测把光比压缩从 55% 弄成了 86%。真正需要守住的两件事另有人管：
//!   ① 纯黑由**乘法**天然保住（`0 × 任何倍率 = 0`）；② 病态的过度抬升由 `base_limit` 封顶。
//!   用户想要更深的黑场，用 `blacks` 参数 —— 不该在这里偷偷替他设。
//! * **detail 增益带软饱和**：`|g·d| ≤ limit` 时增量**完全按斜率 `g` 生效**，
//!   超过才平滑收住。硬增益会把强边缘推成光晕（halo）与「塑料 HDR 感」，
//!   而软饱和是**自限制**的：小细节照常放大，大跳变自动收住。
//! * **四道门控**（阴影 / 噪点 / 端点 / 局部对比）见 [`Gate`] —— 少了任何一道，
//!   效果在某一类照片上就会翻车。
//! * **只动亮度、按比例缩 RGB**：色相与饱和度关系不变（比逐通道曲线安全）。
//!
//! # 参考与出处
//!
//! 这一支不是自创：base/detail 分解 + 分级增益是色调映射的主线（Tumblin & Turk 1999 的
//! 「压缩大尺度、保留小尺度」→ Durand & Dorsey 2002 的双边 → Farbman et al. 2008 的 WLS
//! 多尺度 → He et al. 2010 的引导滤波 → Mantiuk et al. 2006 的 detail factor 参数）。
//! 这里做的是把它们落成一条**可实时、可单测、带门控**的实现。
//!
//! # 已知边界
//!
//! * **会放大噪点**：`D1` 里就是噪点。门控压住了平坦区的增益，但真正干净的解法是先降噪
//!   （M3-W4 的 `lumaNr`）—— 所以这个模块在管线里排在降噪**之后**是更顺的次序。
//! * **强度 0 必须逐位恒等**：`apply_inplace` 在 `strength <= 0` 时直接返回，
//!   且所有增量在 `g = 0` 时严格为 0 —— 单测钉住了这条。

use super::filters::{GuidedModel, Plane, RowUpsampler};
use super::pipeline::{LinearImage, luma_of};

/// 最小的可表示线性亮度（= u16 编码里的 1）—— `log2` 的地板。
const MIN_LINEAR: f32 = 1.0 / 65535.0;

/// 亮度倍率的硬上限（安全网：只在病态输入上才会碰到，正常范围远小于它）。
const MAX_RATIO: f32 = 16.0;
/// 亮度倍率的硬下限（同上）。
const MIN_RATIO: f32 = 1.0 / 16.0;

/// 参数范围下限（`hi − lo` 不许比它小，否则除零）。
const MIN_SPAN: f32 = 1e-3;

/// 分位数用的直方图格数。
const PERCENTILE_BINS: usize = 1024;

/// **动态反差的可调参数**（生产值就是 [`Default`]；探针与测试可以改它们做对比）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalToneOpts {
    /// 细尺度分解的窗口半径（**分析分辨率**下；分析图通常是全图的 1/4，所以实际观感半径 ×4）
    pub fine_radius: usize,
    /// 中尺度分解的窗口半径（同上）
    pub coarse_radius: usize,
    /// 细尺度引导滤波的正则项（log2 亮度的平方）—— 越小越贴边、detail 越少
    pub fine_eps: f32,
    /// 中尺度引导滤波的正则项（同上）
    pub coarse_eps: f32,
    /// 强度 1 时 base 保留的比例（1 = 完全不压；越小压得越狠）
    pub compression_floor: f32,
    /// 强度 1 时细尺度细节的增益（0.45 = 多出 45%）
    pub fine_boost: f32,
    /// 强度 1 时中尺度细节的增益
    pub coarse_boost: f32,
    /// 细尺度**增量**的软限幅（log2 单位：0.35 ≈ 最多少加 1.27 倍）
    pub fine_limit: f32,
    /// 中尺度**增量**的软限幅（log2 单位：1.0 ≈ 最多少加 2 倍）
    pub coarse_limit: f32,
    /// base 压缩量的软饱和起点（log2 单位）：以下完整生效，以上渐近到 2×
    pub base_limit: f32,
    /// 阴影门控的下限（0.35 = 暗部最多只加 35% 的细节增益）
    pub shadow_floor: f32,
    /// 噪点门控的参考幅度（log2 单位）：局部细节幅度远小于它 ⇒ 判为平坦区 ⇒ 不增益
    pub noise_reference: f32,
    /// 端点门控在高光端最多削掉多少增益（0.7 = 最多少加 70%）
    pub highlight_cut: f32,
}

impl Default for LocalToneOpts {
    fn default() -> Self {
        Self {
            // 分析图 1/4 分辨率 + 半径 2/6 ⇒ 全分辨率观感半径约 8 / 24 像素：
            // 正好把「质感」与「结构」分在两个尺度上（细尺度再往下就是噪点与去马赛克痕迹）
            fine_radius: 2,
            coarse_radius: 6,
            // eps 的量纲是 log2 亮度的平方，它决定「多大算边」：
            //   0.25 ⇒ 幅度 0.5 stop 的方差，此时细纹理只有 ~14% 留在 base（其余进 detail），
            //   而 1 stop 的结构边有 80% 留在 base —— 正是想要的「纹理归细节、边缘归底」。
            // ❗ 曾经取 0.0025（幅度 0.05 stop），结果**所有**信号都被判成边（a≈1）：
            //   纹理全留在 base 里、被压缩一起压平，detail 层无货可增 —— 局部反差当场塜掉一半。
            fine_eps: 0.25,
            // 中尺度同理，但门槛更高：1 stop 的边有 62% 留在 base（否则增益会把结构边推成光晕）
            coarse_eps: 0.60,
            compression_floor: 0.55,
            fine_boost: 0.45,
            coarse_boost: 0.70,
            fine_limit: 0.35,
            coarse_limit: 1.00,
            base_limit: 3.00,
            shadow_floor: 0.35,
            noise_reference: 0.030,
            highlight_cut: 0.70,
        }
    }
}

/// **四道门控**：它们决定这个算子像不像专业软件，而不是滤镜。
///
/// | 门控 | 不做的后果 |
/// | --- | --- |
/// | 阴影 | 阴影抬亮 + 微反差 = 噪点放大器（暗部一片沙） |
/// | 噪点 | 平坦区（天空、墙面）里全是颗粒 |
/// | 端点 | 死黑死白附近出现「爬行」的假细节 |
/// | 局部对比 | 强边缘推成光晕（halo）与塑料感 |
#[derive(Debug, Clone, Copy)]
struct Gate {
    shadow: f32,
    noise: f32,
    clip: f32,
}

impl Gate {
    #[inline]
    fn combined(self) -> f32 {
        self.shadow * self.noise * self.clip
    }
}

/// 分析结果：**与强度无关**的一切中间量。
///
/// 存的是**低分辨率系数**（`a`/`b`）而不是全分辨率图 —— 全分辨率上重新算一次
/// `a·L + b` 只要两次乘加，而存全分辨率的 base/detail 要多几百 MB
/// （24MP 单通道 f32 就是 96MB，三层就是 288MB）。这也是「能实时」的关键。
#[derive(Debug, Clone)]
pub struct LocalToneState {
    fine: GuidedModel,
    coarse: GuidedModel,
    /// 门控（与系数同分辨率）
    gate: Plane,
    /// 粗 base 的分位数（log2 亮度）：压缩曲线的支点与范围
    low: f32,
    high: f32,
    opts: LocalToneOpts,
}

/// 一次 `apply` 里被解析出来的标量（避免在像素循环里反复算）。
#[derive(Debug, Clone, Copy)]
struct Resolved {
    mid: f32,
    alpha: f32,
    base_limit: f32,
    fine_gain: f32,
    coarse_gain: f32,
    fine_limit: f32,
    coarse_limit: f32,
}

impl LocalToneState {
    /// **分析**（贵，但只依赖图像与尺度）：在一张**已经过线性链**（WB / 曝光 / 反差 /
    /// 高光 / 黑区）的图像上估出 base/detail 分解与门控。
    ///
    /// 传进来的图**不需要是全分辨率**：分析量都是低频的，生产路径上传 1/4 大小的图
    /// （24MP → 1.5MP，成本降到 1/16），系数照旧能用在全分辨率的 `apply` 上 ——
    /// 因为用的是局部线性模型（`a·L + b`），边缘由 `apply` 那张全分辨率的 L 决定。
    #[must_use]
    pub fn analyze(chained: &LinearImage, opts: &LocalToneOpts) -> Self {
        let luma = luma_plane(chained);
        // ① 细尺度：base 贴边 ⇒ D1 里是纹理
        let fine = GuidedModel::analyze(&luma, opts.fine_radius, opts.fine_eps, 1);
        let base_fine = fine.apply(&luma);
        // ② 中尺度：在细 base 上再滤一次 ⇒ D2 里是结构
        let coarse = GuidedModel::analyze(&base_fine, opts.coarse_radius, opts.coarse_eps, 1);
        let base_coarse = coarse.apply(&base_fine);

        // ③ 分位数定支点与范围（用 p02/p98，不用 min/max —— 一颗高光星点不该定义整张照片的调性）
        let (low, high) = percentiles(&base_coarse.values, 0.02, 0.98);

        // ④ 门控：噪点门控看的是**细尺度细节幅度**（平坦区里它接近 0）
        let span = (high - low).max(MIN_SPAN);
        let gate = Plane::from_fn(luma.width, luma.height, |x, y| {
            let index = y * (luma.width as usize) + x;
            let detail = luma.values[index] - base_fine.values[index];
            let position = (base_coarse.values[index] - low) / span;
            let gate = Gate {
                shadow: opts.shadow_floor
                    + (1.0 - opts.shadow_floor) * smoothstep(0.0, 0.30, position),
                noise: detail.abs() / (detail.abs() + opts.noise_reference),
                clip: 1.0 - opts.highlight_cut * smoothstep(0.80, 0.995, position),
            };
            gate.combined()
        });

        Self {
            fine,
            coarse,
            gate,
            low,
            high,
            opts: *opts,
        }
    }

    /// 分析用的粗 base 范围（log2 亮度）—— 探针与测试要拿它做断言。
    #[must_use]
    pub fn base_range(&self) -> (f32, f32) {
        (self.low, self.high)
    }

    /// **应用**（便宜）：按强度把局部色调映射作用到线性图上（原地）。
    ///
    /// * `strength ∈ [0, 1]`（界面上的 0..100 除以 100）；`<= 0` 直接返回 ⇒ **逐位恒等**；
    /// * 输入输出都是**线性 sRGB u16**（与 `LinearImage` 的约定一致）；
    /// * 尺寸可以与 `analyze` 时不同（预览档 / 1:1 档共用同一个 `LocalToneState`）——
    ///   系数按归一化坐标插值，所以两个档位的观感一致。
    pub fn apply_inplace(&self, image: &mut LinearImage, strength: f32) {
        // NaN 也要当 0 处理 —— 所以写「小于等于 0 或者是 NaN」而不是 `!(strength > 0.0)`
        if strength <= 0.0 || strength.is_nan() || !image.is_consistent() {
            // 恒等：强度 0 或 NaN 一律不动画面
            return;
        }
        let strength = strength.min(1.0);
        let resolved = self.resolve(strength);

        let width = image.width as usize;
        let threads = std::thread::available_parallelism()
            .map_or(1, std::num::NonZeroUsize::get)
            .min(16);
        let height = image.height as usize;
        if threads <= 1 || height < 32 {
            apply_rows(self, &resolved, &mut image.rgb, width, height, 0);
            return;
        }
        let rows_per_chunk = height.div_ceil(threads);
        std::thread::scope(|scope| {
            let mut remaining = image.rgb.as_mut_slice();
            let mut first_row = 0usize;
            while !remaining.is_empty() {
                let rows = (remaining.len() / (width * 3)).min(rows_per_chunk).max(1);
                let take = rows * width * 3;
                let (chunk, rest) = remaining.split_at_mut(take);
                remaining = rest;
                let state = self;
                let resolved = &resolved;
                let start = first_row;
                first_row += rows;
                scope.spawn(move || apply_rows(state, resolved, chunk, width, height, start));
            }
        });
    }

    /// 把强度解析成一组标量（`apply` 的像素循环里不再做任何除法/幂）。
    fn resolve(&self, strength: f32) -> Resolved {
        let span = (self.high - self.low).max(MIN_SPAN);
        let keep = 1.0 - strength * (1.0 - self.opts.compression_floor);
        // 有理式压缩：T(mid ± span/2) = mid ± keep·span/2 ⇒ α = (1/keep − 1)·2/span
        let alpha = (1.0 / keep - 1.0) * 2.0 / span;
        Resolved {
            mid: (self.low + self.high) * 0.5,
            alpha,
            base_limit: self.opts.base_limit,
            fine_gain: self.opts.fine_boost * strength,
            coarse_gain: self.opts.coarse_boost * strength,
            fine_limit: self.opts.fine_limit,
            coarse_limit: self.opts.coarse_limit,
        }
    }
}

/// 一段像素的 `apply`（**唯一**的像素循环）。
///
/// `width`/`height` 是**整张图**的尺寸（上采样器靠它算缩放比），
/// `first_row` 只是这一段在整张图里的起始行 —— 两者不能混。
/// ❗ 曾经把 `first_row + 本段行数` 当成 height 传进来：分析图是全图的 1/4 时，
///   每个块各自算出一个不同的缩放比，块边界会出现横向条带。
fn apply_rows(
    state: &LocalToneState,
    resolved: &Resolved,
    rows: &mut [u16],
    width: usize,
    height: usize,
    first_row: usize,
) {
    debug_assert_eq!(rows.len() % (width * 3), 0);
    let mut fine_a = RowUpsampler::new(&state.fine.a, width as u32, height as u32);
    let mut fine_b = RowUpsampler::new(&state.fine.b, width as u32, height as u32);
    let mut coarse_a = RowUpsampler::new(&state.coarse.a, width as u32, height as u32);
    let mut coarse_b = RowUpsampler::new(&state.coarse.b, width as u32, height as u32);
    let mut gate = RowUpsampler::new(&state.gate, width as u32, height as u32);

    for (local_row, line) in rows.as_chunks_mut::<3>().0.chunks_mut(width).enumerate() {
        let y = first_row + local_row;
        fine_a.set_row(y);
        fine_b.set_row(y);
        coarse_a.set_row(y);
        coarse_b.set_row(y);
        gate.set_row(y);
        for (x, pixel) in line.iter_mut().enumerate() {
            let rgb = [
                linear_of(pixel[0]),
                linear_of(pixel[1]),
                linear_of(pixel[2]),
            ];
            let luma = luma_of(rgb);
            let l = luma.max(MIN_LINEAR).log2();

            // 分解（全分辨率、由低分辨率系数插值而来）
            let base_fine = fine_a.at(x) * l + fine_b.at(x);
            let base_coarse = coarse_a.at(x) * base_fine + coarse_b.at(x);
            let detail_fine = l - base_fine;
            let detail_coarse = base_fine - base_coarse;

            // ③ base 压缩（黑场锚住）+ ④ 分级增益（带门控与软限幅）
            //
            // ❗ 这里是整个模块最容易写错的一行：`added_detail` 返回的是**增量**，
            //   而细节本身（D1/D2）永远都要加回去 —— 门控只调「多给的那部分」。
            //   曾写成 `target = compressed + gate·(added_D1 + added_D2)`，等于用增量
            //   替掉了细节：拉杆一离开 0，整张图的纹理全消失、只剩一层压缩过的 base。
            //   （s = 0 的恒等单测看不见它，因为那条路径提前返回了 —— 守它的是
            //   `tiny_strength_is_almost_identity` 那条测试。）
            let compressed = compress_base(base_coarse, resolved);
            let gate = gate.at(x);
            let target = compressed
                + detail_coarse
                + gate * added_detail(detail_coarse, resolved.coarse_gain, resolved.coarse_limit)
                + detail_fine
                + gate * added_detail(detail_fine, resolved.fine_gain, resolved.fine_limit);

            // ⑤ 只动亮度：按 2^(L'−L) 缩 RGB（保色相与通道间比例）
            let ratio = (target - l).exp2().clamp(MIN_RATIO, MAX_RATIO);
            for channel in 0..3 {
                pixel[channel] = encoded_of(rgb[channel] * ratio);
            }
        }
    }
}

/// base 压缩：绕支点的有理式软压缩（软限幅封顶）。
#[inline]
fn compress_base(base: f32, resolved: &Resolved) -> f32 {
    let delta = base - resolved.mid;
    let raw = resolved.mid + delta / (1.0 + resolved.alpha * delta.abs());
    base + soft_saturate(raw - base, resolved.base_limit)
}

/// **detail 增益的增量**（带软限幅）：`g·d / (1 + |g·d|/limit)`。
///
/// `g = 0` ⇒ 恰好 0（强度 0 时逐位恒等的来源）；`|g·d|` 大 ⇒ 饱和到 `±limit`。
#[inline]
fn added_detail(detail: f32, gain: f32, limit: f32) -> f32 {
    if gain <= 0.0 || gain.is_nan() {
        return 0.0;
    }
    soft_saturate(gain * detail, limit)
}

/// **软饱和**：`|x| ≤ limit` 时**原样返回**，超过后平滑收住、渐近到 `±2·limit`。
///
/// ❗ 这里必须是「以下恒等、以上才收」，不能用 `x/(1+|x|/limit)` 那种形式 ——
/// 后者在 limit 以下**也在按比例衰减**（|x| = limit/2 时已经吃掉 33%）。
/// 曾经把它当成安全网去封 base 压缩量，结果 1.54 stops 的压缩被削成 0.95，
/// 光比压缩从承诺的 55% 掉到 75% —— 而单看代码完全看不出来。
/// 连续性：在 `|x| = limit` 处两侧导数都是 1，不会在图上留下折点。
#[inline]
fn soft_saturate(value: f32, limit: f32) -> f32 {
    let magnitude = value.abs();
    if magnitude <= limit {
        return value;
    }
    let excess = magnitude - limit;
    let capped = limit + excess / (1.0 + excess / limit);
    if value < 0.0 {
        -capped
    } else {
        capped
    }
}

/// `smoothstep`（Hermite）：`edge0` 以下 0、`edge1` 以上 1，中间平滑过渡。
#[inline]
fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if edge1 <= edge0 {
        return if value >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 线性 sRGB u16 → f32。
#[inline]
fn linear_of(encoded: u16) -> f32 {
    f32::from(encoded) / 65535.0
}

/// f32 线性 → u16（夹取 + 四舍五入；**不许出现 NaN 漏进编码**）。
#[inline]
fn encoded_of(linear: f32) -> u16 {
    if !linear.is_finite() {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let value = (linear.clamp(0.0, 1.0) * 65535.0 + 0.5) as u32;
    u16::try_from(value.min(65535)).unwrap_or(u16::MAX)
}

/// 从线性图取 `log2` 亮度平面。
fn luma_plane(image: &LinearImage) -> Plane {
    let mut values = Vec::with_capacity(image.rgb.len() / 3);
    for pixel in image.rgb.as_chunks::<3>().0 {
        let luma = luma_of([linear_of(pixel[0]), linear_of(pixel[1]), linear_of(pixel[2])]);
        values.push(luma.max(MIN_LINEAR).log2());
    }
    Plane {
        width: image.width,
        height: image.height,
        values,
    }
}

/// 直方图分位数（`low_fraction` / `high_fraction` ∈ [0, 1]）。
///
/// 用直方图而不是排序：分析图有 150 万个数，排序要 O(N log N)，而 1024 格直方图一遍就够。
fn percentiles(values: &[f32], low_fraction: f32, high_fraction: f32) -> (f32, f32) {
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for value in values {
        if value.is_finite() {
            min = min.min(*value);
            max = max.max(*value);
        }
    }
    if !min.is_finite() || !max.is_finite() {
        return (-1.0, 0.0);
    }
    if max - min < 1e-4 {
        return (min, min + MIN_SPAN);
    }
    #[allow(clippy::cast_precision_loss)]
    let scale = PERCENTILE_BINS as f32 / (max - min);
    let mut histogram = vec![0u32; PERCENTILE_BINS];
    for value in values {
        if !value.is_finite() {
            continue;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let bin = (((value - min) * scale) as usize).min(PERCENTILE_BINS - 1);
        histogram[bin] += 1;
    }
    let total: u32 = histogram.iter().sum();
    if total == 0 {
        return (min, max);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let low_target = (total as f32 * low_fraction) as u32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let high_target = (total as f32 * high_fraction) as u32;
    let mut cumulative = 0u32;
    let mut low = min;
    let mut high = max;
    let mut low_found = false;
    for (index, count) in histogram.iter().enumerate() {
        cumulative += count;
        #[allow(clippy::cast_precision_loss)]
        let value = min + (index as f32 + 0.5) / scale;
        if !low_found && cumulative >= low_target {
            low = value;
            low_found = true;
        }
        if cumulative >= high_target {
            high = value;
            break;
        }
    }
    (low, high.max(low + MIN_SPAN))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 合成图：一条横跨 8 stops 的斜坡 + 棋盘纹理（纹理幅度按 stops 给，便于断言）。
    fn synthetic(width: u32, height: u32, stops: f32, texture_stops: f32) -> LinearImage {
        let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);
        for y in 0..height as usize {
            for x in 0..width as usize {
                #[allow(clippy::cast_precision_loss)]
                let position = x as f32 / (width as f32 - 1.0).max(1.0);
                let base = (-stops * (1.0 - position)).exp2();
                let checker = if (x + y) % 2 == 0 { 1.0 } else { -1.0 };
                let value = (base * (texture_stops * checker).exp2()).clamp(0.0, 1.0);
                rgb.extend([encoded_of(value), encoded_of(value), encoded_of(value)]);
            }
        }
        LinearImage::new(width, height, rgb).expect("形状对得上")
    }

    /// 逆光合成图：左暗右亮（**平滑过渡**，不是两半平色），暗部带纹理。
    ///
    /// 为什么不用「左半平色 / 右半平色」：那样 p02 恰好等于暗部的全部像素值，
    /// 黑场锚会把整个暗部一起锚住 —— 测试会假红，而真照片里暗部总是有层次。
    fn backlit(width: u32, height: u32) -> LinearImage {
        let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);
        for y in 0..height as usize {
            for x in 0..width as usize {
                #[allow(clippy::cast_precision_loss)]
                let t = x as f32 / (width as f32 - 1.0).max(1.0);
                let smooth = t * t * (3.0 - 2.0 * t);
                // −7.5 stops → −0.5 stops
                let stops = -7.5 + 7.0 * smooth;
                let checker: f32 = if (x + y) % 2 == 0 { 1.0 } else { -1.0 };
                let value = stops.exp2() * (0.15 * checker).exp2();
                rgb.extend([encoded_of(value), encoded_of(value), encoded_of(value)]);
            }
        }
        LinearImage::new(width, height, rgb).expect("形状对得上")
    }

    fn luma_stats(image: &LinearImage) -> (f32, f32) {
        let mut values: Vec<f32> = image
            .rgb
            .as_chunks::<3>()
            .0
            .iter()
            .map(|pixel| luma_of([linear_of(pixel[0]), linear_of(pixel[1]), linear_of(pixel[2])]))
            .collect();
        values.sort_by(f32::total_cmp);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let low = values[values.len() / 20];
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let high = values[values.len() - 1 - values.len() / 20];
        (low, high)
    }

    /// 相邻像素差的平均绝对值（**对数域**——局部反差的代理量）。
    ///
    /// 为什么必须在对数域：线性域的相邻差会被「整体压暗」带偏 —— 亮部被压下来之后
    /// 那里的绝对差自然变小，那是色调压缩的本意，不是反差塔了。而眼睛看的是**比值**：
    /// 同一个纹理在暗部与亮部「看着一样明显」就是对数域相等。
    fn local_contrast(image: &LinearImage) -> f32 {
        let width = image.width as usize;
        let height = image.height as usize;
        let mut sum = 0f64;
        let mut count = 0usize;
        for y in 0..height {
            for x in 1..width {
                let left = luma_of([
                    linear_of(image.rgb[(y * width + x - 1) * 3]),
                    linear_of(image.rgb[(y * width + x - 1) * 3 + 1]),
                    linear_of(image.rgb[(y * width + x - 1) * 3 + 2]),
                ]);
                let right = luma_of([
                    linear_of(image.rgb[(y * width + x) * 3]),
                    linear_of(image.rgb[(y * width + x) * 3 + 1]),
                    linear_of(image.rgb[(y * width + x) * 3 + 2]),
                ]);
                let left = left.max(MIN_LINEAR).log2();
                let right = right.max(MIN_LINEAR).log2();
                sum += f64::from((right - left).abs());
                count += 1;
            }
        }
        if count == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_possible_truncation)]
        let value = (sum / count as f64) as f32;
        value
    }

    /// 光比（stops）：p95 与 p05 的对数距离。
    fn range_in_stops(image: &LinearImage) -> f32 {
        let (low, high) = luma_stats(image);
        high.max(MIN_LINEAR).log2() - low.max(MIN_LINEAR).log2()
    }

    #[test]
    fn tiny_strength_is_almost_identity() {
        // 强度很小但**不为 0** 时，输出必须逼近输入 —— 这条是「细节永远要加回去」的守门测试。
        // （s = 0 那条路径提前返回，看不见「用增量替掉细节」这类错误；这里走的是真算法路径。）
        let source = synthetic(64, 48, 6.0, 0.35);
        let state = LocalToneState::analyze(&source, &LocalToneOpts::default());
        let mut image = source.clone();
        state.apply_inplace(&mut image, 1e-6);
        let mut worst = 0f32;
        for (before, after) in source.rgb.iter().zip(image.rgb.iter()) {
            let before = linear_of(*before).max(MIN_LINEAR).log2();
            let after = linear_of(*after).max(MIN_LINEAR).log2();
            worst = worst.max((before - after).abs());
        }
        assert!(
            worst < 0.01,
            "强度 1e-6 就该接近恒等，实际最大差 {worst} stops（细节被增量替掉了？）"
        );
    }

    #[test]
    fn strength_zero_is_bit_exact() {
        // 「强度 0 不动画面」是这个模块最硬的一条：拖回 0 必须与没打开过完全一致
        let source = backlit(64, 48);
        let state = LocalToneState::analyze(&source, &LocalToneOpts::default());
        let mut image = source.clone();
        state.apply_inplace(&mut image, 0.0);
        assert_eq!(image, source);
    }

    #[test]
    fn negative_or_nan_strength_is_also_a_noop() {
        let source = backlit(32, 24);
        let state = LocalToneState::analyze(&source, &LocalToneOpts::default());
        for strength in [-1.0f32, f32::NAN] {
            let mut image = source.clone();
            state.apply_inplace(&mut image, strength);
            assert_eq!(image, source, "强度 {strength} 不该动画面");
        }
    }

    #[test]
    fn compression_narrows_the_global_range_and_lifts_the_dark_side() {
        // 「降整体光比」的可量化定义：**stops 数**收窄到约 55%（就是 compression_floor 的承诺），
        // 且暗部被抬亮、亮部被压住
        let source = backlit(96, 64);
        let state = LocalToneState::analyze(&source, &LocalToneOpts::default());
        let mut image = source.clone();
        state.apply_inplace(&mut image, 1.0);

        let before = range_in_stops(&source);
        let after = range_in_stops(&image);
        let ratio = after / before;
        assert!(
            (0.45..=0.70).contains(&ratio),
            "光比没收到 55% 附近：{before:.2} stops → {after:.2} stops（{ratio:.2}）"
        );
        let (source_low, source_high) = luma_stats(&source);
        let (low, high) = luma_stats(&image);
        assert!(low > source_low, "暗部没被抬亮：{source_low} → {low}");
        assert!(high < source_high, "亮部没被压住：{source_high} → {high}");
    }

    #[test]
    fn black_stays_black() {
        // 黑场锚：纯黑（线性 0）拉满强度也不许被抬成灰
        let mut rgb = vec![0u16; 32 * 32 * 3];
        // 右边一半给点亮，免得整张图全黑让分位数退化
        for y in 0..32usize {
            for x in 16..32usize {
                let value = encoded_of(0.2);
                rgb[(y * 32 + x) * 3] = value;
                rgb[(y * 32 + x) * 3 + 1] = value;
                rgb[(y * 32 + x) * 3 + 2] = value;
            }
        }
        let source = LinearImage::new(32, 32, rgb).expect("形状对得上");
        let state = LocalToneState::analyze(&source, &LocalToneOpts::default());
        let mut image = source.clone();
        state.apply_inplace(&mut image, 1.0);
        let darkest = image.rgb.iter().copied().min().expect("非空");
        assert!(
            f32::from(darkest) <= 1.0,
            "纯黑被抬到了 {} 级（应当仍是 0）",
            darkest
        );
    }

    #[test]
    fn local_contrast_is_not_lost() {
        // 这是整个模块存在的理由：整体光比压下去之后，**局部**反差不能跟着塌。
        // 用相邻像素差的平均绝对值当代理量：它必须不降（并留一点余量）。
        let source = synthetic(128, 96, 6.0, 0.35);
        let state = LocalToneState::analyze(&source, &LocalToneOpts::default());
        let before = local_contrast(&source);
        let mut image = source.clone();
        state.apply_inplace(&mut image, 1.0);
        let after = local_contrast(&image);
        assert!(
            after >= before * 0.95,
            "局部反差塌了：{before:.5} → {after:.5}"
        );
    }

    #[test]
    fn flat_areas_are_not_amplified() {
        // 噪点门控：一块纯平坦（只有极小抖动）的区域，拉满强度也不该被「加出」纹理
        let width = 64u32;
        let height = 48u32;
        let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);
        for y in 0..height as usize {
            for x in 0..width as usize {
                let jitter = if (x * 7 + y * 13) % 5 == 0 { 1e-4 } else { -1e-4 };
                let value = encoded_of(0.18 + jitter);
                rgb.extend([value, value, value]);
            }
        }
        let source = LinearImage::new(width, height, rgb).expect("形状对得上");
        let state = LocalToneState::analyze(&source, &LocalToneOpts::default());
        let mut image = source.clone();
        state.apply_inplace(&mut image, 1.0);
        let spread = |img: &LinearImage| -> f32 {
            let values: Vec<f32> = img
                .rgb
                .as_chunks::<3>()
                .0
                .iter()
                .map(|pixel| linear_of(pixel[0]))
                .collect();
            let mean = values.iter().sum::<f32>() / values.len() as f32;
            (values.iter().map(|v| (v - mean).abs()).sum::<f32>() / values.len() as f32).max(1e-9)
        };
        let before = spread(&source);
        let after = spread(&image);
        assert!(
            after < before * 1.5,
            "平坦区被加出了纹理：{before:.6} → {after:.6}"
        );
    }

    #[test]
    fn output_is_monotone_along_a_ramp() {
        // 软限幅 + 压缩都必须是单调的：否则会在平滑渐变里造出假的「反相带」
        let source = synthetic(256, 8, 8.0, 0.0);
        let state = LocalToneState::analyze(&source, &LocalToneOpts::default());
        let mut image = source.clone();
        state.apply_inplace(&mut image, 1.0);
        let row = 4usize;
        let mut previous = -1f32;
        for x in 0..image.width as usize {
            let value = linear_of(image.rgb[(row * 256 + x) * 3]);
            assert!(
                value >= previous - 1e-5,
                "第 {x} 列反了：{previous} → {value}"
            );
            previous = value;
        }
    }

    #[test]
    fn hue_is_preserved() {
        // 只动亮度：三通道的**比例**不该变（色相偏移的代理判据）
        let mut rgb = Vec::new();
        for y in 0..32usize {
            for x in 0..32usize {
                let tint = 1.0 + 0.3 * ((x as f32 / 32.0) - 0.5);
                rgb.push(encoded_of(0.05 * tint));
                rgb.push(encoded_of(0.10));
                rgb.push(encoded_of(0.20 / tint));
                let _ = y;
            }
        }
        let source = LinearImage::new(32, 32, rgb).expect("形状对得上");
        let state = LocalToneState::analyze(&source, &LocalToneOpts::default());
        let mut image = source.clone();
        state.apply_inplace(&mut image, 1.0);
        for (index, (before, after)) in source
            .rgb
            .as_chunks::<3>()
            .0
            .iter()
            .zip(image.rgb.as_chunks::<3>().0.iter())
            .enumerate()
        {
            let ratio_before = f64::from(before[0]) / f64::from(before[1]).max(1.0);
            let ratio_after = f64::from(after[0]) / f64::from(after[1]).max(1.0);
            assert!(
                (ratio_before - ratio_after).abs() < 0.02,
                "第 {index} 个像素色比变了：{ratio_before} → {ratio_after}"
            );
        }
    }

    /// 纵向平滑渐变（无纹理）：每行的亮度都不一样，用来查「整帧都被处理到、且没有分块痕迹」。
    fn vertical_gradient(width: u32, height: u32) -> LinearImage {
        let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);
        for y in 0..height as usize {
            #[allow(clippy::cast_precision_loss)]
            let t = y as f32 / (height as f32 - 1.0).max(1.0);
            let value = (-7.0 + 6.5 * t).exp2();
            for _ in 0..width as usize {
                rgb.extend([encoded_of(value); 3]);
            }
        }
        LinearImage::new(width, height, rgb).expect("形状对得上")
    }

    #[test]
    fn applying_at_a_larger_size_covers_every_row_without_banding() {
        // 生产路径就是「在 1/4 分析图上估系数、在全分辨率上应用」，而 apply 是按行分块并行的。
        // 这条测试同时钉两件事：
        //   ① 整帧都被处理到（不是只有上牛张）—— 每行都得有应有的效果；
        //   ② 相邻行不许跳变 —— 分块并行把缩放比算错的话，块边界会出现横向条带。
        let full = vertical_gradient(128, 96);
        let small = full.downscaled_to(32);
        let state = LocalToneState::analyze(&small, &LocalToneOpts::default());
        let mut image = full.clone();
        state.apply_inplace(&mut image, 1.0);

        let mut effects = Vec::with_capacity(96);
        for y in 0..96usize {
            let mut sum = 0f32;
            for x in 0..128usize {
                let index = (y * 128 + x) * 3;
                let before = linear_of(full.rgb[index]).max(MIN_LINEAR).log2();
                let after = linear_of(image.rgb[index]).max(MIN_LINEAR).log2();
                sum += after - before;
            }
            effects.push(sum / 128.0);
        }
        // ① 最暗行被抬、最亮行被压
        assert!(effects[0] > 0.3, "最暗行没被抬：{}", effects[0]);
        assert!(effects[95] < -0.3, "最亮行没被压：{}", effects[95]);
        // ② 行间平滑
        let mut worst = 0f32;
        for pair in effects.windows(2) {
            worst = worst.max((pair[1] - pair[0]).abs());
        }
        assert!(worst < 0.05, "行间跳变 {worst} stops（分块边界条带？）");
    }

    #[test]
    fn works_on_a_different_resolution_than_the_analysis() {
        // 预览档 / 1:1 档共用同一个 state：小图上应用不许 panic，也不许把画面搞黑
        let source = synthetic(128, 96, 6.0, 0.3);
        let state = LocalToneState::analyze(&source, &LocalToneOpts::default());
        let small = source.downscaled_to(32);
        let mut image = small.clone();
        state.apply_inplace(&mut image, 1.0);
        let mean_before = small.rgb.iter().map(|v| f64::from(*v)).sum::<f64>() / small.rgb.len() as f64;
        let mean_after = image.rgb.iter().map(|v| f64::from(*v)).sum::<f64>() / image.rgb.len() as f64;
        assert!(mean_after > 0.0, "整张图变黑了");
        assert!(
            (mean_after - mean_before).abs() < mean_before.max(1.0),
            "小图上跑出了离谱的亮度变化：{mean_before:.0} → {mean_after:.0}"
        );
    }

    #[test]
    fn tiny_images_do_not_panic() {
        // 1×1 / 2×1：退化尺寸（降采样、窗口、分位数都得活着）
        for (width, height) in [(1u32, 1u32), (2, 1), (1, 2), (3, 3)] {
            let mut rgb = Vec::new();
            for index in 0..(width as usize) * (height as usize) {
                let value = encoded_of(0.05 + 0.1 * index as f32);
                rgb.extend([value, value, value]);
            }
            let source = LinearImage::new(width, height, rgb).expect("形状对得上");
            let state = LocalToneState::analyze(&source, &LocalToneOpts::default());
            let mut image = source.clone();
            state.apply_inplace(&mut image, 1.0);
            assert!(image.is_consistent());
        }
    }

    #[test]
    fn strength_is_monotone_in_the_effect() {
        // 强度必须单调：0.25 的效果要落在 0 与 0.5 之间（否则拉杆会「跳」）。
        // 用**暗半边的线性均值**做判据：p05 那种分位点会被黑场锚与纹理细节带偏，
        // 而「暗部整体被抬亮」是这个算子的承诺，均值才是它的直接度量。
        let source = backlit(64, 48);
        let state = LocalToneState::analyze(&source, &LocalToneOpts::default());
        let dark_mean = |image: &LinearImage| -> f32 {
            let width = image.width as usize;
            let mut sum = 0f64;
            let mut count = 0usize;
            for y in 0..image.height as usize {
                for x in 0..width / 2 {
                    let index = (y * width + x) * 3;
                    sum += f64::from(luma_of([
                        linear_of(image.rgb[index]),
                        linear_of(image.rgb[index + 1]),
                        linear_of(image.rgb[index + 2]),
                    ]));
                    count += 1;
                }
            }
            #[allow(clippy::cast_possible_truncation)]
            let value = (sum / count as f64) as f32;
            value
        };
        let base = dark_mean(&source);
        let mut lifted = [0f32; 3];
        for (slot, strength) in [0.25f32, 0.5, 1.0].iter().enumerate() {
            let mut image = source.clone();
            state.apply_inplace(&mut image, *strength);
            lifted[slot] = dark_mean(&image);
        }
        assert!(lifted[0] > base, "0.25 没抬暗部：{base} → {}", lifted[0]);
        assert!(lifted[1] > lifted[0], "0.5 不比 0.25 强：{lifted:?}");
        assert!(lifted[2] > lifted[1], "1.0 不比 0.5 强：{lifted:?}");
    }

    #[test]
    fn external_vector_pins_the_compression_math() {
        // **外部给定**：压缩曲线的解析值（按定义式手算，不是跑一遍记下来）
        //   span = 4（lo = −6, hi = −2）、强度 1、compression_floor = 0.55
        //   keep = 0.55 ⇒ α = (1/0.55 − 1)·2/4 = 0.4090909
        //   base = −2（= hi）⇒ delta = 0 ⇒ T = −2（支点处恒等，曲线穿过中点）
        //   base = −4（支点）⇒ T = −4
        let opts = LocalToneOpts {
            compression_floor: 0.55,
            ..LocalToneOpts::default()
        };
        let resolved = Resolved {
            mid: -4.0,
            alpha: (1.0 / 0.55 - 1.0) * 2.0 / 4.0,
            base_limit: 1e6,
            fine_gain: 0.0,
            coarse_gain: 0.0,
            fine_limit: 1.0,
            coarse_limit: 1.0,
        };
        assert!((compress_base(-4.0, &resolved) + 4.0).abs() < 1e-5, "支点必须不动");
        // 两端各压到 55%：hi = −2 ⇒ delta = +2 ⇒ T = −4 + 2/(1+0.4090909·2) = −4 + 1.1 = −2.9
        assert!(
            (compress_base(-2.0, &resolved) + 2.9).abs() < 1e-4,
            "上端压缩值不对：{}",
            compress_base(-2.0, &resolved)
        );
        // 对称：lo 端同理
        assert!(
            (compress_base(-6.0, &resolved) + 5.1).abs() < 1e-4,
            "下端压缩值不对：{}",
            compress_base(-6.0, &resolved)
        );
        let _ = opts;
    }

    #[test]
    fn detail_gain_is_exact_below_the_limit_and_saturates_above() {
        // 软饱和的解析性质（三条，缺一条就会在画面上留下痕迹）：
        //   ① `|g·d| ≤ limit` 时**完全等于** `g·d`（线性区不许被削弱）
        //   ② 单调（否则平滑渐变里会出反相带）
        //   ③ 渐近到 `2·limit`（不会无限放大）
        let limit = 0.5f32;
        assert!(added_detail(0.0, 3.0, limit).abs() < 1e-9, "零细节必须零增量");
        // ① 线性区：|g·d| = 0.45 ≤ 0.5 ⇒ 精确等于 g·d
        assert!((added_detail(0.15, 3.0, limit) - 0.45).abs() < 1e-6);
        // ② 单调
        let mut previous = f32::NEG_INFINITY;
        for step in 0..400 {
            let value = added_detail(step as f32 * 0.01, 3.0, limit);
            assert!(value >= previous - 1e-6, "软饱和不单调：{previous} → {value}");
            previous = value;
        }
        // ③ 渐近
        assert!(added_detail(1000.0, 3.0, limit) < limit * 2.0);
        assert!(added_detail(-1000.0, 3.0, limit) > -limit * 2.0);
        // 增益为 0 时不许有任何增量（强度 0 逐位恒等的另一半）
        assert!(added_detail(0.5, 0.0, limit).abs() < 1e-9);
    }

    #[test]
    fn soft_saturate_is_identity_below_the_knee() {
        // 这一条是 2026-09-24 那个「光比没收够」的直接守门人：
        // 曾经用 `x/(1+|x|/limit)` 当限幅，它在 limit 以下也在衰减。
        let limit = 2.0f32;
        assert!((soft_saturate(1.5, limit) - 1.5).abs() < 1e-9);
        assert!((soft_saturate(-1.5, limit) + 1.5).abs() < 1e-9);
        assert!((soft_saturate(2.0, limit) - 2.0).abs() < 1e-9);
        // 越过拐点后仍连续，且导数不跳变
        let just_below = soft_saturate(limit - 1e-4, limit);
        let just_above = soft_saturate(limit + 1e-4, limit);
        assert!((just_above - just_below - 2e-4).abs() < 1e-6, "拐点处不连续");
        assert!(soft_saturate(1e6, limit) < limit * 2.0);
    }

    #[test]
    fn percentiles_are_robust_to_outliers() {
        // 一颗高光星点（1.0）不该定义整张照片的上分位
        let mut values = vec![-6.0f32; 10_000];
        values[0] = 0.0;
        let (low, high) = percentiles(&values, 0.02, 0.98);
        assert!((low + 6.0).abs() < 0.1, "下分位被带跑了：{low}");
        assert!((high + 6.0).abs() < 0.1, "上分位被星点带跑了：{high}");
    }

    #[test]
    fn percentiles_handle_degenerate_input() {
        let flat = vec![-3.0f32; 100];
        let (low, high) = percentiles(&flat, 0.02, 0.98);
        assert!(high > low, "常量输入也要给出非零跨度，否则除零");
        let empty: Vec<f32> = Vec::new();
        let (low, high) = percentiles(&empty, 0.02, 0.98);
        assert!(low.is_finite() && high.is_finite());
    }
}
