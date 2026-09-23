//! **缓存格式探针**：量 AVIF / JPEG 的编码耗时与体积（M3-W3 定缓存格式用）。
//!
//! ```bash
//! cargo run -p raybend --example avif-probe
//! ```
//!
//! 为什么留着它：`AVIF_SPEED` / `AVIF_QUALITY` 这两个常量是**实测选出来的**
//! （人类 2026-09-24 定「缓存图一律 AVIF 90 / 4:4:4」之后，速度档位得自己量）。
//! 换机器、换 ravif 版本之后重跑一遍就知道该不该调 —— 别凭感觉改。
//!
//! 注意：合成图是**高频噪声**（比真实照片难压），所以这里的数字是保守下限；
//! 真实照片两种格式都会更小、AVIF 的优势更明显。

use std::time::Instant;

use image::ImageEncoder;
use raybend::thumbnail::render::{AVIF_QUALITY, AVIF_SPEED, encode_avif};

fn synthetic(width: u32, height: u32) -> Vec<u8> {
    let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);
    for y in 0..height {
        for x in 0..width {
            rgb.extend([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8]);
        }
    }
    rgb
}

fn main() {
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    println!("AVIF 质量 {AVIF_QUALITY} / 速度 {AVIF_SPEED} / 可用线程 {threads}");
    println!(
        "{:<14} {:<12} {:>10}  {:>8}   {:>14}  {:>8}",
        "场景", "尺寸", "AVIF", "体积", "JPEG(q82)", "体积"
    );
    for (label, width, height) in [
        ("grid", 384u32, 288u32),
        ("strip", 192, 128),
        ("screen", 1920, 1280),
        ("large", 2560, 1707),
        ("full", 5184, 3888),
    ] {
        let rgb = synthetic(width, height);

        let start = Instant::now();
        let avif = encode_avif(&rgb, width, height).expect("AVIF 编码");
        let avif_ms = start.elapsed().as_secs_f64() * 1000.0;

        let start = Instant::now();
        let mut jpeg = Vec::new();
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 82);
        encoder
            .write_image(&rgb, width, height, image::ExtendedColorType::Rgb8)
            .expect("JPEG 编码");
        let jpeg_ms = start.elapsed().as_secs_f64() * 1000.0;

        println!(
            "{label:<14} {:<12} {avif_ms:>8.1} ms  {:>5} KB   {jpeg_ms:>11.1} ms  {:>5} KB",
            format!("{width}×{height}"),
            avif.len() / 1024,
            jpeg.len() / 1024
        );
    }
}
