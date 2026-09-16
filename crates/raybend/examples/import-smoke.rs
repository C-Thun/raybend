//! 真实磁盘上的导入闭环冒烟（`plans/M1-6.md` §6）。
//!
//! 与单测的分工：单测用**内存假实现**保证逻辑对（秒级、可重复）；
//! 这个例子把**真文件**导进**真库**，然后逐条断言落盘路径与库里的行 ——
//! 也就是「`cargo test` 绿了」与「照片真的进库了」之间那道缝。
//!
//! 用法：
//!
//! ```bash
//! cargo run -p raybend --example import-smoke -- <库根> <源目录> [模版]
//! ```
//!
//! 退出码：有断言失败 → 1。

use std::path::{Path, PathBuf};

use raybend::import::fsops::{FsScanner, RepoFs};
use raybend::import::progress::{BatchHandle, BatchProgress};
use raybend::import::runner::{run_batch, Control, Deps, RunRequest};
use raybend::import::sink::CatalogSink;
use raybend::import::template;
use raybend::store::db::{CatalogDb, OpenOpts};
use raybend::store::time;

const DEFAULT_TEMPLATE: &str = ":CYEAR-:CMONTH-:CDAY/MY:SEQ000";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(repo_root) = args.first().map(PathBuf::from) else {
        eprintln!("用法：import-smoke <库根> <源目录> [模版]");
        std::process::exit(2);
    };
    let Some(source) = args.get(1).map(PathBuf::from) else {
        eprintln!("用法：import-smoke <库根> <源目录> [模版]");
        std::process::exit(2);
    };
    let template_source = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| DEFAULT_TEMPLATE.to_string());

    let mut check = Checks::default();
    let now = time::now_millis();

    // ── 建库（库里带着模版）──────────────────────────────
    let catalog = match CatalogDb::create(
        &repo_root,
        "冒烟库",
        Some(&template_source),
        OpenOpts::unbacked_up(now),
    ) {
        Ok(catalog) => catalog,
        Err(error) => {
            eprintln!("建库失败：{error}");
            std::process::exit(1);
        }
    };
    println!("库：{}", repo_root.display());
    println!("源：{}", source.display());
    println!("模版：{template_source}\n");

    // ── 第一遍：真导入 ───────────────────────────────────
    let (first, thumbs) = run_once(&catalog, &source, &template_source, true, now);
    println!("【第一遍】{:?}", first);
    println!("缩略图入队 {} 张\n", thumbs);

    // 落盘：把 photos/ 下面真正生成了什么打出来
    let tree = walk(&repo_root.join("photos"));
    println!("photos/ 里现在有 {} 个文件：", tree.len());
    for path in &tree {
        println!("  {path}");
    }
    println!();

    // ── 断言（落盘）──────────────────────────────────────
    check.is("导入项全部成功", first.imported == first.total && first.failed == 0);
    check.is("照片落在 photos/ 下", !tree.is_empty());
    check.is(
        "文件名带上模版前缀 MY",
        tree.iter().all(|p| p.rsplit('/').next().unwrap_or("").starts_with("MY")),
    );
    check.is(
        "序号从 001 起（模版里有 :SEQ000）",
        tree.iter().any(|p| p.contains("MY001.")),
    );
    check.is(
        "只含文档的目录没有建出来",
        !repo_root.join("photos").join("只含文档_placeholder").exists()
            && !tree.iter().any(|p| p.contains("只含文档")),
    );
    check.is(
        "源层级透传（日本/Kyoto）",
        tree.iter().any(|p| p.contains("/日本/Kyoto/")),
    );

    // JPEG 与它的 RAW：同目录层级、同名、RAW 在 _RAW/ 下
    let jpegs: Vec<&String> = tree.iter().filter(|p| p.to_lowercase().ends_with(".jpg")).collect();
    let raws: Vec<&String> = tree.iter().filter(|p| p.contains("/_RAW/")).collect();
    check.is("有 JPEG 落盘", !jpegs.is_empty());
    check.with("RAW 进了 _RAW/ 并且与 JPEG 同名", format!("{} 个 RAW / {} 个 JPEG", raws.len(), jpegs.len()), || {
        raws.iter().all(|raw| {
            let stem = raw.rsplit('/').next().unwrap_or("").rsplit_once('.').map(|(s, _)| s).unwrap_or("");
            let dir = raw.trim_end_matches(|c| c != '/').trim_end_matches("_RAW/").trim_end_matches('/');
            jpegs
                .iter()
                .any(|jpg| jpg.starts_with(dir) && jpg.rsplit('/').next().unwrap_or("").rsplit_once('.').map(|(s, _)| s) == Some(stem))
        })
    });

    // ── 断言（库里）─────────────────────────────────────
    let summary = read_summary(&catalog, &mut check);
    println!("\n库里的行：{summary}");

    // ── 第二遍：同一批再导一次（应当全部跳过）─────────────
    let files_before = tree.len();
    let (second, _) = run_once(&catalog, &source, &template_source, true, now + 1_000);
    let tree_after = walk(&repo_root.join("photos"));
    println!("【第二遍】{:?}", second);
    check.is("第二遍全部跳过（避免重复导入）", second.skipped == second.total && second.imported == 0);
    check.is("第二遍没有多出文件", tree_after.len() == files_before);

    // ── 汇总 ────────────────────────────────────────────
    println!();
    if check.failed.is_empty() {
        println!("✅ 全部通过（{} 项断言）", check.passed);
        std::process::exit(0);
    }
    for line in &check.failed {
        println!("❌ {line}");
    }
    println!("❌ {} 项断言失败（通过 {}）", check.failed.len(), check.passed);
    std::process::exit(1);
}

/// 跑一遍（一个源目录 = 一个 run）。
fn run_once(
    catalog: &CatalogDb,
    source: &Path,
    template_source: &str,
    avoid_duplicates: bool,
    now: i64,
) -> (raybend::import::runner::RunCounts, usize) {
    let parsed = template::parse(template_source).expect("模版");
    let job = RunRequest {
        index: 0,
        source_root: source.to_path_buf(),
        photos_dir: "photos".to_string(),
        template: parsed,
        template_source: template_source.to_string(),
        include_subdirs: true,
        avoid_duplicates,
        excluded: std::sync::Arc::new(std::collections::HashSet::new()),
    };
    let mut sink = CatalogSink::new(catalog, now);
    let ops = RepoFs::new(catalog.root());
    let scanner = FsScanner;
    let control = Control::new();
    let handle = BatchHandle::new(BatchProgress::new("smoke", now));
    let mut thumbs = 0usize;
    let mut on_progress = |progress: &BatchProgress| {
        // 只在阶段变化时吭一声（真实运行时这里是发事件）
        if progress.failed > 0 {
            for error in progress.errors.iter().take(3) {
                eprintln!("  ! {} —— {}", error.source, error.reason);
            }
        }
    };
    let mut enqueue_thumbs = |paths: &[String]| -> raybend::Result<usize> {
        thumbs += paths.len();
        Ok(paths.len())
    };
    let deps = Deps {
        sink: &mut sink,
        ops: &ops,
        scanner: &scanner,
        control: &control,
        handle: &handle,
        on_progress: &mut on_progress,
        enqueue_thumbs: &mut enqueue_thumbs,
        now_ms: now,
        throttle: std::time::Duration::from_millis(100),
    };
    let outcome = run_batch(deps, &[job]);
    (outcome.counts, thumbs)
}

/// 库里该有的样子（返回一句摘要，顺便断言）。
fn read_summary(catalog: &CatalogDb, check: &mut Checks) -> String {
    catalog
        .read(|conn| {
            let assets: i64 = conn.query_row("SELECT count(*) FROM assets", [], |r| r.get(0))?;
            let files: i64 = conn.query_row("SELECT count(*) FROM asset_files", [], |r| r.get(0))?;
            let imported: i64 = conn.query_row(
                "SELECT count(*) FROM import_items WHERE status = 'imported'",
                [],
                |r| r.get(0),
            )?;
            let seqs: i64 = conn.query_row("SELECT count(*) FROM seq_counters", [], |r| r.get(0))?;
            let sourced: i64 = conn.query_row(
                "SELECT count(*) FROM asset_files WHERE source_path IS NOT NULL",
                [],
                |r| r.get(0),
            )?;
            check.is("库里有进口项且都是 imported", imported > 0);
            check.is("序号计数写回了 seq_counters", seqs > 0);
            check.is("每个文件都记下了来源路径", sourced == files);
            check.is("位图与 RAW 配成一条资产（资产数 < 文件数）", assets < files && assets > 0);
            Ok(format!(
                "assets={assets} asset_files={files} imported_items={imported} seq_counters={seqs} 有来源={sourced}"
            ))
        })
        .unwrap_or_else(|error| format!("读库失败：{error}"))
}

/// 列出 `photos/` 下的所有文件（库内相对路径，`/` 分隔）。
fn walk(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}

#[derive(Default)]
struct Checks {
    passed: usize,
    failed: Vec<String>,
}

impl Checks {
    fn is(&mut self, name: &str, ok: bool) {
        self.with(name, String::new(), || ok);
    }

    fn with(&mut self, name: &str, detail: impl Into<String>, ok: impl FnOnce() -> bool) {
        if ok() {
            self.passed += 1;
            println!("  ✓ {name}");
        } else {
            let detail = detail.into();
            self.failed
                .push(if detail.is_empty() { name.to_string() } else { format!("{name}（{detail}）") });
            println!("  ✗ {name}");
        }
    }
}
