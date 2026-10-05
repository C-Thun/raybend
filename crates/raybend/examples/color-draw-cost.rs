//! Same-renderer A/B draw cost; no readback, source uploads or per-frame ICC.
use raybend::{
    color::{display::ScreenTransform, icc, proof::ProofTransform, working::WorkingImage},
    develop::{CurveSet, DevelopParams, working::WorkingTonePlan},
    render::{OffscreenRenderer, RenderImage, Viewport},
};
use std::{sync::Arc, time::Instant};
#[path = "../src/color/icc_fixtures.rs"]
#[allow(dead_code)]
mod icc_fixtures;

fn report(
    renderer: &mut OffscreenRenderer,
    viewport: &Viewport,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut times = renderer.measure_draws(viewport, (1920, 1080), 40)?;
    times.sort();
    println!(
        "{name}: median={:.3} ms p95={:.3} ms",
        times[times.len() / 2].as_secs_f64() * 1000.0,
        times[times.len() * 95 / 100].as_secs_f64() * 1000.0
    );
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let working = WorkingImage::from_linear_srgb(
        1920,
        1080,
        (0..1920 * 1080)
            .map(|i| [(i % 1920) as f32 / 1919.0, (i / 1920) as f32 / 1079.0, 0.4])
            .collect(),
    )?;
    let legacy = RenderImage::from_rgb8(1920, 1080, &vec![127; 1920 * 1080 * 3]).unwrap();
    let mut renderer = OffscreenRenderer::with_image(None, legacy, "color-draw-cost")?;
    println!("adapter={:?}", renderer.adapter_info());
    let mut viewport = Viewport {
        image_size: (1920, 1080),
        viewport_size: (1920.0, 1080.0),
        ..Default::default()
    };
    viewport.refit();
    report(&mut renderer, &viewport, "legacy sRGB8")?;
    renderer.set_display_transform(Some(Arc::new(ScreenTransform::from_icc(
        &icc::display_p3_icc()?,
    )?)));
    report(&mut renderer, &viewport, "legacy sRGB8+P3 ICC")?;
    renderer.set_display_transform(Some(Arc::new(ScreenTransform::from_icc(
        &icc::adobe_rgb_icc()?,
    )?)));
    report(&mut renderer, &viewport, "legacy sRGB8+Adobe RGB TRC")?;
    renderer.set_display_transform(None);
    let start = Instant::now();
    renderer.set_image(RenderImage::from_working(&working).unwrap());
    println!(
        "float packing+source upload={:.3} ms",
        start.elapsed().as_secs_f64() * 1000.0
    );
    report(&mut renderer, &viewport, "V2 source only")?;
    let params =
        DevelopParams::from_values([("exposure".into(), 0.5), ("contrast".into(), 20.0)], None)?;
    let start = Instant::now();
    let tone = Arc::new(WorkingTonePlan::new(&params, &CurveSet::identity()).gpu_description());
    println!(
        "tone preparation={:.3} ms",
        start.elapsed().as_secs_f64() * 1000.0
    );
    let start = Instant::now();
    renderer.set_global_tone(Some(tone));
    println!(
        "tone upload={:.3} ms",
        start.elapsed().as_secs_f64() * 1000.0
    );
    report(&mut renderer, &viewport, "V2 global tone")?;
    let target = icc::display_p3_icc()?;
    let start = Instant::now();
    renderer.set_display_transform(Some(Arc::new(ScreenTransform::from_icc(&target)?)));
    println!(
        "display preparation+upload={:.3} ms",
        start.elapsed().as_secs_f64() * 1000.0
    );
    report(&mut renderer, &viewport, "V2 tone+ICC")?;
    renderer.set_display_transform(Some(Arc::new(ScreenTransform::from_icc(
        &icc::adobe_rgb_icc()?,
    )?)));
    report(&mut renderer, &viewport, "V2 tone+Adobe RGB TRC")?;
    renderer.set_display_transform(Some(Arc::new(ScreenTransform::from_icc(&target)?)));
    renderer.set_proof(
        Some(Arc::new(ProofTransform::from_icc(&icc::srgb_icc()?)?)),
        true,
    );
    report(&mut renderer, &viewport, "V2 tone+ICC+proof warning")?;
    let params = DevelopParams::from_values(
        [
            ("exposure".into(), 0.5),
            ("contrast".into(), 20.0),
            ("saturation".into(), 30.0),
            ("vibrance".into(), 20.0),
        ],
        None,
    )?;
    renderer.set_global_tone(Some(Arc::new(
        WorkingTonePlan::new(&params, &CurveSet::identity()).gpu_description(),
    )));
    report(&mut renderer, &viewport, "V2 tone+chroma+ICC+proof warning")?;
    renderer.set_display_transform(None);
    renderer.set_proof(None, false);
    for lab in [false, true] {
        let profile = icc_fixtures::profile(4, icc_fixtures::LutKind::Mab, lab);
        let started = Instant::now();
        let transform = Arc::new(ScreenTransform::from_icc(&profile)?);
        println!(
            "CLUT Lab={lab} preparation={:.3} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
        let started = Instant::now();
        renderer.set_display_transform(Some(transform));
        println!(
            "CLUT Lab={lab} upload={:.3} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
        report(
            &mut renderer,
            &viewport,
            &format!("V2 complex tone+CLUT Lab={lab}"),
        )?;
        renderer.set_proof(
            Some(Arc::new(ProofTransform::from_icc(&icc::display_p3_icc()?)?)),
            true,
        );
        report(
            &mut renderer,
            &viewport,
            &format!("V2 complex tone+CLUT Lab={lab}+proof warning"),
        )?;
        renderer.set_proof(None, false);
    }
    for path in std::env::args().skip(1) {
        let profile = icc::RgbIcc::parse(&std::fs::read(path)?, icc::IccRole::Display)?;
        let started = Instant::now();
        let transform = Arc::new(ScreenTransform::from_icc(&profile)?);
        println!(
            "external {} {} preparation={:.3} ms",
            profile.id().as_str(),
            transform.method(),
            started.elapsed().as_secs_f64() * 1000.0
        );
        let started = Instant::now();
        renderer.set_display_transform(Some(transform));
        println!(
            "external upload={:.3} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
        report(&mut renderer, &viewport, "V2 complex tone+external ICC")?;
        renderer.set_proof(
            Some(Arc::new(ProofTransform::from_icc(&icc::display_p3_icc()?)?)),
            true,
        );
        report(
            &mut renderer,
            &viewport,
            "V2 complex tone+external ICC+proof warning",
        )?;
        renderer.set_proof(None, false);
    }
    renderer.set_sc_rgb_reference(Some(203.0 / 80.0));
    report(&mut renderer, &viewport, "V2 tone+scRGB FP16")?;
    Ok(())
}
