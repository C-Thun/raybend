//! RAW 解码冒烟：真实样本 + 崩溃隔离演练。
//!
//! ```bash
//! # 目录（递归找 RAW，按大小升序取前 N 张，快）
//! cargo run -p raybend --release --example raw-smoke -- /mnt/c/src/tmp/pic --limit 8
//!
//! # 单张 + 完整解码（1:1 路径）
//! cargo run -p raybend --release --example raw-smoke -- /path/IMG.RW2 --full
//!
//! # 隔离演练：让 worker 自己 panic / 卡住，验证主进程存活且能自动重建
//! cargo run -p raybend --release --example raw-smoke -- --crash
//! cargo run -p raybend --release --example raw-smoke -- --timeout
//! ```
//!
//! 退出码：0 = 全部符合预期；1 = 有不符合预期的项。

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use raybend::media::kind::{MediaKind, kind_of_file};
use raybend::raw::backend::{DecodeRequest, PixelSource};
use raybend::raw::worker::{RawWorker, WorkerError};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut limit = 8usize;
    let mut full = false;
    let mut crash = false;
    let mut timeout = false;
    let mut root: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--limit" => {
                i += 1;
                limit = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(8);
            }
            "--full" => full = true,
            "--crash" => crash = true,
            "--timeout" => timeout = true,
            other => root = Some(PathBuf::from(other)),
        }
        i += 1;
    }

    let mut failures = 0usize;

    if crash {
        failures += drill_crash();
    }
    if timeout {
        failures += drill_timeout();
    }

    if let Some(root) = root {
        failures += decode_samples(&root, limit, full);
    } else if !crash && !timeout {
        eprintln!("用法：raw-smoke <目录或文件> [--limit N] [--full] [--crash] [--timeout]");
        std::process::exit(2);
    }

    if failures == 0 {
        println!("\n✅ 全部符合预期");
    } else {
        println!("\n❌ 有 {failures} 项不符合预期");
        std::process::exit(1);
    }
}

/// 收集 RAW 文件，按**文件大小升序**取前 `limit` 张（小文件跑得快，先把链路跑通）。
fn collect_raw(root: &Path, limit: usize) -> Vec<PathBuf> {
    let mut files: Vec<(u64, PathBuf)> = Vec::new();
    if root.is_file() {
        files.push((std::fs::metadata(root).map(|m| m.len()).unwrap_or(0), root.to_path_buf()));
    } else {
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(meta) = entry.metadata() else { continue };
                if meta.is_dir() {
                    stack.push(path);
                } else if let Some(name) = path.file_name().and_then(|n| n.to_str())
                    && kind_of_file(name) == MediaKind::Raw
                {
                    files.push((meta.len(), path));
                }
            }
        }
    }
    files.sort();
    files.into_iter().take(limit).map(|(_, p)| p).collect()
}

fn decode_samples(root: &Path, limit: usize, full: bool) -> usize {
    let files = collect_raw(root, limit);
    if files.is_empty() {
        println!("⚠️  {} 下没有找到 RAW 文件", root.display());
        return 1;
    }
    println!("样本 {} 张（来自 {}）\n", files.len(), root.display());
    println!(
        "{:<28} {:>6} {:>12} {:>18} {:>9}",
        "文件", "长边", "像素来源", "尺寸", "耗时 ms"
    );

    let mut worker = RawWorker::new();
    let mut millis: Vec<f64> = Vec::new();
    let mut failures = 0usize;
    let mut previews = 0usize;
    let mut decoded = 0usize;

    for path in &files {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let req = if full {
            DecodeRequest::full(path)
        } else {
            DecodeRequest::thumb(path, 768)
        };
        let started = Instant::now();
        let outcome = worker.decode(&req);
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        millis.push(ms);
        match outcome {
            Ok(image) => {
                match image.source {
                    PixelSource::EmbeddedPreview => previews += 1,
                    PixelSource::Decoded => decoded += 1,
                }
                println!(
                    "{:<28} {:>6} {:>12} {:>18} {:>9.1}",
                    shorten(&name, 28),
                    image.width.max(image.height),
                    image.source.as_str(),
                    format!("{}×{}", image.width, image.height),
                    ms
                );
            }
            Err(e) => {
                failures += 1;
                println!(
                    "{:<28} {:>6} {:>12} {:>18} {:>9.1}",
                    shorten(&name, 28),
                    "-",
                    "失败",
                    short_error(&e),
                    ms
                );
            }
        }
    }

    millis.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let sum: f64 = millis.iter().sum();
    println!(
        "\n合计 {} 张：内嵌预览 {} / 完整解码 {} / 失败 {}",
        files.len(),
        previews,
        decoded,
        failures
    );
    if !millis.is_empty() {
        println!(
            "耗时：平均 {:.1} ms，中位 {:.1} ms，P95 {:.1} ms，最慢 {:.1} ms",
            sum / millis.len() as f64,
            percentile(&millis, 50.0),
            percentile(&millis, 95.0),
            millis.last().copied().unwrap_or_default()
        );
    }
    failures
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = ((p / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn short_error(e: &WorkerError) -> String {
    let text = e.to_string();
    shorten(&text, 40)
}

/// 演练一：worker 自己 panic —— 主进程必须活着，并且下一次请求能重建进程。
fn drill_crash() -> usize {
    println!("── 隔离演练：worker panic ──");
    let mut worker = RawWorker::new();
    let mut failures = 0;

    match worker.ping() {
        Ok(()) => println!("  1. 先探活：✅ 正常"),
        Err(e) => {
            println!("  1. 先探活：❌ {e}");
            failures += 1;
        }
    }

    match worker.crash_for_test() {
        Ok(()) => {
            println!("  2. 注入 panic：❌ 不该成功");
            failures += 1;
        }
        Err(WorkerError::Crashed(_)) | Err(WorkerError::Protocol(_)) => {
            println!("  2. 注入 panic：✅ 主进程活着，收到的是「子进程死了」")
        }
        Err(e) => {
            println!("  2. 注入 panic：❌ 意料之外的失败类型：{e}");
            failures += 1;
        }
    }

    match worker.ping() {
        Ok(()) => println!("  3. 崩溃后重建：✅ 能重新起进程"),
        Err(e) => {
            println!("  3. 崩溃后重建：❌ {e}");
            failures += 1;
        }
    }
    println!();
    failures
}

/// 演练二：worker 卡住 —— 看门狗必须把它杀掉，而不是让主进程永远等下去。
fn drill_timeout() -> usize {
    println!("── 隔离演练：worker 卡住（2 秒超时）──");
    let mut worker = RawWorker::with_timeout(Duration::from_secs(2));
    let mut failures = 0;

    match worker.sleep_for_test(60) {
        Err(WorkerError::Timeout(_)) => {
            println!("  1. 卡住的请求：✅ 被看门狗终止（没有永久挂住）")
        }
        Ok(()) => {
            println!("  1. 卡住的请求：❌ 不该成功");
            failures += 1;
        }
        Err(e) => {
            println!("  1. 卡住的请求：❌ 意料之外的失败类型：{e}");
            failures += 1;
        }
    }

    match worker.ping() {
        Ok(()) => println!("  2. 超时后重建：✅ 能重新起进程"),
        Err(e) => {
            println!("  2. 超时后重建：❌ {e}");
            failures += 1;
        }
    }
    println!();
    failures
}
