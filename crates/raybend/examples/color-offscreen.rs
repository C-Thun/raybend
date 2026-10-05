//! Reproducible native GPU smoke and independent LCMS pixel comparison.
//! `cargo run -p raybend --example color-offscreen`
//! No window interaction; readback timing is not a production FPS benchmark.
use raybend::{
    color::{display::ScreenTransform, icc, working::WorkingImage},
    render::{OffscreenRenderer, RenderImage, Viewport},
};
use std::sync::Arc;

#[path = "../src/color/icc_fixtures.rs"]
mod icc_fixtures;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut samples = Vec::new();
    for i in 0..128 {
        let v = i as f32 / 127.0;
        samples.push([
            v,
            ((i * 31) % 128) as f32 / 127.0,
            ((i * 67) % 128) as f32 / 127.0,
        ]);
        samples.push([v * 0.005; 3]);
    }
    samples.extend([[-0.2, 0.3, 1.4], [1.3, -0.1, 0.4]]);
    let width = samples.len() as u32;
    let working = WorkingImage::new(width, 1, samples)?;
    let texture = RenderImage::from_working(&working).ok_or("half texture failed")?;
    let mut renderer = OffscreenRenderer::with_image(None, texture, "color-offscreen")?;
    let mut viewport = Viewport {
        image_size: (width, 1),
        viewport_size: (width as f32, 1.0),
        ..Default::default()
    };
    viewport.refit();
    let mut profiles = vec![
        icc::srgb_icc()?,
        icc::display_p3_icc()?,
        icc::adobe_rgb_icc()?,
    ];
    for (version, kind) in [
        (2, icc_fixtures::LutKind::Lut8),
        (4, icc_fixtures::LutKind::Lut8),
        (2, icc_fixtures::LutKind::Lut16),
        (4, icc_fixtures::LutKind::Lut16),
        (4, icc_fixtures::LutKind::Mab),
    ] {
        for lab in [false, true] {
            profiles.push(icc_fixtures::profile(version, kind, lab));
        }
    }
    assert!(matches!(
        ScreenTransform::from_icc(&icc_fixtures::sharp_profile()),
        Err(icc::IccError::GpuClutAccuracy)
    ));
    for path in std::env::args().skip(1) {
        profiles.push(icc::RgbIcc::parse(
            &std::fs::read(path)?,
            icc::IccRole::Display,
        )?);
    }
    for profile in profiles {
        let started = std::time::Instant::now();
        let transform = match ScreenTransform::from_icc(&profile) {
            Ok(value) => value,
            Err(error) => {
                println!(
                    "{} display rejected: {error}; preparation {:?}",
                    profile.id().as_str(),
                    started.elapsed()
                );
                continue;
            }
        };
        println!(
            "{} {} display preparation {:?}",
            profile.id().as_str(),
            transform.method(),
            started.elapsed()
        );
        renderer.set_display_transform(Some(Arc::new(transform)));
        let pixels = renderer.render(&viewport, (width, 1));
        assert_eq!(pixels.len(), width as usize * 4);
        // Independent LCMS reference, with original f32 samples. GPU half packing,
        // compensation LUT and attachment encoding together get a 2-code budget.
        let reference = working.to_rgb16(&profile)?;
        let mut max_error = 0;
        for (pixel, expected) in pixels.chunks_exact(4).zip(reference) {
            assert_eq!(pixel[3], 255);
            for c in 0..3 {
                let value = ((u32::from(expected[c]) + 128) / 257) as u8;
                max_error = max_error.max(pixel[c].abs_diff(value));
            }
        }
        assert!(
            max_error <= 2,
            "{} error {max_error}",
            profile.id().as_str()
        );
        renderer.simulate_device_loss();
        renderer.recover()?;
        assert_eq!(pixels, renderer.render(&viewport, (width, 1)));
        println!(
            "{} GPU/ICC max error={max_error}/255",
            profile.id().as_str()
        );
    }
    renderer.set_display_transform(None);
    for target in [
        icc::srgb_icc()?,
        icc::display_p3_icc()?,
        icc::adobe_rgb_icc()?,
    ] {
        let proof = Arc::new(raybend::color::proof::ProofTransform::from_icc(&target)?);
        renderer.set_proof(Some(Arc::clone(&proof)), false);
        let pixels = renderer.render(&viewport, (width, 1));
        let device = working.to_rgb16(&target)?;
        let back = icc::input_rgb16_to_working(&target, &device)?;
        let reference = icc::working_to_output_rgb16(&icc::srgb_icc()?, &back)?;
        let mut error = 0;
        for (pixel, expected) in pixels.chunks_exact(4).zip(reference) {
            for c in 0..3 {
                error = error.max(pixel[c].abs_diff(((u32::from(expected[c]) + 128) / 257) as u8));
            }
        }
        assert!(error <= 3, "RGB proof error={error}");
        renderer.simulate_device_loss();
        renderer.recover()?;
        assert_eq!(pixels, renderer.render(&viewport, (width, 1)));
        renderer.set_proof(Some(Arc::clone(&proof)), true);
        let warnings = renderer.render(&viewport, (width, 1));
        for (pixel, source) in warnings.chunks_exact(4).zip(working.pixels()) {
            if proof.simulate(*source).1 {
                assert!(pixel[0] >= 254 && pixel[1] <= 1 && pixel[2] >= 254);
            }
        }
        println!(
            "RGB proof {}, max error={error}/255, warning and recovery passed",
            target.id().as_str()
        );
    }
    renderer.set_proof(None, false);
    use raybend::develop::{
        curve::{Curve, CurveSet},
        params::DevelopParams,
        pipeline::DevelopStages,
        working::{WorkingReferenceImage, WorkingTonePlan, render_working_reference},
    };
    let source = WorkingReferenceImage::from_working(working.clone())?;
    let mut curves = CurveSet::identity();
    curves.rgb = Curve::from_points(vec![[0.0, 0.05], [0.4, 0.5], [1.0, 0.95]])?;
    for contrast in [-100.0, 0.0, 100.0] {
        let params = DevelopParams::from_values(
            [
                ("exposure".into(), 0.5),
                ("contrast".into(), contrast),
                ("saturation".into(), 30.0),
                ("vibrance".into(), 20.0),
            ],
            None,
        )?;
        let tone = WorkingTonePlan::new(&params, &curves);
        let started = std::time::Instant::now();
        let description = Arc::new(tone.gpu_description());
        let prepare = started.elapsed();
        renderer.set_global_tone(Some(description));
        let pixels = renderer.render(&viewport, (width, 1));
        let reference = render_working_reference(&source, &tone, &DevelopStages::default())?
            .to_rgb16(&icc::srgb_icc()?)?;
        let mut error = 0;
        for (pixel, expected) in pixels.chunks_exact(4).zip(reference) {
            for c in 0..3 {
                error = error.max(pixel[c].abs_diff(((u32::from(expected[c]) + 128) / 257) as u8));
            }
        }
        assert!(error <= 2, "GPU tone contrast={contrast} error={error}");
        println!(
            "tone contrast={contrast}, max error={error}/255, prepare={prepare:?}, source unchanged"
        );
    }
    renderer.set_global_tone(None);
    // An unmanaged legacy upload still follows the identical original shader path.
    renderer.set_display_transform(None);
    renderer.set_image(RenderImage::from_rgb8(1, 1, &[31, 127, 241]).unwrap());
    viewport.image_size = (1, 1);
    viewport.viewport_size = (1.0, 1.0);
    viewport.refit();
    let pixel = renderer.render(&viewport, (1, 1));
    for c in 0..3 {
        assert!(pixel[c].abs_diff([31, 127, 241][c]) <= 1);
    }
    renderer.simulate_device_loss();
    renderer.recover()?;
    assert_eq!(renderer.render(&viewport, (1, 1)), pixel);
    println!("legacy upload and device recovery passed");
    renderer.set_image(RenderImage::from_working(&working).unwrap());
    viewport.image_size = (width, 1);
    viewport.viewport_size = (width as f32, 1.0);
    viewport.refit();
    renderer.set_sc_rgb_reference(Some(203.0 / 80.0));
    let actual = renderer.render(&viewport, (width, 1));
    assert_eq!(actual.len(), width as usize * 8);
    for (pixel, input) in actual.chunks_exact(8).zip(working.pixels()) {
        let rgb = raybend::develop::color::mat3_vec3(
            [
                [1.660491, -0.5876411, -0.07284986],
                [-0.1245505, 1.1328999, -0.00834942],
                [-0.01815076, -0.1005789, 1.1187297],
            ],
            *input,
        );
        let expected = raybend::render::output_space::sdr_sc_rgb(rgb, 203.0 / 80.0);
        for c in 0..3 {
            let value =
                half::f16::from_bits(u16::from_le_bytes([pixel[c * 2], pixel[c * 2 + 1]])).to_f32();
            assert!(
                (value - expected[c]).abs() < 0.005,
                "scRGB {value} {expected:?}"
            );
        }
    }
    renderer.simulate_device_loss();
    renderer.recover()?;
    assert_eq!(actual, renderer.render(&viewport, (width, 1)));
    renderer.set_sc_rgb_reference(None);
    println!("scRGB FP16, SDR white, wide-gamut range and recovery passed");
    Ok(())
}
