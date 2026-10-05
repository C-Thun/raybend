//! V2 adjustment bridge: Rec.2020 storage ↔ unbounded linear-sRGB reference math.
//!
//! Existing spatial operators keep their reference domain, with float samples rather
//! than bounded u16. No ICC transform is run here. The input bridge is prepared once;
//! curves/chroma/creative LUTs use a signed sRGB transfer, preserving out-of-domain
//! residuals. This is deliberately not described as native Rec.2020 adjustment math.

use super::pipeline::{DevelopStages, apply_chroma_unbounded, chain_linear};
use super::sample::RgbView;
use super::{color, curve::CurveSet, params::DevelopParams, pipeline::Resolved};
use crate::color::working::{
    LINEAR_REC2020_TO_SRGB, LINEAR_SRGB_TO_REC2020, WorkingImage, WorkingImageError,
};
use std::sync::OnceLock;

/// Prepared once per decoded source; owns the same allocation as the working image.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkingReferenceImage {
    pub width: u32,
    pub height: u32,
    pub(crate) rgb: Vec<f32>,
}

impl WorkingReferenceImage {
    pub fn from_working(mut image: WorkingImage) -> Result<Self, WorkingImageError> {
        let (width, height) = image.dimensions();
        for pixel in image.pixels_mut() {
            *pixel = color::mat3_vec3(LINEAR_REC2020_TO_SRGB, *pixel);
            if pixel.iter().any(|v| !v.is_finite()) {
                return Err(WorkingImageError::NonFinite);
            }
        }
        Ok(Self {
            width,
            height,
            rgb: image.into_flat(),
        })
    }

    pub fn view(&self) -> RgbView<'_, f32> {
        RgbView {
            width: self.width,
            height: self.height,
            rgb: &self.rgb,
        }
    }

    /// Resize uses the same float area operator; its arithmetic is independent of primaries.
    pub fn downscaled_to(&self, edge: u32) -> Self {
        let (width, height, rgb) = crate::color::working::downscale_float_rgb(self.view(), edge);
        Self { width, height, rgb }
    }
}

/// Tables sample the existing tone/curve math, not a second approximation formula.
/// Endpoint singularities in the contrast curve use the exact scalar definition.
pub struct WorkingTonePlan {
    resolved: Resolved,
    curves: CurveSet,
    tone: Option<Vec<f32>>,
    curve_tables: Option<[Vec<f32>; 3]>,
    transfer: &'static TransferTables,
}

/// Small per-parameter data for GPU global adjustment. No source pixels are baked
/// into this table; the CPU definition remains the same WorkingTonePlan.
#[derive(Debug, Clone)]
pub struct GpuToneDescription {
    pub(crate) gains: [f32; 3],
    pub(crate) chroma: (f32, f32),
    pub(crate) tables: Vec<[f32; 4]>,
    pub(crate) low_offsets: [f32; 3],
    pub(crate) high_offsets: [f32; 3],
    pub(crate) linear_output: bool,
}

pub const GPU_TONE_SAMPLES: usize = 8192;
pub const GPU_TONE_ROWS: usize = 3;

impl GpuToneDescription {
    pub fn shaped_linear(position: f32) -> f32 {
        if position <= 0.5 {
            2.0 * position * position
        } else {
            1.0 - 2.0 * (1.0 - position).powi(2)
        }
    }
}

const TONE_STEPS: usize = 8192;

impl WorkingTonePlan {
    pub fn new(params: &DevelopParams, curves: &CurveSet) -> Self {
        let resolved = Resolved::new(params);
        let tone = ["contrast", "highlights", "blacks"]
            .iter()
            .any(|id| params.value(id).abs() > 1e-6)
            .then(|| {
                (0..=TONE_STEPS)
                    .map(|i| chain_linear(i as f32 / TONE_STEPS as f32, 1.0, &resolved))
                    .collect()
            });
        let curve_tables = (!curves.is_identity()).then(|| {
            std::array::from_fn(|c| {
                (0..=TONE_STEPS)
                    .map(|i| curve_at(curves, c, i as f32 / TONE_STEPS as f32))
                    .collect()
            })
        });
        Self {
            resolved,
            curves: curves.clone(),
            tone,
            curve_tables,
            transfer: transfer_tables(),
        }
    }

    fn tone_at(&self, value: f32) -> f32 {
        let Some(table) = &self.tone else {
            return value;
        };
        if (0.01..=0.99).contains(&value) {
            interpolate(table, value)
        } else {
            chain_linear(value, 1.0, &self.resolved)
        }
    }

    fn display_reference(&self, mut rgb: [f32; 3]) -> [f32; 3] {
        let chroma = self.resolved.needs_chroma();
        if self.curve_tables.is_none() && !chroma {
            return rgb;
        }
        for (c, v) in rgb.iter_mut().enumerate() {
            *v = self.transfer.encode(*v);
            if let Some(tables) = &self.curve_tables {
                *v = if (0.0..=1.0).contains(v) {
                    interpolate(&tables[c], *v)
                } else {
                    curve_at(&self.curves, c, *v)
                };
            }
        }
        if chroma {
            // The parameter interpretation is shared with the legacy pipeline.
            let key = self.chroma();
            rgb = apply_chroma_unbounded(rgb, key.0, key.1);
        }
        rgb.map(|v| self.transfer.decode(v))
    }

    fn chroma(&self) -> (f32, f32) {
        self.resolved.chroma()
    }

    pub fn map_pixel(&self, pixel: [f32; 3]) -> [f32; 3] {
        let gains = self.resolved.gains();
        self.display_reference(std::array::from_fn(|c| self.tone_at(pixel[c] * gains[c])))
    }

    /// Analysis uses precisely the tone half of this plan, before display curves.
    pub fn chain_reference(&self, source: &WorkingReferenceImage) -> Vec<f32> {
        let gains = self.resolved.gains();
        source
            .rgb
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|pixel| std::array::from_fn::<_, 3, _>(|c| self.tone_at(pixel[c] * gains[c])))
            .collect()
    }

    pub fn gpu_description(&self) -> GpuToneDescription {
        let chroma = self.chroma();
        let linear_output = chroma.0.abs() < 0.000001 && chroma.1.abs() < 0.000001;
        let mut tables = Vec::with_capacity(GPU_TONE_SAMPLES * GPU_TONE_ROWS);
        for row in 0..2 {
            for i in 0..GPU_TONE_SAMPLES {
                let position = i as f32 / (GPU_TONE_SAMPLES - 1) as f32;
                let value = if row == 0 {
                    self.transfer
                        .encode(self.tone_at(GpuToneDescription::shaped_linear(position)))
                } else {
                    position * 8.0 - 4.0
                };
                let mut rgb = std::array::from_fn::<_, 3, _>(|c| curve_at(&self.curves, c, value));
                if linear_output {
                    rgb = rgb.map(|v| self.transfer.decode(v));
                }
                tables.push([rgb[0], rgb[1], rgb[2], 1.0]);
            }
        }
        for i in 0..GPU_TONE_SAMPLES {
            let value = self
                .transfer
                .decode(i as f32 / (GPU_TONE_SAMPLES - 1) as f32 * 8.0 - 4.0);
            tables.push([value, value, value, 1.0]);
        }
        GpuToneDescription {
            gains: self.resolved.gains(),
            chroma,
            tables,
            linear_output,
            low_offsets: std::array::from_fn(|c| curve_at(&self.curves, c, -4.0) + 4.0),
            high_offsets: std::array::from_fn(|c| curve_at(&self.curves, c, 4.0) - 4.0),
        }
    }
}

fn curve_at(curves: &CurveSet, channel: usize, value: f32) -> f32 {
    let value = curves.rgb.eval_extended(curves.base.eval_extended(value));
    match channel {
        0 => &curves.r,
        1 => &curves.g,
        _ => &curves.b,
    }
    .eval_extended(value)
}

#[inline]
fn interpolate(table: &[f32], value: f32) -> f32 {
    let p = value * (table.len() - 1) as f32;
    let lo = (p as usize).min(table.len() - 1);
    let hi = (lo + 1).min(table.len() - 1);
    table[lo] + (table[hi] - table[lo]) * (p - lo as f32)
}

struct TransferTables {
    encode: Vec<f32>,
    decode: Vec<f32>,
    encode_extended: Vec<f32>,
    decode_extended: Vec<f32>,
}

fn transfer_tables() -> &'static TransferTables {
    static TABLES: OnceLock<TransferTables> = OnceLock::new();
    TABLES.get_or_init(|| TransferTables {
        encode: (0..=65536)
            .map(|i| color::linear_to_srgb(i as f32 / 65536.0))
            .collect(),
        decode: (0..=65536)
            .map(|i| color::srgb_to_linear(i as f32 / 65536.0))
            .collect(),
        encode_extended: (0..=16384)
            .map(|i| color::linear_to_srgb(1.0 + 15.0 * i as f32 / 16384.0))
            .collect(),
        decode_extended: (0..=16384)
            .map(|i| color::srgb_to_linear(1.0 + 3.0 * i as f32 / 16384.0))
            .collect(),
    })
}

impl TransferTables {
    #[inline]
    fn encode(&self, value: f32) -> f32 {
        let v = value.abs();
        value.signum()
            * if v <= 1.0 {
                interpolate(&self.encode, v)
            } else if v <= 16.0 {
                interpolate(&self.encode_extended, (v - 1.0) / 15.0)
            } else {
                color::linear_to_srgb(v)
            }
    }
    #[inline]
    fn decode(&self, value: f32) -> f32 {
        let v = value.abs();
        value.signum()
            * if v <= 1.0 {
                interpolate(&self.decode, v)
            } else if v <= 4.0 {
                interpolate(&self.decode_extended, (v - 1.0) / 3.0)
            } else {
                color::srgb_to_linear(v)
            }
    }
}

#[cfg(test)]
fn signed_encode(value: f32) -> f32 {
    transfer_tables().encode(value)
}
#[cfg(test)]
fn signed_decode(value: f32) -> f32 {
    transfer_tables().decode(value)
}

/// Source/tone plan may be cached across slider jobs; every spatial stage has one implementation.
pub fn render_working_reference(
    source: &WorkingReferenceImage,
    tone: &WorkingTonePlan,
    stages: &DevelopStages<'_>,
) -> Result<WorkingImage, WorkingImageError> {
    let warped = stages
        .lens
        .filter(|map| !map.is_identity())
        .map(|map| super::lens::warp_rgb(source.view(), map));
    let view = warped.as_deref().map_or_else(
        || source.view(),
        |rgb| RgbView {
            rgb,
            ..source.view()
        },
    );
    let denoised = stages
        .denoise
        .filter(|plan| !plan.is_identity())
        .map(|plan| match stages.nr_method {
            super::denoise::NrMethod::Fast => super::denoise::denoise_rgb(view, plan),
            super::denoise::NrMethod::High => super::bm3d::denoise_high_rgb(
                view,
                plan,
                &std::sync::atomic::AtomicBool::new(false),
            )
            .expect("not cancelled"),
        });
    let input = denoised.as_deref().unwrap_or(view.rgb);
    let mut output = vec![0.0; input.len()];
    let gains = tone.resolved.gains();
    let finish_in_pass = stages.local_tone.is_none()
        && stages.sharpen.is_none_or(|plan| plan.is_identity())
        && stages.lut.is_none();
    let map = |src: &[f32], dst: &mut [f32]| {
        for (pixel, output) in src
            .as_chunks::<3>()
            .0
            .iter()
            .zip(dst.as_chunks_mut::<3>().0)
        {
            let rgb = std::array::from_fn(|c| tone.tone_at(pixel[c] * gains[c]));
            *output = if finish_in_pass {
                color::mat3_vec3(LINEAR_SRGB_TO_REC2020, tone.display_reference(rgb))
            } else {
                rgb
            };
        }
    };
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(8);
    if input.len() < 256 * 1024 || threads == 1 {
        map(input, &mut output);
    } else {
        let chunk = (input.len() / 3).div_ceil(threads) * 3;
        std::thread::scope(|scope| {
            for (src, dst) in input.chunks(chunk).zip(output.chunks_mut(chunk)) {
                let map = &map;
                scope.spawn(move || map(src, dst));
            }
        });
    }
    if finish_in_pass {
        return WorkingImage::from_flat(source.width, source.height, output);
    }
    if let Some((state, strength)) = stages.local_tone {
        state.apply_rgb(&mut output, source.width, source.height, strength);
    }
    for pixel in output.as_chunks_mut::<3>().0 {
        *pixel = tone.display_reference(*pixel);
    }
    if stages.sharpen.is_some_and(|plan| !plan.is_identity()) || stages.lut.is_some() {
        for value in &mut output {
            *value = tone.transfer.encode(*value);
        }
        if let Some(plan) = stages.sharpen.filter(|plan| !plan.is_identity()) {
            super::sharpen::sharpen_display(&mut output, source.width, source.height, plan);
        }
        if let Some(lut) = stages.lut {
            for pixel in output.as_chunks_mut::<3>().0 {
                *pixel = lut.eval_extended(*pixel);
            }
        }
        for value in &mut output {
            *value = tone.transfer.decode(*value);
        }
    }
    for pixel in output.as_chunks_mut::<3>().0 {
        *pixel = color::mat3_vec3(LINEAR_SRGB_TO_REC2020, *pixel);
    }
    WorkingImage::from_flat(source.width, source.height, output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::develop::pipeline::DevelopStages;

    fn stages<'a>() -> DevelopStages<'a> {
        DevelopStages {
            lens: None,
            nr_method: Default::default(),
            denoise: None,
            local_tone: None,
            sharpen: None,
            lut: None,
        }
    }

    #[test]
    fn neutral_and_exposure_keep_unbounded_gamut_and_float_precision() {
        let pixels = vec![[-0.2, 0.18, 1.7], [0.123456, 0.123457, 0.123458]];
        let image = WorkingImage::new(2, 1, pixels.clone()).unwrap();
        let input = WorkingReferenceImage::from_working(image).unwrap();
        let params = DevelopParams::default();
        let tone = WorkingTonePlan::new(&params, &CurveSet::identity());
        let output = render_working_reference(&input, &tone, &stages()).unwrap();
        for (actual, expected) in output
            .pixels()
            .iter()
            .flatten()
            .zip(pixels.iter().flatten())
        {
            assert!((actual - expected).abs() < 3e-6, "{actual} != {expected}");
        }
        let params = DevelopParams::from_values([("exposure".into(), 1.0)], None).unwrap();
        let tone = WorkingTonePlan::new(&params, &CurveSet::identity());
        let output = render_working_reference(&input, &tone, &stages()).unwrap();
        assert!((output.pixels()[0][2] - 3.4).abs() < 5e-6);
        assert!(output.pixels()[0][0] < 0.0);
    }

    #[test]
    fn sampled_tone_matches_scalar_reference_including_extreme_endpoints() {
        for amount in [-100.0, -10.0, 10.0, 100.0] {
            let params = DevelopParams::from_values(
                [
                    ("contrast".into(), amount),
                    ("highlights".into(), amount),
                    ("blacks".into(), -amount),
                ],
                None,
            )
            .unwrap();
            let tone = WorkingTonePlan::new(&params, &CurveSet::identity());
            for i in 0..10001 {
                let v = i as f32 / 10000.0;
                assert!(
                    (tone.tone_at(v) - chain_linear(v, 1.0, &tone.resolved)).abs() < 8e-6,
                    "amount={amount}, v={v}, fast={}, exact={}",
                    tone.tone_at(v),
                    chain_linear(v, 1.0, &tone.resolved)
                );
            }
            assert_eq!(tone.tone_at(-0.4), -0.4);
            assert_eq!(tone.tone_at(4.0), 4.0);
        }
    }

    #[test]
    fn signed_transfer_and_identity_creative_lut_keep_negative_and_over_one() {
        let lut = super::super::lut::Lut::parse_cube("LUT_1D_SIZE 2\n0 0 0\n1 1 1\n").unwrap();
        for v in [-2.0, -0.1, 0.0, 0.001, 0.5, 1.0, 2.0] {
            assert!(
                (signed_decode(signed_encode(v)) - v).abs() < 2e-6,
                "transfer round trip {v}"
            );
            assert!((lut.eval_extended([v; 3])[0] - v).abs() < 1e-6);
        }
        let input = WorkingReferenceImage::from_working(
            WorkingImage::new(1, 1, vec![[-0.1, 0.5, 1.8]]).unwrap(),
        )
        .unwrap();
        let tone = WorkingTonePlan::new(&DevelopParams::default(), &CurveSet::identity());
        let mut stages = stages();
        stages.lut = Some(&lut);
        let output = render_working_reference(&input, &tone, &stages).unwrap();
        assert!(output.pixels()[0][0] < 0.0 && output.pixels()[0][2] > 1.0);
    }
    #[test]
    fn every_spatial_operator_retains_float_range_and_reuses_legacy_math() {
        use crate::develop::{
            DevelopPlans,
            local_tone::{LocalToneOpts, LocalToneState},
        };
        let image = WorkingImage::from_linear_srgb(16, 16, vec![[-0.2, 1.5, 0.4]; 256]).unwrap();
        let input = WorkingReferenceImage::from_working(image).unwrap();
        let params = DevelopParams::from_values(
            [
                ("lumaNr".into(), 15.0),
                ("colorNr".into(), 20.0),
                ("sharpenAmount".into(), 50.0),
            ],
            None,
        )
        .unwrap();
        let plans = DevelopPlans::from_params(16, 16, &params);
        let tone = WorkingTonePlan::new(&params, &CurveSet::identity());
        let local = LocalToneState::analyze_rgb(input.view(), &LocalToneOpts::default());
        let lens = crate::develop::lens::LensMap::new(&plans.lens);
        for method in [
            crate::develop::NrMethod::Fast,
            crate::develop::NrMethod::High,
        ] {
            let stages = DevelopStages {
                lens: Some(&lens),
                nr_method: method,
                denoise: Some(&plans.denoise),
                local_tone: Some((&local, 0.3)),
                sharpen: Some(&plans.sharpen),
                lut: None,
            };
            let output = render_working_reference(&input, &tone, &stages).unwrap();
            let reference = WorkingReferenceImage::from_working(output).unwrap();
            for pixel in reference.view().rgb.as_chunks::<3>().0 {
                assert!(
                    pixel[0] < -0.19 && pixel[1] > 1.49,
                    "{method:?} clipped {pixel:?}"
                );
                assert!(pixel.iter().all(|v| v.is_finite()));
            }
        }
    }

    #[test]
    fn working_bridge_rejects_overflow_and_geometry_retains_signed_range() {
        let image = WorkingImage::new(1, 1, vec![[f32::MAX, -f32::MAX, 0.0]]).unwrap();
        assert!(matches!(
            WorkingReferenceImage::from_working(image),
            Err(WorkingImageError::NonFinite)
        ));
        let geometry = crate::develop::geometry::EditGeometry {
            crop: Some(crate::develop::geometry::CropRect {
                x: 0.25,
                y: 0.0,
                width: 0.5,
                height: 1.0,
            }),
            ..Default::default()
        };
        let rgb = [-0.2, 1.5, 0.4].repeat(4);
        let (w, h, output) = crate::develop::geometry::apply_rgb((4, 1), &rgb, geometry).unwrap();
        assert_eq!((w, h), (2, 1));
        assert_eq!(output, [-0.2, 1.5, 0.4].repeat(2));
        for orientation in 1..=8 {
            let float =
                crate::develop::sample::orient_rgb(RgbView::new(2, 2, &rgb).unwrap(), orientation);
            let integers = [100_u16, 200, 300].repeat(4);
            let old = crate::develop::pipeline::LinearImage::new(2, 2, integers)
                .unwrap()
                .oriented(orientation);
            assert_eq!((float.0, float.1), (old.width, old.height));
            assert!(float.2.iter().any(|v| *v < 0.0) && float.2.iter().any(|v| *v > 1.0));
        }
    }

    #[test]
    #[ignore = "manual performance budget, no hardware-dependent CI threshold"]
    fn bench_working_two_megapixel_slider_cost() {
        use std::time::Instant;
        let linear = super::super::pipeline::LinearImage::new(
            2000,
            1000,
            (0..2_000_000_u32)
                .flat_map(|i| {
                    [
                        (i * 31 % 65536) as u16,
                        (i * 113 % 65536) as u16,
                        (i * 211 % 65536) as u16,
                    ]
                })
                .collect(),
        )
        .unwrap();
        let working = WorkingImage::from_linear_srgb(
            2000,
            1000,
            linear
                .rgb
                .as_chunks::<3>()
                .0
                .iter()
                .map(|p| p.map(|v| f32::from(v) / 65535.0))
                .collect(),
        )
        .unwrap();
        let source = WorkingReferenceImage::from_working(working).unwrap();
        let params = DevelopParams::from_values(
            [
                ("exposure".into(), 0.4),
                ("contrast".into(), 25.0),
                ("saturation".into(), 20.0),
            ],
            None,
        )
        .unwrap();
        let curves = CurveSet::identity();
        let now = Instant::now();
        let tone = WorkingTonePlan::new(&params, &curves);
        let setup = now.elapsed();
        let _ = render_working_reference(&source, &tone, &stages()).unwrap();
        let now = Instant::now();
        let new = render_working_reference(&source, &tone, &stages()).unwrap();
        let managed = now.elapsed();
        let now = Instant::now();
        let old = super::super::pipeline::render_develop(&linear, &params, &curves, &stages());
        let legacy = now.elapsed();
        assert_eq!(new.pixels().len() * 3, old.len());
        eprintln!(
            "2MP setup={setup:?}, float adjustment={managed:?}, legacy={legacy:?}; output ICC excluded from slider path"
        );
    }
}
