//! **显影管线的性能与像素证据探针**（M3-W3）。
//!
//! ```bash
//! # 纯合成数据：量管线本身的耗时（24MP / 2.5MP 两档 × 几组参数）
//! cargo run -p raybend --example develop-probe
//!
//! # 带一张真照片：量「线性解码 + 管线」的端到端耗时（RAW 走 worker）
//! cargo run -p raybend --example develop-probe -- /mnt/c/src/tmp/pic/P1000019.RW2
//! ```
//!
//! 为什么要有它：`AGENTS.md` §2.8 —— 交互帧率这类东西 Agent 只能量数字、不能下结论，
//! 但**数字必须是真的**。实施记录里的耗时就是这里跑出来的。

use std::path::Path;
use std::time::Instant;

use raybend::develop::denoise::denoise_fast;
use raybend::develop::lens::{LensMap, warp_lens};
use raybend::develop::{
    CurveSet, DevelopParams, DevelopPlans, DevelopStages, LinearImage, render_develop, render_rgb8,
};

fn synthetic(width: u32, height: u32) -> LinearImage {
    let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);
    for y in 0..height {
        for x in 0..width {
            #[allow(clippy::cast_precision_loss)]
            let r = ((x as f32) / (width as f32)).clamp(0.0, 1.0);
            #[allow(clippy::cast_precision_loss)]
            let g = ((y as f32) / (height as f32)).clamp(0.0, 1.0);
            let b = ((r + g) / 2.0).clamp(0.0, 1.0);
            for value in [r, g, b] {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let encoded = (value * 65535.0 + 0.5) as u32;
                rgb.push(u16::try_from(encoded.min(65535)).unwrap_or(u16::MAX));
            }
        }
    }
    LinearImage::new(width, height, rgb).expect("形状对得上")
}

fn params(values: &[(&str, f64)]) -> DevelopParams {
    let mut params = DevelopParams::new(Some(5200.0));
    for (id, value) in values {
        params.set(id, *value).expect("参数合法");
    }
    params
}

fn time(label: &str, source: &LinearImage, params: &DevelopParams, curves: &CurveSet) {
    // 先跑一遍热热身（分配输出缓冲、把页表摸热）
    let _ = render_rgb8(source, params, curves);
    let runs = 3;
    let mut total = std::time::Duration::ZERO;
    for _ in 0..runs {
        let start = Instant::now();
        let out = render_rgb8(source, params, curves);
        total += start.elapsed();
        std::hint::black_box(&out);
    }
    let average = total / runs;
    let pixels = u64::from(source.width) * u64::from(source.height);
    #[allow(clippy::cast_precision_loss)]
    let megapixels = pixels as f64 / 1_000_000.0;
    #[allow(clippy::cast_precision_loss)]
    let millis = average.as_secs_f64() * 1000.0;
    println!(
        "{label:<44} {:>7.2} MP   {millis:>8.1} ms   {:.1} ms/MP",
        megapixels,
        millis / megapixels
    );
}

/// 量一趟 `render_develop`（可选阶段全在里面）。
fn time_with(
    label: &str,
    source: &LinearImage,
    params: &DevelopParams,
    curves: &CurveSet,
    stages: &DevelopStages<'_>,
) {
    let _ = render_develop(source, params, curves, stages);
    let runs = 3;
    let mut total = std::time::Duration::ZERO;
    for _ in 0..runs {
        let start = Instant::now();
        let out = render_develop(source, params, curves, stages);
        total += start.elapsed();
        std::hint::black_box(&out);
    }
    let average = total / runs;
    let pixels = u64::from(source.width) * u64::from(source.height);
    #[allow(clippy::cast_precision_loss)]
    let megapixels = pixels as f64 / 1_000_000.0;
    #[allow(clippy::cast_precision_loss)]
    let millis = average.as_secs_f64() * 1000.0;
    println!(
        "{label:<44} {:>7.2} MP   {millis:>8.1} ms   {:.1} ms/MP",
        megapixels,
        millis / megapixels
    );
}

/// 量一个单独操作（微基准：把「谁贵」从合并的数字里拆出来）。
fn time_op(label: &str, runs: usize, mut op: impl FnMut()) {
    op();
    let mut total = std::time::Duration::ZERO;
    for _ in 0..runs {
        let start = Instant::now();
        op();
        total += start.elapsed();
    }
    let average = total / u32::try_from(runs).unwrap_or(1);
    #[allow(clippy::cast_precision_loss)]
    let millis = average.as_secs_f64() * 1000.0;
    println!("{label:<44} {millis:>10.1} ms");
}

fn main() {
    let path = std::env::args().nth(1);
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    println!("可用并行度：{threads}");
    println!(
        "{:<44} {:>9}   {:>11}   每 MP",
        "场景", "像素", "耗时"
    );

    let curves = CurveSet::identity();
    let default = DevelopParams::new(Some(5200.0));
    let exposure = params(&[("exposure", 0.5)]);
    let all_tone = params(&[
        ("exposure", 0.5),
        ("contrast", 40.0),
        ("highlights", -50.0),
        ("blacks", 30.0),
        ("temperature", 4200.0),
    ]);
    let with_chroma = params(&[
        ("exposure", 0.5),
        ("contrast", 40.0),
        ("highlights", -50.0),
        ("blacks", 30.0),
        ("temperature", 4200.0),
        ("saturation", 30.0),
        ("vibrance", 40.0),
    ]);

    // 预览档（长边 1920 的 16:9）与 1:1 档（24MP）
    let preview = synthetic(1920, 1080);
    let full = synthetic(6000, 4000);

    time("预览档 1920×1080 · 默认参数", &preview, &default, &curves);
    time("预览档 1920×1080 · 曝光", &preview, &exposure, &curves);
    time("预览档 1920×1080 · 全部调性", &preview, &all_tone, &curves);
    time("预览档 1920×1080 · 带色度", &preview, &with_chroma, &curves);
    time("1:1 档 6000×4000 · 默认参数", &full, &default, &curves);
    time("1:1 档 6000×4000 · 曝光", &full, &exposure, &curves);
    time("1:1 档 6000×4000 · 全部调性", &full, &all_tone, &curves);
    time("1:1 档 6000×4000 · 带色度", &full, &with_chroma, &curves);
    time("1:1 档 6000×4000 · 缩到预览档", &full, &default, &curves);

    // ── M3-W4：可选阶段（镜头 / 降噪 / 锐化）的代价 ──
    println!("\n── 可选阶段（M3-W4）──");
    let detail = params(&[
        ("lumaNr", 60.0),
        ("colorNr", 60.0),
        ("sharpenAmount", 60.0),
        ("sharpenRadius", 50.0),
    ]);
    let lens_params = params(&[
        ("distortion", 40.0),
        ("vignette", 40.0),
        ("chromatic", 40.0),
    ]);
    let everything = params(&[
        ("exposure", 0.5),
        ("contrast", 40.0),
        ("saturation", 30.0),
        ("lumaNr", 60.0),
        ("colorNr", 60.0),
        ("sharpenAmount", 60.0),
        ("sharpenRadius", 50.0),
        ("distortion", 40.0),
        ("vignette", 40.0),
        ("chromatic", 40.0),
    ]);
    for (tier, source) in [("预览档 1920×1080", &preview), ("1:1 档 6000×4000", &full)] {
        // 先把两件最贵的事**单独**量一遍（合在一起的数字看不出是谁贵）
        let probe_plans = DevelopPlans::from_params(source.width, source.height, &detail);
        let probe_lens = DevelopPlans::from_params(source.width, source.height, &lens_params);
        let probe_map = LensMap::new(&probe_lens.lens);
        time_op(&format!("{tier} ·（单独）warp_lens"), 5, || {
            std::hint::black_box(warp_lens(source, &probe_map));
        });
        time_op(&format!("{tier} ·（单独）denoise_fast"), 5, || {
            std::hint::black_box(denoise_fast(source, &probe_plans.denoise));
        });
        let plans = DevelopPlans::from_params(source.width, source.height, &detail);
        time_with(
            &format!("{tier} · 降噪（亮度+色度 60）"),
            source,
            &detail,
            &curves,
            &DevelopStages {
                denoise: Some(&plans.denoise),
                ..DevelopStages::default()
            },
        );
        time_with(
            &format!("{tier} · 锐化（60 / 半径 2.5px）"),
            source,
            &detail,
            &curves,
            &DevelopStages {
                sharpen: Some(&plans.sharpen),
                ..DevelopStages::default()
            },
        );
        let lens_plans = DevelopPlans::from_params(source.width, source.height, &lens_params);
        let lens_map = LensMap::new(&lens_plans.lens);
        time_with(
            &format!("{tier} · 镜头手动校正（畸变/暗角/色差 40）"),
            source,
            &lens_params,
            &curves,
            &DevelopStages {
                lens: Some(&lens_map),
                ..DevelopStages::default()
            },
        );
        let all_plans = DevelopPlans::from_params(source.width, source.height, &everything);
        let all_map = LensMap::new(&all_plans.lens);
        time_with(
            &format!("{tier} · 全部阶段"),
            source,
            &everything,
            &curves,
            &DevelopStages {
                lens: Some(&all_map),
                denoise: Some(&all_plans.denoise),
                local_tone: None,
                sharpen: Some(&all_plans.sharpen),
                ..DevelopStages::default()
            },
        );
    }

    // 缩放的耗时单独量（它不是管线的一部分，但拖动档位时会走）
    let start = Instant::now();
    let small = full.downscaled_to(1920);
    println!(
        "\n6000×4000 → {}×{} 缩放：{:.1} ms",
        small.width,
        small.height,
        start.elapsed().as_secs_f64() * 1000.0
    );

    if let Some(path) = path {
        let path = Path::new(&path);
        println!("\n── 真照片：{} ──", path.display());
        let is_raw = raybend::display::pixels::is_raw_photo(path);
        if is_raw {
            // RAW 走**线性解码**（完整解码 + 去马赛克，M3-W3 的显影输入）
            let request = raybend::raw::backend::DecodeRequest::full(path);
            let start = Instant::now();
            let worker = raybend::raw::worker::shared();
            let result = worker.lock().expect("worker 锁").decode_linear(&request);
            let elapsed = start.elapsed();
            match result {
                Ok(image) => {
                    println!(
                        "线性解码（{}×{}，{} 值，方向 {:?}，拍摄色温 {:?}）：{:.1} ms",
                        image.width,
                        image.height,
                        image.rgb.len(),
                        image.orientation,
                        image.as_shot_temperature.map(|k| format!("{k:.0}K")),
                        elapsed.as_secs_f64() * 1000.0
                    );
                    // 与编辑器同一条构造路径：文件头方向优先（worker 对 TIFF 家族不可靠）+ 按方向摆正
                    let mut image = image;
                    image.orientation =
                        raybend::media::exif::raw_orientation(path, image.orientation);
                    println!("取方向（文件头优先）：{:?}", image.orientation);
                    let linear = raybend::develop::LinearImage::from_raw16(image)
                        .expect("形状对");
                    println!(
                        "摆正后的线性源：{}×{}（编辑器拿到的就是这个尺寸）",
                        linear.width, linear.height
                    );
                    time("RAW 线性源 · 全部调性", &linear, &all_tone, &curves);
                    let start = Instant::now();
                    let small = linear.downscaled_to(1920);
                    println!(
                        "缩到预览档（{}×{}）：{:.1} ms",
                        small.width,
                        small.height,
                        start.elapsed().as_secs_f64() * 1000.0
                    );
                    // 输出一张 PNG 当像素证据（人类可以目视）
                    let out = render_rgb8(&small, &all_tone, &curves);
                    if let Some(dir) = std::env::args().nth(2) {
                        let target = Path::new(&dir).join("develop-probe-raw.png");
                        let buffer = image::RgbImage::from_raw(small.width, small.height, out)
                            .expect("尺寸对");
                        buffer.save(&target).expect("写 PNG");
                        println!("像素证据：{}", target.display());
                    }
                }
                Err(error) => println!("线性解码失败：{error}"),
            }
        } else {
            let start = Instant::now();
            match raybend::display::pixels(path, raybend::display::PixelSize::Full) {
                Ok(Some(pixels)) => {
                    println!(
                        "解码（8bit sRGB，{}×{}，{}）：{:.1} ms",
                        pixels.width,
                        pixels.height,
                        pixels.size.as_str(),
                        start.elapsed().as_secs_f64() * 1000.0
                    );
                    let linear = LinearImage::from_srgb8(pixels.width, pixels.height, &pixels.rgb)
                        .expect("形状对");
                    time("真照片（线性化后）· 全部调性", &linear, &all_tone, &curves);
                }
                Ok(None) => println!("解不开这张照片"),
                Err(error) => println!("解码失败：{error}"),
            }
        }
    }
}
