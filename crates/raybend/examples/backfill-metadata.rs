//! 老库回填：把 `taken_at IS NULL` 的资产重读一遍 EXIF 写回库。
//!
//! 用法：
//!
//! ```bash
//! cargo run -p raybend --example backfill-metadata -- <库根> [--dry-run]
//! ```
//!
//! * `--dry-run` 只读不写（跑一遍看会补多少张，不动库）。
//!
//! 为什么需要它：2026-09-18 之前的导入**不写 EXIF**（根因见 `import/sink.rs`），
//! 那段时间导入的库里时间/器材全是 NULL。新导入已经写对了，这个例子负责补老库。
//! 只填空值、不动用户数据（`store::backfill` 里写了规则）。

use std::path::PathBuf;

use raybend::store::backfill;
use raybend::store::db::{CatalogDb, OpenOpts};
use raybend::store::time;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let Some(root) = args.iter().find(|a| !a.starts_with("--")).map(PathBuf::from) else {
        eprintln!("用法：backfill-metadata <库根> [--dry-run]");
        std::process::exit(2);
    };
    if !root.join("catalog.db").is_file() {
        eprintln!("✗ {} 下没有 catalog.db —— 这不是一个库", root.display());
        std::process::exit(2);
    }

    let now = time::now_millis();
    let catalog = match CatalogDb::open(&root, OpenOpts::new(None, now)) {
        Ok(db) => db,
        Err(error) => {
            eprintln!("✗ 打开库失败：{error}");
            std::process::exit(1);
        }
    };

    if dry_run {
        let count: usize = catalog
            .read(|conn| {
                Ok(conn.query_row(
                    "SELECT count(*) FROM assets WHERE taken_at IS NULL",
                    [],
                    |row| Ok(row.get::<_, i64>(0)? as usize),
                )?)
            })
            .unwrap_or(0);
        println!("（dry-run）这个库里有 {count} 张照片没有拍摄时间，正式跑会给它们补上");
        return;
    }

    match backfill::backfill_metadata(&catalog, now) {
        Ok(report) => {
            println!("库：{}", root.display());
            println!("候选资产：      {}", report.candidates);
            println!("这次补上：      {}", report.filled);
            println!("读完仍缺失：    {}", report.still_missing);
            println!("文件不在磁盘上：{}", report.unreadable);
            if report.candidates == report.unreadable && report.candidates > 0 {
                println!("提示：一张都没读到 —— 库是不是离线（外接盘没插）？");
            }
        }
        Err(error) => {
            eprintln!("✗ 回填失败：{error}");
            std::process::exit(1);
        }
    }
}
