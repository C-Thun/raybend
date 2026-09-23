//! **动态反差（local tone）探针**：耗时表 + 像素证据。
//!
//! ```bash
//! cargo run -p raybend --release --example local-tone-probe -- <照片> <输出目录>
//! cargo run -p raybend --example local-tone-probe                      # 只跑合成图
//! ```
//!
//! 这张表决定「参数一变就重算全尺寸」这条口径还站不站得住：
//! `analyze` 贵但**与强度无关**（可以常驻缓存），`apply` 便宜（拖动时每帧跑的那一段）。
//! 两者都必须在 24MP 上给出数字，不能靠估。

use std::path::Path;
use std::time::Instant;

use raybend::develop::local_tone::{LocalToneOpts, LocalToneState};
use raybend::develop::{CurveSet, DevelopParams, LinearImage, chain_image, render_rgb8};

/// 逆光合成图：左暗右亮 + 细纹理（与 `local_tone` 的单测同一套形状）。
fn synthetic(width: u32, height: u32) -> LinearImage {
    let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);
    for y in 0..height as usize {
        for x in 0..width as usize {
            #[allow(clippy::cast_precision_loss)]
            let t = x as f32 / (width as f32 - 1.0).max(1.0);
            let smooth = t * t * (3.0 - 2.0 * t);
            let stops = -7.5 + 7.0 * smooth;
            let checker: f32 = if (x + y) % 2 == 0 { 1.0 } else { -1.0 };
            let value = stops.exp2() * (0.2 * checker).exp2();
            let encoded = encoded_of(value);
            rgb.extend([encoded, encoded, encoded]);
        }
    }
    LinearImage::new(width, height, rgb).expect("形状对得上")
}

fn encoded_of(linear: f32) -> u16 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let value = (linear.clamp(0.0, 1.0) * 65535.0 + 0.5) as u32;
    u16::try_from(value.min(65535)).unwrap_or(u16::MAX)
}

fn linear_of(encoded: u16) -> f32 {
    f32::from(encoded) / 65535.0
}

fn luma_log2(image: &LinearImage, pixel_index: usize) -> f32 {
    let base = pixel_index * 3;
    let r = linear_of(image.rgb[base]);
    let g = linear_of(image.rgb[base + 1]);
    let b = linear_of(image.rgb[base + 2]);
    (0.2126 * r + 0.7152 * g + 0.0722 * b).max(1.0 / 65535.0).log2()
}

/// p02 / p98 的 log2 亮度（stops）——「光比」与「黑场」两个口径都用它。
fn percentiles(image: &LinearImage) -> (f32, f32) {
    let mut values: Vec<f32> = (0..image.rgb.len() / 3)
        .map(|index| luma_log2(image, index))
        .collect();
    values.sort_by(f32::total_cmp);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let low = values[(values.len() as f32 * 0.02) as usize];
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let high = values[values.len() - 1 - (values.len() as f32 * 0.02) as usize];
    (low, high)
}

fn millis(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    let path = std::env::args().nth(1);
    let out_dir = std::env::args().nth(2);
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    println!("可用并行度：{threads}");

    let params = DevelopParams::new(Some(5200.0));
    let curves = CurveSet::identity();

    // ── 耗时：合成图（尺寸对齐真实照片）────────────────────────
    println!("\n── 耗时（合成图）──");
    println!("{:<34} {:>12} {:>12}", "场景", "像素", "耗时 ms");
    let full = synthetic(6000, 4000);
    let preview = full.downscaled_to(1920);
    let analysis_source = full.downscaled_to(1500);

    let start = Instant::now();
    let analysis_chained = chain_image(&analysis_source, &params);
    println!(
        "{:<34} {:>12} {:>12.1}",
        "① 1/4 图（6000→1500）",
        analysis_source.rgb.len() / 3,
        millis(start)
    );

    let start = Instant::now();
    let state = LocalToneState::analyze(&analysis_chained, &LocalToneOpts::default());
    let analyze_ms = millis(start);
    println!(
        "{:<34} {:>12} {:>12.1}",
        "② analyze（与强度无关）",
        analysis_chained.rgb.len() / 3,
        analyze_ms
    );

    for (label, image) in [("③ apply 预览档 1920", &preview), ("④ apply 1:1 档 6000", &full)] {
        let mut target = image.clone();
        let start = Instant::now();
        state.apply_inplace(&mut target, 1.0);
        println!(
            "{:<34} {:>12} {:>12.1}",
            label,
            image.rgb.len() / 3,
            millis(start)
        );
    }

    // ── 画质：光比与局部反差 ───────────────────────────────
    println!("\n── 效果（合成图 6000×4000 的预览档）──");
    let (source_low, source_high) = percentiles(&preview);
    println!(
        "{:<10} {:>12} {:>12} {:>10} {:>12}",
        "强度", "暗部 p02", "亮部 p98", "光比 stops", "相对原始"
    );
    let before_stops = source_high - source_low;
    for strength in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
        let mut image = preview.clone();
        state.apply_inplace(&mut image, strength);
        let (low, high) = percentiles(&image);
        println!(
            "{:<10.2} {:>12.4} {:>12.4} {:>10.2} {:>11.0}%",
            strength,
            low,
            high,
            high - low,
            (high - low) / before_stops * 100.0
        );
    }
    println!(
        "（暗部 p02 是**被抬起来**的 —— 这正是「降光比」的一半；另一半是亮部被压住）"
    );

    // ── 真照片 ──────────────────────────────────────────
    let Some(path) = path else {
        println!("\n（没给照片路径，跳过真照片那一段）");
        return;
    };
    let path = Path::new(&path);
    if !raybend::display::pixels::is_raw_photo(path) {
        println!("\n不是 RAW（本探针只跑 RAW 那条线性路）：{}", path.display());
        return;
    }
    println!("\n── 真照片：{} ──", path.display());
    let request = raybend::raw::backend::DecodeRequest::full(path);
    let start = Instant::now();
    let worker = raybend::raw::worker::shared();
    let result = worker.lock().expect("worker 锁").decode_linear(&request);
    let decoded = match result {
        Ok(decoded) => decoded,
        Err(error) => {
            println!("线性解码失败：{error}");
            return;
        }
    };
    println!(
        "线性解码 {}×{}：{:.0} ms（拍摄色温 {:?}）",
        decoded.width,
        decoded.height,
        millis(start),
        decoded.as_shot_temperature.map(|k| format!("{k:.0}K"))
    );
    let source = LinearImage::new(decoded.width, decoded.height, decoded.rgb).expect("形状对");
    let params = DevelopParams::new(decoded.as_shot_temperature);

    // 分析图：长边缩到全图的 1/4（生产路径就是这么干的）
    let long = decoded.width.max(decoded.height);
    let start = Instant::now();
    let analysis_source = source.downscaled_to((long / 4).max(256));
    let analysis_chained = chain_image(&analysis_source, &params);
    println!(
        "分析图 {}×{} + 线性链：{:.0} ms",
        analysis_source.width,
        analysis_source.height,
        millis(start)
    );
    let start = Instant::now();
    let state = LocalToneState::analyze(&analysis_chained, &LocalToneOpts::default());
    println!("analyze：{:.0} ms", millis(start));
    let (low, high) = state.base_range();
    println!("base 范围（log2）：{low:.2} … {high:.2} = {:.2} stops", high - low);

    let preview = source.downscaled_to(1920);
    let (source_low, source_high) = percentiles(&preview);
    println!(
        "\n{:<10} {:>12} {:>12} {:>10} {:>11}",
        "强度", "暗部 p02", "亮部 p98", "光比 stops", "相对原始"
    );
    for strength in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
        let mut image = preview.clone();
        state.apply_inplace(&mut image, strength);
        let (low, high) = percentiles(&image);
        println!(
            "{:<10.2} {:>12.4} {:>12.4} {:>10.2} {:>10.0}%",
            strength,
            low,
            high,
            high - low,
            (high - low) / (source_high - source_low) * 100.0
        );
    }
    // 耗时（真照片）
    for (label, strength) in [("apply 预览档 · 强度 1.0", 1.0f32), ("apply 1:1 档 · 强度 1.0", 1.0)] {
        let image = if label.contains("1:1") { &source } else { &preview };
        let mut target = image.clone();
        let start = Instant::now();
        state.apply_inplace(&mut target, strength);
        println!(
            "{label}（{}×{}）：{:.0} ms",
            image.width,
            image.height,
            millis(start)
        );
    }

    // 像素证据
    if let Some(dir) = out_dir {
        let dir = Path::new(&dir);
        let render = |name: &str, strength: f32| {
            let mut image = preview.clone();
            state.apply_inplace(&mut image, strength);
            let out = render_rgb8(&image, &params, &curves);
            let buffer =
                image::RgbImage::from_raw(preview.width, preview.height, out).expect("尺寸对");
            let target = dir.join(name);
            buffer.save(&target).expect("写 PNG");
            println!("像素证据：{}", target.display());
        };
        render("local-tone-000.png", 0.0);
        render("local-tone-050.png", 0.5);
        render("local-tone-100.png", 1.0);
    }
}
