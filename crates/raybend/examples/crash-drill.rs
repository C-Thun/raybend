//! 崩溃恢复演练（`plans/M1-7.md` 步骤 2）。
//!
//! 做什么：造一个库和一批源文件 → 真导入 → **在导入中途把进程硬杀掉**
//! （`libc::_exit`，与 SIGKILL 等价：不跑析构、不刷缓冲、不做收尾）→
//! 检查库与磁盘是否仍然自洽 → 再跑一遍同一批导入，确认**不重复、不覆盖、半成品续上**。
//!
//! 为什么必须真的杀进程：`cargo test` 里的假实现能覆盖逻辑，但覆盖不了
//! 「SQLite 停在未收尾状态时重新打开会怎样」「库里的行与磁盘上的文件对不对得上」
//! —— 这两件事只有真文件 + 真库 + 真崩溃才验得到。
//!
//! 用法（驱动脚本 `scripts/crash-drill.sh` 按顺序调这几条）：
//!
//! ```bash
//! crash-drill seed    <工作目录> <张数>
//! crash-drill import  <库根> <源目录> <已入库到第几张时自杀>
//! crash-drill inspect <库根>
//! crash-drill rerun   <库根> <源目录>
//! ```
//!
//! 退出码：断言失败 → 1；`import` 没被杀掉就跑完了 → 3（说明参数没配好，不是产品问题）。

use std::path::{Path, PathBuf};
use std::time::Duration;

use raybend::import::fsops::{FsScanner, RepoFs};
use raybend::import::progress::{BatchHandle, BatchProgress};
use raybend::import::runner::{run_batch, Control, Deps, ImportSink, RunRequest};
use raybend::import::sink::CatalogSink;
use raybend::import::template;
use raybend::store::db::{CatalogDb, OpenOpts};
use raybend::store::time;

/// 与 `import-smoke` 用同一套模版：日期目录 + 递增序号。
const TEMPLATE: &str = ":CYEAR-:CMONTH-:CDAY/MY:SEQ000";

/// 源文件的体积：够让复制不是瞬间完成，又不占地方（24 KB × 600 ≈ 14 MB）。
const FILE_BYTES: usize = 24 * 1024;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("");
    let rest = args.get(1..).unwrap_or_default();

    let mut check = Checks::default();
    match mode {
        "seed" => seed(rest, &mut check),
        "import" => start_import(rest),
        "import-once" => import_once(rest),
        "inspect" => inspect(rest, &mut check),
        "rerun" => rerun(rest, &mut check),
        _ => {
            eprintln!("用法：crash-drill <seed|import|inspect|rerun> …（见文件头）");
            std::process::exit(2);
        }
    }

    println!();
    if check.failed.is_empty() {
        println!("✅ 通过（{} 项断言）", check.passed);
        std::process::exit(0);
    }
    for line in &check.failed {
        println!("❌ {line}");
    }
    println!("❌ {} 项断言失败（通过 {}）", check.failed.len(), check.passed);
    std::process::exit(1);
}

// ── seed：造库 + 造源目录 ────────────────────────────────

fn seed(args: &[String], check: &mut Checks) {
    let work = PathBuf::from(args.first().expect("工作目录"));
    let count: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(600);
    let repo = work.join("repo");
    let src = work.join("src");

    std::fs::create_dir_all(&src).expect("建源目录");
    let catalog = CatalogDb::create(
        &repo,
        "崩溃演练库",
        Some(TEMPLATE),
        OpenOpts::unbacked_up(time::now_millis()),
    )
    .expect("建库");
    drop(catalog);

    for index in 1..=count {
        // 扫描器按**扩展名**认图（`media/kind.rs`），所以内容无所谓；
        // 但别写成空文件，好让「复制」这一步真的搬点字节。
        let mut bytes = vec![0u8; FILE_BYTES];
        bytes[0] = 0xFF;
        bytes[1] = 0xD8; // JPEG 的 SOI，纯为肉眼看着像
        // 把序号写进内容：这样续跑之后可以按**内容**判「这张是不是重复落盘了」，
        // 而不是靠文件名猜（名字会因重名兜底、宽度回绕而变化）。
        bytes[8..16].copy_from_slice(&(index as u64).to_be_bytes());
        std::fs::write(src.join(format!("P{index:05}.jpg")), bytes).expect("写源文件");
    }

    let written = std::fs::read_dir(&src).map(|it| it.count()).unwrap_or(0);
    println!("工作目录：{}", work.display());
    println!("库：{}    源：{}", repo.display(), src.display());
    check.is("源目录里造出了指定张数", written == count);
}

// ── import：真导入，并在中途硬杀 ─────────────────────────

fn start_import(args: &[String]) {
    let repo = PathBuf::from(args.first().expect("库根"));
    let src = PathBuf::from(args.get(1).expect("源目录"));
    let kill_after: i64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100);

    let now = time::now_millis();
    let catalog = CatalogDb::open(&repo, OpenOpts::unbacked_up(now)).expect("打开库");

    // 「杀手」线程：盯住**已提交**的入库张数，到数就把自己 `_exit` 掉。
    // 用它而不是 `sleep 之后 kill` 的原因：这里杀在**确定的**进度点上，可重复。
    // `_exit` 不跑析构、不刷用户态缓冲、不给 SQLite 收尾的机会 —— 与 SIGKILL 等价。
    let db_path = catalog.path().to_path_buf();
    std::thread::spawn(move || loop {
        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
            let imported: i64 = conn
                .query_row(
                    "SELECT count(*) FROM import_items WHERE status = 'imported'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            if imported >= kill_after {
                eprintln!("[杀手] 库里已经有 {imported} 张 → 立刻杀进程（模拟崩溃）");
                // SAFETY: `_exit` 只接受一个状态码、没有前置条件；这里**就是要**它立刻
                // 终止进程（跳过析构、缓冲与 SQLite 收尾），与 SIGKILL 等价。之后不会再有代码执行。
                unsafe { libc::_exit(137) };
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    });

    let (counts, thumbs) = run_once(&catalog, &src, true, now);
    // 走到这里说明导入在杀手动手前就结束了 —— 参数没配好，不是产品的问题
    println!("⚠️  导入没被杀掉就跑完了：{counts:?}（缩略图入队 {thumbs}）");
    println!("    把「已入库到第几张时自杀」调小，或把张数调大");
    std::process::exit(3);
}

// ── import-once：一次跑到完，顺便量吞吐（性能基线的载荷）────

fn import_once(args: &[String]) {
    let repo = PathBuf::from(args.first().expect("库根"));
    let src = PathBuf::from(args.get(1).expect("源目录"));
    let now = time::now_millis();
    let catalog = CatalogDb::open(&repo, OpenOpts::unbacked_up(now)).expect("打开库");

    let started = std::time::Instant::now();
    let (counts, thumbs) = run_once(&catalog, &src, true, now);
    let elapsed = started.elapsed().as_secs_f64().max(0.001);

    let processed = counts.imported + counts.skipped;
    let megabytes = counts.bytes as f64 / 1_048_576.0;
    println!("跑完：{counts:?}（缩略图入队 {thumbs}）");
    println!(
        "耗时 {elapsed:.2} 秒 → 处理 {processed} 张（{:.1} 张/秒），导入 {:.1} MB（{:.1} MB/秒）",
        processed as f64 / elapsed,
        megabytes,
        megabytes / elapsed
    );
}

// ── inspect：崩溃之后，库与磁盘还自洽吗 ───────────────────

fn inspect(args: &[String], check: &mut Checks) {
    let repo = PathBuf::from(args.first().expect("库根"));
    let now = time::now_millis();

    // ① 库还能不能打开（WAL 是未收尾状态，靠重放恢复）
    let catalog = match CatalogDb::open(&repo, OpenOpts::unbacked_up(now)) {
        Ok(catalog) => catalog,
        Err(error) => {
            println!("❌ 崩溃之后库打不开了：{error}");
            std::process::exit(1);
        }
    };
    check.is("崩溃之后库仍能打开", true);

    let mut unfinished = 0i64;
    let mut pending = 0i64;
    let mut imported = 0i64;
    let mut pending_with_file = Vec::new();

    catalog
        .read(|conn| {
            let integrity: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
            check.is(
                "PRAGMA integrity_check 通过（WAL 重放没留下坏账）",
                integrity == "ok",
            );
            if integrity != "ok" {
                println!("    integrity_check 说：{integrity}");
            }

            unfinished = conn.query_row(
                "SELECT count(*) FROM import_runs WHERE state = 'running'",
                [],
                |row| row.get(0),
            )?;
            pending = conn.query_row(
                "SELECT count(*) FROM import_items WHERE status = 'pending'",
                [],
                |row| row.get(0),
            )?;
            unfinished = conn.query_row(
                "SELECT count(*) FROM import_runs WHERE state = 'running'",
                [],
                |row| row.get(0),
            )?;
            imported = conn.query_row(
                "SELECT count(*) FROM import_items WHERE status = 'imported'",
                [],
                |row| row.get(0),
            )?;

            // 「复制完但还没登记」的痕迹：目标文件已经在磁盘上，行却还是 pending。
            // 这是**允许**的（崩在复制与提交之间），但必须能数出来 —— 续跑时要靠它避免重复复制。
            let mut stmt = conn.prepare(
                "SELECT target_rel FROM import_items
                  WHERE status = 'pending' AND target_rel IS NOT NULL",
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            for row in rows {
                let target: String = row?;
                if repo.join(&target).exists() {
                    pending_with_file.push(target);
                }
            }
            Ok(())
        })
        .unwrap_or_else(|error| {
            println!("❌ 读库失败：{error}");
        });

    println!(
        "崩溃现场：未收尾批次 {unfinished} 个 / imported {imported} / pending {pending} \
         （其中目标文件已存在 {}）",
        pending_with_file.len()
    );
    check.is("留下了「没收尾」的批次（state=running）—— 这是恢复的抓手", unfinished >= 1);
    check.is("确实崩在中途（既有已入库的，也有没做完的）", imported > 0 && pending > 0);

    // ② 库里每一张已复制的照片，磁盘上都要真的在（且不是半截）
    let files = walk(&repo.join("photos"));
    let mut missing = Vec::new();
    catalog
        .read(|conn| {
            let mut stmt =
                conn.prepare("SELECT target_rel FROM import_items WHERE status = 'imported'")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            for row in rows {
                let target: String = row?;
                if !repo.join(&target).exists() {
                    missing.push(target);
                }
            }
            Ok(())
        })
        .unwrap_or_else(|error| println!("❌ 读库失败：{error}"));
    check.is(
        "库里标记为「已导入」的照片在磁盘上都在（没有只记账不落盘）",
        missing.is_empty(),
    );
    if !missing.is_empty() {
        println!("    缺了 {} 个，例如：{:?}", missing.len(), &missing[..missing.len().min(3)]);
    }

    // ③ 磁盘上不能有「库里不认识」的孤儿文件
    let mut known = std::collections::HashSet::new();
    catalog
        .read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT target_rel FROM import_items WHERE target_rel IS NOT NULL",
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            for row in rows {
                known.insert(fold(&row?));
            }
            Ok(())
        })
        .unwrap_or_else(|error| println!("❌ 读库失败：{error}"));
    let orphans: Vec<&String> = files
        .iter()
        .filter(|path| !path.ends_with(".part"))
        .filter(|path| !known.contains(&fold(&format!("photos/{path}"))))
        .collect();
    check.is("磁盘上没有库里不认识的孤儿文件", orphans.is_empty());
    if !orphans.is_empty() {
        println!("    孤儿 {} 个，例如：{:?}", orphans.len(), &orphans[..orphans.len().min(3)]);
    }

    // ④ 半成品能被生产入口认出来（续跑就是靠它不加 `_01`）
    let mut sink = CatalogSink::new(&catalog, now);
    match sink.stale_pending() {
        Ok(stale) => {
            println!("stale_pending 认出 {} 条半成品", stale.len());
            check.is(
                "半成品能被 stale_pending 认出来（它给的是未收尾批次里的全部 pending 行 —— 续跑靠它不加 _01）",
                stale.len() as i64 == pending,
            );
        }
        Err(error) => {
            check.is(&format!("stale_pending 不该报错：{error}"), false);
        }
    }

    // ⑤ `.part` 残留：允许（正是「崩在复制中」的痕迹），但要数出来
    let parts = files.iter().filter(|path| path.ends_with(".part")).count();
    println!(".part 残留 {parts} 个（允许：说明那次复制被打断了）");
}

// ── rerun：再跑一遍同一批，必须不重复、不覆盖、半成品续上 ──

fn rerun(args: &[String], check: &mut Checks) {
    let repo = PathBuf::from(args.first().expect("库根"));
    let src = PathBuf::from(args.get(1).expect("源目录"));
    let now = time::now_millis();
    let catalog = CatalogDb::open(&repo, OpenOpts::unbacked_up(now)).expect("打开库");

    let (counts, thumbs) = run_once(&catalog, &src, true, now);
    println!("续跑结果：{counts:?}（缩略图入队 {thumbs}）");

    let mut pending = 0i64;
    let mut imported = 0i64;
    let mut skipped = 0i64;
    let mut failed = 0i64;
    let mut unfinished = 0i64;
    let mut assets = 0i64;
    let mut files_in_db = 0i64;
    let mut targets = std::collections::HashSet::new();
    catalog
        .read(|conn| {
            pending = conn.query_row(
                "SELECT count(*) FROM import_items WHERE status = 'pending'",
                [],
                |row| row.get(0),
            )?;
            imported = conn.query_row(
                "SELECT count(*) FROM import_items WHERE status = 'imported'",
                [],
                |row| row.get(0),
            )?;
            skipped = conn.query_row(
                "SELECT count(*) FROM import_items WHERE status = 'skipped'",
                [],
                |row| row.get(0),
            )?;
            failed = conn.query_row(
                "SELECT count(*) FROM import_items WHERE status = 'failed'",
                [],
                |row| row.get(0),
            )?;
            unfinished = conn.query_row(
                "SELECT count(*) FROM import_runs WHERE state = 'running'",
                [],
                |row| row.get(0),
            )?;
            assets = conn.query_row("SELECT count(*) FROM assets", [], |row| row.get(0))?;
            files_in_db = conn.query_row("SELECT count(*) FROM asset_files", [], |row| row.get(0))?;
            let mut stmt = conn.prepare(
                "SELECT target_rel FROM import_items
                  WHERE target_rel IS NOT NULL AND status = 'imported'",
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            for row in rows {
                targets.insert(fold(&row?));
            }
            Ok(())
        })
        .unwrap_or_else(|error| println!("❌ 读库失败：{error}"));

    // 每个 run 的状态一览（看清「谁收尾了、谁没收尾」）
    catalog
        .read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT r.id, r.state, r.imported, r.skipped, r.failed,
                        (SELECT count(*) FROM import_items i WHERE i.run_id = r.id AND i.status='pending')
                   FROM import_runs r ORDER BY r.id",
            )?;
            println!("  run 一览：");
            for row in stmt.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })? {
                let (id, state, imported, skipped, failed, pending) = row?;
                println!(
                    "    run {id}: state={state} imported={imported} skipped={skipped} failed={failed} pending={pending}"
                );
            }
            Ok(())
        })
        .unwrap_or_else(|error| println!("❌ 读库失败：{error}"));


    println!(
        "库里的账：imported {imported} / skipped {skipped} / failed {failed} / pending {pending}；\
         资产 {assets} / 文件 {files_in_db}；不同目标路径 {}",
        targets.len()
    );

    // 注意口径：**未收尾的批次就该留着 `state=running`**（重启后界面靠它提示
    // 「上次导入被中断」，见 `src-tauri/src/import.rs` 的文件头），它的 `pending` 行
    // 也要留着 —— 那是「这个位置是我们自己占的」的凭据，续跑靠它避免重复落盘。
    // 所以这里断言的是**契约**，不是「全都收尾了」。
    check.is(
        "未收尾批次仍留着 running 标记（界面靠它提示「上次导入被中断」）",
        unfinished >= 1,
    );
    check.is(
        "未收尾批次的 pending 行留着（这是续跑判「自己占的位置」的凭据）",
        pending > 0,
    );
    check.is("续跑没有产生失败项", failed == 0);

    // **按内容**核对「不重复、不丢、不多」：每张源文件的内容在库里必须恰好出现一次。
    // 名字靠不住（重名兜底会加 `_01`、序号写满宽度会回绕），内容才作数。
    let mut src_markers = std::collections::HashMap::new();
    for entry in std::fs::read_dir(&src).into_iter().flatten().flatten() {
        if let Some(marker) = marker_of(&entry.path()) {
            *src_markers.entry(marker).or_insert(0usize) += 1;
        }
    }
    let mut disk_markers = std::collections::HashMap::new();
    for rel in walk(&repo.join("photos")) {
        if rel.ends_with(".part") {
            continue;
        }
        if let Some(marker) = marker_of(&repo.join("photos").join(&rel)) {
            *disk_markers.entry(marker).or_insert(0usize) += 1;
        }
    }
    let duplicated: Vec<&u64> = disk_markers
        .iter()
        .filter(|(_, count)| **count > 1)
        .map(|(marker, _)| marker)
        .collect();
    check.is(
        "磁盘上没有任何一张照片出现两次（按内容核）",
        duplicated.is_empty(),
    );
    if !duplicated.is_empty() {
        println!("    重复的内容标记：{:?}", &duplicated[..duplicated.len().min(5)]);
    }
    check.is(
        "每张源照片在库里都恰好有一份（不丢也不多）",
        disk_markers == src_markers,
    );
    if disk_markers != src_markers {
        println!(
            "    源里 {} 种内容，盘上 {} 种",
            src_markers.len(),
            disk_markers.len()
        );
    }

    let on_disk = walk(&repo.join("photos"));
    let parts = on_disk.iter().filter(|path| path.ends_with(".part")).count();
    println!(".part 残留 {parts} 个");
    let photos: Vec<&String> = on_disk.iter().filter(|path| !path.ends_with(".part")).collect();
    println!(
        "磁盘上 {} 个文件（排掉 .part 后 {} 个）；库里 imported 的目标路径 {} 个",
        on_disk.len(),
        photos.len(),
        targets.len()
    );
    check.is(
        "磁盘文件数与库里的目标路径数一致",
        photos.len() == targets.len(),
    );
    check.is(
        "磁盘上没有重复照片（每个目标只有一份）",
        targets.len() as i64 == imported,
    );

    // 库里不认识的文件（孤儿）与库里没有的文件（缺账）都要为零
    let disk_folded: std::collections::HashSet<String> =
        photos.iter().map(|p| fold(&format!("photos/{p}"))).collect();
    check.is(
        "磁盘与库里一一对应（既无孤儿也无缺账）",
        disk_folded == targets,
    );

    let expected = photos.len() as i64;
    check.is(
        "assets / asset_files 与磁盘文件数对得上（没有 RAW，所以一比一）",
        assets == expected && files_in_db == expected,
    );
}

// ── 共用 ────────────────────────────────────────────────

/// 跑一遍导入（与 `import-smoke` 同一套接线；一个源目录 = 一个 run）。
fn run_once(
    catalog: &CatalogDb,
    source: &Path,
    avoid_duplicates: bool,
    now: i64,
) -> (raybend::import::runner::RunCounts, usize) {
    let parsed = template::parse(TEMPLATE).expect("模版");
    let job = RunRequest {
        index: 0,
        source_root: source.to_path_buf(),
        photos_dir: "photos".to_string(),
        template: parsed,
        template_source: TEMPLATE.to_string(),
        include_subdirs: true,
        avoid_duplicates,
    };
    let mut sink = CatalogSink::new(catalog, now);
    let ops = RepoFs::new(catalog.root());
    let scanner = FsScanner;
    let control = Control::new();
    let handle = BatchHandle::new(BatchProgress::new("crash-drill", now));
    let mut thumbs = 0usize;
    let mut on_progress = |_progress: &BatchProgress| {};
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
        throttle: Duration::from_millis(100),
    };
    (run_batch(deps, &[job]).counts, thumbs)
}

/// 列出 `photos/` 下的所有文件（相对 `photos/`，`/` 分隔）。
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

/// 读文件里那个「我是第几张」的标记（seed 写进去的）。
fn marker_of(path: &Path) -> Option<u64> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 16];
    file.read_exact(&mut head).ok()?;
    Some(u64::from_be_bytes(head[8..16].try_into().ok()?))
}

/// 库里存的是**折叠过**的路径（小写），比较一律先折叠。
fn fold(path: &str) -> String {
    path.replace('\\', "/").to_lowercase()
}

#[derive(Default)]
struct Checks {
    passed: usize,
    failed: Vec<String>,
}

impl Checks {
    fn is(&mut self, what: &str, ok: bool) {
        if ok {
            self.passed += 1;
            println!("  ✓ {what}");
        } else {
            self.failed.push(what.to_string());
        }
    }
}
