//! **边缘保持滤波**：局部色调映射（`super::local_tone`）的底层算子。
//!
//! 这里只有两件东西：**可分离箱式滤波**（O(N)，滑动窗，不随半径变慢）
//! 与**快速引导滤波**（He & Sun 2015：在 1/scale 分辨率上估 `a`/`b`，
//! 回到全分辨率用局部线性模型 `a·I + b` 应用）。
//!
//! # 为什么是引导滤波，不是双边滤波
//!
//! 双边滤波在强边缘附近会出现**梯度反转**（gradient reversal）：边缘一侧本该更亮的像素
//! 反而变暗，于是 base 层在边缘处长出反向的过冲，detail 层就带着一圈光晕。
//! 引导滤波是**局部线性模型**，结构上不会反转（`a` 恒为非负、且 `a·I + b` 在窗口内单调），
//! 而代价只是几次箱式滤波 —— 这正是 darktable 的 tone equalizer 选它的原因
//! （参考实现见 `src/common/guided_filter.c`，GPL-3.0，依 AGENTS.md §2 可组合）。
//!
//! # 为什么在低分辨率上估系数
//!
//! `a`/`b` 是**低频量**（它们是窗口内的均值与协方差比），在全分辨率上算它们
//! 只是把同一条平滑曲线算得更贵。在 1/4 分辨率上估出来、再双线性插值应用：
//! 箱式滤波的成本降到 1/16，而因为用的是**局部线性模型**（不是把输出上采样），
//! 全分辨率的边缘仍然由全分辨率的 guide 决定 —— 这就是「快速引导滤波」的全部秘密。
//!
//! # 两个容易踩的边界
//!
//! 1. **边界一律夹取**（边缘像素重复），不是补零 —— 补零会在图像四周造出一圈假的暗边，
//!    base 层跟着变暗，detail 层就在四周长出一圈亮边；
//! 2. **`var` 可能微负**（`mean(x²) − mean(x)²` 在 f32 下会因消去误差变成 −1e-9），
//!    夹到 0 —— 不夹的话 `a = var/(var+eps)` 会给出一个微小的负数，把 base 层推成反相。

/// **单通道平面**（f32，行主序紧密排列）。
///
/// 用 f32 不用 u16：这里装的是 `log2` 亮度与滤波中间量，动态范围横跨十几个 stops，
/// 定点编码的量化误差会直接变成可见的色带。
#[derive(Debug, Clone, PartialEq)]
pub struct Plane {
    pub width: u32,
    pub height: u32,
    /// 行主序，长度 = `width × height`。
    pub values: Vec<f32>,
}

impl Plane {
    /// 从数据建（长度对不上返回 `None`，**不猜、不补零**）。
    #[must_use]
    pub fn new(width: u32, height: u32, values: Vec<f32>) -> Option<Self> {
        let expected = (width as usize).checked_mul(height as usize)?;
        if width == 0 || height == 0 || values.len() != expected {
            return None;
        }
        Some(Self {
            width,
            height,
            values,
        })
    }

    /// 全 `value` 的平面。
    #[must_use]
    pub fn filled(width: u32, height: u32, value: f32) -> Self {
        Self {
            width,
            height,
            values: vec![value; (width as usize) * (height as usize)],
        }
    }

    /// 从 `(x, y) → f32` 造一张（测试与合成图用）。
    #[must_use]
    pub fn from_fn(width: u32, height: u32, mut f: impl FnMut(usize, usize) -> f32) -> Self {
        let mut values = Vec::with_capacity((width as usize) * (height as usize));
        for y in 0..height as usize {
            for x in 0..width as usize {
                values.push(f(x, y));
            }
        }
        Self {
            width,
            height,
            values,
        }
    }

    /// 取一个像素（越界夹取；**不 panic**，滤波器的边界靠它兜底）。
    #[must_use]
    pub fn get(&self, x: usize, y: usize) -> f32 {
        let x = x.min(self.width as usize - 1);
        let y = y.min(self.height as usize - 1);
        self.values[y * (self.width as usize) + x]
    }

    /// 尺寸与长度对得上吗（跨线程序列化之后要防一手）。
    #[must_use]
    pub fn is_consistent(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.values.len() == (self.width as usize) * (self.height as usize)
    }
}

/// **可分离箱式滤波**（边界夹取）。
///
/// 两遍一维滑动窗，每遍 O(N) —— **与半径无关**：半径 64 和半径 4 一样快。
/// 用滑动窗而不是前缀和：前缀和要多一遍内存往返，而这里的窗口在纵向推进时
/// 天然可以「加一行、减一行」地维护（见 [`blur_columns`]），缓存友好得多。
///
/// `radius = 0` 原样返回（恒等）。
///
/// # 精度
///
/// 滑动窗的浮点误差会随步数缓慢累积。生产路径上本函数**只在 ≤1/4 分辨率的分析图上跑**
/// （高 1000 行量级），误差在 1e-3 以下 —— 远低于 8bit 显示量化（1/255）。
/// 全分辨率那条路走的是 [`RowUpsampler`]，不做箱式滤波。
#[must_use]
pub fn box_blur(src: &Plane, radius: usize) -> Plane {
    let (width, height) = (src.width as usize, src.height as usize);
    if radius == 0 || width == 0 || height == 0 {
        return src.clone();
    }
    let mut horizontal = vec![0f32; width * height];
    blur_rows(&src.values, &mut horizontal, width, height, radius);
    let mut out = vec![0f32; width * height];
    blur_columns(&horizontal, &mut out, width, height, radius);
    Plane {
        width: src.width,
        height: src.height,
        values: out,
    }
}

/// 横向箱式滤波（每行一个滑动窗）。
fn blur_rows(src: &[f32], dst: &mut [f32], width: usize, height: usize, radius: usize) {
    debug_assert!(width > 0 && height > 0);
    for y in 0..height {
        let row = &src[y * width..(y + 1) * width];
        let out = &mut dst[y * width..(y + 1) * width];
        let first_hi = radius.min(width - 1);
        let mut sum = 0f32;
        for value in &row[..=first_hi] {
            sum += *value;
        }
        let mut previous_lo = 0usize;
        let mut previous_hi = first_hi;
        for (x, slot) in out.iter_mut().enumerate() {
            let lo = x.saturating_sub(radius);
            let hi = (x + radius).min(width - 1);
            // 窗口一次只挪一格 ⇒ 这两个分支最多各走一次（夹取时一次都不走）
            if previous_hi < hi {
                for value in &row[previous_hi + 1..=hi] {
                    sum += *value;
                }
            }
            // 离开窗口的是 [previous_lo, lo) —— **不是** `lo..previous_lo`（那是空区间，
            // 窗口就只加不减、和值一路飘；这个反了的方向 2026-09-24 被单测当场抓住）
            if previous_lo < lo {
                for value in &row[previous_lo..lo] {
                    sum -= *value;
                }
            }
            previous_lo = lo;
            previous_hi = hi;
            #[allow(clippy::cast_precision_loss)]
            let count = (hi - lo + 1) as f32;
            *slot = sum / count;
        }
    }
}

/// 纵向箱式滤波（**逐列**维护滑动和，但按行推进 —— 顺序访问，不做转置）。
fn blur_columns(src: &[f32], dst: &mut [f32], width: usize, height: usize, radius: usize) {
    debug_assert!(width > 0 && height > 0);
    let mut sum = vec![0f32; width];
    let first_hi = radius.min(height - 1);
    for row in 0..=first_hi {
        let line = &src[row * width..(row + 1) * width];
        for x in 0..width {
            sum[x] += line[x];
        }
    }
    let mut previous_lo = 0usize;
    let mut previous_hi = first_hi;
    for y in 0..height {
        let lo = y.saturating_sub(radius);
        let hi = (y + radius).min(height - 1);
        for row in (previous_hi + 1)..=hi {
            let line = &src[row * width..(row + 1) * width];
            for x in 0..width {
                sum[x] += line[x];
            }
        }
        for row in previous_lo..lo {
            let line = &src[row * width..(row + 1) * width];
            for x in 0..width {
                sum[x] -= line[x];
            }
        }
        previous_lo = lo;
        previous_hi = hi;
        #[allow(clippy::cast_precision_loss)]
        let count = (hi - lo + 1) as f32;
        let out = &mut dst[y * width..(y + 1) * width];
        for x in 0..width {
            out[x] = sum[x] / count;
        }
    }
}

/// **区域平均降采样**（比例不整除时按实际像素数平均，不丢边角）。
///
/// 与 `LinearImage::downscaled_to` 同一套箱式口径 —— 预览档缩放比常在 2–8 倍，
/// 区域平均正是这个比例下的理想低通。
#[must_use]
pub fn downsample_box(src: &Plane, width: u32, height: u32) -> Plane {
    let (target_w, target_h) = (width as usize, height as usize);
    let (source_w, source_h) = (src.width as usize, src.height as usize);
    if target_w == 0 || target_h == 0 || source_w == 0 || source_h == 0 {
        return src.clone();
    }
    let mut values = vec![0f32; target_w * target_h];
    for y in 0..target_h {
        let y0 = y * source_h / target_h;
        let y1 = (((y + 1) * source_h) / target_h).max(y0 + 1);
        for x in 0..target_w {
            let x0 = x * source_w / target_w;
            let x1 = (((x + 1) * source_w) / target_w).max(x0 + 1);
            let mut sum = 0f32;
            for row in y0..y1 {
                let line = &src.values[row * source_w..(row + 1) * source_w];
                for value in &line[x0..x1] {
                    sum += *value;
                }
            }
            #[allow(clippy::cast_precision_loss)]
            let count = ((y1 - y0) * (x1 - x0)) as f32;
            values[y * target_w + x] = sum / count;
        }
    }
    Plane {
        width,
        height,
        values,
    }
}

/// **低分辨率平面 → 全分辨率的逐行双线性上采样器**。
///
/// 为什么按行：全分辨率逐像素做双线性要从低分辨率图里读 4 个点，24MP 上就是 1 亿次
/// 跨行随机读（低分辨率图再小也装不进 L1）。而纵向插值权重**每 `scale` 行才变一次** ——
/// 于是先把一整行纵向插好放进行缓冲（长度只有低分辨率的宽），逐像素只剩横向 2 抽头，
/// 读的是刚写进去、还在 L1 里的那一行。
pub struct RowUpsampler<'a> {
    source: &'a Plane,
    /// 全分辨率 / 低分辨率（纵向）
    scale_y: f32,
    /// 纵向插值后的一行（长度 = 低分辨率的宽）
    row: Vec<f32>,
    /// 横向的两个源下标与权重 —— **整帧只算一次**（与像素无关），
    /// 否则每像素都要做一次浮点除法 + floor + 浮点转整数（实测这是最大的一笔开销）
    left: Vec<u32>,
    right: Vec<u32>,
    fraction: Vec<f32>,
}

impl<'a> RowUpsampler<'a> {
    /// 建一个上采样器（`target_width`/`target_height` = **全分辨率**的尺寸）。
    #[must_use]
    pub fn new(source: &'a Plane, target_width: u32, target_height: u32) -> Self {
        #[allow(clippy::cast_precision_loss)]
        let scale_y = (target_height as f32 / source.height as f32).max(1.0);
        let low_width = source.width as usize;
        let target = target_width as usize;
        let mut left = Vec::with_capacity(target);
        let mut right = Vec::with_capacity(target);
        let mut fraction = Vec::with_capacity(target);
        #[allow(clippy::cast_precision_loss)]
        let scale_x = (target_width as f32 / source.width as f32).max(1.0);
        for x in 0..target {
            #[allow(clippy::cast_precision_loss)]
            let gx = ((x as f32 + 0.5) / scale_x - 0.5).max(0.0);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let x0 = (gx.floor() as usize).min(low_width - 1);
            let x1 = (x0 + 1).min(low_width - 1);
            #[allow(clippy::cast_possible_truncation)]
            left.push(x0 as u32);
            #[allow(clippy::cast_possible_truncation)]
            right.push(x1 as u32);
            fraction.push((gx - x0 as f32).clamp(0.0, 1.0));
        }
        let mut this = Self {
            source,
            scale_y,
            row: vec![0f32; low_width],
            left,
            right,
            fraction,
        };
        this.set_row(0);
        this
    }

    /// 换到第 `y` 行（全分辨率坐标）：把纵向插值结果铺进行缓冲。
    pub fn set_row(&mut self, y: usize) {
        let low_width = self.source.width as usize;
        let low_height = self.source.height as usize;
        #[allow(clippy::cast_precision_loss)]
        let gy = ((y as f32 + 0.5) / self.scale_y - 0.5).max(0.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let y0 = (gy.floor() as usize).min(low_height - 1);
        let y1 = (y0 + 1).min(low_height - 1);
        let fy = (gy - y0 as f32).clamp(0.0, 1.0);
        let top = &self.source.values[y0 * low_width..(y0 + 1) * low_width];
        let bottom = &self.source.values[y1 * low_width..(y1 + 1) * low_width];
        if y1 == y0 {
            self.row.copy_from_slice(top);
            return;
        }
        for index in 0..low_width {
            self.row[index] = top[index] + (bottom[index] - top[index]) * fy;
        }
    }

    /// 取第 `x` 列（全分辨率坐标）的插值结果 —— 横向 2 抽头，无除法无分支。
    #[inline]
    #[must_use]
    pub fn at(&self, x: usize) -> f32 {
        let left = self.left[x] as usize;
        let right = self.right[x] as usize;
        let a = self.row[left];
        a + (self.row[right] - a) * self.fraction[x]
    }
}

/// **引导滤波的系数**：`out = a·I + b`（`I` = guide）。
///
/// `a`/`b` 存在低分辨率上（`1/scale`），用 [`RowUpsampler`] 插值到全分辨率应用。
/// 这也正是它比「把滤波输出上采样」好的地方：边缘由**全分辨率的 guide** 决定，
/// 插值误差只落在低频的 `a`/`b` 上。
#[derive(Debug, Clone, PartialEq)]
pub struct GuidedModel {
    /// 低分辨率系数 `a = var / (var + eps) ∈ [0, 1)`
    pub a: Plane,
    /// 低分辨率系数 `b = mean·(1 − a)`
    pub b: Plane,
    /// 分析时用的降采样倍率（1 = 全分辨率；只用于说明与测试，应用时按实际尺寸算）
    pub scale: u32,
    /// 窗口半径（**低分辨率**坐标下）
    pub radius: usize,
}

impl GuidedModel {
    /// 在（可选的）低分辨率上估计系数。
    ///
    /// * `guide` 既当引导图又当输入图（self-guided）—— 局部色调映射要的正是
    ///   「把亮度拆成 base + detail」，不需要第二张引导图；
    /// * `eps` 是**正则项**：`eps` 越小越「贴边」（base 越接近原图、detail 越少），
    ///   越大越接近普通箱式模糊。它同时决定了「多大算边缘」—— 所以它的量纲是
    ///   `log2` 亮度的平方（见 `local_tone` 里的取值与理由）；
    /// * `scale > 1` 时先在区域平均降采样上估计 —— 这是快速引导滤波的加速来源。
    #[must_use]
    pub fn analyze(guide: &Plane, radius: usize, eps: f32, scale: u32) -> Self {
        let scale = scale.max(1);
        let low = if scale <= 1 {
            guide.clone()
        } else {
            let width = (guide.width / scale).max(1);
            let height = (guide.height / scale).max(1);
            downsample_box(guide, width, height)
        };
        let mean = box_blur(&low, radius);
        let squared = Plane {
            width: low.width,
            height: low.height,
            values: low.values.iter().map(|value| value * value).collect(),
        };
        let mean_squared = box_blur(&squared, radius);

        let mut a = Plane::filled(low.width, low.height, 0.0);
        let mut b = Plane::filled(low.width, low.height, 0.0);
        for index in 0..low.values.len() {
            let m = mean.values[index];
            // 消去误差可能给出 -1e-9 这种「负方差」：夹到 0，否则 a 会变成负数
            let variance = (mean_squared.values[index] - m * m).max(0.0);
            let ai = variance / (variance + eps);
            a.values[index] = ai;
            b.values[index] = m * (1.0 - ai);
        }
        // 引导滤波的第二组均值：先把 a/b 各自平均，再应用（He et al. 2010 的 (5)(6) 式）
        Self {
            a: box_blur(&a, radius),
            b: box_blur(&b, radius),
            scale,
            radius,
        }
    }

    /// 在全分辨率上应用：`out = a↑·guide + b↑`。
    #[must_use]
    pub fn apply(&self, guide: &Plane) -> Plane {
        let mut out = vec![0f32; (guide.width as usize) * (guide.height as usize)];
        self.apply_into(guide, &mut out);
        Plane {
            width: guide.width,
            height: guide.height,
            values: out,
        }
    }

    /// 应用进一块现成的缓冲（长度必须 = `width × height`）。
    pub fn apply_into(&self, guide: &Plane, out: &mut [f32]) {
        let width = guide.width as usize;
        let height = guide.height as usize;
        debug_assert_eq!(out.len(), width * height);
        let mut sampler_a = RowUpsampler::new(&self.a, guide.width, guide.height);
        let mut sampler_b = RowUpsampler::new(&self.b, guide.width, guide.height);
        for y in 0..height {
            sampler_a.set_row(y);
            sampler_b.set_row(y);
            let source = &guide.values[y * width..(y + 1) * width];
            let target = &mut out[y * width..(y + 1) * width];
            for x in 0..width {
                target[x] = sampler_a.at(x) * source[x] + sampler_b.at(x);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **朴素参考实现**（O(N·r)，边界夹取）—— 只用来对照，别用在生产路径上。
    fn naive_box_blur(src: &Plane, radius: usize) -> Plane {
        Plane::from_fn(src.width, src.height, |x, y| {
            let mut sum = 0f32;
            let mut count = 0f32;
            let x0 = x.saturating_sub(radius);
            let x1 = (x + radius).min(src.width as usize - 1);
            let y0 = y.saturating_sub(radius);
            let y1 = (y + radius).min(src.height as usize - 1);
            for row in y0..=y1 {
                for column in x0..=x1 {
                    sum += src.get(column, row);
                    count += 1.0;
                }
            }
            sum / count
        })
    }

    fn ramp(width: u32, height: u32) -> Plane {
        Plane::from_fn(width, height, |x, y| {
            #[allow(clippy::cast_precision_loss)]
            {
                (x as f32) * 0.7 + (y as f32) * 0.3
            }
        })
    }

    #[test]
    fn plane_rejects_bad_shapes() {
        assert!(Plane::new(2, 2, vec![0.0; 4]).is_some());
        assert!(Plane::new(2, 2, vec![0.0; 3]).is_none());
        assert!(Plane::new(0, 2, vec![]).is_none());
        assert!(Plane::new(2, 0, vec![]).is_none());
        assert!(Plane::new(2, 2, vec![0.0; 5]).is_none());
    }

    #[test]
    fn box_blur_radius_zero_is_identity() {
        let source = ramp(7, 5);
        assert_eq!(box_blur(&source, 0), source);
    }

    #[test]
    fn box_blur_keeps_a_constant_flat_everywhere() {
        // 边界夹取 ⇒ 常量图在任何半径下都必须原样（补零的话四周会塌下去）
        for radius in [1usize, 3, 9, 64] {
            let source = Plane::filled(9, 7, 0.42);
            let blurred = box_blur(&source, radius);
            for value in &blurred.values {
                assert!((value - 0.42).abs() < 1e-6, "半径 {radius} 下常量被改成了 {value}");
            }
        }
    }

    #[test]
    fn box_blur_matches_naive_reference() {
        // 非整除尺寸 + 各种半径：与朴素实现对到 1e-4（滑动窗 vs 直接求和）
        let source = Plane::from_fn(11, 6, |x, y| {
            #[allow(clippy::cast_precision_loss)]
            {
                ((x * 37 + y * 91) % 23) as f32 * 0.13 - 1.5
            }
        });
        for radius in [1usize, 2, 3, 5, 16] {
            let fast = box_blur(&source, radius);
            let slow = naive_box_blur(&source, radius);
            for (index, (a, b)) in fast.values.iter().zip(slow.values.iter()).enumerate() {
                assert!(
                    (a - b).abs() < 1e-4,
                    "半径 {radius} 第 {index} 个：{a} vs {b}"
                );
            }
        }
    }

    #[test]
    fn box_blur_handles_single_row_and_column() {
        // 退化尺寸：宽 1 / 高 1 不能 panic，也不能把值算错
        let column = Plane::from_fn(1, 5, |_, y| {
            #[allow(clippy::cast_precision_loss)]
            {
                y as f32
            }
        });
        let blurred = box_blur(&column, 1);
        assert_eq!(blurred.width, 1);
        assert_eq!(blurred.height, 5);
        assert!((blurred.get(0, 0) - 0.5).abs() < 1e-6, "顶部夹取：0 与 0");
        assert!((blurred.get(0, 2) - 2.0).abs() < 1e-6, "中间：1+2+3 / 3");

        let row = Plane::from_fn(5, 1, |x, _| {
            #[allow(clippy::cast_precision_loss)]
            {
                x as f32
            }
        });
        let blurred = box_blur(&row, 2);
        assert_eq!(blurred.width, 5);
        assert_eq!(blurred.height, 1);
        assert!((blurred.get(4, 0) - 3.0).abs() < 1e-6, "右端夹取：(2+3+4+4+4)/5");
    }

    #[test]
    fn downsample_box_averages_regions() {
        let source = Plane::from_fn(4, 4, |x, _| if x < 2 { 1.0 } else { 3.0 });
        let half = downsample_box(&source, 2, 2);
        assert_eq!(half.width, 2);
        assert_eq!(half.height, 2);
        for value in &half.values {
            // 左半边全是 1、右半边全是 3 ⇒ 2×2 的块平均正好是 1 与 3
            assert!((*value - 1.0).abs() < 1e-6 || (*value - 3.0).abs() < 1e-6);
        }
        assert!((half.get(0, 0) - 1.0).abs() < 1e-6);
        assert!((half.get(1, 0) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn downsample_box_keeps_edges_of_non_divisible_sizes() {
        // 5 列降到 2 列：不整除时按实际像素数平均，最右一列不能被丢掉
        let source = Plane::from_fn(5, 1, |x, _| {
            #[allow(clippy::cast_precision_loss)]
            {
                x as f32
            }
        });
        let down = downsample_box(&source, 2, 1);
        assert_eq!(down.width, 2);
        // 块 0 = 列 [0, 2)（0,1 → 0.5）；块 1 = 列 [2, 5)（2,3,4 → 3.0）—— 最右一列没被丢
        assert!((down.get(0, 0) - 0.5).abs() < 1e-6);
        assert!((down.get(1, 0) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn row_upsampler_reproduces_a_constant() {
        let low = Plane::filled(3, 2, 7.25);
        let mut sampler = RowUpsampler::new(&low, 12, 8);
        for y in 0..8 {
            sampler.set_row(y);
            for x in 0..12 {
                assert!((sampler.at(x) - 7.25).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn row_upsampler_stays_between_neighbours_on_a_ramp() {
        // 单调斜坡上采样后必须仍单调、且不越界（双线性的基本保证）
        let low = Plane::from_fn(4, 4, |x, y| {
            #[allow(clippy::cast_precision_loss)]
            {
                x as f32 * 10.0 + y as f32 * 5.0
            }
        });
        let mut sampler = RowUpsampler::new(&low, 16, 16);
        let mut previous_row_value = f32::NEG_INFINITY;
        for y in 0..16 {
            sampler.set_row(y);
            let first = sampler.at(0);
            let last = sampler.at(15);
            assert!(first >= previous_row_value - 1e-6, "行间不单调");
            previous_row_value = first;
            assert!(first <= last + 1e-6, "行内不单调");
            assert!(first >= -1e-6 && last <= 45.0 + 1e-6, "越界：{first}..{last}");
        }
    }

    #[test]
    fn guided_model_on_a_constant_guide_is_that_constant() {
        let guide = Plane::filled(16, 12, -3.5);
        let model = GuidedModel::analyze(&guide, 2, 0.01, 1);
        let out = model.apply(&guide);
        for value in &out.values {
            assert!((value + 3.5).abs() < 1e-4, "常量引导图被改成了 {value}");
        }
    }

    #[test]
    fn guided_model_preserves_a_step_edge_better_than_a_box_blur() {
        // 左半 0、右半 1 的台阶：引导滤波的 base 应当比同半径箱式滤波**陡得多**。
        // 这条是「halo 从哪来」的守门测试 —— 一旦有人把 guide 换成普通模糊就会红。
        let step = Plane::from_fn(32, 8, |x, _| if x < 16 { 0.0 } else { 1.0 });
        let model = GuidedModel::analyze(&step, 3, 0.0001, 1);
        let guided = model.apply(&step);
        let boxed = box_blur(&step, 3);

        // 台阶两侧各取一个像素：引导滤波应当几乎原样保留跳变
        let left = guided.get(15, 4);
        let right = guided.get(16, 4);
        assert!(
            right - left > 0.9,
            "引导滤波把台阶抹平了：{left} → {right}"
        );
        let boxed_jump = boxed.get(16, 4) - boxed.get(15, 4);
        assert!(
            right - left > boxed_jump * 1.5,
            "引导滤波的跳变 {} 不比箱式的 {} 陡",
            right - left,
            boxed_jump
        );
    }

    #[test]
    fn guided_model_rejects_gradient_reversal() {
        // 梯度反转的判据：base 层不许出现「离台阶越远反而越靠近台阶值」的非单调。
        // 这里直接检查单调性：从左到右，base 必须单调不降。
        let step = Plane::from_fn(48, 4, |x, _| if x < 24 { 0.0 } else { 1.0 });
        let model = GuidedModel::analyze(&step, 5, 0.001, 1);
        let base = model.apply(&step);
        let mut previous = f32::NEG_INFINITY;
        for x in 0..48 {
            let value = base.get(x, 2);
            assert!(
                value >= previous - 1e-6,
                "第 {x} 列出现梯度反转：{previous} → {value}"
            );
            previous = value;
        }
    }

    #[test]
    fn guided_model_at_scale_four_tracks_the_full_resolution_guide() {
        // 快速引导滤波（1/4 分辨率估系数）与全分辨率估计的差距应当很小 ——
        // 这是「在低分辨率上估 a/b」这件事成立的前提，也是性能预算的根据。
        let source = Plane::from_fn(64, 64, |x, y| {
            #[allow(clippy::cast_precision_loss)]
            {
                let base = ((x / 8) as f32) * 0.25;
                let texture = if (x + y) % 2 == 0 { 0.05 } else { -0.05 };
                base + texture
            }
        });
        let exact = GuidedModel::analyze(&source, 4, 0.002, 1).apply(&source);
        let fast = GuidedModel::analyze(&source, 1, 0.002, 4).apply(&source);
        let mut worst = 0f32;
        for (a, b) in exact.values.iter().zip(fast.values.iter()) {
            worst = worst.max((a - b).abs());
        }
        // 实测 1/4 分辨率估系数与全分辨率估计的最大偏差 ≈ 0.031 log2（≈ 2% 亮度）——
        // 这就是「快」的代价，而生产路径上分析图本来就是全图的 1/4，这个偏差可以接受。
        assert!(worst < 0.05, "快速版与全分辨率版差了 {worst}（log2 亮度单位）");
    }
}
