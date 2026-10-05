//! 常见 RGB 矩阵/TRC 配置的快路径。色彩语义由 Little CMS 解析标签并用独立
//! 参考变换逐配置核对；CLUT、缺标签或误差超预算时退回 Little CMS，不近似猜测。

use lcms2::{Profile, Tag, TagSignature};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

use super::ProfileId;
use super::icc::{
    IccError, RgbIcc, input_rgb16_to_working_reference, linear_rec2020,
    working_to_output_rgb16_reference,
};

pub(crate) struct FastMatrixTrcInput {
    curves: [Vec<f32>; 3],
    to_working: [[f32; 3]; 3],
}

pub(crate) struct FastMatrixTrcOutput {
    inverse_curves: [Vec<f32>; 3],
    from_working: [[f32; 3]; 3],
}

type TransformCache<T> = OnceLock<Mutex<VecDeque<(ProfileId, Option<Arc<T>>)>>>;
const CACHE_PROFILES: usize = 8;
static INPUT_CACHE: TransformCache<FastMatrixTrcInput> = OnceLock::new();
static OUTPUT_CACHE: TransformCache<FastMatrixTrcOutput> = OnceLock::new();

fn cached<T>(
    cache: &'static TransformCache<T>,
    profile: &RgbIcc,
    build: impl FnOnce(&RgbIcc) -> Result<Option<T>, IccError>,
) -> Result<Option<Arc<T>>, IccError> {
    let entries = cache.get_or_init(|| Mutex::new(VecDeque::new()));
    {
        let mut entries = entries.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(index) = entries.iter().position(|(id, _)| id == profile.id()) {
            let entry = entries
                .remove(index)
                .expect("cache index came from this queue");
            let result = entry.1.clone();
            entries.push_back(entry);
            return Ok(result);
        }
    }
    let built = build(profile)?.map(Arc::new);
    let mut entries = entries.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some((_, existing)) = entries.iter().find(|(id, _)| id == profile.id()) {
        return Ok(existing.clone());
    }
    entries.push_back((profile.id().clone(), built.clone()));
    if entries.len() > CACHE_PROFILES {
        entries.pop_front();
    }
    Ok(built)
}

pub(crate) fn input_transform(
    profile: &RgbIcc,
) -> Result<Option<Arc<FastMatrixTrcInput>>, IccError> {
    cached(&INPUT_CACHE, profile, FastMatrixTrcInput::build)
}

pub(crate) fn output_transform(
    profile: &RgbIcc,
) -> Result<Option<Arc<FastMatrixTrcOutput>>, IccError> {
    cached(&OUTPUT_CACHE, profile, FastMatrixTrcOutput::build)
}

impl FastMatrixTrcInput {
    pub(crate) fn build(source: &RgbIcc) -> Result<Option<Self>, IccError> {
        let profile = source.profile()?;
        if has_pipeline_tags(&profile) {
            return Ok(None);
        }
        let target = linear_rec2020()?;
        let Some(source_matrix) = colorant_matrix(&profile) else {
            return Ok(None);
        };
        let Some(target_matrix) = colorant_matrix(&target) else {
            return Ok(None);
        };
        let Some(target_inverse) = crate::develop::color::mat3_inverse(target_matrix) else {
            return Ok(None);
        };
        let mut to_working = [[0.0_f32; 3]; 3];
        for channel in 0..3 {
            let column = [
                source_matrix[0][channel],
                source_matrix[1][channel],
                source_matrix[2][channel],
            ];
            let mapped = crate::develop::color::mat3_vec3(target_inverse, column);
            for row in 0..3 {
                to_working[row][channel] = mapped[row];
            }
        }
        if to_working.iter().flatten().any(|value| !value.is_finite()) {
            return Ok(None);
        }
        let curves = [
            curve_table(&profile, TagSignature::RedTRCTag),
            curve_table(&profile, TagSignature::GreenTRCTag),
            curve_table(&profile, TagSignature::BlueTRCTag),
        ];
        let [Some(red), Some(green), Some(blue)] = curves else {
            return Ok(None);
        };
        let candidate = Self {
            curves: [red, green, blue],
            to_working,
        };
        // Profile 的白点、chad、版本差异可能改变 LCMS 解释。只有该 ICC 在整条
        // 测试网格上贴合基准才采用快路径，不能凭“有六个标签”直接信任矩阵。
        let mut samples = Vec::with_capacity(6 * 6 * 6 + 3);
        for red in [0, 1024, 8192, 32768, 49152, 65535] {
            for green in [0, 1024, 8192, 32768, 49152, 65535] {
                for blue in [0, 1024, 8192, 32768, 49152, 65535] {
                    samples.push([red, green, blue]);
                }
            }
        }
        samples.extend([[123, 4567, 60000], [12345, 54321, 32767], [65000, 39, 999]]);
        let reference = input_rgb16_to_working_reference(source, &samples)?;
        let fast = candidate.apply(&samples)?;
        let agrees = reference.iter().zip(&fast).all(|(expected, actual)| {
            expected
                .iter()
                .zip(actual)
                .all(|(a, b)| (a - b).abs() <= 0.000_5)
        });
        Ok(agrees.then_some(candidate))
    }

    pub(crate) fn apply(&self, pixels: &[[u16; 3]]) -> Result<Vec<[f32; 3]>, IccError> {
        let mut output = Vec::with_capacity(pixels.len());
        for pixel in pixels {
            output.push(self.apply_pixel(*pixel)?);
        }
        Ok(output)
    }

    /// RGB8 位图直接从解码器缓冲读取；不先为每个通道扩成 u16 整图。
    pub(crate) fn apply_rgb8(&self, pixels: &[[u8; 3]]) -> Result<Vec<[f32; 3]>, IccError> {
        let mut output = Vec::with_capacity(pixels.len());
        for pixel in pixels {
            output.push(self.apply_pixel(pixel.map(|value| u16::from(value) * 257))?);
        }
        Ok(output)
    }

    #[inline(always)]
    fn apply_pixel(&self, pixel: [u16; 3]) -> Result<[f32; 3], IccError> {
        let linear = [
            self.curves[0][pixel[0] as usize],
            self.curves[1][pixel[1] as usize],
            self.curves[2][pixel[2] as usize],
        ];
        let mapped = crate::develop::color::mat3_vec3(self.to_working, linear);
        if mapped.iter().any(|channel| !channel.is_finite()) {
            return Err(IccError::NonFinitePixel);
        }
        Ok(mapped)
    }
}

fn has_pipeline_tags(profile: &Profile) -> bool {
    [
        TagSignature::AToB0Tag,
        TagSignature::AToB1Tag,
        TagSignature::AToB2Tag,
        TagSignature::DToB0Tag,
        TagSignature::DToB1Tag,
        TagSignature::DToB2Tag,
        TagSignature::DToB3Tag,
    ]
    .into_iter()
    .any(|tag| profile.has_tag(tag))
}

impl FastMatrixTrcOutput {
    pub(crate) fn proof_matrix(&self) -> [[f32;3];3] { self.from_working }
    pub(crate) fn display_description(&self) -> ([[f32; 3]; 3], Vec<[f32; 4]>) {
        // Store the compensation for an sRGB surface, rather than device code:
        // the attachment's hardware OETF then writes exactly the ICC RGB code.
        let samples = (0..8192).map(|i| {
            // Both the CPU inverse curve and screen table use sqrt(linear)
            // as their address: uniform linear samples lose dark gamma detail.
            let position = i as f32 / 8191.0 * 65535.0;
            let low = position as usize;
            let high = (low + 1).min(65535);
            let mut pixel = [0.0; 4];
            for (c, value) in pixel[..3].iter_mut().enumerate() {
                let table = &self.inverse_curves[c];
                let encoded = table[low] + (table[high] - table[low]) * (position - low as f32);
                *value = crate::develop::color::srgb_to_linear(encoded);
            }
            pixel[3] = 1.0;
            pixel
        }).collect();
        (self.from_working, samples)
    }

    pub(crate) fn build(target: &RgbIcc) -> Result<Option<Self>, IccError> {
        let profile = target.profile()?;
        if has_output_pipeline_tags(&profile) {
            return Ok(None);
        }
        let working = linear_rec2020()?;
        let Some(target_matrix) = colorant_matrix(&profile) else {
            return Ok(None);
        };
        let Some(working_matrix) = colorant_matrix(&working) else {
            return Ok(None);
        };
        let Some(inverse) = crate::develop::color::mat3_inverse(target_matrix) else {
            return Ok(None);
        };
        let mut from_working = [[0.0_f32; 3]; 3];
        for channel in 0..3 {
            let column = [
                working_matrix[0][channel],
                working_matrix[1][channel],
                working_matrix[2][channel],
            ];
            let mapped = crate::develop::color::mat3_vec3(inverse, column);
            for row in 0..3 {
                from_working[row][channel] = mapped[row];
            }
        }
        let inverse_curves = [
            inverse_curve_table(&profile, TagSignature::RedTRCTag),
            inverse_curve_table(&profile, TagSignature::GreenTRCTag),
            inverse_curve_table(&profile, TagSignature::BlueTRCTag),
        ];
        let [Some(red), Some(green), Some(blue)] = inverse_curves else {
            return Ok(None);
        };
        let candidate = Self {
            inverse_curves: [red, green, blue],
            from_working,
        };
        // 输出端还要核对负值、过曝和越域原色，不能只测 0..1 中性样本。
        let mut samples = Vec::with_capacity(8 * 8 * 8);
        for red in [-0.5, -0.01, 0.0, 0.01, 0.18, 0.5, 1.0, 2.0] {
            for green in [-0.5, -0.01, 0.0, 0.01, 0.18, 0.5, 1.0, 2.0] {
                for blue in [-0.5, -0.01, 0.0, 0.01, 0.18, 0.5, 1.0, 2.0] {
                    samples.push([red, green, blue]);
                }
            }
        }
        for i in 0..256 { samples.push([i as f32 / 255.0 * 0.001; 3]); }
        let reference = working_to_output_rgb16_reference(target, &samples)?;
        let fast = candidate.apply(&samples);
        let agrees = reference.iter().zip(&fast).all(|(expected, actual)| {
            expected
                .iter()
                .zip(actual)
                .all(|(a, b)| a.abs_diff(*b) <= 32)
        });
        Ok(agrees.then_some(candidate))
    }

    pub(crate) fn apply(&self, pixels: &[[f32; 3]]) -> Vec<[u16; 3]> {
        pixels
            .iter()
            .map(|pixel| self.encode_pixel(*pixel))
            .collect()
    }

    /// 大图导出直接写编码器要用的交错 u16 缓冲，避免再保留整张
    /// `Vec<[u16; 3]>` 并复制一遍（60MP 时这份临时量约 343 MiB）。
    pub(crate) fn apply_flat(&self, pixels: &[[f32; 3]]) -> Vec<u16> {
        let mut output = Vec::with_capacity(pixels.len() * 3);
        for pixel in pixels {
            output.extend_from_slice(&self.encode_pixel(*pixel));
        }
        output
    }

    #[inline(always)]
    fn encode_pixel(&self, pixel: [f32; 3]) -> [u16; 3] {
        let linear = crate::develop::color::mat3_vec3(self.from_working, pixel);
        std::array::from_fn(|channel| {
            let position = linear[channel].clamp(0.0, 1.0).sqrt() * 65535.0;
            let low = position as usize;
            let high = (low + 1).min(65535);
            let fraction = position - low as f32;
            let encoded = self.inverse_curves[channel][low] * (1.0 - fraction)
                + self.inverse_curves[channel][high] * fraction;
            (encoded.clamp(0.0, 1.0) * 65535.0).round() as u16
        })
    }
}

fn has_output_pipeline_tags(profile: &Profile) -> bool {
    if has_pipeline_tags(profile) {
        return true;
    }
    [
        TagSignature::BToA0Tag,
        TagSignature::BToA1Tag,
        TagSignature::BToA2Tag,
        TagSignature::BToD0Tag,
        TagSignature::BToD1Tag,
        TagSignature::BToD2Tag,
        TagSignature::BToD3Tag,
    ]
    .into_iter()
    .any(|tag| profile.has_tag(tag))
}

pub(super) fn colorant_matrix(profile: &Profile) -> Option<[[f32; 3]; 3]> {
    let columns = [
        TagSignature::RedColorantTag,
        TagSignature::GreenColorantTag,
        TagSignature::BlueColorantTag,
    ]
    .map(|tag| match profile.read_tag(tag) {
        Tag::CIEXYZ(xyz) => Some([xyz.X as f32, xyz.Y as f32, xyz.Z as f32]),
        _ => None,
    });
    let [Some(red), Some(green), Some(blue)] = columns else {
        return None;
    };
    let matrix = [
        [red[0], green[0], blue[0]],
        [red[1], green[1], blue[1]],
        [red[2], green[2], blue[2]],
    ];
    matrix
        .iter()
        .flatten()
        .all(|value| value.is_finite())
        .then_some(matrix)
}

fn curve_table(profile: &Profile, tag: TagSignature) -> Option<Vec<f32>> {
    let Tag::ToneCurve(curve) = profile.read_tag(tag) else {
        return None;
    };
    let table: Vec<f32> = (0..=u16::MAX)
        .map(|value| curve.eval(f32::from(value) / 65535.0))
        .collect();
    table.iter().all(|value| value.is_finite()).then_some(table)
}

fn inverse_curve_table(profile: &Profile, tag: TagSignature) -> Option<Vec<f32>> {
    let Tag::ToneCurve(curve) = profile.read_tag(tag) else {
        return None;
    };
    let inverse = curve.reversed();
    let table: Vec<f32> = (0..=u16::MAX)
        .map(|value| inverse.eval((f32::from(value) / 65535.0).powi(2)))
        .collect();
    table.iter().all(|value| value.is_finite()).then_some(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::icc::{adobe_rgb_icc, display_p3_icc, srgb_icc};

    #[test]
    fn built_in_matrix_profiles_use_checked_fast_path() {
        for profile in [
            srgb_icc().unwrap(),
            display_p3_icc().unwrap(),
            adobe_rgb_icc().unwrap(),
        ] {
            let fast = FastMatrixTrcInput::build(&profile).unwrap();
            assert!(
                fast.is_some(),
                "{} unexpectedly fell back",
                profile.id().as_str()
            );
            let output = FastMatrixTrcOutput::build(&profile).unwrap();
            assert!(
                output.is_some(),
                "{} output unexpectedly fell back",
                profile.id().as_str()
            );
        }
    }

    #[test]
    fn fast_transforms_match_lcms_on_non_grid_colors_and_gamut_edges() {
        let mut state = 0x1234_5678_u32;
        let mut encoded = Vec::with_capacity(4096);
        let mut working = Vec::with_capacity(4096);
        for _ in 0..4096 {
            let mut next = || {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                state
            };
            encoded.push([next() as u16, next() as u16, next() as u16]);
            working.push([
                (next() as f64 / u32::MAX as f64 * 3.0 - 1.0) as f32,
                (next() as f64 / u32::MAX as f64 * 3.0 - 1.0) as f32,
                (next() as f64 / u32::MAX as f64 * 3.0 - 1.0) as f32,
            ]);
        }
        for profile in [
            srgb_icc().unwrap(),
            display_p3_icc().unwrap(),
            adobe_rgb_icc().unwrap(),
        ] {
            let fast_input = FastMatrixTrcInput::build(&profile).unwrap().unwrap();
            let fast_output = FastMatrixTrcOutput::build(&profile).unwrap().unwrap();
            let reference_input = input_rgb16_to_working_reference(&profile, &encoded).unwrap();
            let actual_input = fast_input.apply(&encoded).unwrap();
            for (reference, actual) in reference_input.iter().zip(actual_input) {
                for (reference, actual) in reference.iter().zip(actual) {
                    assert!((reference - actual).abs() <= 0.0005);
                }
            }
            let reference_output = working_to_output_rgb16_reference(&profile, &working).unwrap();
            let actual_output = fast_output.apply(&working);
            for (reference, actual) in reference_output.iter().zip(actual_output) {
                for (reference, actual) in reference.iter().zip(actual) {
                    assert!(reference.abs_diff(actual) <= 32);
                }
            }
        }
    }

    #[test]
    fn profile_cache_reuses_checked_transforms_and_flat_output_matches() {
        let profile = display_p3_icc().unwrap();
        let first = output_transform(&profile).unwrap().unwrap();
        let second = output_transform(&profile).unwrap().unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        let samples = [[-0.2, 0.5, 1.3], [0.7, 0.3, 0.12]];
        let expected: Vec<u16> = first.apply(&samples).into_iter().flatten().collect();
        assert_eq!(first.apply_flat(&samples), expected);
        let input_first = input_transform(&profile).unwrap().unwrap();
        let input_second = input_transform(&profile).unwrap().unwrap();
        assert!(Arc::ptr_eq(&input_first, &input_second));
    }
}
