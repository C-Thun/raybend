//! 可重复的合成图 BM3D 耗时冒烟；不代表照片视觉质量验收。
use raybend::develop::{bm3d::denoise_high, denoise::DenoisePlan, pipeline::LinearImage};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let width: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(320);
    let height: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(240);
    let mut seed = 7_u32;
    let rgb = (0..width * height)
        .flat_map(|i| {
            let v = 10000 + (i % width) * 35000 / width.max(1);
            std::array::from_fn::<_, 3, _>(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (v + (seed >> 16) % 3000) as u16
            })
        })
        .collect();
    let source = LinearImage { width, height, rgb };
    let started = std::time::Instant::now();
    let out = denoise_high(
        &source,
        &DenoisePlan {
            luma: 0.6,
            chroma: 0.6,
        },
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    println!(
        "BM3D {width}x{height}: {:.3}s; {} samples",
        started.elapsed().as_secs_f64(),
        out.rgb.len()
    );
}
