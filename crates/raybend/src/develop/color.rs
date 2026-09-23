//! 色彩基础：sRGB 传递函数、色温 ↔ 色度、白平衡增益。
//!
//! # 这一份是**唯一**的色彩数学
//!
//! 管线（`super::pipeline`）、RAW 的 as-shot 色温估计（`crate::raw::rawler_backend` 里的调用）
//! 与显示变换都从这里取函数 —— 不允许在别处再写一份 `pow(2.2)` 或第二个色温近似公式
//! （`AGENTS.md` §2.12）。
//!
//! # 色温的两套算法（人类 2026-09-24 定）
//!
//! 界面上的色温是**绝对 K**（2500–10000K），但两条路的底子不同：
//!
//! | 路 | 底子 | 怎么算 |
//! | --- | --- | --- |
//! | RAW | 相机元数据里有 as-shot 白平衡系数 + 色彩矩阵 | **真绝对**：从 `wb_coeffs` 反算 as-shot 色温（[`cct_from_camera_neutral`]），再用 [`temperature_gain_ratio`] 求「目标 K 相对 as-shot K」的增益 |
//! | JPG / RAW 内嵌预览 | 相机已经白平衡过了，图里没有「原始色温」这回事 | **表面绝对、本质相对**：以 EXIF 推出来的色温（读不到就用 6250K）为基准，同样用 [`temperature_gain_ratio`] |
//!
//! 两条路共用同一个增益函数 —— 差别只在**基准从哪来**（`DevelopParams::baseline`），
//! 所以不会出现「同一件事两处写」。
//!
//! # 色温近似公式的出处
//!
//! * 普朗克轨迹 1667–4000K：Kim et al. (1999) 的三次近似（论文 *Design of Advanced
//!   Color Gamut Mapping Using CIECAM97s*）；
//! * 日光轨迹 4000–25000K：Kang et al. (2002) 的三次近似。
//!
//! 两条都是行业里通行的近似（darktable / libraw / 各种 `color-temperature` 实现都用它们），
//! 精度在几十 K 量级 —— 对「把标尺挪到照片自己的色温上」这件事完全够。

/// sRGB 的 EOTF：显示值（0..1）→ 线性光（0..1）。
///
/// 负值原样返回（管线里允许轻微负值存在，编码时再夹）。
#[must_use]
pub fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.0 {
        return value;
    }
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB 的 OETF：线性光（0..1）→ 显示值（0..1）。
#[must_use]
pub fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.0 {
        return value;
    }
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

/// 256 项的「8bit sRGB → 线性」查表（位图那两条路每张图都要用，别每次 `powf`）。
#[must_use]
pub fn srgb8_to_linear_table() -> [f32; 256] {
    let mut table = [0.0f32; 256];
    for (index, slot) in table.iter_mut().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let value = index as f32 / 255.0;
        *slot = srgb_to_linear(value);
    }
    table
}

/// sRGB → XYZ（D65）。
pub const SRGB_TO_XYZ_D65: [[f32; 3]; 3] = [
    [0.412_456_4, 0.357_576_1, 0.180_437_5],
    [0.212_672_9, 0.715_152_2, 0.072_175_0],
    [0.019_333_9, 0.119_192, 0.950_304_1],
];

/// XYZ → sRGB（D65）。
pub const XYZ_TO_SRGB_D65: [[f32; 3]; 3] = [
    [3.240_454_2, -1.537_138_5, -0.498_531_4],
    [-0.969_266, 1.876_010_8, 0.041_556_0],
    [0.055_643_4, -0.204_025_9, 1.057_225_2],
];

/// 3×3 矩阵乘向量。
#[must_use]
pub fn mat3_vec3(matrix: [[f32; 3]; 3], vector: [f32; 3]) -> [f32; 3] {
    [
        matrix[0][0] * vector[0] + matrix[0][1] * vector[1] + matrix[0][2] * vector[2],
        matrix[1][0] * vector[0] + matrix[1][1] * vector[1] + matrix[1][2] * vector[2],
        matrix[2][0] * vector[0] + matrix[2][1] * vector[1] + matrix[2][2] * vector[2],
    ]
}

/// 3×3 求逆（行列式太小就返回 `None` —— 不猜、不返回单位矩阵）。
#[must_use]
pub fn mat3_inverse(matrix: [[f32; 3]; 3]) -> Option<[[f32; 3]; 3]> {
    let [[a, b, c], [d, e, f], [g, h, i]] = matrix;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    Some([
        [
            (e * i - f * h) * inv,
            (c * h - b * i) * inv,
            (b * f - c * e) * inv,
        ],
        [
            (f * g - d * i) * inv,
            (a * i - c * g) * inv,
            (c * d - a * f) * inv,
        ],
        [
            (d * h - e * g) * inv,
            (b * g - a * h) * inv,
            (a * e - b * d) * inv,
        ],
    ])
}

/// 色温（K）→ CIE 1931 色度 `(x, y)`。
///
/// 输入会被夹到近似公式的适用区间（1667–25000K）—— 我们的拉杆只到 2500–10000K，
/// 这里放宽只是为了给 as-shot 估计留余量。
#[must_use]
pub fn kelvin_to_xy(kelvin: f32) -> (f32, f32) {
    let t = kelvin.clamp(1667.0, 25000.0);
    let t2 = t * t;
    let t3 = t2 * t;
    let x = if t <= 4000.0 {
        -0.266_123_9e9 / t3 - 0.234_358_9e6 / t2 + 0.877_695_6e3 / t + 0.179_910
    } else {
        -3.025_846_9e9 / t3 + 2.107_038e6 / t2 + 0.222_634_7e3 / t + 0.240_390
    };
    let x2 = x * x;
    let x3 = x2 * x;
    let y = if t <= 2222.0 {
        -1.106_381_4 * x3 - 1.348_110_2 * x2 + 2.185_558_3 * x - 0.202_196_83
    } else if t <= 4000.0 {
        -0.954_947_6 * x3 - 1.374_185_9 * x2 + 2.091_37 * x - 0.167_488_67
    } else {
        3.081_758 * x3 - 5.873_387 * x2 + 3.751_129_9 * x - 0.370_014_83
    };
    (x, y)
}

/// 色度 `(x, y)` → XYZ（Y = 1）。
#[must_use]
pub fn xy_to_xyz(x: f32, y: f32) -> [f32; 3] {
    let y = if y.abs() < 1e-6 { 1e-6 } else { y };
    [x / y, 1.0, (1.0 - x - y) / y]
}

/// 某个色温的**中性化增益**（`[r, g, b]`，绿归一为 1）。
///
/// 语义：把「这个色温的光」变成中性（R=G=B）所需要的通道倍率。
/// 也就是对画面乘上它 ⇒ 画面看起来像是**在这个色温下白平衡**的结果。
#[must_use]
pub fn white_balance_gains(kelvin: f32) -> [f32; 3] {
    let (x, y) = kelvin_to_xy(kelvin);
    let white = mat3_vec3(XYZ_TO_SRGB_D65, xy_to_xyz(x, y));
    // 除以色光的 RGB（= 乘 1/白），再按绿归一 —— 这样亮度不会被白平衡整体抬高或压低
    let green = if white[1].abs() < 1e-6 { 1e-6 } else { white[1] };
    [
        green / if white[0].abs() < 1e-6 { 1e-6 } else { white[0] },
        1.0,
        green / if white[2].abs() < 1e-6 { 1e-6 } else { white[2] },
    ]
}

/// 「目标色温相对基准色温」的增益比（绿归一为 1）。
///
/// * `target == baseline` ⇒ 正好 `[1, 1, 1]`（**这条是「不动就不变」的保证**，有测试钉着）；
/// * 拖到**高 K**（10000K）⇒ 抬红压蓝 ⇒ **画面变暖**；拖到**低 K**（3000K）⇒ 压红抬蓝 ⇒ 画面变冷。
///
/// # 方向为什么是这个方向（反直觉，写清楚免得后人「修」反）
///
/// 拉杆上的 K 是**「假定当时的光有多暖」**，不是「我要把画面调多暖」：
/// 说「当时的光是 3000K（很暖）」⇒ 转换器就要把这个暖色**冷下来** ⇒ 画面偏蓝。
/// Lightroom / ACR 的色温滑杆也是这个方向（左端蓝、右端黄），实测一致。
#[must_use]
pub fn temperature_gain_ratio(target_kelvin: f32, baseline_kelvin: f32) -> [f32; 3] {
    let target = white_balance_gains(target_kelvin);
    let base = white_balance_gains(baseline_kelvin);
    let green = (target[1] / base[1]).max(1e-6);
    [
        (target[0] / base[0]) / green,
        1.0,
        (target[2] / base[2]) / green,
    ]
}

/// 从**相机中性方向**（未白平衡的相机 RGB）估 as-shot 色温。
///
/// 相机中性 = `(1/wb_r, 1/wb_g, 1/wb_b)`（`wb_coeffs` 正是「把这张照片的照明变成中性」
/// 的倍率，所以它的倒数就是照明在相机里的响应）。乘上相机矩阵的逆就得到 XYZ，
/// 取色度再反查色温。
///
/// 反查用**同一个** [`kelvin_to_xy`] 前向模型做数值搜索（扫描 + 二分），
/// 不用 McCamy 那种另一个公式 —— 两套公式不一致会让「as-shot 时画面中性」这条性质漂掉。
#[must_use]
pub fn cct_from_camera_neutral(cam_neutral: [f32; 3], xyz_to_cam: [[f32; 3]; 3]) -> Option<f32> {
    let cam_to_xyz = mat3_inverse(xyz_to_cam)?;
    let xyz = mat3_vec3(cam_to_xyz, cam_neutral);
    if !xyz.iter().all(|v| v.is_finite()) || xyz[1].abs() < 1e-9 {
        return None;
    }
    let (_x, y) = (xyz[0] / xyz[1], 1.0);
    let _ = y;
    let sum = xyz[0] + xyz[1] + xyz[2];
    if sum.abs() < 1e-9 {
        return None;
    }
    let (x, y) = (xyz[0] / sum, xyz[1] / sum);
    cct_from_xy(x, y)
}

/// 色度 `(x, y)` → 色温（在 [`kelvin_to_xy`] 的前向模型上做数值反查）。
///
/// 先粗扫（对数间隔，覆盖 1667–25000K）找到最近的一段，再二分细化。
#[must_use]
pub fn cct_from_xy(x: f32, y: f32) -> Option<f32> {
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    let low = 1667.0f32;
    let high = 25000.0f32;
    let samples = 96usize;
    let ratio = high / low;
    let mut best = (f32::INFINITY, low);
    for index in 0..=samples {
        #[allow(clippy::cast_precision_loss)]
        let t = low * ratio.powf(index as f32 / samples as f32);
        let (cx, cy) = kelvin_to_xy(t);
        let distance = (cx - x) * (cx - x) + (cy - y) * (cy - y);
        if distance < best.0 {
            best = (distance, t);
        }
    }
    // 在最近样本的两侧做二分（对数区间）
    #[allow(clippy::cast_precision_loss)]
    let step = ratio.powf(1.0 / samples as f32);
    let mut lo = (best.1 / step).max(low);
    let mut hi = (best.1 * step).min(high);
    for _ in 0..48 {
        let mid = (lo * hi).sqrt();
        let (mx, my) = kelvin_to_xy(mid);
        let mid_d = (mx - x) * (mx - x) + (my - y) * (my - y);
        let (lx, ly) = kelvin_to_xy(lo);
        let lo_d = (lx - x) * (lx - x) + (ly - y) * (ly - y);
        if mid_d < lo_d {
            lo = mid;
        } else {
            hi = mid;
        }
        if (hi / lo - 1.0).abs() < 1e-4 {
            break;
        }
    }
    Some((lo * hi).sqrt())
}

/// EXIF `LightSource`（0x9208）枚举值 → 标称色温（K）。
///
/// 这是标准 EXIF 里**唯一**能直接给出照明色温的地方（EXIF 没有「色温 K」这个字段，
/// 有的机型把 K 写进 MakerNote 或 DNG 的 `ColorTemperature` —— 那两个不在本表的范围里）。
/// 表里没有的（0 = unknown / 255 = other）返回 `None`：**不许猜**，
/// 猜出来的基准会让「载入时标尺归位」变成随机跳。
#[must_use]
pub fn light_source_kelvin(light_source: u32) -> Option<f32> {
    Some(match light_source {
        1 => 5500.0,   // Daylight
        2 => 4200.0,   // Fluorescent（宽泛，取常见值）
        3 => 2856.0,   // Tungsten（Standard Light A）
        4 => 5500.0,   // Flash
        9 => 5500.0,   // Fine weather
        10 => 6500.0,  // Cloudy
        11 => 7500.0,  // Shade
        12 => 6500.0,  // Daylight fluorescent
        13 => 6500.0,  // Day white fluorescent
        14 => 5000.0,  // Cool white fluorescent
        15 => 4200.0,  // White fluorescent
        17 => 2856.0,  // Standard Light A
        18 => 4874.0,  // Standard Light B
        19 => 6774.0,  // Standard Light C
        20 => 5503.0,  // D55
        21 => 6504.0,  // D65
        22 => 7504.0,  // D75
        23 => 5003.0,  // D50
        24 => 2856.0,  // ISO studio tungsten
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, tolerance: f32) -> bool {
        (a - b).abs() <= tolerance
    }

    #[test]
    fn srgb_transfer_round_trips() {
        // 外部给定：sRGB 0.5 的线性值 = 0.2140411（IEC 61966-2-1 定义算出来的）
        assert!(close(srgb_to_linear(0.5), 0.214_041_14, 1e-6));
        // 线性 0.18（中灰）→ sRGB ≈ 0.4613561
        assert!(close(linear_to_srgb(0.18), 0.461_356_1, 1e-5));
        assert!(close(linear_to_srgb(1.0), 1.0, 1e-6));
        assert!(close(srgb_to_linear(1.0), 1.0, 1e-6));
        assert_eq!(srgb_to_linear(0.0), 0.0);
        assert_eq!(linear_to_srgb(0.0), 0.0);
        // 负值原样穿过（管线里允许轻微负值）
        assert_eq!(srgb_to_linear(-0.1), -0.1);
        for step in 0..=100 {
            let v = step as f32 / 100.0;
            assert!(
                close(linear_to_srgb(srgb_to_linear(v)), v, 1e-5),
                "往返不一致：{v}"
            );
        }
    }

    #[test]
    fn srgb8_table_matches_the_function() {
        let table = srgb8_to_linear_table();
        assert_eq!(table[0], 0.0);
        assert!(close(table[255], 1.0, 1e-6));
        for (index, value) in table.iter().enumerate() {
            assert!(close(*value, srgb_to_linear(index as f32 / 255.0), 1e-7));
        }
    }

    #[test]
    fn mat3_inverse_is_actually_inverse() {
        let identity = mat3_vec3(
            mat3_inverse(SRGB_TO_XYZ_D65).expect("可逆"),
            mat3_vec3(SRGB_TO_XYZ_D65, [0.3, 0.5, 0.2]),
        );
        assert!(close(identity[0], 0.3, 1e-5));
        assert!(close(identity[1], 0.5, 1e-5));
        assert!(close(identity[2], 0.2, 1e-5));
        assert!(mat3_inverse([[1.0, 2.0, 3.0], [2.0, 4.0, 6.0], [1.0, 1.0, 1.0]]).is_none());
    }

    #[test]
    fn kelvin_locus_is_sane() {
        // 6500K 应当落在 D65 附近（(0.3127, 0.3290)），容差给足（近似公式）
        let (x, y) = kelvin_to_xy(6500.0);
        assert!(close(x, 0.3127, 0.006), "6500K 的 x = {x}");
        assert!(close(y, 0.3290, 0.006), "6500K 的 y = {y}");
        // 2856K（Standard Light A）≈ (0.4476, 0.4074)
        let (ax, ay) = kelvin_to_xy(2856.0);
        assert!(close(ax, 0.4476, 0.01), "2856K 的 x = {ax}");
        assert!(close(ay, 0.4074, 0.01), "2856K 的 y = {ay}");
        // 单调：色温越高，x 越小（越蓝）
        let mut previous = f32::INFINITY;
        for k in (2000..=12000).step_by(500) {
            let (x, _) = kelvin_to_xy(k as f32);
            assert!(x < previous, "{k}K 的 x 应当比前一个小（x = {x}）");
            previous = x;
        }
        // 超范围也要给出有限值（夹取而不是 NaN）
        assert!(kelvin_to_xy(100.0).0.is_finite());
        assert!(kelvin_to_xy(1_000_000.0).0.is_finite());
    }

    #[test]
    fn gains_are_neutral_at_the_same_temperature() {
        // **最关键的一条**：目标 == 基准 ⇒ 正好不动（否则「载入后画面就已经变了」）
        for k in [2500.0, 5000.0, 6250.0, 10000.0] {
            let ratio = temperature_gain_ratio(k, k);
            assert!(close(ratio[0], 1.0, 1e-5), "{k}K 的红增益 {ratio:?}");
            assert!(close(ratio[1], 1.0, 1e-6));
            assert!(close(ratio[2], 1.0, 1e-5), "{k}K 的蓝增益 {ratio:?}");
        }
    }

    #[test]
    fn slider_direction_matches_lightroom() {
        // 低 K（说「当时的光很暖」）⇒ 画面被冷下来：压红、抬蓝
        let cooler = temperature_gain_ratio(3000.0, 6500.0);
        assert!(cooler[0] < 1.0, "低 K 应当压红（画面变冷）：{cooler:?}");
        assert!(cooler[2] > 1.0, "低 K 应当抬蓝（画面变冷）：{cooler:?}");
        // 高 K（说「当时的光很蓝」）⇒ 画面被暖回来：抬红、压蓝
        let warmer = temperature_gain_ratio(10000.0, 6500.0);
        assert!(warmer[0] > 1.0, "高 K 应当抬红（画面变暖）：{warmer:?}");
        assert!(warmer[2] < 1.0, "高 K 应当压蓝（画面变暖）：{warmer:?}");
        // 绿通道归一
        assert!(close(warmer[1], 1.0, 1e-6));
        assert!(close(cooler[1], 1.0, 1e-6));
    }

    #[test]
    fn cct_inverts_the_locus() {
        for k in [2000.0f32, 2856.0, 4000.0, 5500.0, 6500.0, 8000.0, 12000.0] {
            let (x, y) = kelvin_to_xy(k);
            let back = cct_from_xy(x, y).expect("能反查");
            let error = (back - k).abs() / k;
            assert!(error < 0.02, "{k}K 反查成 {back}K（相对误差 {error}）");
        }
        assert!(cct_from_xy(f32::NAN, 0.3).is_none());
    }

    #[test]
    fn camera_neutral_recovers_a_known_illuminant() {
        // 造一个「相机矩阵」：直接拿 sRGB→XYZ 的逆当 xyz_to_cam（即相机空间就是 sRGB）。
        // 那么「相机中性 = 某个色温的白」时，反查必须回到那个色温。
        let xyz_to_cam = mat3_inverse(SRGB_TO_XYZ_D65).expect("可逆");
        for k in [3000.0f32, 5000.0, 6500.0] {
            let (x, y) = kelvin_to_xy(k);
            let white_xyz = xy_to_xyz(x, y);
            let white_srgb = mat3_vec3(XYZ_TO_SRGB_D65, white_xyz);
            let back = cct_from_camera_neutral(white_srgb, xyz_to_cam).expect("能反查");
            assert!(
                (back - k).abs() / k < 0.03,
                "{k}K 的中性方向反查成 {back}K"
            );
        }
        assert!(cct_from_camera_neutral([0.0, 0.0, 0.0], xyz_to_cam).is_none());
    }

    #[test]
    fn exif_light_source_table_only_answers_what_it_knows() {
        assert_eq!(light_source_kelvin(3), Some(2856.0));
        assert_eq!(light_source_kelvin(21), Some(6504.0));
        assert_eq!(light_source_kelvin(0), None, "unknown 不许猜");
        assert_eq!(light_source_kelvin(255), None);
        assert_eq!(light_source_kelvin(12345), None);
    }
}
