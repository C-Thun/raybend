//! 导入工作区的**数据路径**实测：目录枚列 → 照片清单 → 补读时间 → 现场出缩略图。
//!
//! ```bash
//! cargo run -q --release -p raybend --example source-grid-smoke -- /mnt/c/src/tmp/pic
//! cargo run -q --release -p raybend --example source-grid-smoke -- /mnt/c/src/tmp/pic --thumbs 24
//! ```
//!
//! 它跑的是 M1-5 网格**真正会调的那几个函数**（`media::source` + `thumbnail::render_now`），
//! 只是把 Tauri 的 IPC 与前端渲染换成了打印。用途有两个：
//!
//! * 在真实照片上确认「路径能跑通、数字对得上」（Agent 的冒烟职责）；
//! * 顺手量一下耗时，给「进一个大目录要等多久」提供真实数据。
//!
//! ⚠️ WSL 下读 `/mnt/c` 走 9p，比 Windows 本地盘慢得多 —— **绝对值只能当上界参考**。

use std::path::PathBuf;
use std::time::Instant;

use raybend::media::exif::TakenAtSource;
use raybend::media::source;
use raybend::store::volumes;
use raybend::thumbnail::{SizeClass, ThumbsDb, render_now};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(root) = args.next().map(PathBuf::from) else {
        eprintln!("用法：source-grid-smoke <目录> [--thumbs N]");
        std::process::exit(2);
    };
    let flags: Vec<String> = args.collect();
    let thumbs: usize = flags
        .iter()
        .position(|a| a == "--thumbs")
        .and_then(|i| flags.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(12);

    println!("目录：{}", root.display());

    // ── ① 驱动器枚列（左列第一层）──────────────────────────────
    let started = Instant::now();
    let volumes = volumes::list();
    println!(
        "驱动器：{} 个（{:.1}ms）",
        volumes.len(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    for volume in volumes.iter().take(6) {
        println!("  {:<28} {}", volume.path, volume.kind.label());
    }

    // ── ② 目录树的一层（只列目录）──────────────────────────────
    let started = Instant::now();
    match source::list_dirs(&root) {
        Ok(dirs) => println!(
            "子目录：{} 个（{:.1}ms）",
            dirs.len(),
            started.elapsed().as_secs_f64() * 1000.0
        ),
        Err(e) => println!("子目录：读不了（{e}）"),
    }

    // ── ③ 照片清单（中列；不下钻子目录）────────────────────────
    let started = Instant::now();
    let listing = match source::scan_photos(&root, false) {
        Ok(listing) => listing,
        Err(e) => {
            eprintln!("扫描失败：{e}");
            std::process::exit(1);
        }
    };
    let scan_ms = started.elapsed().as_secs_f64() * 1000.0;
    println!(
        "照片：{} 张，跳过 {}，问题 {}（{:.1}ms）",
        listing.len(),
        listing.skipped,
        listing.problems.len(),
        scan_ms
    );

    let mut by_source: Vec<(TakenAtSource, usize)> = Vec::new();
    let mut total_bytes: u64 = 0;
    for item in &listing.items {
        total_bytes += item.size_bytes;
        if let Some(taken) = item.taken_at {
            match by_source.iter_mut().find(|(s, _)| *s == taken.source) {
                Some((_, count)) => *count += 1,
                None => by_source.push((taken.source, 1)),
            }
        }
    }
    println!(
        "总大小：{:.1} GB",
        total_bytes as f64 / 1024.0 / 1024.0 / 1024.0
    );
    println!("兜底时间来源（列表阶段）：");
    for (source_kind, count) in &by_source {
        println!("  {source_kind:?}: {count}");
    }

    // ── ④ 补读真实拍摄时间（用户按「按时间」时才走这条）─────────
    let paths: Vec<PathBuf> = listing
        .items
        .iter()
        .filter(|item| item.taken_at.map(|t| t.source) != Some(TakenAtSource::Exif))
        .map(|item| item.path.clone())
        .collect();
    let started = Instant::now();
    let times = source::read_times(&paths);
    let with_time = times.iter().filter(|entry| entry.taken_at.is_some()).count();
    let total_ms = started.elapsed().as_secs_f64() * 1000.0;
    println!(
        "补读时间：{} 张（读到 {with_time} 张），{:.1}ms → 平均 {:.1}ms/张",
        paths.len(),
        total_ms,
        if paths.is_empty() {
            0.0
        } else {
            total_ms / paths.len() as f64
        }
    );

    // 按扩展名拆开：RAW 与 JPEG 的 EXIF 读取路径不同（容器 vs 裸 TIFF），
    // 在慢盘上差别可能是数量级 —— 这条拆解就是用来回答「慢在谁身上」的
    let mut by_ext: Vec<(String, usize, f64)> = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_else(|| "(无)".to_string());
        let one = Instant::now();
        let _ = raybend::media::exif::read_file(path);
        let elapsed = one.elapsed().as_secs_f64() * 1000.0;
        match by_ext.iter_mut().find(|(name, _, _)| *name == ext) {
            Some((_, count, sum)) => {
                *count += 1;
                *sum += elapsed;
            }
            None => by_ext.push((ext, 1, elapsed)),
        }
        if index % 50 == 0 {
            print!(".");
        }
    }
    println!();
    for (ext, count, sum) in &by_ext {
        println!("  {ext:<5} {count:>4} 张，平均 {:.1}ms/张", sum / *count as f64);
    }

    // ── ⑤ 现场出缩略图（网格滚动时每张一次）────────────────────
    if thumbs > 0 {
        let cache_dir = std::env::temp_dir().join("raybend-source-grid-smoke");
        std::fs::create_dir_all(&cache_dir).expect("建临时缓存目录失败");
        let thumbs_db = ThumbsDb::open(&cache_dir, 1_789_516_800_000).expect("打开缓存失败");
        println!("缩略图缓存：{}", cache_dir.display());

        let sample = listing.len().min(thumbs);
        let started = Instant::now();
        let mut rendered = 0usize;
        let mut hit = 0usize;
        let mut bytes_total = 0usize;
        for item in listing.items.iter().take(sample) {
            let before = Instant::now();
            match render_now(&thumbs_db, &item.path, SizeClass::Grid, 1_789_516_800_000) {
                Ok(bytes) => {
                    bytes_total += bytes.len();
                    if before.elapsed().as_millis() < 5 {
                        hit += 1;
                    } else {
                        rendered += 1;
                    }
                }
                Err(e) => println!("  {} 出图失败：{e}", item.file_name),
            }
        }
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        println!(
            "缩略图：{sample} 张（疑似命中 {hit} / 新渲染 {rendered}），{elapsed:.0}ms，平均 {:.1}ms/张，平均 {} KB",
            elapsed / sample.max(1) as f64,
            bytes_total / sample.max(1) / 1024
        );

        // 再来一遍：全命中，用来看缓存的读速
        let started = Instant::now();
        for item in listing.items.iter().take(sample) {
            let _ = render_now(&thumbs_db, &item.path, SizeClass::Grid, 1_789_516_800_000);
        }
        println!(
            "再取一遍（应全部命中）：{:.0}ms，平均 {:.2}ms/张",
            started.elapsed().as_secs_f64() * 1000.0,
            started.elapsed().as_secs_f64() * 1000.0 / sample.max(1) as f64
        );
    }

    println!("\n（WSL 走 9p：绝对耗时只作上界参考；滚动流畅度要在 Windows 真机上看）");
}
