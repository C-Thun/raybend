//! 诊断：对着**真实目录**跑一遍「读拍摄时间」（`media::source::read_times`，
//! 也就是导入网格按时间分组时后端干的活），把每个文件的**时间与来源**打出来。
//!
//! 为什么需要它：人类报「按时间模式下，同一天的分组标题出现两次、一次纯位图一次纯 RAW」。
//! 我的模拟脚本（python）只能读到 JPG 的 EXIF、读不到 RW2 的 —— 但那是脚本的局限，
//! 应用里走的是另一条路（`exif::read_file_for`，RAW 感知）。到底是**后端读不到**、
//! 还是**前端没把结果并回去**，这个探针一句话就能分开。
//!
//! 用法：
//! ```text
//! cargo run -p raybend --example source-times-probe -- <目录> [文件名过滤子串]
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

fn main() {
    let Some(dir) = std::env::args().nth(1) else {
        eprintln!("用法：source-times-probe <目录> [文件名过滤子串]");
        std::process::exit(2);
    };
    let filter = std::env::args().nth(2);

    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| {
            eprintln!("读不了目录 {dir}：{e}");
            std::process::exit(2);
        })
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file())
        .filter(|path| match &filter {
            Some(needle) => path.to_string_lossy().contains(needle.as_str()),
            None => true,
        })
        .collect();
    paths.sort();

    let started = std::time::Instant::now();
    let entries = raybend::media::source::read_times(&paths);
    let elapsed = started.elapsed();

    let mut by_source: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_ext: BTreeMap<(String, String), usize> = BTreeMap::new();
    for entry in &entries {
        let source = entry
            .taken_at
            .as_ref()
            .map_or_else(|| "读不到".to_string(), |t| format!("{:?}", t.source));
        *by_source.entry(source.clone()).or_insert(0) += 1;
        let ext = entry
            .path
            .extension()
            .map_or_else(String::new, |e| e.to_string_lossy().to_uppercase());
        *by_ext.entry((ext, source)).or_insert(0) += 1;
    }

    println!("读 {} 个文件，耗时 {elapsed:?}", entries.len());
    println!("\n=== 时间来源分布 ===");
    for (source, count) in &by_source {
        println!("  {source:<12} {count}");
    }
    println!("\n=== 按扩展名 × 来源 ===");
    for ((ext, source), count) in &by_ext {
        println!("  {ext:<6} {source:<12} {count}");
    }

    println!("\n=== 前 12 个（时间 / 来源 / 文件名）===");
    for entry in entries.iter().take(12) {
        let (stamp, source) = match &entry.taken_at {
            Some(t) => (
                chrono_like(t.millis),
                format!("{:?}", t.source),
            ),
            None => ("-".to_string(), "读不到".to_string()),
        };
        let name = entry
            .path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        println!("  {stamp}  {source:<8} {name}");
    }
}

/// 把 Unix 毫秒印成 `MM-DD HH:MM:SS`（UTC；只是为了肉眼比对，不参与判断）。
fn chrono_like(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // 1970-01-01 起的天数 → 年月日（简单的民用历算法，够用）
    let mut year = 1970i64;
    let mut day_of_year = days;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let len = if leap { 366 } else { 365 };
        if day_of_year < len {
            break;
        }
        day_of_year -= len;
        year += 1;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let month_len = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1;
    for len in month_len {
        if day_of_year < len {
            break;
        }
        day_of_year -= len;
        month += 1;
    }
    format!(
        "{:02}-{:02} {:02}:{:02}:{:02}",
        month,
        day_of_year + 1,
        h,
        m,
        s
    )
}
