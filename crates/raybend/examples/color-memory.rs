//! Independent-process 60MP measurement: /usr/bin/time -v target/debug/examples/color-memory [deferred|packed] [gpu]
//! Packed reconstructs the previous neutral f32→half path; both modes use bounded
//! current GPU staging. This is a synthetic resource test, never color/GUI E2E.
use raybend::{
    color::working::WorkingImage,
    develop::{
        CurveSet, DevelopParams, DevelopStages,
        denoise::NrMethod,
        working::{WorkingReferenceImage, WorkingTonePlan, render_working_reference},
    },
    render::{OffscreenRenderer, RenderImage, Viewport, working_preview::WorkingPreview},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let start = std::time::Instant::now();
    let source = WorkingReferenceImage::from_working(WorkingImage::new(
        8000,
        7500,
        vec![[0.18, 0.4, 1.2]; 60_000_000],
    )?)?;
    let mut preview = WorkingPreview::new(source);
    let setup = start.elapsed();
    let started = std::time::Instant::now();
    let image = if args.get(1).is_some_and(|v| v == "packed") {
        let source = preview.tier_source(0);
        let neutral = render_working_reference(
            &source,
            &WorkingTonePlan::new(&DevelopParams::default(), &CurveSet::identity()),
            &DevelopStages::default(),
        )?;
        RenderImage::from_working(&neutral).ok_or("half overflow")?
    } else {
        (*preview
            .render(
                0,
                &DevelopParams::default(),
                &CurveSet::identity(),
                NrMethod::Fast,
                None,
                None,
                None,
            )?
            .image)
            .clone()
    };
    println!(
        "60MP source setup {setup:?}; frame prepare {:?}; resident packed bytes={}",
        started.elapsed(),
        image.byte_len()
    );
    if args.iter().any(|v| v == "gpu" || v == "async") {
        let started = std::time::Instant::now();
        let mut renderer = OffscreenRenderer::with_image(
            None,
            RenderImage::solid(2000, 1000, [19, 83, 147, 255]),
            "memory-60mp",
        )?;
        println!(
            "GPU context {:?}; adapter {:?}",
            started.elapsed(),
            renderer.adapter_info()
        );
        let started = std::time::Instant::now();
        if args.iter().any(|v| v == "async") {
            let image = std::sync::Arc::new(image);
            let mut old_view = Viewport {
                image_size: (2000, 1000),
                viewport_size: (800.0, 400.0),
                ..Default::default()
            };
            old_view.refit();
            let old = renderer.render(&old_view, (800, 400));
            let mut baseline=Vec::new();
            for _ in 0..24 {
                let tick=std::time::Instant::now();
                assert_eq!(renderer.render(&old_view,(800,400)),old);
                baseline.push(tick.elapsed());
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
            baseline.sort();
            println!("old-frame readback baseline (800x400): median={:?} p95={:?}",baseline[baseline.len()/2],baseline[baseline.len()*95/100]);
            let start = std::time::Instant::now();
            let queued = renderer.prepare_shared_image(image.clone())?;
            let dispatch = start.elapsed();
            assert!(!queued);
            let mut draws = Vec::new();
            let mut polls = Vec::new();
            loop {
                let tick = std::time::Instant::now();
                let ready = renderer.prepare_shared_image(image.clone())?;
                polls.push(tick.elapsed());
                if ready {
                    break;
                }
                let tick = std::time::Instant::now();
                assert_eq!(
                    renderer.render(&old_view, (800, 400)),
                    old,
                    "incomplete texture became visible"
                );
                draws.push(tick.elapsed());
                assert!(
                    start.elapsed() < std::time::Duration::from_secs(30),
                    "async source upload timed out"
                );
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
            assert!(draws.len() > 1, "no concurrent old frames were rendered");
            draws.sort();
            polls.sort();
            let commit = std::time::Instant::now();
            renderer.commit_prepared_image(image)?;
            let commit = commit.elapsed();
            println!(
                "async dispatch={dispatch:?}, poll max={:?}, commit={commit:?}, total={:?}; old complete frames={}, draw median={:?} p95={:?} (800x400 with readback)",
                polls.last().unwrap(),
                start.elapsed(),
                draws.len(),
                draws[draws.len() / 2],
                draws[(draws.len() * 95 / 100).min(draws.len() - 1)]
            );
        } else {
            renderer.set_image(image);
        }
        println!(
            "60MP {} {:?}",
            if args.iter().any(|v|v=="async") {"async harness (includes initial old-frame readback)"} else {"source upload (includes deferred conversion, excludes context creation)"},
            started.elapsed(),
        );
        let mut view = Viewport {
            image_size: (8000, 7500),
            viewport_size: (800.0, 750.0),
            ..Default::default()
        };
        view.refit();
        let pixels = renderer.render(&view, (800, 750));
        assert_eq!(pixels.len(), 800 * 750 * 4);
        std::hint::black_box(renderer);
    } else {
        std::hint::black_box(image);
    }
    std::hint::black_box(preview);
    Ok(())
}
