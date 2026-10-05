//! Validated native mBA stages. LCMS remains the full-chain authority; this
//! bounded reader only describes the RGB subset we can execute on the GPU.
//! Keeping curves and the native CLUT separate avoids baking across steep
//! transfer curves and clipping planes (e.g. ICC's official sRGB v4 profile).
use super::{
    display_clut::{XYZ_MAX, pcs_address, validation_samples},
    icc::{IccError, RgbIcc, linear_rec2020, working_to_output_rgb16_reference},
};
use std::sync::Arc;
#[derive(Debug, Clone)]
pub(crate) struct NativeDisplay {
    pub lab: bool,
    pub matrix: [[f32; 4]; 3],
    /// B, M, A curves; ICC function id then gamma/a/b/c/d/e/f.
    pub curves: [[f32; 8]; 9],
    pub dimensions: [u32; 3],
    pub samples: Arc<Vec<[f32; 4]>>,
    pub has_clut: bool,
}
impl NativeDisplay {
    pub fn build(profile: &RgbIcc) -> Result<Option<([[f32; 3]; 3], Self)>, IccError> {
        if profile.tag_bytes(b"B2D1").is_some() {
            return Ok(None);
        }
        let Some(bytes) = profile
            .tag_bytes(b"B2A1")
            .or_else(|| profile.tag_bytes(b"B2A0"))
        else {
            return Ok(None);
        };
        let Some(candidate) = Self::read(bytes, &profile.bytes()[20..24]) else {
            return Ok(None);
        };
        let matrix = super::matrix_trc::colorant_matrix(&linear_rec2020()?)
            .ok_or(IccError::InvalidProfile)?;
        let probes = validation_samples();
        let expected = working_to_output_rgb16_reference(profile, &probes)?;
        let max = probes
            .iter()
            .zip(&expected)
            .map(|(p, e)| {
                let actual = candidate.encoded(crate::develop::color::mat3_vec3(matrix, *p));
                if actual.iter().any(|v| !v.is_finite()) {
                    return f32::INFINITY;
                }
                (0..3)
                    .map(|c| (actual[c] - f32::from(e[c]) / 65535.0).abs())
                    .fold(0.0_f32, f32::max)
            })
            .fold(0.0_f32, f32::max);
        if max > 0.75 / 255.0 || !max.is_finite() {
            return Ok(None);
        }
        Ok(Some((matrix, candidate)))
    }
    fn read(bytes: &[u8], pcs: &[u8]) -> Option<Self> {
        if bytes.get(..4)? != b"mBA " || bytes.get(8..10)? != [3, 3] {
            return None;
        }
        let lab = match pcs {
            b"Lab " => true,
            b"XYZ " => false,
            _ => return None,
        };
        let offset = |at| -> Option<usize> {
            Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize)
        };
        let mut curves = [[0.0; 8]; 9];
        for curve in &mut curves {
            curve[1] = 1.0;
        }
        for (row, at) in [12, 20, 28].into_iter().enumerate() {
            let mut cursor = offset(at)?;
            if cursor == 0 {
                continue;
            }
            for c in 0..3 {
                let (curve, size) = read_curve(bytes.get(cursor..)?)?;
                curves[row * 3 + c] = curve;
                cursor = cursor.checked_add(size)?;
            }
        }
        let mut matrix = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ];
        let at = offset(16)?;
        if at > 0 {
            for r in 0..3 {
                for c in 0..3 {
                    matrix[r][c] = fixed(bytes, at + (r * 3 + c) * 4)?;
                }
                matrix[r][3] = fixed(bytes, at + 36 + r * 4)?;
            }
        }
        let at = offset(24)?;
        let has_clut = at > 0;
        let (dimensions, samples) = if has_clut {
            let grid = bytes.get(at..at + 20)?;
            let dims = [u32::from(grid[0]), u32::from(grid[1]), u32::from(grid[2])];
            if dims.iter().any(|v| *v < 2) || !matches!(grid[16], 1 | 2) {
                return None;
            }
            let count = dims
                .iter()
                .try_fold(1_usize, |a, v| a.checked_mul(*v as usize))?;
            if count.checked_mul(16)? > 32 * 1024 * 1024 {
                return None;
            }
            // Original ICC remains subject to the 16MiB input bound.
            let size = count.checked_mul(3)?.checked_mul(grid[16] as usize)?;
            let table = bytes.get(at + 20..at.checked_add(20)?.checked_add(size)?)?;
            let mut result = vec![[0.0, 0.0, 0.0, 1.0]; count];
            // ICC last axis is fastest; GPU x is fastest.
            for x in 0..dims[0] as usize {
                for y in 0..dims[1] as usize {
                    for z in 0..dims[2] as usize {
                        let src = ((x * dims[1] as usize + y) * dims[2] as usize + z) * 3;
                        let dst = (z * dims[1] as usize + y) * dims[0] as usize + x;
                        for c in 0..3 {
                            result[dst][c] = if grid[16] == 1 {
                                f32::from(table[src + c]) / 255.0
                            } else {
                                f32::from(u16::from_be_bytes(
                                    table[(src + c) * 2..(src + c + 1) * 2].try_into().ok()?,
                                )) / 65535.0
                            };
                        }
                    }
                }
            }
            (dims, result)
        } else {
            ([1; 3], vec![[0.0, 0.0, 0.0, 1.0]])
        };
        Some(Self {
            lab,
            matrix,
            curves,
            dimensions,
            samples: Arc::new(samples),
            has_clut,
        })
    }
    pub fn encoded(&self, xyz: [f32; 3]) -> [f32; 3] {
        let mut rgb = if self.lab {
            pcs_address(xyz, true)
        } else {
            xyz.map(|v| v / XYZ_MAX)
        };
        rgb = std::array::from_fn(|c| curve(self.curves[c], rgb[c]));
        rgb = std::array::from_fn(|r| {
            self.matrix[r][0] * rgb[0]
                + self.matrix[r][1] * rgb[1]
                + self.matrix[r][2] * rgb[2]
                + self.matrix[r][3]
        });
        rgb = std::array::from_fn(|c| curve(self.curves[3 + c], rgb[c]));
        if self.has_clut {
            rgb = sample(&self.samples, self.dimensions, rgb, self.lab);
        }
        rgb = std::array::from_fn(|c| curve(self.curves[6 + c], rgb[c]).clamp(0.0, 1.0));
        rgb
    }
    pub fn surface_linear(&self, xyz: [f32; 3]) -> [f32; 3] {
        self.encoded(xyz).map(crate::develop::color::srgb_to_linear)
    }
    pub fn uniform(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(352);
        for v in self
            .matrix
            .iter()
            .flatten()
            .chain(self.curves.iter().flatten())
        {
            output.extend(v.to_ne_bytes());
        }
        for v in [
            if self.has_clut { 1.0_f32 } else { 0.0 },
            if self.lab { 1.0 } else { 0.0 },
            0.0,
            0.0,
        ] {
            output.extend(v.to_ne_bytes());
        }
        output
    }
}
fn fixed(bytes: &[u8], at: usize) -> Option<f32> {
    Some(i32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as f32 / 65536.0)
}
fn read_curve(bytes: &[u8]) -> Option<([f32; 8], usize)> {
    let mut output = [0.0; 8];
    output[1] = 1.0;
    match bytes.get(..4)? {
        b"para" => {
            let kind = u16::from_be_bytes(bytes.get(8..10)?.try_into().ok()?) as usize;
            let count = *([1, 3, 4, 5, 7].get(kind)?);
            output[0] = kind as f32;
            for i in 0..count {
                output[i + 1] = fixed(bytes, 12 + i * 4)?;
            }
            if output[1] <= 0.0 || !output.iter().all(|v| v.is_finite()) {
                return None;
            }
            Some((output, 12 + count * 4))
        }
        b"curv" => {
            let count = u32::from_be_bytes(bytes.get(8..12)?.try_into().ok()?);
            if count == 0 {
                Some((output, 12))
            } else if count == 1 {
                output[1] =
                    f32::from(u16::from_be_bytes(bytes.get(12..14)?.try_into().ok()?)) / 256.0;
                Some((output, 16))
            } else {
                None
            }
        }
        _ => None,
    }
}
fn curve(p: [f32; 8], x: f32) -> f32 {
    let [kind, g, a, b, c, d, e, f] = p;
    let power = |base: f32| if base > 0.0 { base.powf(g) } else { 0.0 };
    match kind as u32 {
        0 => {
            if g == 1.0 {
                x
            } else {
                power(x)
            }
        }
        1 => {
            if a.abs() < 1e-9 || x < -b / a {
                0.0
            } else {
                power(a * x + b)
            }
        }
        2 => {
            if a.abs() < 1e-9 {
                0.0
            } else if x < (-b / a).max(0.0) {
                c
            } else if a * x + b > 0.0 {
                power(a * x + b) + c
            } else {
                0.0
            }
        }
        3 => {
            if x >= d {
                power(a * x + b)
            } else {
                c * x
            }
        }
        4 => {
            if x >= d {
                power(a * x + b) + e
            } else {
                c * x + f
            }
        }
        _ => unreachable!(),
    }
}
fn sample(table: &[[f32; 4]], dims: [u32; 3], rgb: [f32; 3], trilinear: bool) -> [f32; 3] {
    let position =
        std::array::from_fn::<_, 3, _>(|c| rgb[c].clamp(0.0, 1.0) * (dims[c] - 1) as f32);
    let low = position.map(|v| v as u32);
    let high = std::array::from_fn::<_, 3, _>(|c| (low[c] + 1).min(dims[c] - 1));
    let frac = std::array::from_fn::<_, 3, _>(|c| position[c] - low[c] as f32);
    let load = |index: [u32; 3]| {
        let value = table[((index[2] * dims[1] + index[1]) * dims[0] + index[0]) as usize];
        [value[0], value[1], value[2]]
    };
    if trilinear {
        let mut output = [0.0; 3];
        for z in 0..2 {
            for y in 0..2 {
                for x in 0..2 {
                    let bits = [x, y, z];
                    let index =
                        std::array::from_fn(|c| if bits[c] == 0 { low[c] } else { high[c] });
                    let weight = (0..3)
                        .map(|c| if bits[c] == 0 { 1.0 - frac[c] } else { frac[c] })
                        .product::<f32>();
                    let value = load(index);
                    for c in 0..3 {
                        output[c] += weight * value[c];
                    }
                }
            }
        }
        output
    } else {
        let mut axes = [0, 1, 2];
        axes.sort_by(|a, b| frac[*b].total_cmp(&frac[*a]));
        let mut one = low;
        one[axes[0]] = high[axes[0]];
        let mut two = one;
        two[axes[1]] = high[axes[1]];
        let values = [load(low), load(one), load(two), load(high)];
        let weights = [
            1.0 - frac[axes[0]],
            frac[axes[0]] - frac[axes[1]],
            frac[axes[1]] - frac[axes[2]],
            frac[axes[2]],
        ];
        std::array::from_fn(|c| (0..4).map(|i| values[i][c] * weights[i]).sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_parametric_curves_match_lcms_including_branch_edges_and_negative_values() {
        for p in [
            [0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            [0.0, 2.2, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            [1.0, 2.2, 0.8, -0.1, 0.0, 0.0, 0.0, 0.0],
            [2.0, 1.9, 1.2, -0.2, 0.03, 0.0, 0.0, 0.0],
            [
                3.0,
                2.4,
                1.0 / 1.055,
                0.055 / 1.055,
                1.0 / 12.92,
                0.04045,
                0.0,
                0.0,
            ],
            [4.0, 0.4167, 12.0, -4.8, 137.0, 0.4036, -0.055, -55.33],
        ] {
            let count = [1, 3, 4, 5, 7][p[0] as usize];
            let parameters: Vec<f64> = p[1..1 + count].iter().map(|v| f64::from(*v)).collect();
            let reference = lcms2::ToneCurve::new_parametric(p[0] as i16 + 1, &parameters).unwrap();
            for value in [
                -1.0, 0.0, 0.001, 0.04044, 0.04045, 0.04046, 0.1, 0.3, 0.40359, 0.4036, 0.40361,
                0.5, 1.0, 2.0,
            ] {
                let actual = curve(p, value);
                let expected = reference.eval(value);
                assert!(
                    (actual - expected).abs() < 0.00003,
                    "{p:?} {value}: {actual} != {expected}"
                );
            }
        }
    }
    #[test]
    fn native_mba_stages_are_not_replaced_by_matrix_and_tables_keep_fp32() {
        for lab in [false, true] {
            let profile = crate::color::icc_fixtures::profile(
                4,
                crate::color::icc_fixtures::LutKind::Mab,
                lab,
            );
            let (_, native) = NativeDisplay::build(&profile).unwrap().unwrap();
            assert_eq!(native.dimensions, [5; 3]);
            assert_eq!(native.uniform().len(), 352);
            assert!(native.has_clut);
            let cached = crate::color::display::ScreenTransform::from_icc(&profile).unwrap();
            let again = crate::color::display::ScreenTransform::from_icc(&profile).unwrap();
            assert!(Arc::ptr_eq(
                &cached.native.unwrap().samples,
                &again.native.unwrap().samples
            ));
        }
    }
    #[test]
    fn malformed_native_stages_never_escape_the_checked_reader() {
        for bytes in [vec![], b"mBA ".to_vec(), vec![0; 32]] {
            assert!(NativeDisplay::read(&bytes, b"Lab ").is_none());
        }
        assert!(read_curve(b"para\0\0\0\0\0\x09\0\0").is_none());
    }
}
