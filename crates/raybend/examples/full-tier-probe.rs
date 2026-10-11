//! **真身档（`SizeClass::Full`）的真库探针与性能底数**（`specs/view-true-image.md` §0.4 / §3）。
//!
//! ```bash
//! cargo run -q --release -p raybend --example full-tier-probe -- <库根> [文件名片段]
//! ```
//!
//! 它回答四件事（都是「Agent 只能量、不能下结论」的那类数字）：
//!
//! 1. **尺寸口径**：逐变体打印 `store::sizes::display_size`（sooc / raw / latest / 每条定稿）——
//!    「每个变体只认自己那一份」（崔总 2026-10-12 真机：「raw 缩进了 sooc 的尺寸里」）；
//! 2. **真身真的是原像素**：`Full` 渲染出来的宽高 = 源/成片自己的尺寸（不缩放、不用内嵌预览）；
//! 3. **时间轴**：冷缓存渲染耗时（对 `specs/view-true-image.md` §0.4 的 3s / 5s 预算）、
//!    写缓存与命中缓存的耗时（「来回切要快」的定量依据）；
//! 4. **缓存往返**：库内 `cache/full/<资产>/…-full-…jpg` 真的写进去、读出来一致。
//!
//! 只读原库：**复制一份再跑**（`cp -r <库> /tmp/xxx`），探针不写目录外的东西。

use std::path::{Path, PathBuf};
use std::time::Instant;

use raybend::display::{CacheClass, FullCache};
use raybend::store::db::{CatalogDb, OpenOpts};
use raybend::store::develop::EditBase;
use raybend::store::sizes::{SizeVariant, display_size};
use raybend::store::time;
use raybend::thumbnail::render::{SizeClass, render_file_with_profiles};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(root) = args.first().map(PathBuf::from) else {
        eprintln!("用法：full-tier-probe <库根> [文件名片段]");
        std::process::exit(2);
    };
    let needle = args.get(1).cloned().unwrap_or_default();
    let now = time::now_millis();
    let catalog = CatalogDb::open(&root, OpenOpts::new(None, now)).expect("打开库");

    let asset_id = catalog
        .read(|conn| {
            let id: Option<i64> = conn
                .query_row(
                    "SELECT a.id FROM assets a JOIN asset_files f ON f.asset_id = a.id
                      WHERE f.role = 'bitmap' AND f.rel_path LIKE ?1 ORDER BY a.id LIMIT 1",
                    [format!("%{needle}%")],
                    |row| row.get(0),
                )
                .ok();
            Ok(id)
        })
        .expect("查库");
    let Some(asset_id) = asset_id else {
        eprintln!("✗ 没找到匹配「{needle}」的资产");
        std::process::exit(1);
    };

    // ── 1. 尺寸口径：逐变体 ───────────────────────────────────────────────
    println!("=== 资产 {asset_id} 的尺寸口径 ===");
    let (rel_bitmap, rel_raw, stack) = catalog
        .read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT role, rel_path, display_width, display_height FROM asset_files
                  WHERE asset_id = ?1 AND missing_since IS NULL ORDER BY id",
            )?;
            let files: Vec<(String, String, Option<i64>, Option<i64>)> = stmt
                .query_map([asset_id], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for (role, path, w, h) in &files {
                println!("  {role:<7} 行：{w:?}×{h:?} · {path}");
            }
            let stack = raybend::store::develop::load(conn, asset_id)?;
            let label = if stack.is_empty() {
                "(空/没编辑)".to_string()
            } else {
                format!("base={}", stack.source_base.as_str())
            };
            println!("  编辑栈：{label}");
            for (name, variant) in [
                ("sooc  ", SizeVariant::Sooc),
                ("raw   ", SizeVariant::Raw),
                ("latest", SizeVariant::Latest),
            ] {
                println!("  display_size({}) = {:?}", name.trim(), display_size(conn, asset_id, variant)?);
            }
            let issues: Vec<(i64, String)> = conn
                .prepare("SELECT id, name FROM issues WHERE asset_id = ?1 ORDER BY ordinal, id")?
                .query_map([asset_id], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            for (id, name) in &issues {
                println!(
                    "  display_size(issue:{id}) [{name}] = {:?}",
                    display_size(conn, asset_id, SizeVariant::Issue(*id))?
                );
            }
            Ok((
                files.iter().find(|(role, ..)| role == "bitmap").map(|(_, path, ..)| path.clone()),
                files.iter().find(|(role, ..)| role == "raw").map(|(_, path, ..)| path.clone()),
                stack,
            ))
        })
        .expect("读库");

    // ── 2/3/4. 真身渲染 + 缓存往返 ────────────────────────────────────────
    // 用哪个源：编辑栈的基底优先（真身要的就是这个变体的像素）
    let base = if stack.is_empty() { EditBase::Sooc } else { stack.source_base };
    let source_rel = match base {
        EditBase::Raw => rel_raw.clone().or_else(|| rel_bitmap.clone()),
        EditBase::Sooc => rel_bitmap.clone(),
    };
    let Some(source_rel) = source_rel else {
        eprintln!("✗ 没有可用的源文件");
        std::process::exit(1);
    };
    let source = root.join(raybend::store::path_semantics::to_platform_path(&source_rel));
    println!("\n=== 真身（Full）渲染 · 源 = {} · 基底 = {} ===", source.display(), base.as_str());

    // 没有 ICC（探针不碰 app.db 的色彩配置）：V2 色彩栈会由此报错，老照片这条路依然是通的
    let profile = |id: &raybend::color::ProfileId, _role: raybend::color::icc::IccRole| {
        Err(raybend::Error::Unsupported(format!("探针没接 ICC：{}", id.as_str()))) as raybend::Result<_>
    };
    let edit = if stack.is_empty() { None } else { Some(&stack) };

    let started = Instant::now();
    let thumb = render_file_with_profiles(
        &source,
        SizeClass::Full,
        edit,
        None,
        None,
        raybend::thumbnail::render::EncodeFormat::Avif,
        profile,
    )
        .expect("真身渲染")
        .expect("解不开这张照片");
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    println!(
        "  冷缓存真身：{elapsed:.0} ms · {}×{} · {:.1} MB（JPEG）",
        thumb.width,
        thumb.height,
        thumb.data.len() as f64 / 1_048_576.0
    );

    let signature = raybend::media::source::source_signature(&source).expect("源签名");
    let variant = if stack.is_empty() {
        base.as_str().to_string()
    } else {
        format!("latest-{}", &raybend::store::issues::profile_hash(&stack).expect("栈指纹")[..12])
    };
    let name = FullCache::source_name(&variant, &signature);
    let cache = FullCache::open(&root).expect("打开缓存");
    let started = Instant::now();
    cache
        .write_class(asset_id, &name, base, CacheClass::Full, raybend::thumbnail::render::PIPELINE_VERSION, &thumb.data)
        .expect("写缓存");
    let write_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let back = cache
        .read_class(asset_id, &name, base, CacheClass::Full, raybend::thumbnail::render::PIPELINE_VERSION)
        .expect("读缓存");
    let read_ms = started.elapsed().as_secs_f64() * 1000.0;
    println!(
        "  写缓存 {write_ms:.0} ms · 命中缓存 {read_ms:.0} ms · 一致：{}",
        back == thumb.data
    );
    println!(
        "  缓存路径：{}",
        cache
            .path_for_class(asset_id, &name, base, CacheClass::Full, raybend::thumbnail::render::PIPELINE_VERSION)
            .display()
    );
    println!("  缓存总占用：{:.1} MB · 清扫（5GB 上限）删了 {} 个文件", cache.used_bytes() as f64 / 1_048_576.0, cache.enforce_capacity(5 * 1024 * 1024 * 1024));
    let _ = Path::new(".");
}
