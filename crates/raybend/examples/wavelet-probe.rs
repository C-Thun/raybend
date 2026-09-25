//! Headless GPU compute/transfer timing, not GUI or photographic quality acceptance.
use raybend::{
    develop::{denoise::DenoisePlan, pipeline::LinearImage},
    render::wavelet::WaveletGpu,
};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let width: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1920);
    let height: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1080);
    assert!(width > 0 && height > 0);
    let started = std::time::Instant::now();
    let mut gpu = WaveletGpu::new(false).expect("hardware compute adapter");
    println!(
        "{}; initialization {:.3}s",
        gpu.adapter_name(),
        started.elapsed().as_secs_f64()
    );
    let rgb = (0..width * height)
        .flat_map(|i| [16000 + (i.wrapping_mul(7919) % 1601) as u16; 3])
        .collect();
    let source = LinearImage { width, height, rgb };
    for run in 1..=3 {
        let started = std::time::Instant::now();
        let out = gpu
            .denoise(
                &source,
                &DenoisePlan {
                    luma: 0.6,
                    chroma: 0.6,
                },
            )
            .unwrap();
        println!(
            "wavelet {width}x{height} #{run}: {:.3}s; {} samples",
            started.elapsed().as_secs_f64(),
            out.rgb.len()
        );
    }
}
