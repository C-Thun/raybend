//! **一次性存量修复**：给「温度是显式值、但 `as_shot_k` 为空」的编辑栈回填解码真值。
//!
//! 依据 `specs/editor-session-pipeline.md` §6.8（崔总 2026-10-10 定：**先看留证再执行**）。
//! 背景：白平衡是 `temperature_gain_ratio(target, baseline)`，基线（这张照片的 as-shot）
//! 缺失时显式温度没有「相对谁」的解释 —— 老数据里这类行会在重开时按静态默认 6250K 解释，
//! 看起来偏暖/偏冷。修复 = 把 `as_shot_k` 补成该照片的**解码真值**（观感会变回中性）。
//!
//! ```bash
//! # 预演（默认，只读：不写库、不动盘上文件）
//! cargo build -p raybend --bin raybend-raw-worker
//! cargo run -p raybend --release --example fix-as-shot-k -- "D:\photos\repo"
//! # 确认后写入（人类决定）
//! cargo run -p raybend --release --example fix-as-shot-k -- "D:\photos\repo" --apply
//! ```
//!
//! 退出码：`0` = 跑完（含「没有候选」）；`2` = 用法 / 打开失败。
//! 库必须已是当前 schema 版本（旧库先用应用打开完成迁移 —— 与 `xmp-audit` 同口径）。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use raybend::raw::backend::DecodeRequest;
use raybend::raw::worker;
use raybend::store::db::{CatalogDb, OpenOpts};
use raybend::store::develop::{self, EditBase};
use raybend::store::migration::{DbKind, schema_version, supported_version};
use raybend::store::time;

const USAGE: &str = "用法：cargo run -p raybend --release --example fix-as-shot-k -- \"<库根>\" [--apply]\n\
                     （默认预演，只读；加 --apply 才写库。库须为当前 schema 版本）";

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let Some(root) = args.next().map(PathBuf::from) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let apply = match args.next().as_deref().and_then(|value| value.to_str()) {
        None => false,
        Some("--apply") => true,
        Some(other) => {
            eprintln!("未知参数：{other}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if !root.is_dir() || !root.join("catalog.db").is_file() {
        eprintln!("不是库根（没有 catalog.db）：{}", root.display());
        return ExitCode::from(2);
    }

    // 版本闸门：不拿这个工具顺手迁库（与 `xmp-audit` 同一口径）。
    match catalog_version(&root.join("catalog.db")) {
        Ok(version) if version == supported_version(DbKind::Catalog) => {}
        Ok(version) => {
            eprintln!(
                "库 schema 版本 {version}，本工具支持 {}；请先用应用打开这个库（完成迁移）再跑。",
                supported_version(DbKind::Catalog)
            );
            return ExitCode::from(2);
        }
        Err(error) => {
            eprintln!("读不到库版本：{error}");
            return ExitCode::from(2);
        }
    }

    let catalog = match CatalogDb::open(&root, OpenOpts::unbacked_up(time::now_millis())) {
        Ok(catalog) => catalog,
        Err(error) => {
            eprintln!("打不开库 {}：{error}", root.display());
            return ExitCode::from(2);
        }
    };

    let candidates = match catalog.read(develop::stacks_missing_as_shot_baseline) {
        Ok(candidates) => candidates,
        Err(error) => {
            eprintln!("读候选失败：{error}");
            let _ = catalog.retire();
            return ExitCode::from(2);
        }
    };
    if candidates.is_empty() {
        println!("没有「显式温度 + as_shot_k 为空」的编辑栈：无需修复。");
        let _ = catalog.retire();
        return ExitCode::SUCCESS;
    }

    println!(
        "找到 {} 条候选（{}）：{}",
        candidates.len(),
        if apply { "写入模式" } else { "预演，只读" },
        candidates
            .iter()
            .map(|(asset, kelvin)| format!("#{asset} {kelvin}K"))
            .collect::<Vec<_>>()
            .join("、")
    );

    let mut fixed = 0usize;
    let mut skipped = 0usize;
    for (asset_id, temperature) in &candidates {
        let asset_id = *asset_id;
        let temperature = *temperature;
        let source = match catalog.read(|conn| develop::edit_target(conn, asset_id, EditBase::Raw))
        {
            Ok(Some(rel)) => catalog.root().join(rel.replace('/', std::path::MAIN_SEPARATOR_STR)),
            Ok(None) => {
                println!("  #{asset_id}：没有 RAW 源文件，跳过");
                skipped += 1;
                continue;
            }
            Err(error) => {
                println!("  #{asset_id}：读源失败（{error}），跳过");
                skipped += 1;
                continue;
            }
        };
        let shot = match decode_as_shot(&source) {
            Ok(Some(shot)) => shot,
            Ok(None) => {
                println!("  #{asset_id}：解码没有给出 as-shot（{}），跳过", source.display());
                skipped += 1;
                continue;
            }
            Err(error) => {
                println!("  #{asset_id}：解码失败（{error}），跳过");
                skipped += 1;
                continue;
            }
        };
        if !apply {
            println!(
                "  #{asset_id}：temperature {temperature}K（基线缺失）→ as_shot_k 将回填 {shot}K"
            );
            println!("     文件：{}", source.display());
            println!("     预期观感：白平衡回到中性（当前偏低 K 的偏暖会消失）");
            continue;
        }
        let saved = catalog.write_tx(move |conn| {
            let mut stack = develop::load(conn, asset_id)?;
            stack.as_shot_k = Some(shot);
            develop::save(conn, asset_id, &stack, time::now_millis())
        });
        match saved {
            Ok(_) => {
                println!("  #{asset_id}：as_shot_k ← {shot}K（temperature 保持 {temperature}K）");
                fixed += 1;
            }
            Err(error) => {
                println!("  #{asset_id}：写入失败（{error}），跳过");
                skipped += 1;
            }
        }
    }

    if apply {
        println!("完成：修复 {fixed} 条，跳过 {skipped} 条。");
    } else if skipped > 0 {
        println!("预演：{skipped} 条无法取得解码真值（上面的跳过行）；其余可修复。");
    }
    let _ = catalog.retire();
    ExitCode::SUCCESS
}

/// 解码一张 RAW 的线性源，只取 as-shot 色温（与编辑器视角同一条解码路）。
fn decode_as_shot(path: &Path) -> Result<Option<f32>, String> {
    let request = DecodeRequest::full(path);
    let worker = worker::shared();
    let mut guard = worker.lock().map_err(|_| "RAW worker 锁中毒".to_string())?;
    let decoded = guard.decode_linear(&request).map_err(|e| e.to_string())?;
    Ok(decoded.as_shot_temperature)
}

/// 只读地读 `user_version` —— 打不开或不是 SQLite 都返回错误字符串。
fn catalog_version(path: &Path) -> Result<i64, String> {
    let conn =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| error.to_string())?;
    schema_version(&conn).map_err(|error| error.to_string())
}
