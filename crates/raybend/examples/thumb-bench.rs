//! 缩略图吞吐实测：在真实照片目录上跑完整管线，按阶段报耗时。
//!
//! ```bash
//! cargo run -q --release -p raybend --example thumb-bench -- /mnt/c/src/tmp/pic
//! cargo run -q --release -p raybend --example thumb-bench -- /mnt/c/src/tmp/pic --limit 50
//! ```
//!
//! 报告内容与它的用途：
//!
//! * **分阶段耗时**（读文件 / 解码 / 摆正 / 缩放 / 编码）—— 决定优化方向；
//! * **P50 / P95 单张耗时** —— 平均值会骗人，尾巴才决定体感；
//! * **缩放算法对比**（Lanczos3 / 三角 / 盒式）—— 给「质量 vs 速度」的取舍提供数据；
//! * **缓存写读吞吐** —— 确认 SQLite BLOB 这条路不是瓶颈；
//! * **RAW 路径** —— RAW 走 [`raybend::raw`] 的 worker（内嵌预览优先）；
//!   只有「真解不开」才落到程序画的占位图。
//!
//! ⚠️ 在 WSL 下读 `/mnt/c` 走 9p，比 Windows 本地盘慢得多。所以**绝对值只能当
//! 上界参考**，主要看「阶段占比」与「算法之间的相对差距」。

use std::path::PathBuf;
use std::time::{Duration, Instant};

use image::imageops::FilterType;
use raybend::media::kind::MediaKind;
use raybend::media::scan::{self, Cancel, ScanEvent, ScanOptions};
use raybend::store::file_id::FileId;
use raybend::thumbnail::cache::{self, ThumbsDb};
use raybend::thumbnail::render::{self, SizeClass};

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(root) = args.next().map(PathBuf::from) else {
        eprintln!("用法：thumb-bench <照片目录> [--limit N]");
        std::process::exit(2);
    };
    let flags: Vec<String> = args.collect();
    let limit: usize = flags
        .iter()
        .position(|a| a == "--limit")
        .and_then(|i| flags.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);

    // ① 扫出照片
    let mut files = Vec::new();
    scan::scan(&root, &ScanOptions::default(), &Cancel::new(), |ev| {
        if let ScanEvent::File(f) = ev {
            files.push(f);
        }
        Ok(())
    })
    .expect("扫描失败");
    files.truncate(limit);

    let jpgs: Vec<_> = files
        .iter()
        .filter(|f| f.kind == MediaKind::Image)
        .collect();
    let raws: Vec<_> = files.iter().filter(|f| f.kind == MediaKind::Raw).collect();
    println!(
        "照片      {} 张（位图 {} · RAW {}）",
        files.len(),
        jpgs.len(),
        raws.len()
    );

    let tmp = tempfile::tempdir().expect("临时目录");
    let thumbs = ThumbsDb::open(tmp.path().join("cache"), 0).expect("打开缓存库");

    // ② 完整管线（走 produce：含缓存命中判断与写入）
    let mut times: Vec<Duration> = Vec::new();
    let mut total_in: u64 = 0;
    let mut rendered = 0usize;
    let mut placeholders = 0usize;

    let started = Instant::now();
    for f in &files {
        let job = raybend::thumbnail::ThumbJob::new("bench", &f.rel_path, SizeClass::Grid);
        let abs = root.join(&f.rel_path);
        let root_owned = root.clone();
        let t = Instant::now();
        let outcome = thumbs
            .write(move |conn| {
                let _ = &root_owned;
                raybend::thumbnail::worker::produce(conn, &root_owned, &job, 0)
            })
            .expect("渲染失败");
        times.push(t.elapsed());
        total_in += f.size_bytes;
        match outcome {
            raybend::thumbnail::Outcome::Rendered => rendered += 1,
            raybend::thumbnail::Outcome::Placeholder => placeholders += 1,
            raybend::thumbnail::Outcome::Cached => {}
        }
        let _ = abs;
    }
    let wall = started.elapsed();

    let stats = thumbs.read(cache::stats).unwrap();
    let total_out = stats.bytes.max(0) as u64;

    let mut sorted = times.clone();
    sorted.sort();
    println!(
        "\n完整管线（grid 384，JPEG q82）\n  渲染 {} · 占位 {} · 总计 {:.1} 秒",
        rendered,
        placeholders,
        wall.as_secs_f64()
    );
    println!(
        "  吞吐 {:.1} 张/秒 · 平均 {:.1} ms · P50 {:.1} ms · P95 {:.1} ms · 最慢 {:.0} ms",
        files.len() as f64 / wall.as_secs_f64().max(1e-9),
        ms(wall) / files.len().max(1) as f64,
        ms(percentile(&sorted, 0.50)),
        ms(percentile(&sorted, 0.95)),
        ms(*sorted.last().unwrap_or(&Duration::ZERO)),
    );
    println!(
        "  输入 {:.1} MB → 缓存 {:.1} MB（{:.0} KB/张，压缩比 {:.1}%）",
        total_in as f64 / 1e6,
        total_out as f64 / 1e6,
        stats.bytes as f64 / stats.entries.max(1) as f64 / 1024.0,
        stats.bytes as f64 / total_in.max(1) as f64 * 100.0
    );

    // ③ 缓存命中速度（第二遍应当几乎不耗时）
    let t = Instant::now();
    for f in &files {
        let job = raybend::thumbnail::ThumbJob::new("bench", &f.rel_path, SizeClass::Grid);
        let root_owned = root.clone();
        let _ = thumbs
            .write(move |conn| raybend::thumbnail::worker::produce(conn, &root_owned, &job, 1));
    }
    let hit_wall = t.elapsed();
    println!(
        "\n缓存命中路径\n  {} 张 {:.1} ms（{:.2} ms/张）",
        files.len(),
        ms(hit_wall),
        ms(hit_wall) / files.len().max(1) as f64
    );

    // ④ 分阶段耗时 + 算法对比（只在位图上做：RAW 的解码在 worker 进程里，阶段拆不开）
    let sample: Vec<_> = jpgs.iter().take(30).collect();
    if sample.is_empty() {
        println!("\n（没有位图，跳过分阶段与算法对比）");
        return;
    }
    let mut read_t = Duration::ZERO;
    let mut decode_t = Duration::ZERO;
    let mut filter_times: Vec<(&str, Duration)> = vec![
        ("Lanczos3", Duration::ZERO),
        ("两段式", Duration::ZERO),
        ("Triangle", Duration::ZERO),
        ("Nearest", Duration::ZERO),
    ];
    let mut encode_t = Duration::ZERO;
    let mut boxes = Vec::new();

    for f in &sample {
        let abs = root.join(&f.rel_path);
        let t = Instant::now();
        let bytes = std::fs::read(&abs).unwrap_or_default();
        read_t += t.elapsed();

        let t = Instant::now();
        let Ok(img) = image::load_from_memory(&bytes) else {
            continue;
        };
        decode_t += t.elapsed();

        for (name, acc) in filter_times.iter_mut() {
            let t = Instant::now();
            let resized = match *name {
                "Lanczos3" => img.resize(384, 384, FilterType::Lanczos3),
                "两段式" => render::resize_for_thumb(img.clone(), 384),
                "Triangle" => img.resize(384, 384, FilterType::Triangle),
                _ => img.clone().thumbnail(384, 384),
            };
            let d = t.elapsed();
            *acc += d;
            if *name == "两段式" {
                boxes.push(render::encode(resized, SizeClass::Grid, None, false, None).unwrap());
            }
        }

        // 真正编码一遍（之前只测了构造编码器，等于没测）
        let t = Instant::now();
        let resized = img.resize(384, 384, FilterType::Lanczos3);
        let rgb = resized.to_rgb8();
        let mut out = Vec::new();
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 82);
        enc.encode_image(&rgb).unwrap();
        encode_t += t.elapsed();
    }

    let n = sample.len().max(1) as f64;
    println!("\n分阶段（位图 {} 张，平均每张）", sample.len());
    println!("  读文件 {:.2} ms（含 9p 的开销）", ms(read_t) / n);
    println!("  解码   {:.2} ms", ms(decode_t) / n);
    println!("  编码   {:.2} ms（JPEG q82）", ms(encode_t) / n);
    println!("  缩放对比（同一批图，长边缩到 384；默认走的就是「两段式」）：");
    for (name, d) in &filter_times {
        println!("    {name:<9} {:.2} ms", ms(*d) / n);
    }
    if let Some(b) = boxes.first() {
        println!(
            "  两段式产出的缩略图平均 {:.0} KB（{}×{}）",
            b.data.len() as f64 / 1024.0,
            b.width,
            b.height
        );
    }

    // ⑤ 占位图（真解不开时的兜底路径）
    let t = Instant::now();
    for _ in 0..raws.len().max(1) {
        let _ = render::placeholder(MediaKind::Raw, SizeClass::Grid).unwrap();
    }
    println!(
        "\n占位图（解不开时的兜底）\n  {} 张 {:.0} ms（{:.3} ms/张）—— 只是画图 + 编码，没有解码",
        raws.len(),
        ms(t.elapsed()),
        ms(t.elapsed()) / raws.len().max(1) as f64
    );

    // ⑥ 缓存库体量
    println!(
        "\n缓存库\n  条目 {} · {:.1} MB（{} 尺度分布：{:?}）",
        stats.entries,
        stats.bytes as f64 / 1e6,
        stats.by_size.len(),
        stats.by_size
    );
    let _ = FileId::try_read(&root);
}
