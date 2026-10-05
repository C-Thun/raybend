//! LCMS-authoritative SDR display bake. The LUT lives in native PCS, not a
//! clipped working RGB cube: negative/over-one working RGB reaches PCS intact.
use super::icc::{IccError, RgbIcc, linear_rec2020, working_to_output_rgb16_reference};
use crate::develop::color::{mat3_vec3, srgb_to_linear};
use lcms2::{
    CIExyY, ColorSpaceSignature, Flags, Intent, PixelFormat, Profile, ThreadContext, Transform,
};

pub(super) const XYZ_MAX: f32 = 65535.0 / 32768.0;
const CODE_BUDGET: f32 = 0.75 / 255.0;

#[derive(Debug, Clone)]
pub(crate) struct DisplayClut {
    pub edge: u32,
    pub lab: bool,
    pub lab_scale: f32,
    /// RGB device code (half-rounded); sRGB surface compensation is AFTER interpolation.
    pub samples: std::sync::Arc<Vec<[f32; 4]>>,
}
impl DisplayClut {
    pub fn build(profile: &RgbIcc) -> Result<([[f32; 3]; 3], Self), IccError> {
        let target = profile.profile()?;
        let lab = match target.pcs() {
            ColorSpaceSignature::XYZData => false,
            ColorSpaceSignature::LabData => true,
            _ => return Err(IccError::UnsupportedGpuTransform),
        };
        // LCMS applies 65280/65535 before legacy mft2 Lab CLUTs, even
        // when such a tag is stored in a v4 header (cmsio1.c _cmsReadOutputLUT).
        let lab_scale = if lab && output_is_lut16(profile) {
            65535.0 / 65280.0
        } else {
            1.0
        };
        // Matrix is read from the SAME generated working profile as LCMS reference.
        let matrix = super::matrix_trc::colorant_matrix(&linear_rec2020()?)
            .ok_or(IccError::InvalidProfile)?;
        let context = ThreadContext::new();
        let pcs = if lab {
            Profile::new_lab4_context(
                &context,
                &CIExyY {
                    x: 0.3457,
                    y: 0.3585,
                    Y: 1.0,
                },
            )
            .map_err(|_| IccError::InvalidProfile)?
        } else {
            Profile::new_xyz_context(&context)
        };
        let target = Profile::new_icc_context(&context, profile.bytes())
            .map_err(|_| IccError::InvalidProfile)?;
        let transform = Transform::new_flags_context(
            &context,
            &pcs,
            if lab {
                PixelFormat::Lab_FLT
            } else {
                PixelFormat::XYZ_FLT
            },
            &target,
            PixelFormat::RGB_16,
            Intent::RelativeColorimetric,
            Flags::NO_OPTIMIZE,
        )
        .map_err(|_| IccError::InvalidProfile)?;
        let probes = validation_samples();
        let reference = working_to_output_rgb16_reference(profile, &probes)?;
        // Bounded 2 MiB / 17 MiB GPU tables. Reject sharply discontinuous profiles
        // when the maximum table still misses the independent reference budget.
        for edge in [33_u32, 65, 129] {
            let count = edge.pow(3) as usize;
            let mut samples = Vec::with_capacity(count);
            // One z-slice of temporary input/output; never hold a second whole cube.
            for z in 0..edge {
                let mut input = Vec::with_capacity((edge * edge) as usize);
                for y in 0..edge {
                    for x in 0..edge {
                        let p = [x, y, z].map(|v| v as f32 / (edge - 1) as f32);
                        input.push(if lab {
                            [
                                p[0] * 100.0 * lab_scale,
                                p[1] * 255.0 * lab_scale - 128.0,
                                p[2] * 255.0 * lab_scale - 128.0,
                            ]
                        } else {
                            p.map(|v| v * v * XYZ_MAX)
                        });
                    }
                }
                let mut output = vec![[0_u16; 3]; input.len()];
                transform.transform_pixels(&input, &mut output);
                for value in output {
                    let rgb = value.map(|v| half::f16::from_f32(f32::from(v) / 65535.0).to_f32());
                    samples.push([rgb[0], rgb[1], rgb[2], 1.0]);
                }
            }
            let lut = Self {
                edge,
                lab,
                lab_scale,
                samples: std::sync::Arc::new(samples),
            };
            let max_error = probes
                .iter()
                .zip(&reference)
                .flat_map(|(sample, expected)| {
                    let actual = lut.encoded(mat3_vec3(matrix, *sample));
                    (0..3).map(move |c| {
                        if actual[c].is_finite() {
                            (actual[c] - f32::from(expected[c]) / 65535.0).abs()
                        } else {
                            f32::INFINITY
                        }
                    })
                })
                .fold(0.0_f32, f32::max);
            if max_error <= CODE_BUDGET {
                // Check every cell center in native PCS in addition to unrelated
                // working-domain probes. Strong local curvature cannot hide in a
                // sparsely sampled working-space random set.
                let mut good = true;
                for z in 0..edge - 1 {
                    let mut input = Vec::with_capacity(((edge - 1) * (edge - 1)) as usize);
                    let mut xyz = Vec::with_capacity(input.capacity());
                    for y in 0..edge - 1 {
                        for x in 0..edge - 1 {
                            let p = [x, y, z].map(|v| (v as f32 + 0.5) / (edge - 1) as f32);
                            let value = if lab {
                                [
                                    p[0] * 100.0 * lab_scale,
                                    p[1] * 255.0 * lab_scale - 128.0,
                                    p[2] * 255.0 * lab_scale - 128.0,
                                ]
                            } else {
                                p.map(|v| v * v * XYZ_MAX)
                            };
                            input.push(value);
                            xyz.push(if lab { lab_to_xyz(value) } else { value });
                        }
                    }
                    let mut expected = vec![[0_u16; 3]; input.len()];
                    transform.transform_pixels(&input, &mut expected);
                    if xyz.iter().zip(expected).any(|(p, e)| {
                        let actual = lut.encoded(*p);
                        (0..3).any(|c| {
                            !actual[c].is_finite()
                                || (actual[c] - f32::from(e[c]) / 65535.0).abs() > CODE_BUDGET
                        })
                    }) {
                        good = false;
                        break;
                    }
                }
                if good {
                    return Ok((matrix, lut));
                }
            }
        }
        Err(IccError::GpuClutAccuracy)
    }
    pub fn encoded(&self, xyz: [f32; 3]) -> [f32; 3] {
        let p = pcs_address(xyz, self.lab)
            .map(|v| (v / self.lab_scale).clamp(0.0, 1.0) * (self.edge - 1) as f32);
        let lo = p.map(|v| v as usize);
        let hi = lo.map(|v| (v + 1).min(self.edge as usize - 1));
        let t = std::array::from_fn::<_, 3, _>(|c| p[c] - lo[c] as f32);
        let n = self.edge as usize;
        let mut result = [0.0; 3];
        for z in 0..2 {
            for y in 0..2 {
                for x in 0..2 {
                    let xyz = [x, y, z];
                    let index: [usize; 3] =
                        std::array::from_fn(|c| if xyz[c] == 0 { lo[c] } else { hi[c] });
                    let weight = (0..3)
                        .map(|c| if xyz[c] == 0 { 1.0 - t[c] } else { t[c] })
                        .product::<f32>();
                    let value = self.samples[(index[2] * n + index[1]) * n + index[0]];
                    for c in 0..3 {
                        result[c] += weight * value[c];
                    }
                }
            }
        }
        result
    }
    pub fn surface_linear(&self, xyz: [f32; 3]) -> [f32; 3] {
        self.encoded(xyz).map(srgb_to_linear)
    }
}
pub(super) fn pcs_address(xyz: [f32; 3], lab: bool) -> [f32; 3] {
    if !lab {
        return xyz.map(|v| (v / XYZ_MAX).clamp(0.0, 1.0).sqrt());
    }
    // ICC D50, CIE epsilon/kappa. Do not clip XYZ before this nonlinear mapping.
    let f = |v: f32| {
        if v > 216.0 / 24389.0 {
            v.cbrt()
        } else {
            (24389.0 / 27.0 * v + 16.0) / 116.0
        }
    };
    let xyz = [f(xyz[0] / 0.9642), f(xyz[1]), f(xyz[2] / 0.8249)];
    [
        (116.0 * xyz[1] - 16.0) / 100.0,
        (500.0 * (xyz[0] - xyz[1]) + 128.0) / 255.0,
        (200.0 * (xyz[1] - xyz[2]) + 128.0) / 255.0,
    ]
}
fn lab_to_xyz(lab: [f32; 3]) -> [f32; 3] {
    let fy = (lab[0] + 16.0) / 116.0;
    let f = [fy + lab[1] / 500.0, fy, fy - lab[2] / 200.0];
    let inv = |v: f32| {
        if v > 6.0 / 29.0 {
            v * v * v
        } else {
            (116.0 * v - 16.0) / (24389.0 / 27.0)
        }
    };
    [inv(f[0]) * 0.9642, inv(f[1]), inv(f[2]) * 0.8249]
}
fn output_is_lut16(profile: &RgbIcc) -> bool {
    profile.tag_bytes(b"B2D1").is_none()
        && profile
            .tag_bytes(b"B2A1")
            .or_else(|| profile.tag_bytes(b"B2A0"))
            .and_then(|v| v.get(..4))
            == Some(b"mft2")
}
pub(super) fn validation_samples() -> Vec<[f32; 3]> {
    let mut result = Vec::new();
    for r in [-4.0, -0.2, 0.0, 0.001, 0.01, 0.18, 0.5, 1.0, 2.0, 4.0] {
        for g in [-0.2, 0.0, 0.002, 0.1, 0.7, 1.0, 2.0] {
            for b in [-0.2, 0.0, 0.002, 0.1, 0.7, 1.0, 2.0] {
                result.push([r, g, b]);
            }
        }
    }
    for i in 0..1024 {
        result.push([i as f32 / 1023.0 * 0.02; 3]);
    }
    let mut seed = 0x6d2b79f5_u32;
    for i in 0..8192 {
        let mut next = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            seed as f32 / u32::MAX as f32
        };
        let scale = if i % 2 == 0 { 1.0 } else { 8.0 };
        let offset = if scale == 1.0 { 0.0 } else { -3.0 };
        result.push(std::array::from_fn(|_| next() * scale + offset));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{
        display::ScreenTransform,
        icc::{input_rgb8_to_working, input_rgb16_to_working, input_rgb16_to_working_reference},
        icc_fixtures::{LutKind, profile},
    };

    #[test]
    fn a_sharp_display_profile_is_rejected_without_relaxing_the_budget() {
        assert!(matches!(
            ScreenTransform::from_icc(&crate::color::icc_fixtures::sharp_profile()),
            Err(IccError::GpuClutAccuracy)
        ));
    }

    #[test]
    #[ignore = "requires an explicitly selected native GPU; readback smoke, not GUI E2E"]
    fn gpu_clut_fixtures_match_lcms_and_recover() {
        use crate::render::{OffscreenRenderer, RenderImage, Viewport};
        let samples: Vec<[f32; 3]> = (0..1024)
            .map(|i| {
                if i % 4 == 0 {
                    [i as f32 / 1023.0 * 0.01; 3]
                } else {
                    [
                        (i * 73 % 1024) as f32 / 512.0 - 0.2,
                        (i * 137 % 1024) as f32 / 700.0 - 0.1,
                        (i * 313 % 1024) as f32 / 500.0 - 0.4,
                    ]
                }
            })
            .collect();
        let working = crate::color::working::WorkingImage::new(1024, 1, samples).unwrap();
        let image = RenderImage::from_working(&working).unwrap();
        let mut renderer = OffscreenRenderer::with_image(None, image, "clut-fixture-test").unwrap();
        eprintln!("CLUT GPU adapter {:?}", renderer.adapter_info());
        let mut viewport = Viewport {
            image_size: (1024, 1),
            viewport_size: (1024.0, 1.0),
            ..Default::default()
        };
        viewport.refit();
        for (version, kind) in [
            (2, LutKind::Lut8),
            (4, LutKind::Lut8),
            (2, LutKind::Lut16),
            (4, LutKind::Lut16),
            (4, LutKind::Mab),
        ] {
            for lab in [false, true] {
                let profile = profile(version, kind, lab);
                renderer.set_display_transform(Some(std::sync::Arc::new(
                    ScreenTransform::from_icc(&profile).unwrap(),
                )));
                let actual = renderer.render(&viewport, (1024, 1));
                let expected =
                    working_to_output_rgb16_reference(&profile, working.pixels()).unwrap();
                let mut max_error = 0;
                for (a, e) in actual.chunks_exact(4).zip(expected) {
                    for c in 0..3 {
                        max_error =
                            max_error.max(a[c].abs_diff(((u32::from(e[c]) + 128) / 257) as u8));
                    }
                }
                assert!(
                    max_error <= 2,
                    "{version} {kind:?} Lab={lab}: {max_error}/255"
                );
                eprintln!("CLUT {version} {kind:?} Lab={lab}: max {max_error}/255");
                renderer.simulate_device_loss();
                renderer.recover().unwrap();
                assert_eq!(actual, renderer.render(&viewport, (1024, 1)));
            }
        }
    }
    #[test]
    fn independent_lut8_lut16_mab_mba_keep_exact_input_and_checked_display() {
        for (version, kind) in [
            (2, LutKind::Lut8),
            (4, LutKind::Lut8),
            (2, LutKind::Lut16),
            (4, LutKind::Lut16),
            (4, LutKind::Mab),
        ] {
            for lab in [false, true] {
                let profile = profile(version, kind, lab);
                let input: Vec<[u16; 3]> = (0..2048_u32)
                    .map(|i| {
                        [
                            i.wrapping_mul(31) as u16,
                            i.wrapping_mul(137) as u16,
                            i.wrapping_mul(233) as u16,
                        ]
                    })
                    .collect();
                assert_eq!(
                    input_rgb16_to_working(&profile, &input).unwrap(),
                    input_rgb16_to_working_reference(&profile, &input).unwrap(),
                    "{version} {kind:?} {lab}"
                );
                let eight: Vec<[u8; 3]> = input.iter().map(|p| p.map(|v| v as u8)).collect();
                let expanded: Vec<[u16; 3]> = eight
                    .iter()
                    .map(|p| p.map(|v| u16::from(v) * 257))
                    .collect();
                assert_eq!(
                    input_rgb8_to_working(&profile, &eight).unwrap(),
                    input_rgb16_to_working_reference(&profile, &expanded).unwrap()
                );
                let display = ScreenTransform::from_icc(&profile)
                    .unwrap_or_else(|e| panic!("{version} {kind:?} lab={lab}: {e}"));
                assert!(display.clut.is_some() || display.native.is_some());
                // Independent non-bake samples including very dark and signed wide gamut.
                let samples: Vec<[f32; 3]> = (0..3000)
                    .map(|i| {
                        if i % 4 == 0 {
                            [i as f32 / 3000.0 * 0.003; 3]
                        } else {
                            [
                                (i * 73 % 3011) as f32 / 1500.0 - 0.4,
                                (i * 127 % 3011) as f32 / 1300.0 - 0.5,
                                (i * 499 % 3011) as f32 / 1200.0 - 0.6,
                            ]
                        }
                    })
                    .collect();
                let expected = working_to_output_rgb16_reference(&profile, &samples).unwrap();
                for (input, expected) in samples.iter().zip(expected) {
                    let actual = display
                        .surface_linear(*input)
                        .map(crate::develop::color::linear_to_srgb);
                    for c in 0..3 {
                        assert!(
                            (actual[c] - f32::from(expected[c]) / 65535.0).abs() <= 1.0 / 255.0,
                            "{version} {kind:?} lab={lab} {input:?} {actual:?} {expected:?}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn large_input_parallel_chunks_and_cache_are_exact() {
        let profile = profile(4, LutKind::Mab, false);
        let input: Vec<[u16; 3]> = (0..131_073_u32)
            .map(|i| {
                [
                    i as u16,
                    i.wrapping_mul(313) as u16,
                    i.wrapping_mul(719) as u16,
                ]
            })
            .collect();
        let expected = input_rgb16_to_working_reference(&profile, &input).unwrap();
        for _ in 0..2 {
            assert_eq!(input_rgb16_to_working(&profile, &input).unwrap(), expected);
        }
        assert!(input_rgb16_to_working(&profile, &[]).unwrap().is_empty());
    }
}
