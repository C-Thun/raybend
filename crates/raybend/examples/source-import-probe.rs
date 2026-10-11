//! **临时诊断探针**（2026-10-11，import tiles 慢）：在**真机**上量 import 数据路径每一段的耗时。
//!
//! ```bash
//! source-import-probe <照片目录> <缓存目录> [--limit N] [--hits <已有缓存目录>] [--raw-only]
//! ```
//!
//! 量的四段（与 `ImportWorkspace` 真实调用一一对应）：
//!
//! 1. `scan_photos`（列表阶段，进目录必走）；
//! 2. **读文件信息**（`MetaCache::ensure_dir` + `read_photo_meta`：宽高/方向**和拍摄时间**
//!    一次读头拿回，未命中项并行读 6 线程）—— 2026-10-11 起时间不再有单独的命令；
//! 3. `render_now` 冷/热两遍（每张缩略图的真实代价 + 缓存是否命中）；
//! 4. `--hits`：拿一份**已有的** sources 缓存库，看当前 `render_sig` 的命中率
//!    （回答「为什么每次都要重渲染」）。
//!
//! 还会做一次并发对照：4 线程同时跑 RAW 与单线程跑同样的张数，判断 RAW 那条路
//! 是不是被 worker 串行化了。
//!
//! 这不是产品代码，也不进测试；诊断做完即删。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use image::ImageEncoder;

use raybend::media::source;
use raybend::thumbnail::cache::{self, ThumbsDb};
use raybend::thumbnail::render::render_sig;
use raybend::thumbnail::worker::cache_key_for;
use raybend::thumbnail::{SizeClass, render_now};

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// 按场景选口径：库内 `render_now`（AVIF）/ 库外 `render_source_now`（JPEG）。
/// 按场景选口径：库内 `render_now`（AVIF + 磁盘缓存）/ 库外 `render_source`（JPEG + **只在内存**）。
fn render_one(
    db: &ThumbsDb,
    memory: &raybend::thumbnail::memory::SourceThumbs,
    path: &std::path::Path,
    size: SizeClass,
    now: i64,
    source: bool,
) -> raybend::Result<Vec<u8>> {
    if source {
        let key = raybend::thumbnail::memory::SourceKey::of(
            path,
            size,
            raybend::thumbnail::render::render_sig_source(size),
        );
        memory
            .get_or_render(&key, || raybend::thumbnail::worker::render_source(path, size))
            .map(|thumb| thumb.data)
    } else {
        render_now(db, path, size, now)
    }
}

/// 「一屏」的真实等待：24 张照片，4 路并发（前端队列的并发数），全新缓存。
fn screen_bench(all: &[PathBuf], cache_dir: &std::path::Path, now: i64, source_mode: bool) {
    for (label, sample) in [
        (
            "24 张 JPG",
            all.iter()
                .filter(|p| {
                    p.extension()
                        .is_some_and(|e| matches!(e.to_string_lossy().to_lowercase().as_str(), "jpg" | "jpeg"))
                })
                .take(24)
                .cloned()
                .collect::<Vec<_>>(),
        ),
        (
            "24 张 RAW",
            all.iter()
                .filter(|p| {
                    p.extension()
                        .is_some_and(|e| matches!(e.to_string_lossy().to_lowercase().as_str(), "rw2" | "orf" | "nef" | "cr2" | "arw"))
                })
                .take(24)
                .cloned()
                .collect::<Vec<_>>(),
        ),
    ] {
        if sample.is_empty() {
            continue;
        }
        let dir = cache_dir.join("screen");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = ThumbsDb::open(&dir, now).unwrap();
        // 库外那份缓存按口径新建一份「空内存」——进程内缓存，冷启动就该是空的
        let memory = raybend::thumbnail::memory::SourceThumbs::new();
        let index = AtomicUsize::new(0);
        let started = Instant::now();
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let sample = &sample;
                let db = &db;
                let memory = &memory;
                let index = &index;
                scope.spawn(move || {
                    loop {
                        let at = index.fetch_add(1, Ordering::Relaxed);
                        let Some(path) = sample.get(at) else { break };
                        let _ = render_one(db, memory, path, SizeClass::Grid, now, source_mode);
                    }
                });
            }
        });
        println!(
            "{label}{}（4 路并发、全新缓存）：{:.0}ms —— 一屏要等这么久",
            if source_mode { " · 库外口径（JPEG）" } else { " · 库内口径（AVIF）" },
            ms(started.elapsed())
        );
    }
}

/// 选中一张时要读的 EXIF 到底多贵（回答「放队列里还是选中时拿」）。
fn exif_cost(items: &[raybend::media::source::SourceItem]) {
    println!("\n== EXIF 成本（选中一张才会走）");
    let mut open = 0.0f64;
    let mut fields = 0.0f64;
    let mut lens = 0.0f64;
    let mut lens_calls = 0usize;
    let mut counted = 0usize;
    for item in items.iter().take(40) {
        if counted >= 12 {
            break;
        }
        counted += 1;
        let path = item.path.as_path();
        let started = Instant::now();
        let data = raybend::media::exif::read_file_for(path);
        open += ms(started.elapsed());
        let started = Instant::now();
        let _ = raybend::media::exif::read_fields_for(path);
        fields += ms(started.elapsed());
        let is_raw = item.kind == raybend::media::kind::MediaKind::Raw;
        if is_raw && data.lens.as_ref().is_none_or(|name| name.trim().is_empty()) {
            let started = Instant::now();
            let _permit = raybend::raw::worker::acquire(raybend::raw::DecodePriority::Low);
            let _ = raybend::raw::worker::shared()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .lens_name(path);
            lens += ms(started.elapsed());
            lens_calls += 1;
        }
    }
    println!("  读 EXIF（头）  {:.2} ms/张（{counted} 张）", open / counted as f64);
    println!("  全部标签       {:.2} ms/张", fields / counted as f64);
    if lens_calls > 0 {
        println!(
            "  RAW 补镜头名   {:.0} ms/次（{lens_calls} 次，走 worker 的 Low 档）",
            lens / lens_calls as f64
        );
    }
}

/// 进 view 看的那张（screen 1920）：来源文件用 AVIF 还是 JPEG。
fn view_cost(items: &[raybend::media::source::SourceItem]) {
    use raybend::thumbnail::render::{EncodeFormat, render_file_with_edit_and_lut};
    println!("\n== 进 view（screen 1920）成本：来源文件的两种口径");
    for (label, kind) in [
        ("JPG", raybend::media::kind::MediaKind::Image),
        ("RAW", raybend::media::kind::MediaKind::Raw),
    ] {
        let sample: Vec<_> = items.iter().filter(|i| i.kind == kind).take(2).collect();
        if sample.is_empty() {
            continue;
        }
        for (name, format) in [
            ("avif q90（现状）", EncodeFormat::Avif),
            ("jpeg q92（库外口径）", EncodeFormat::Jpeg),
        ] {
            let mut total = 0.0;
            for item in &sample {
                let started = Instant::now();
                let _ = render_file_with_edit_and_lut(
                    &item.path,
                    SizeClass::Screen,
                    None,
                    None,
                    None,
                    format,
                );
                total += ms(started.elapsed());
            }
            println!(
                "  {label} {name:<20} {:.0} ms/张",
                total / sample.len() as f64
            );
        }
    }
}

/// 同一批图、同一档尺寸，换编码器设置比一比。
fn encode_matrix(jpgs: &[PathBuf]) {
    println!("\n== 编码矩阵（{} 张 JPG，长边 384）", jpgs.len());
    let mut rows: Vec<(String, f64, usize, usize)> = Vec::new();
    for path in jpgs {
        let Ok(bytes) = std::fs::read(path) else { continue };
        let Ok(img) = image::load_from_memory(&bytes) else { continue };
        let img = raybend::thumbnail::render::resize_for_thumb(img, 384);
        let rgb = img.to_rgb8();
        let (w, h) = rgb.dimensions();
        let raw = rgb.as_raw();

        let cases: [(&str, u8, Option<usize>, u8); 4] = [
            ("avif q90 speed10 threads=1（现状）", 10, Some(1), 90),
            ("avif q90 speed10 threads=auto", 10, None, 90),
            ("avif q80 speed10 threads=auto", 10, None, 80),
            ("avif q70 speed10 threads=auto", 10, None, 70),
        ];
        // 每档跑两轮：第一轮含 rav1e 的初始化（线程池 / CPU 特性探测），第二轮才是稳态
        for round in 0..2 {
            for (label, speed, threads, quality) in cases {
                let label = if round == 0 {
                    label.to_string()
                } else {
                    format!("{label} · 第二轮")
                };
                let started = Instant::now();
                let mut out = Vec::new();
                {
                    let mut encoder = image::codecs::avif::AvifEncoder::new_with_speed_quality(
                        &mut out,
                        speed,
                        quality,
                    )
                    .with_num_threads(threads);
                    let _ = encoder.write_image(raw, w, h, image::ExtendedColorType::Rgb8);
                }
                let elapsed = ms(started.elapsed());
                match rows.iter_mut().find(|(name, ..)| *name == label) {
                    Some((_, sum, count, size)) => {
                        *sum += elapsed;
                        *count += 1;
                        *size += out.len();
                    }
                    None => rows.push((label.clone(), elapsed, 1, out.len())),
                }
            }
        }
        let started = Instant::now();
        let mut out = Vec::new();
        {
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 82);
            let _ = encoder.encode_image(&rgb);
        }
        let elapsed = ms(started.elapsed());
        match rows.iter_mut().find(|(name, ..)| name == "jpeg q82（旧口径）") {
            Some((_, sum, count, size)) => {
                *sum += elapsed;
                *count += 1;
                *size += out.len();
            }
            None => rows.push(("jpeg q82（旧口径）".to_string(), elapsed, 1, out.len())),
        }
    }
    for (label, sum, count, size) in &rows {
        println!(
            "  {label:<38} {:>7.1}ms/张  {:>6.1} KB（{count} 张）",
            sum / *count as f64,
            *size as f64 / *count as f64 / 1024.0
        );
    }
}

/// 4 个并发编码（= 前端队列的并发数）下，两种线程设置的实际墙钟。
fn encode_throughput(jpgs: &[PathBuf]) {
    println!("\n== 4 路并发编码（队列并发数）");
    let prepared: Vec<(Vec<u8>, u32, u32)> = jpgs
        .iter()
        .filter_map(|path| {
            let bytes = std::fs::read(path).ok()?;
            let img = image::load_from_memory(&bytes).ok()?;
            let img = raybend::thumbnail::render::resize_for_thumb(img, 384);
            let rgb = img.to_rgb8();
            let (w, h) = rgb.dimensions();
            Some((rgb.into_raw(), w, h))
        })
        .collect();
    let work: Vec<(Vec<u8>, u32, u32)> = prepared.iter().cycle().take(8).cloned().collect();
    if work.is_empty() {
        return;
    }
    for (label, threads) in [
        ("threads=1（现状）", Some(1usize)),
        ("threads=auto", None),
    ] {
        let index = AtomicUsize::new(0);
        let started = Instant::now();
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let work = &work;
                let index = &index;
                scope.spawn(move || {
                    loop {
                        let at = index.fetch_add(1, Ordering::Relaxed);
                        let Some((pixels, w, h)) = work.get(at) else {
                            break;
                        };
                        let mut out = Vec::new();
                        let mut encoder =
                            image::codecs::avif::AvifEncoder::new_with_speed_quality(&mut out, 10, 90)
                                .with_num_threads(threads);
                        let _ = encoder.write_image(pixels, *w, *h, image::ExtendedColorType::Rgb8);
                    }
                });
            }
        });
        println!(
            "  {label:<20} {} 张墙钟 {:.0}ms（{:.1} 张/秒）",
            work.len(),
            ms(started.elapsed()),
            work.len() as f64 / started.elapsed().as_secs_f64()
        );
    }
}

/// RAW 那条路是不是被 worker 串行化了：同一批文件，「一张接一张」对「4 线程同时」。
fn raw_concurrency(cache_dir: &std::path::Path, raws: &[PathBuf], now: i64) {
    let raws: Vec<PathBuf> = raws.iter().take(4).cloned().collect();
    if raws.is_empty() {
        println!("没有 RAW 可测");
        return;
    }
    println!("\n== RAW 并发对照（{} 张，Grid 档，全新缓存）", raws.len());

    let seq_dir = cache_dir.join("seq");
    let _ = std::fs::remove_dir_all(&seq_dir);
    std::fs::create_dir_all(&seq_dir).unwrap();
    let seq_db = ThumbsDb::open(&seq_dir, now).unwrap();
    let started = Instant::now();
    for path in &raws {
        let _ = render_now(&seq_db, path, SizeClass::Grid, now);
    }
    let seq = ms(started.elapsed());

    let par_dir = cache_dir.join("par");
    let _ = std::fs::remove_dir_all(&par_dir);
    std::fs::create_dir_all(&par_dir).unwrap();
    let par_db = ThumbsDb::open(&par_dir, now).unwrap();
    let started = Instant::now();
    std::thread::scope(|scope| {
        for path in &raws {
            let db = &par_db;
            scope.spawn(move || {
                let _ = render_now(db, path, SizeClass::Grid, now);
            });
        }
    });
    let par = ms(started.elapsed());
    println!("  串行（1 张接 1 张）：{seq:.1}ms");
    println!("  4 线程同时：{par:.1}ms");
    println!(
        "  → {}",
        if par < seq * 0.6 {
            "并行有效"
        } else {
            "被串行化（几乎等于串行）"
        }
    );
}

fn kind_of(path: &std::path::Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| "(none)".into())
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(root) = args.next().map(PathBuf::from) else {
        eprintln!("用法：source-import-probe <照片目录> <缓存目录> [--limit N] [--hits 目录] [--raw-only]");
        std::process::exit(2);
    };
    let Some(cache_dir) = args.next().map(PathBuf::from) else {
        eprintln!("缺缓存目录");
        std::process::exit(2);
    };
    let flags: Vec<String> = args.collect();
    let flag = |name: &str| -> Option<String> {
        flags
            .iter()
            .position(|a| a == name)
            .and_then(|i| flags.get(i + 1))
            .cloned()
    };
    let limit: usize = flag("--limit").and_then(|v| v.parse().ok()).unwrap_or(12);
    let hits_dir = flag("--hits").map(PathBuf::from);
    let raw_only = flags.iter().any(|a| a == "--raw-only");
    let encode_bench = flags.iter().any(|a| a == "--encode-bench");
    let source_mode = flags.iter().any(|a| a == "--source");
    let raw_conc = flags.iter().any(|a| a == "--raw-concurrency");
    let now = 1_789_516_800_000i64;

    println!("== 目录：{}", root.display());

    // ① 列表阶段
    let started = Instant::now();
    let listing = source::scan_photos(&root, false).expect("扫描失败");
    println!(
        "① scan_photos：{} 张，{:.1}ms（{:.2}ms/张）",
        listing.len(),
        ms(started.elapsed()),
        ms(started.elapsed()) / listing.len().max(1) as f64
    );

    let all: Vec<PathBuf> = listing.items.iter().map(|i| i.path.clone()).collect();
    let raws: Vec<PathBuf> = listing
        .items
        .iter()
        .filter(|i| i.kind == raybend::media::kind::MediaKind::Raw)
        .map(|i| i.path.clone())
        .take(limit)
        .collect();
    let jpgs: Vec<PathBuf> = listing
        .items
        .iter()
        .filter(|i| i.kind == raybend::media::kind::MediaKind::Image)
        .map(|i| i.path.clone())
        .take(limit)
        .collect();

    // ② 读文件信息（宽高/方向 + 拍摄时间，一次读头；未命中项并行读）
    let mut meta_cache = raybend::media::meta_cache::MetaCache::new();
    let inputs: Vec<raybend::media::meta_cache::MetaInput> = listing
        .items
        .iter()
        .map(|item| raybend::media::meta_cache::MetaInput {
            relative: item.file_name.clone(),
            file_size: item.size_bytes,
            mtime_ms: item.mtime_ms.unwrap_or(0),
        })
        .collect();
    let started = Instant::now();
    let batch = meta_cache
        .ensure_dir(
            Path::new(&root),
            &inputs,
            Instant::now(),
            raybend::media::meta::read_photo_meta,
        )
        .expect("读文件信息");
    println!(
        "② 读文件信息（宽高/方向 + 拍摄时间，并行）：{} 张（读 {} / 命中 {}），{:.1}ms（{:.2}ms/张）",
        all.len(),
        batch.read_count,
        batch.cached_count,
        ms(started.elapsed()),
        ms(started.elapsed()) / all.len().max(1) as f64
    );
    println!(
        "   其中读到拍摄时间 {} 张（来源 exif 的 {} 张）",
        batch.metas.iter().filter(|m| m.taken_at_ms.is_some()).count(),
        batch
            .metas
            .iter()
            .filter(|m| m.taken_at_source == Some(raybend::media::exif::TakenAtSource::Exif))
            .count()
    );

    if flags.iter().any(|a| a == "--exif") {
        exif_cost(&listing.items);
        return;
    }
    if flags.iter().any(|a| a == "--view") {
        view_cost(&listing.items);
        return;
    }
    if encode_bench {
        encode_matrix(&jpgs);
        encode_throughput(&jpgs);
        return;
    }
    if raw_conc {
        raw_concurrency(&cache_dir, &raws, now);
        return;
    }
    if flags.iter().any(|a| a == "--screen") {
        screen_bench(
            &listing.items.iter().map(|i| i.path.clone()).collect::<Vec<_>>(),
            &cache_dir,
            now,
            source_mode,
        );
        return;
    }
    if raw_only {
        println!("\n（--raw-only：跳过 ③④）");
        return;
    }

    // ③ 缩略图冷/热两遍
    std::fs::create_dir_all(&cache_dir).expect("建缓存目录失败");
    let db = ThumbsDb::open(&cache_dir, now).expect("打开缓存失败");
    // 库外那份住内存（`AGENTS.md` §2.24）
    let memory = raybend::thumbnail::memory::SourceThumbs::new();
    println!("\n== 缓存库：{}", cache_dir.display());
    for (label, paths) in [("JPG", &jpgs), ("RAW", &raws)] {
        if paths.is_empty() {
            continue;
        }
        println!("- {label}：{} 张", paths.len());
        let mut cold = Vec::new();
        let mut kinds = Vec::new();
        for path in paths {
            let started = Instant::now();
            let bytes = render_one(&db, &memory, path, SizeClass::Grid, now, source_mode).expect("渲染失败");
            cold.push(ms(started.elapsed()));
            kinds.push((kind_of(path), bytes.len()));
        }
        for (index, value) in cold.iter().enumerate() {
            println!(
                "    #{} {:>9.1}ms  {:>7} B  {}",
                index,
                value,
                kinds[index].1,
                kinds[index].0
            );
        }
        let sum: f64 = cold.iter().sum();
        println!(
            "    冷渲染合计 {:.1}ms，平均 {:.1}ms/张",
            sum,
            sum / cold.len() as f64
        );

        let started = Instant::now();
        for path in paths {
            let _ = render_one(&db, &memory, path, SizeClass::Grid, now, source_mode).expect("渲染失败");
        }
        println!(
            "    再取一遍（应全部命中）：{:.1}ms（{:.2}ms/张）",
            ms(started.elapsed()),
            ms(started.elapsed()) / paths.len() as f64
        );
    }

    // ④ 并发对照：RAW 是不是被串行化
    if raws.len() >= 2 {
        let pool: Vec<PathBuf> = raws.clone();
        let started = Instant::now();
        let index = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let pool = &pool;
                let db = &db;
                let memory = &memory;
                let index = &index;
                scope.spawn(move || {
                    loop {
                        let at = index.fetch_add(1, Ordering::Relaxed);
                        if at >= pool.len() * 2 {
                            break;
                        }
                        let Some(path) = pool.get(at % pool.len()) else {
                            break;
                        };
                        // 按口径走（库外走内存，不碰这个 db）；以前这里写死 `render_now`，
                        // 于是「--source」跑完也会在缓存库里留下一堆行（误导过一轮读数）
                        let _ = render_one(db, memory, path, SizeClass::Screen, now, source_mode);
                    }
                });
            }
        });
        println!(
            "- 4 线程争用（{} 张 RAW × 2，Screen 档，全部新渲染）：{:.1}ms 墙钟",
            pool.len(),
            ms(started.elapsed())
        );
    }

    // ⑤ 命中率：拿已有的缓存库问「当前签名下有没有」
    if let Some(existing) = hits_dir {
        let existing = std::fs::canonicalize(&existing).unwrap_or(existing);
        println!("\n== 命中率（已有缓存库 {}）", existing.display());
        let old = ThumbsDb::open(&existing, now).expect("打开已有缓存失败");
        let mut has_grid = 0usize;
        let mut has_screen = 0usize;
        let mut total = 0usize;
        for item in &listing.items {
            let key = cache_key_for(&item.path, &item.path.to_string_lossy());
            let grid = old
                .read(|conn| cache::has(conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid)))
                .unwrap_or(false);
            let screen = old
                .read(|conn| cache::has(conn, &key, SizeClass::Screen, render_sig(SizeClass::Screen)))
                .unwrap_or(false);
            if grid {
                has_grid += 1;
            }
            if screen {
                has_screen += 1;
            }
            total += 1;
        }
        println!(
            "当前签名（{} / {}）命中：grid {has_grid}/{total}，screen {has_screen}/{total}",
            render_sig(SizeClass::Grid),
            render_sig(SizeClass::Screen)
        );
    }
}
