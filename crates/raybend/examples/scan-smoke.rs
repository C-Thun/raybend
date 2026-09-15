//! 扫描冒烟：对一个真实目录跑 `media::scan`，只统计、**不写库**。
//!
//! 用法：
//! ```bash
//! cargo run -p raybend --example scan-smoke -- /mnt/c/src/tmp/pic
//! cargo run -p raybend --example scan-smoke -- /mnt/c/src/tmp/pic --all --hidden
//! ```
//!
//! 输出：文件/目录计数、按类型与扩展名分布、跳过的理由分布、位图↔RAW 配对情况、
//! 吞吐（文件/秒、MB/秒）与遇到的问题。用来回答「扫描够不够快、配对对不对」。

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use raybend::media::kind::MediaKind;
use raybend::media::scan::{self, Cancel, ScanEvent, ScanOptions};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(root) = args.next().map(PathBuf::from) else {
        eprintln!("用法：scan-smoke <目录> [--all] [--hidden] [--follow]");
        std::process::exit(2);
    };
    let flags: Vec<String> = args.collect();
    let opts = ScanOptions {
        include_other: flags.iter().any(|a| a == "--all"),
        include_hidden: flags.iter().any(|a| a == "--hidden"),
        follow_symlinks: flags.iter().any(|a| a == "--follow"),
        ..Default::default()
    };

    let started = Instant::now();
    let mut files: Vec<scan::ScannedFile> = Vec::new();
    let mut total_bytes: u64 = 0;
    let outcome = scan::scan(&root, &opts, &Cancel::new(), |ev| {
        if let ScanEvent::File(f) = ev {
            total_bytes += f.size_bytes;
            files.push(f);
        }
        Ok(())
    })
    .expect("扫描失败");

    let wall = started.elapsed();
    let secs = wall.as_secs_f64();

    println!("根目录  {}", root.display());
    println!("耗时    {:.2} 秒", secs);
    println!(
        "结果    目录 {} · 文件 {} · 跳过 {}",
        outcome.dirs, outcome.files, outcome.skipped
    );
    println!(
        "吞吐    {:.0} 文件/秒 · {:.1} MB/秒 · {:.2} MB 合计",
        outcome.files as f64 / secs.max(1e-6),
        total_bytes as f64 / 1e6 / secs.max(1e-6),
        total_bytes as f64 / 1e6
    );

    // 类型分布
    let mut by_kind: HashMap<&str, usize> = HashMap::new();
    for f in &files {
        let k = match f.kind {
            MediaKind::Raw => "RAW",
            MediaKind::Image => "位图",
            MediaKind::Other => "其它",
        };
        *by_kind.entry(k).or_default() += 1;
    }
    let mut kinds: Vec<_> = by_kind.into_iter().collect();
    kinds.sort_by(|a, b| b.1.cmp(&a.1));
    println!("类型    {kinds:?}");

    let mut by_ext: HashMap<String, usize> = HashMap::new();
    for f in &files {
        *by_ext
            .entry(f.ext.clone().unwrap_or_else(|| "(无扩展名)".into()))
            .or_default() += 1;
    }
    let mut exts: Vec<_> = by_ext.into_iter().collect();
    exts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    println!("扩展名  {exts:?}");

    // 跳过理由
    let mut reasons: Vec<String> = outcome
        .skipped_by_reason
        .iter()
        .map(|(r, n)| format!("{r:?}={n}"))
        .collect();
    reasons.sort();
    println!("跳过    {}", reasons.join(" · "));

    // 位图 ↔ RAW 配对（同目录同名 stem）
    let mut bitmaps: HashMap<(String, String), usize> = HashMap::new();
    let mut raws: HashMap<(String, String), usize> = HashMap::new();
    for f in &files {
        let dir = f
            .rel_path
            .rsplit_once('/')
            .map_or("", |(d, _)| d)
            .to_string();
        match f.kind {
            MediaKind::Image => {
                *bitmaps
                    .entry((dir.clone(), f.stem_folded.clone()))
                    .or_default() += 1
            }
            MediaKind::Raw => *raws.entry((dir, f.stem_folded.clone())).or_default() += 1,
            MediaKind::Other => {}
        }
    }
    let paired = bitmaps.keys().filter(|k| raws.contains_key(*k)).count();
    let raw_only = raws.keys().filter(|k| !bitmaps.contains_key(*k)).count();
    let bitmap_only = bitmaps.keys().filter(|k| !raws.contains_key(*k)).count();
    println!(
        "配对    位图↔RAW 同名成对 {paired} 组 · 只有 RAW {raw_only} · 只有位图 {bitmap_only}"
    );
    if raw_only > 0 {
        println!("        ⚠️ 「只有 RAW」的那些将来需要真实解码（现在是占位图，见 FUTURE.md B8）");
    }

    if !outcome.problems.is_empty() {
        println!("问题    {} 条，前 5 条：", outcome.problems.len());
        for p in outcome.problems.iter().take(5) {
            println!("        {} → {}", p.rel_path, p.message);
        }
    }
    println!("取消    {}", outcome.cancelled);
}
