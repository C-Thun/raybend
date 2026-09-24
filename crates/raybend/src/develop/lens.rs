//! **镜头校正**（M3-W4）：畸变 / 横向色差（TCA）/ 暗角的模型、数学与像素趟。
//!
//! # 数据从哪来，本模块认识谁
//!
//! 校准系数来自 [lensfun](https://lensfun.github.io/) 的公共数据库（数据 CC BY-SA 3.0），
//! 但**本模块不认识 lensfun**：它只认 [`LensCorrection`] 这个纯数据类型（模型种类 + 已按
//! 焦距/画幅重标定过的系数）。取系数那一层在 `crate::lens`（那里才依赖 lensfun 的 crate）——
//! 与 `raw` 的后端可插拔是同一套分层纪律：上游换了/挂了，管线一行都不用改。
//!
//! # 四条口径（都是 lensfun 官方文档/源码的规矩，**别按直觉改**）
//!
//! 1. **方向**：畸变公式 `Rd = Ru·(1 + k1·Ru²)` 描述的是「无畸变 → 有畸变」。
//!    校正时我们遍历**理想图**（目标像素）、到**源图**取样，用的正是这个**正向**映射
//!    （官方文档 `corrections.html`：「the formulae … map the _undistorted_ coordinate to the
//!    _distorted_ coordinate」）。上游 `EnableDistortionCorrection` 也是
//!    `Reverse==false → Dist_*`。
//!    ⚠️ **`lensfun` crate 的 README 示例写的是 `true`（说 true 才是校正）—— 那是文档错**，
//!    实施时用真照片验证过方向（见 `implementations/`）。
//! 2. **顺序**：暗角是在**原始像素**上测的 ⇒ 增益要按**源坐标**取，而不是目标坐标。
//! 3. **一次重采样**：畸变与 TCA 的坐标一起变换，只插值一次（官方原话）。
//! 4. **归一化坐标**：`NormScale = hypot(36,24) / crop / hypot(W+1, H+1) / real_focal`
//!    （`W = 图像宽 - 1`），`x_norm = x_px · NormScale − CenterX`。这个归一化与像素尺寸无关
//!    （同一张图缩到预览档，归一化坐标不变），所以系数可以跨档位复用。
//!
//! # 与上游的两处**有意偏离**
//!
//! * 上游在 u8/u16 上做暗角（定点乘法）；我们在**线性 u16** 上做 —— 乘性增益与色彩空间无关，
//!   而线性域少一次量化（`FUTURE.md` D1 的 scene-referred 口径）。
//! * 采样越界时**夹取**而不是留黑边：自动缩放（[`LensMap::auto_scale`]）已经保证源图铺满画布，
//!   夹取只是病态系数下的兜底（宁可边缘拉伸，也不要四角黑块）。
//!
//! # 参考与出处
//!
//! 模型公式与牛顿反解的细节对着上游 `libs/lensfun/{mod-coord,mod-subpix,mod-color}.cpp`
//! 与纯 Rust 移植 `vdavid/lensfun-rs`（LGPL-3.0-or-later）核对过；本文件的实现是照着
//! **公开的模型定义**重写的（没有搬运上游代码），许可上与本项目 AGPL-3.0-only 兼容。

use super::pipeline::LinearImage;

// ── 手动微调的最大幅度（拉杆 ±100 映射到这里；改它们 = 改观感）──

/// 手动畸变：`k1` 的最大绝对值（poly3 的 `k1`；0.15 在画幅角上约 5.7% 的径向位移）。
pub const MANUAL_DISTORTION_MAX: f32 = 0.15;
/// 手动暗角：画幅角上的最大增益偏移（`±0.6` ⇒ 0.4×..1.6×）。
pub const MANUAL_VIGNETTE_MAX: f32 = 0.6;
/// 手动色差：红/蓝径向缩放的最大偏移（`±0.25%`）。
pub const MANUAL_TCA_MAX: f32 = 0.0025;
/// 自动缩放留的余量（上游的千分之一：保证真的没有黑边）。
const AUTO_SCALE_MARGIN: f64 = 1.001;
/// 牛顿反解的收敛阈值（与上游 `NEWTON_EPS` 一致）。
const NEWTON_EPS: f64 = 0.000_01;

/* ══════════════════════════════════════════════════════════════
 * 一、模型（纯数据）
 * ══════════════════════════════════════════════════════════════ */

/// 径向畸变模型（lensfun 支持的三种；系数已按焦距/画幅重标定）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Distortion {
    /// `Rd = Ru·(1 + k1·Ru²)`
    Poly3 { k1: f32 },
    /// `Rd = Ru·(1 + k1·Ru² + k2·Ru⁴)`
    Poly5 { k1: f32, k2: f32 },
    /// `Rd = Ru·(a·Ru³ + b·Ru² + c·Ru + 1)`
    Ptlens { a: f32, b: f32, c: f32 },
}

impl Distortion {
    /// 正向：理想半径 → 有畸变半径（上游的 `Dist_*`；**校正取样用的就是它**）。
    #[must_use]
    pub fn forward(self, x: f32, y: f32) -> (f32, f32) {
        match self {
            Self::Poly3 { k1 } => {
                let poly = k1 * (x * x + y * y) + 1.0;
                (x * poly, y * poly)
            }
            Self::Poly5 { k1, k2 } => {
                let r2 = x * x + y * y;
                let poly = 1.0 + k1 * r2 + k2 * r2 * r2;
                (x * poly, y * poly)
            }
            Self::Ptlens { a, b, c } => {
                let r2 = x * x + y * y;
                let r = r2.sqrt();
                let poly = a * r2 * r + b * r2 + c * r + 1.0;
                (x * poly, y * poly)
            }
        }
    }

    /// 反向（牛顿迭代 ≤6 步）：有畸变半径 → 理想半径。
    ///
    /// 不收敛或解出负半径时**返回原坐标**（与上游一致）—— 自动缩放要用它，
    /// 而病态系数下「不动」比「给出 NaN」安全得多。
    #[must_use]
    pub fn inverse(self, x: f32, y: f32) -> (f32, f32) {
        let rd = f64::from(x) * f64::from(x) + f64::from(y) * f64::from(y);
        let rd = rd.sqrt();
        if rd == 0.0 {
            return (x, y);
        }
        let mut ru = rd;
        let converged = {
            let mut step = 0;
            loop {
                let f = self.residual(ru) - rd;
                if (-NEWTON_EPS..NEWTON_EPS).contains(&f) {
                    break true;
                }
                if step > 5 {
                    break false;
                }
                let prime = self.residual_prime(ru);
                if prime.abs() < f64::EPSILON {
                    break false;
                }
                ru -= f / prime;
                step += 1;
            }
        };
        if !converged || ru <= 0.0 {
            return (x, y);
        }
        #[allow(clippy::cast_possible_truncation)]
        let scale = (ru / rd) as f32;
        (x * scale, y * scale)
    }

    /// `Ru·f(Ru)`（反向解要求根的方程）。
    fn residual(self, r: f64) -> f64 {
        match self {
            Self::Poly3 { k1 } => r * (1.0 + f64::from(k1) * r * r),
            Self::Poly5 { k1, k2 } => {
                r * (1.0 + f64::from(k1) * r * r + f64::from(k2) * r.powi(4))
            }
            Self::Ptlens { a, b, c } => r * (f64::from(a) * r.powi(3)
                + f64::from(b) * r * r
                + f64::from(c) * r
                + 1.0),
        }
    }

    /// `d/dr [Ru·f(Ru)]`。
    fn residual_prime(self, r: f64) -> f64 {
        match self {
            Self::Poly3 { k1 } => 1.0 + 3.0 * f64::from(k1) * r * r,
            Self::Poly5 { k1, k2 } => {
                1.0 + 3.0 * f64::from(k1) * r * r + 5.0 * f64::from(k2) * r.powi(4)
            }
            Self::Ptlens { a, b, c } => {
                4.0 * f64::from(a) * r.powi(3)
                    + 3.0 * f64::from(b) * r * r
                    + 2.0 * f64::from(c) * r
                    + 1.0
            }
        }
    }
}

/// 横向色差（TCA）模型：红、蓝相对绿（绿不动）的径向映射。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tca {
    /// `Rd = Ru·k`（每通道一个比值）
    Linear { kr: f32, kb: f32 },
    /// `Rd = Ru·(b·Ru² + c·Ru + v)`，系数按 `[v, c, b]` 排
    Poly3 { red: [f32; 3], blue: [f32; 3] },
}

impl Tca {
    /// 正向（校正取样方向）：返回 `(红, 蓝)` 两个坐标；绿不动。
    #[must_use]
    pub fn forward(self, x: f32, y: f32) -> ((f32, f32), (f32, f32)) {
        match self {
            Self::Linear { kr, kb } => ((x * kr, y * kr), (x * kb, y * kb)),
            Self::Poly3 { red, blue } => (poly3_tca(x, y, red), poly3_tca(x, y, blue)),
        }
    }
}

/// `Rd = Ru·(b·Ru² + c·Ru + v)` 单通道。
fn poly3_tca(x: f32, y: f32, [v, c, b]: [f32; 3]) -> (f32, f32) {
    let r2 = x * x + y * y;
    let poly = if c == 0.0 {
        b * r2 + v
    } else {
        b * r2 + c * r2.sqrt() + v
    };
    (x * poly, y * poly)
}

/// 暗角（`pa` 模型）：`c = 1 + k1·r² + k2·r⁴ + k3·r⁶`。
///
/// **校正 = 乘 `c`**（上游 `Reverse==true` 那条路走的是 `ModifyColor_Vignetting_PA`，
/// 也就是乘 `c`；数据库里的 `k` 是**补偿**系数，不是衰减系数 —— 实测某镜头在画幅角上
/// `c ≈ 0.575`，乘它会更暗、乘 `1/c` 才是补回来，而上游校正路径乘的是 `c`，
/// 所以数据库里存的必然是补偿值）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vignetting {
    pub k1: f32,
    pub k2: f32,
    pub k3: f32,
}

impl Vignetting {
    /// 归一化半径平方 → 增益（`r` 的归一化见模块头第 4 条）。
    #[must_use]
    pub fn gain(self, r2: f32) -> f32 {
        let r4 = r2 * r2;
        let r6 = r4 * r2;
        1.0 + self.k1 * r2 + self.k2 * r4 + self.k3 * r6
    }
}

/// 三根拉杆的手动微调（`−1..1`，0 = 不动）。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ManualLens {
    /// 畸变（叠加在配置文件之上）
    pub distortion: f32,
    /// 暗角
    pub vignette: f32,
    /// 色差
    pub chromatic: f32,
}

impl ManualLens {
    /// 有没有动过（三个都是 0 就是没动）。
    #[must_use]
    pub fn is_identity(self) -> bool {
        self.distortion == 0.0 && self.vignette == 0.0 && self.chromatic == 0.0
    }

    /// 手动畸变叠加的 `k1`。
    fn k1(self) -> f32 {
        self.distortion * MANUAL_DISTORTION_MAX
    }

    /// 手动色差：红/蓝的径向缩放（红放大时蓝缩小）。
    fn tca_scales(self) -> (f32, f32) {
        let d = self.chromatic * MANUAL_TCA_MAX;
        (1.0 + d, 1.0 - d)
    }
}

/// **归一化坐标映射**（lensfun 的惯例；与像素尺寸无关）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Norm {
    /// `hypot(36,24) / crop / hypot(W+1, H+1) / real_focal`
    pub scale: f64,
    /// 光心在归一化坐标里的位置
    pub center_x: f64,
    pub center_y: f64,
    /// 图像半宽/半高/半对角（归一化）
    pub half_w: f64,
    pub half_h: f64,
    pub half_diag: f64,
}

impl Norm {
    /// 按镜头厂商给的 crop factor 与真实焦距算归一化（`real_focal` 缺省用标称焦距）。
    #[must_use]
    pub fn new(width: u32, height: u32, crop: f32, real_focal: f64) -> Self {
        let w = if width >= 2 { f64::from(width - 1) } else { 1.0 };
        let h = if height >= 2 { f64::from(height - 1) } else { 1.0 };
        let crop = if crop.is_finite() && crop > 0.0 {
            f64::from(crop)
        } else {
            1.0
        };
        let focal = if real_focal.is_finite() && real_focal > 0.0 {
            real_focal
        } else {
            1.0
        };
        let scale = 36.0_f64.hypot(24.0) / crop / (w + 1.0).hypot(h + 1.0) / focal;
        Self {
            scale,
            center_x: w / 2.0 * scale,
            center_y: h / 2.0 * scale,
            half_w: w / 2.0 * scale,
            half_h: h / 2.0 * scale,
            half_diag: (w * w + h * h).sqrt() / 2.0 * scale,
        }
    }

    /// 带光心偏移的版本（`center_x/y` 是镜头厂商给的相对偏移，单位 = 短边的一半）。
    #[must_use]
    pub fn with_center_offset(mut self, offset_x: f32, offset_y: f32, width: u32, height: u32) -> Self {
        let w = if width >= 2 { f64::from(width - 1) } else { 1.0 };
        let h = if height >= 2 { f64::from(height - 1) } else { 1.0 };
        let size = w.min(h);
        self.center_x += size / 2.0 * f64::from(offset_x) * self.scale;
        self.center_y += size / 2.0 * f64::from(offset_y) * self.scale;
        self
    }

    /// 像素 → 归一化（**已减光心**）。
    #[inline]
    fn to_norm(self, x: f32, y: f32) -> (f32, f32) {
        #[allow(clippy::cast_possible_truncation)]
        (
            (f64::from(x) * self.scale - self.center_x) as f32,
            (f64::from(y) * self.scale - self.center_y) as f32,
        )
    }

    /// 归一化（**已减光心**）→ 像素。
    #[inline]
    fn to_pixel(self, x: f32, y: f32) -> (f32, f32) {
        #[allow(clippy::cast_possible_truncation)]
        (
            ((f64::from(x) + self.center_x) / self.scale) as f32,
            ((f64::from(y) + self.center_y) / self.scale) as f32,
        )
    }

    /// 该方向上的**源图边界半径**（射线与矩形求交，归一化）。
    #[must_use]
    pub fn border_radius(self, dir_x: f64, dir_y: f64) -> f64 {
        let tx = if dir_x.abs() > f64::EPSILON {
            self.half_w / dir_x.abs()
        } else {
            f64::INFINITY
        };
        let ty = if dir_y.abs() > f64::EPSILON {
            self.half_h / dir_y.abs()
        } else {
            f64::INFINITY
        };
        tx.min(ty)
    }
}

/// 一次校正要用到的全部东西（**纯数据**，由 `crate::lens` 填）。
#[derive(Debug, Clone, PartialEq)]
pub struct LensCorrection {
    pub norm: Norm,
    /// 配置文件给的畸变模型（`None` = 没有配置文件/该镜头没标定）
    pub distortion: Option<Distortion>,
    pub tca: Option<Tca>,
    pub vignetting: Option<Vignetting>,
    pub manual: ManualLens,
}

impl LensCorrection {
    /// 没有配置文件、也没有手动微调 —— 整趟可以跳过。
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.distortion.is_none()
            && self.tca.is_none()
            && self.vignetting.is_none()
            && self.manual.is_identity()
    }
}

/* ══════════════════════════════════════════════════════════════
 * 二、映射（含自动缩放）
 * ══════════════════════════════════════════════════════════════ */

/// **算好的映射**：目标像素 → 源像素（含自动缩放）。
///
/// 自动缩放（[`Self::auto_scale`]）的口径与上游一致：取 8 个边界点（4 角 + 4 边中点）
/// 里**最紧**的那一个，让源图刚好铺满画布 —— 所以它可能是放大（枕形）也可能是略微缩小
/// （桶形）。它的承诺是「**没有空边**」，不是「尽量裁」。
#[derive(Debug, Clone)]
pub struct LensMap {
    norm: Norm,
    /// 目标坐标先乘它再做径向映射（= `1/auto_scale`）
    inv_scale: f64,
    distortion: Option<Distortion>,
    manual_k1: f32,
    tca: Option<Tca>,
    manual_tca: (f32, f32),
    vignetting: Option<Vignetting>,
    manual_vignette: f32,
    /// 归一化下的半对角（手动暗角以画幅角为 1）
    corner_r: f64,
    identity: bool,
}

impl LensMap {
    /// 从纯数据建映射（自动缩放在这里算一次）。
    #[must_use]
    pub fn new(correction: &LensCorrection) -> Self {
        let manual = correction.manual;
        let has_geometry = correction.distortion.is_some() || manual.distortion != 0.0;
        let has_tca = correction.tca.is_some() || manual.chromatic != 0.0;
        let scale = if has_geometry || has_tca {
            auto_scale(correction)
        } else {
            1.0
        };
        Self {
            norm: correction.norm,
            inv_scale: if scale.is_finite() && scale > 0.0 {
                1.0 / scale
            } else {
                1.0
            },
            distortion: correction.distortion,
            manual_k1: manual.k1(),
            tca: correction.tca,
            manual_tca: manual.tca_scales(),
            vignetting: correction.vignetting,
            manual_vignette: manual.vignette,
            corner_r: correction.norm.half_diag.max(f64::EPSILON),
            identity: correction.is_identity(),
        }
    }

    /// 整趟可以跳过吗（没有配置文件、也没有手动微调）。
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.identity
    }

    /// 算出来的自动缩放比（诊断与测试用）。
    #[must_use]
    pub fn auto_scale_value(&self) -> f64 {
        1.0 / self.inv_scale
    }

    /// 目标像素 → 源像素，返回 `[红, 绿, 蓝]` 三个坐标。
    ///
    /// 绿 = 只有畸变（TCA 的参照通道）；红/蓝在此之上再做各自的径向缩放。
    /// 暗角增益请用**绿**的坐标去问（[`Self::vignette_gain`]）——
    /// 那是「原始像素」的位置（模块头第 2 条）。
    #[must_use]
    pub fn source_coords(&self, x: f32, y: f32) -> [[f32; 2]; 3] {
        let (nx, ny) = self.norm.to_norm(x, y);
        #[allow(clippy::cast_possible_truncation)]
        let (sx, sy) = (
            (f64::from(nx) * self.inv_scale) as f32,
            (f64::from(ny) * self.inv_scale) as f32,
        );
        let (gx, gy) = self.distort(sx, sy);
        let (rx, ry) = self.tca_red(gx, gy);
        let (bx, by) = self.tca_blue(gx, gy);
        let (gx_p, gy_p) = self.norm.to_pixel(gx, gy);
        let (rx_p, ry_p) = self.norm.to_pixel(rx, ry);
        let (bx_p, by_p) = self.norm.to_pixel(bx, by);
        [[rx_p, ry_p], [gx_p, gy_p], [bx_p, by_p]]
    }

    /// 归一化坐标上的畸变（配置文件模型 → 手动 poly3 项，顺序与上游一致）。
    #[inline]
    fn distort(&self, x: f32, y: f32) -> (f32, f32) {
        let (mut x, mut y) = match self.distortion {
            Some(model) => model.forward(x, y),
            None => (x, y),
        };
        if self.manual_k1 != 0.0 {
            let poly = self.manual_k1 * (x * x + y * y) + 1.0;
            x *= poly;
            y *= poly;
        }
        (x, y)
    }

    /// 红通道的归一化坐标（配置文件 TCA → 手动缩放）。
    #[inline]
    fn tca_red(&self, x: f32, y: f32) -> (f32, f32) {
        let (r, _) = match self.tca {
            Some(model) => model.forward(x, y),
            None => ((x, y), (x, y)),
        };
        (r.0 * self.manual_tca.0, r.1 * self.manual_tca.0)
    }

    /// 蓝通道的归一化坐标。
    #[inline]
    fn tca_blue(&self, x: f32, y: f32) -> (f32, f32) {
        let (_, b) = match self.tca {
            Some(model) => model.forward(x, y),
            None => ((x, y), (x, y)),
        };
        (b.0 * self.manual_tca.1, b.1 * self.manual_tca.1)
    }

    /// **源像素**处的暗角增益（`1.0` = 不补）。
    #[must_use]
    pub fn vignette_gain(&self, source_x: f32, source_y: f32) -> f32 {
        if self.vignetting.is_none() && self.manual_vignette == 0.0 {
            return 1.0;
        }
        let (nx, ny) = self.norm.to_norm(source_x, source_y);
        let r2 = nx * nx + ny * ny;
        let mut gain = self.vignetting.map_or(1.0, |v| v.gain(r2));
        if self.manual_vignette != 0.0 {
            #[allow(clippy::cast_possible_truncation)]
            let t = (f64::from(r2) / (self.corner_r * self.corner_r)) as f32;
            gain *= 1.0 + self.manual_vignette * MANUAL_VIGNETTE_MAX * t;
        }
        if gain.is_finite() && gain > 0.0 {
            gain
        } else {
            1.0
        }
    }
}

/// **自动缩放**：让源图刚好铺满画布的那个比例（上游 `GetAutoScale` 的口径）。
///
/// 做法：对 8 个边界点，解出「映射到源图边界上的理想半径」`ru`，取 `d / ru` 的最大值，
/// 再乘千分之一余量。`ru` 就是**反向**映射（牛顿解）—— 所以这里用的正是
/// [`Distortion::inverse`]。
#[must_use]
pub fn auto_scale(correction: &LensCorrection) -> f64 {
    let norm = correction.norm;
    let half_w = norm.half_w;
    let half_h = norm.half_h;
    // 4 角 + 4 边中点（与上游同序，不影响结果）
    let points: [(f64, f64); 8] = [
        (half_w, half_h),
        (-half_w, half_h),
        (-half_w, -half_h),
        (half_w, -half_h),
        (half_w, 0.0),
        (-half_w, 0.0),
        (0.0, half_h),
        (0.0, -half_h),
    ];
    let mut scale = 0.01_f64;
    for (px, py) in points {
        let dist = (px * px + py * py).sqrt();
        if dist <= f64::EPSILON {
            continue;
        }
        let (dx, dy) = (px / dist, py / dist);
        let border = norm.border_radius(dx, dy);
        if !border.is_finite() {
            continue;
        }
        #[allow(clippy::cast_possible_truncation)]
        let (bx, by) = ((border * dx) as f32, (border * dy) as f32);
        // 反向映射：源图边界点 → 理想半径
        let (ix, iy) = inverse_map(correction, bx, by);
        let ru = f64::from(ix) * f64::from(ix) + f64::from(iy) * f64::from(iy);
        let ru = ru.sqrt();
        if !ru.is_finite() || ru <= f64::EPSILON {
            continue;
        }
        let point_scale = dist / ru;
        if point_scale > scale {
            scale = point_scale;
        }
    }
    let mut scale = scale * AUTO_SCALE_MARGIN;
    // 有 TCA 时再多留千分之一（上游同样处理：亚像素通道可能把边界推出去一点）
    if correction.tca.is_some() || correction.manual.chromatic != 0.0 {
        scale *= AUTO_SCALE_MARGIN;
    }
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

/// 归一化坐标上的**反向映射**（自动缩放用）：先反手动项、再反配置文件模型。
fn inverse_map(correction: &LensCorrection, x: f32, y: f32) -> (f32, f32) {
    let mut x = x;
    let mut y = y;
    // 手动 poly3 项在正向里是**后**做的 ⇒ 反向里要**先**反掉
    let k1 = correction.manual.k1();
    if k1 != 0.0 {
        let (ix, iy) = Distortion::Poly3 { k1 }.inverse(x, y);
        x = ix;
        y = iy;
    }
    if let Some(model) = correction.distortion {
        let (ix, iy) = model.inverse(x, y);
        x = ix;
        y = iy;
    }
    (x, y)
}

/* ══════════════════════════════════════════════════════════════
 * 三、像素趟
 * ══════════════════════════════════════════════════════════════ */

/// **镜头校正趟**：线性 u16 源 → 线性 u16 目标（同尺寸，一次重采样）。
///
/// 每个目标像素：算三个源坐标（红/绿/蓝）→ 各自双线性采样 → 乘上**源坐标处**的暗角增益。
/// 映射是恒等时直接 `clone`（**逐位一致**，不是「近似不变」）。
#[must_use]
pub fn warp_lens(source: &LinearImage, map: &LensMap) -> LinearImage {
    if map.is_identity() {
        return source.clone();
    }
    let width = source.width as usize;
    let height = source.height as usize;
    let mut out = vec![0u16; source.rgb.len()];
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(16);
    if threads <= 1 || height < 32 {
        warp_rows(source, &mut out, map, 0);
        return LinearImage {
            width: source.width,
            height: source.height,
            rgb: out,
        };
    }
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
            scope.spawn(move || warp_rows(source, chunk, map, start));
        }
    });
    LinearImage {
        width: source.width,
        height: source.height,
        rgb: out,
    }
}

/// 一段行（**唯一**的镜头校正像素循环）。
fn warp_rows(source: &LinearImage, out: &mut [u16], map: &LensMap, first_row: usize) {
    let width = source.width as usize;
    let height = source.height as usize;
    for (local_row, line) in out.as_chunks_mut::<3>().0.chunks_mut(width).enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let y = (first_row + local_row) as f32;
        for (x, pixel) in line.iter_mut().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let x = x as f32;
            let coords = map.source_coords(x, y);
            let gain = map.vignette_gain(coords[1][0], coords[1][1]);
            for channel in 0..3 {
                let [sx, sy] = coords[channel];
                let value = sample_linear(&source.rgb, width, height, sx, sy, channel) * gain;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    pixel[channel] = (value + 0.5).clamp(0.0, 65535.0) as u16;
                }
            }
        }
    }
}

/// 双线性采样（越界夹取 —— 自动缩放已保证不会用到，这里是兜底）。
#[inline]
fn sample_linear(rgb: &[u16], width: usize, height: usize, x: f32, y: f32, channel: usize) -> f32 {
    if !x.is_finite() || !y.is_finite() {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let (max_x, max_y) = ((width - 1) as f32, (height - 1) as f32);
    let x = x.clamp(0.0, max_x);
    let y = y.clamp(0.0, max_y);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let x0 = x.floor() as usize;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    #[allow(clippy::cast_precision_loss)]
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let at = |row: usize, col: usize| f32::from(rgb[(row * width + col) * 3 + channel]);
    let top = at(y0, x0) + (at(y0, x1) - at(y0, x0)) * fx;
    let bottom = at(y1, x0) + (at(y1, x1) - at(y1, x0)) * fx;
    top + (bottom - top) * fy
}

/* ══════════════════════════════════════════════════════════════
 * 四、单测（期望值都是**外部给定**的：手算或上游公式）
 * ══════════════════════════════════════════════════════════════ */

#[cfg(test)]
mod tests {
    use super::*;

    fn linear(width: u32, height: u32, f: impl Fn(u32, u32) -> [u16; 3]) -> LinearImage {
        let mut rgb = Vec::new();
        for y in 0..height {
            for x in 0..width {
                rgb.extend_from_slice(&f(x, y));
            }
        }
        LinearImage::new(width, height, rgb).expect("尺寸与长度对得上")
    }

    fn ramp(width: u32, height: u32) -> LinearImage {
        linear(width, height, |x, y| {
            [
                u16::try_from(x * 1000).unwrap_or(u16::MAX),
                u16::try_from(y * 1000).unwrap_or(u16::MAX),
                40000,
            ]
        })
    }

    fn correction(norm: Norm) -> LensCorrection {
        LensCorrection {
            norm,
            distortion: None,
            tca: None,
            vignetting: None,
            manual: ManualLens::default(),
        }
    }

    // ── 模型：手算的期望值 ──

    #[test]
    fn poly3_forward_matches_hand_computed_value() {
        // k1 = 0.1，(0.5, 0) → r² = 0.25 → 系数 1 + 0.1·0.25 = 1.025 → (0.5125, 0)
        let (x, y) = Distortion::Poly3 { k1: 0.1 }.forward(0.5, 0.0);
        assert!((x - 0.5125).abs() < 1e-6, "x = {x}");
        assert!(y.abs() < 1e-6);
        // 零点必须逐位不动
        assert_eq!(Distortion::Poly3 { k1: 0.1 }.forward(0.0, 0.0), (0.0, 0.0));
    }

    #[test]
    fn poly5_and_ptlens_zero_coefficients_are_identity() {
        for model in [
            Distortion::Poly5 { k1: 0.0, k2: 0.0 },
            Distortion::Ptlens {
                a: 0.0,
                b: 0.0,
                c: 0.0,
            },
        ] {
            assert_eq!(model.forward(0.3, -0.4), (0.3, -0.4));
            assert_eq!(model.inverse(0.3, -0.4), (0.3, -0.4));
        }
    }

    #[test]
    fn inverse_round_trips_every_model() {
        let models = [
            Distortion::Poly3 { k1: -0.12 },
            Distortion::Poly5 {
                k1: -0.08,
                k2: 0.02,
            },
            Distortion::Ptlens {
                a: 0.03,
                b: -0.05,
                c: 0.0,
            },
        ];
        for model in models {
            for (x, y) in [(0.2, 0.1), (0.4, -0.3), (0.6, 0.5)] {
                let (fx, fy) = model.forward(x, y);
                let (ix, iy) = model.inverse(fx, fy);
                assert!(
                    (ix - x).abs() < 1e-4 && (iy - y).abs() < 1e-4,
                    "{model:?} 在 ({x},{y}) 上往返失败：({ix},{iy})"
                );
            }
        }
    }

    #[test]
    fn tca_linear_scales_red_and_blue_apart() {
        let ((rx, ry), (bx, by)) = Tca::Linear {
            kr: 1.001,
            kb: 0.999,
        }
        .forward(100.0, 200.0);
        assert!((rx - 100.1).abs() < 1e-3 && (ry - 200.2).abs() < 1e-3);
        assert!((bx - 99.9).abs() < 1e-3 && (by - 199.8).abs() < 1e-3);
    }

    #[test]
    fn tca_poly3_with_zero_c_is_pure_quadratic() {
        // v=1, c=0, b=0.01 → Rd = Ru·(1 + 0.01·Ru²)；Ru² = 0.25 → 1.0025
        let ((rx, _), _) = Tca::Poly3 {
            red: [1.0, 0.0, 0.01],
            blue: [1.0, 0.0, 0.0],
        }
        .forward(0.5, 0.0);
        assert!((rx - 0.501_25).abs() < 1e-6, "rx = {rx}");
    }

    #[test]
    fn vignetting_gain_matches_hand_computed_value() {
        // k1 = -1.0、其余 0：r² = 0.25 → 1 - 0.25 = 0.75
        let v = Vignetting {
            k1: -1.0,
            k2: 0.0,
            k3: 0.0,
        };
        assert!((v.gain(0.25) - 0.75).abs() < 1e-6);
        assert!((v.gain(0.0) - 1.0).abs() < 1e-6);
    }

    // ── 归一化坐标 ──

    #[test]
    fn norm_scale_matches_lensfun_formula() {
        // 6000×4000、crop=1、f=35：hypot(36,24)/hypot(6000,4000)/35
        // （上游：`Width = imgwidth - 1`，再用 `hypot(Width + 1, Height + 1)` ⇒ 就是原尺寸）
        let norm = Norm::new(6000, 4000, 1.0, 35.0);
        let expected = 36.0_f64.hypot(24.0) / 6000.0_f64.hypot(4000.0) / 35.0;
        assert!((norm.scale - expected).abs() < 1e-18, "{}", norm.scale);
        // 画幅角：归一化半径应约 0.618（= 半对角 mm / 焦距）
        assert!((norm.half_diag - 21.633_3 / 35.0).abs() < 1e-3, "{}", norm.half_diag);
    }

    #[test]
    fn norm_is_scale_invariant_across_tiers() {
        // 同一张图缩到预览档：像素减半，归一化坐标（相对光心）几乎不变
        let full = Norm::new(6000, 4000, 1.0, 35.0);
        let preview = Norm::new(3000, 2000, 1.0, 35.0);
        let (fx, fy) = full.to_norm(1500.0, 1000.0);
        let (px, py) = preview.to_norm(750.0, 500.0);
        assert!((fx - px).abs() < 1e-4 && (fy - py).abs() < 1e-4, "{fx},{fy} vs {px},{py}");
    }

    #[test]
    fn border_radius_hits_the_expected_edge() {
        let norm = Norm::new(6000, 4000, 1.0, 35.0);
        // 正右方向 → 打到 x = half_w
        assert!((norm.border_radius(1.0, 0.0) - norm.half_w).abs() < 1e-12);
        // 3:2 的画幅里 45° 打到的是**上/下边**（半宽 > 半高），不是角
        let d = std::f64::consts::FRAC_1_SQRT_2;
        assert!((norm.border_radius(d, d) - norm.half_h * 2.0_f64.sqrt()).abs() < 1e-12);
        // 正对角方向才打到角上
        let diag = (norm.half_w * norm.half_w + norm.half_h * norm.half_h).sqrt();
        assert!((norm.border_radius(norm.half_w / diag, norm.half_h / diag) - norm.half_diag).abs() < 1e-12);
    }

    // ── 映射与自动缩放 ──

    #[test]
    fn identity_map_keeps_every_pixel_bit_for_bit() {
        let source = ramp(64, 48);
        let map = LensMap::new(&correction(Norm::new(64, 48, 1.0, 35.0)));
        assert!(map.is_identity());
        let out = warp_lens(&source, &map);
        assert_eq!(out.rgb, source.rgb, "恒等映射必须逐位一致");
    }

    #[test]
    fn center_pixel_never_moves() {
        // 光心处半径 0：畸变与 TCA 都不动它
        let norm = Norm::new(101, 101, 1.0, 35.0);
        let mut c = correction(norm);
        c.distortion = Some(Distortion::Poly3 { k1: -0.2 });
        c.tca = Some(Tca::Linear {
            kr: 1.01,
            kb: 0.99,
        });
        let map = LensMap::new(&c);
        let coords = map.source_coords(50.0, 50.0);
        for [x, y] in coords {
            assert!((x - 50.0).abs() < 1e-3 && (y - 50.0).abs() < 1e-3, "{x},{y}");
        }
    }

    #[test]
    fn barrel_autoscale_keeps_every_border_point_inside() {
        // 桶形：自动缩放后，8 个边界点都必须落在源图内（这是「没有空边」的定义）
        let norm = Norm::new(600, 400, 1.0, 35.0);
        let mut c = correction(norm);
        c.distortion = Some(Distortion::Poly3 { k1: -0.3 });
        let map = LensMap::new(&c);
        assert!(map.auto_scale_value() > 0.0);
        for (x, y) in [
            (0.0, 0.0),
            (599.0, 0.0),
            (0.0, 399.0),
            (599.0, 399.0),
            (299.5, 0.0),
            (299.5, 399.0),
            (0.0, 199.5),
            (599.0, 199.5),
        ] {
            let coords = map.source_coords(x, y);
            for [sx, sy] in coords {
                assert!(
                    (-0.5..=599.5).contains(&sx) && (-0.5..=399.5).contains(&sy),
                    "({x},{y}) 映射到 ({sx},{sy})：跑出源图了"
                );
            }
        }
    }

    #[test]
    fn pincushion_autoscale_zooms_in() {
        // 枕形（k1 > 0）：不缩放会露出黑边，所以自动缩放必须**放大**（scale > 1）
        let norm = Norm::new(600, 400, 1.0, 35.0);
        let mut c = correction(norm);
        c.distortion = Some(Distortion::Poly3 { k1: 0.3 });
        let map = LensMap::new(&c);
        assert!(map.auto_scale_value() > 1.0, "scale = {}", map.auto_scale_value());
    }

    #[test]
    fn manual_distortion_alone_still_triggers_autoscale() {
        // 没有配置文件、只有手动拉杆：也要算自动缩放（否则枕形手动量会露黑边）
        let norm = Norm::new(600, 400, 1.0, 35.0);
        let mut c = correction(norm);
        c.manual.distortion = 1.0;
        let map = LensMap::new(&c);
        assert!(!map.is_identity());
        assert!(map.auto_scale_value() > 1.0);
    }

    #[test]
    fn vignette_gain_is_evaluated_on_the_source_side() {
        // 暗角是乘性增益：画幅角（源坐标在角上）拿到的增益应当偏离 1，光心处恰好是 1
        let norm = Norm::new(600, 400, 1.0, 35.0);
        let mut c = correction(norm);
        c.vignetting = Some(Vignetting {
            k1: -0.5,
            k2: 0.0,
            k3: 0.0,
        });
        let map = LensMap::new(&c);
        let center = map.vignette_gain(299.5, 199.5);
        assert!((center - 1.0).abs() < 1e-3, "光心增益 = {center}");
        let corner = map.vignette_gain(599.0, 399.0);
        assert!(corner < 1.0, "角上增益 = {corner}（补偿系数为负 ⇒ 小于 1）");
    }

    #[test]
    fn warp_with_vignetting_brightens_corners_but_not_center() {
        let source = linear(64, 48, |_, _| [30000, 30000, 30000]);
        let norm = Norm::new(64, 48, 1.0, 35.0);
        let mut c = correction(norm);
        // 补偿系数为负 ⇒ 校正增益 < 1 ⇒ 角上变暗；这是「方向正确」的判据
        c.vignetting = Some(Vignetting {
            k1: -0.5,
            k2: 0.0,
            k3: 0.0,
        });
        let out = warp_lens(&source, &LensMap::new(&c));
        let center = out.rgb[(24 * 64 + 32) * 3];
        let corner = out.rgb[0];
        // 光心像素不落在 r=0 上（(32,24) 相对光心还有半个像素），所以给个容差
        assert!(
            center.abs_diff(30000) <= 5,
            "光心不该被暗角明显动到：{center}"
        );
        assert!(corner < center - 1000, "角上应当被乘上 < 1 的增益：{corner} vs {center}");
    }

    #[test]
    fn warp_clamps_out_of_bounds_instead_of_black() {
        // 病态输入：映射跑到图外也不许出现黑边（夹取到边缘像素）
        let source = linear(8, 8, |_, _| [1000, 2000, 3000]);
        let norm = Norm::new(8, 8, 1.0, 35.0);
        let mut c = correction(norm);
        c.distortion = Some(Distortion::Poly3 { k1: 5.0 });
        // 手动把缩放比压成 1（绕过自动缩放）—— 故意造越界
        let mut map = LensMap::new(&c);
        map.inv_scale = 1.0;
        let out = warp_lens(&source, &map);
        for pixel in out.rgb.as_chunks::<3>().0 {
            assert_eq!(*pixel, [1000, 2000, 3000], "越界夹取不该产生 0（黑）");
        }
    }
}
