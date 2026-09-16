//! 迁移 / 备份 / 故意损坏 演练（`plans/M1-7.md` 步骤 3）。
//!
//! 验四件事，都是「做不到就会丢用户数据」的级别：
//!
//! 1. **旧版库能升上来**：手工造一个 v1 schema 的库（带身份行），打开时应当自动迁移到当前
//!    版本、迁移前留快照、数据一条不丢；
//! 2. **快照保留 7 份**：同一个库的第 8 份会把最旧的挤掉（别的库的快照不受牵连）；
//! 3. **故意损坏 → 拒绝打开**：把 `catalog.db` 的头部写成垃圾之后，打开必须**报错**，
//!    绝不能「静默重建一个新库」把用户数据抹掉；
//! 4. **未来版本 → 拒绝打开**：程序比库旧时必须明确说不（而不是硬跑）。
//!
//! 另外附带 `snapshot` / `restore`：验「从快照恢复之后，库能正常打开、数据与之前一致」。
//!
//! 用法（驱动脚本 `scripts/migration-drill.sh` 按顺序调）：
//!
//! ```bash
//! migration-drill seed      <工作目录> <张数>
//! migration-drill old       <工作目录>
//! migration-drill upgrade   <工作目录>
//! migration-drill retention <工作目录>
//! migration-drill snapshot  <库根> <快照文件>
//! migration-drill corrupt   <库根>
//! migration-drill future    <库根> <版本号>
//! migration-drill restore   <快照文件> <库根>
//! ```
//!
//! 退出码：断言失败 → 1。

use std::path::{Path, PathBuf};
use std::time::Duration;

use raybend::import::fsops::{FsScanner, RepoFs};
use raybend::import::progress::{BatchHandle, BatchProgress};
use raybend::import::runner::{run_batch, Control, Deps, RunRequest};
use raybend::import::sink::CatalogSink;
use raybend::import::template;
use raybend::store::db::{CatalogDb, OpenOpts};
use raybend::store::migration::{self, DbKind};
use raybend::store::time;

const TEMPLATE: &str = ":CYEAR-:CMONTH-:CDAY/MY:SEQ000";
const FILE_BYTES: usize = 16 * 1024;
/// v1 schema（用来造一个「旧版库」）。
const CATALOG_V1: &str = include_str!("../src/store/migrations/catalog_0001_init.sql");

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("");
    let rest = args.get(1..).unwrap_or_default();

    let mut check = Checks::default();
    match mode {
        "seed" => seed(rest, &mut check),
        "old" => old(rest, &mut check),
        "upgrade" => upgrade(rest, &mut check),
        "retention" => retention(rest, &mut check),
        "snapshot" => snapshot(rest, &mut check),
        "corrupt" => corrupt(rest, &mut check),
        "future" => future(rest, &mut check),
        "restore" => restore(rest, &mut check),
        _ => {
            eprintln!("用法：migration-drill <seed|old|upgrade|retention|snapshot|corrupt|future|restore> …");
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

// ── seed：一个有照片、有账的真库 ─────────────────────────

fn seed(args: &[String], check: &mut Checks) {
    let work = PathBuf::from(args.first().expect("工作目录"));
    let count: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(12);
    let lib = work.join("lib");
    let src = work.join("src");
    std::fs::create_dir_all(&src).expect("建源目录");

    let now = time::now_millis();
    let catalog = CatalogDb::create(&lib, "演练库", Some(TEMPLATE), OpenOpts::unbacked_up(now))
        .expect("建库");
    for index in 1..=count {
        std::fs::write(src.join(format!("P{index:04}.jpg")), vec![0xABu8; FILE_BYTES])
            .expect("写源文件");
    }
    let (counts, _) = run_once(&catalog, &src, now);
    let (assets, files, imported) = counts_of(&catalog);
    println!("库：{}", lib.display());
    println!("导入：{counts:?}；库里的账 assets={assets} files={files} imported={imported}");

    // 记下来，供 restore 之后比对
    std::fs::write(
        work.join("expected.txt"),
        format!("assets={assets}\nfiles={files}\nimported={imported}\n"),
    )
    .expect("写期望值");

    check.is("导入进来了照片", imported == count as i64);
    check.is("写下了期望值（restore 之后要拿它对）", work.join("expected.txt").exists());
}

// ── old：手工造一个 v1 schema 的库 ───────────────────────

fn old(args: &[String], check: &mut Checks) {
    let work = PathBuf::from(args.first().expect("工作目录"));
    let old_dir = work.join("old");
    let ref_dir = work.join("ref");
    let _ = std::fs::remove_dir_all(&old_dir);
    let _ = std::fs::remove_dir_all(&ref_dir);
    std::fs::create_dir_all(&old_dir).expect("建目录");

    // ① 先建一个真库，只为读出它的「身份行」—— 这样演练不用猜 `repository_meta` 里有哪些键
    let now = time::now_millis();
    let reference = CatalogDb::create(&ref_dir, "旧版演练库", Some(TEMPLATE), OpenOpts::unbacked_up(now))
        .expect("建参考库");
    let meta = reference
        .read(|conn| {
            let mut stmt = conn.prepare("SELECT key, value FROM repository_meta")?;
            let rows = stmt
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("读身份行");
    drop(reference);

    // ② 只用 v1 的建表 SQL 造一个「旧版库」，user_version = 1
    let path = old_dir.join("catalog.db");
    let conn = rusqlite::Connection::open(&path).expect("开文件");
    conn.execute_batch(CATALOG_V1).expect("执行 v1 建表");
    conn.pragma_update(None, "user_version", 1_i64).expect("写版本");
    for (key, value) in &meta {
        conn.execute(
            "INSERT INTO repository_meta (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, value],
        )
        .expect("插身份行");
    }
    conn.execute(
        "INSERT INTO repository_meta (key, value) VALUES ('drill_marker', 'v1 时代写下的数据')",
        [],
    )
    .expect("插标记行");
    drop(conn);

    let _ = std::fs::remove_dir_all(&ref_dir);

    let version: i64 = version_of(&path);
    println!("旧版库：{}（user_version = {version}）", path.display());
    println!("身份行 {} 条 + 一条演练标记", meta.len());
    check.is("造出来的是 v1 的库", version == 1);
}

// ── upgrade：打开旧版库 → 自动迁移 + 留快照 + 数据不丢 ────

fn upgrade(args: &[String], check: &mut Checks) {
    let work = PathBuf::from(args.first().expect("工作目录"));
    let old_dir = work.join("old");
    let backups = work.join("backups");
    let now = time::now_millis();

    let catalog = match CatalogDb::open(
        &old_dir,
        OpenOpts::new(Some(backups.as_path()), now),
    ) {
        Ok(catalog) => catalog,
        Err(error) => {
            println!("❌ 旧版库打不开：{error}");
            std::process::exit(1);
        }
    };

    let report = catalog.report();
    let outcome = report.migration.clone();
    println!("迁移结果：{outcome:?}");

    let migrated = outcome.clone();
    check.is("打开旧版库时确实做了迁移", migrated.is_some());
    if let Some(outcome) = &migrated {
        check.is("是从 v1 升上来的", outcome.from == 1);
        check.is("升到了当前版本", outcome.to == 3);
        check.is("逐条执行了 2、3 号迁移", outcome.applied == vec![2, 3]);
        check.is("迁移前留了快照", outcome.snapshot.is_some());
        if let Some(snapshot) = &outcome.snapshot {
            check.is("快照文件真的存在", snapshot.exists());
            println!("快照：{}", snapshot.display());
        }
    }

    let version = version_of(&catalog.path().to_path_buf());
    check.is("库文件的 user_version 也前进到了 3", version == 3);

    // 数据一条不丢：身份行 + 演练标记还在
    let meta = catalog
        .read(|conn| {
            let mut stmt = conn.prepare("SELECT key, value FROM repository_meta")?;
            let rows = stmt
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("读迁移后的身份行");
    let marker = meta
        .iter()
        .find(|(key, _)| key == "drill_marker")
        .map(|(_, value)| value.as_str());
    check.is(
        "迁移之后标记行还在（数据没丢）",
        marker == Some("v1 时代写下的数据"),
    );
    check.is("身份行还在", meta.iter().any(|(key, _)| key != "drill_marker"));

    // 快照落在 backups/，并且能被轮转逻辑认出来
    let group = latest_snapshot_group(&backups, DbKind::Catalog);
    check.is("backups/ 里认得出这个库的快照组", group.is_some());
    println!("快照分组：{group:?}");
}

// ── retention：每库只留 7 份 ─────────────────────────────

fn retention(args: &[String], check: &mut Checks) {
    let work = PathBuf::from(args.first().expect("工作目录"));
    let backups = work.join("backups");
    std::fs::create_dir_all(&backups).expect("建 backups");
    let group = latest_snapshot_group(&backups, DbKind::Catalog).unwrap_or_else(|| "catalog_drill".to_string());

    // 造 10 份「同一个库」的快照（时间戳定宽 ⇒ 字典序 = 时间序）
    let mut names = Vec::new();
    for index in 0..10 {
        let name = format!("{group}_v1_20260915-1430{index:02}Z.db");
        std::fs::write(backups.join(&name), b"fake snapshot").expect("造快照");
        names.push(name);
    }
    // 再塞一份**别的库**的，它不该被牵连
    let other = "catalog_OTHERXX_v1_20260915-143000Z.db";
    std::fs::write(backups.join(other), b"other repo").expect("造别的库的快照");

    let before = std::fs::read_dir(&backups)
        .expect("读 backups")
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&group))
        .count();
    let removed = migration::rotate_backups(&backups, DbKind::Catalog, &group, 7).expect("轮转");
    let left: Vec<String> = std::fs::read_dir(&backups)
        .expect("读 backups")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(&group))
        .collect();

    println!("轮转前这组有 {before} 份，删掉 {removed} 份；还剩 {} 份", left.len());
    // 别写死数字：backups 里可能还躺着前面步骤留下的真快照（同一个库的同一组）
    check.is("删掉的份数 = 原有的份数 − 7", removed + 7 == before);
    check.is("这组只留 7 份", left.len() == 7);
    check.is("最旧的三份被删掉", !left.contains(&names[0]) && !left.contains(&names[2]));
    check.is("最新的那份还在", left.contains(&names[9]));
    check.is("别的库的快照没被牵连", backups.join(other).exists());
}

// ── snapshot / corrupt / future / restore ────────────────

fn snapshot(args: &[String], check: &mut Checks) {
    let lib = PathBuf::from(args.first().expect("库根"));
    let dest = PathBuf::from(args.get(1).expect("快照文件"));
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).expect("建目录");
    }
    // 与产品同一条机制：`VACUUM INTO`
    let conn = rusqlite::Connection::open(lib.join("catalog.db")).expect("开库");
    conn.execute("VACUUM INTO ?1", [dest.to_string_lossy().as_ref()])
        .expect("做快照");
    println!("快照：{}", dest.display());
    check.is("快照文件做出来了", dest.exists());
}

fn corrupt(args: &[String], check: &mut Checks) {
    use std::io::{Seek, SeekFrom, Write};
    let lib = PathBuf::from(args.first().expect("库根"));
    let path = lib.join("catalog.db");
    // WAL/SHM 先清掉：这里要验的是「主文件坏了会怎样」
    let _ = std::fs::remove_file(lib.join("catalog.db-wal"));
    let _ = std::fs::remove_file(lib.join("catalog.db-shm"));

    let before = std::fs::metadata(&path).expect("stat").len();
    {
        let mut file = std::fs::OpenOptions::new().write(true).open(&path).expect("打开写");
        file.seek(SeekFrom::Start(0)).expect("定位");
        file.write_all(&[0xFFu8; 4096]).expect("写垃圾");
        file.sync_all().expect("落盘");
    }
    let poisoned = std::fs::read(&path).expect("读回");
    println!("把 {} 的前 4KB 写成了垃圾（大小 {before} 字节）", path.display());

    let now = time::now_millis();
    match CatalogDb::open(&lib, OpenOpts::unbacked_up(now)) {
        Ok(_) => check.is("坏掉的库必须拒绝打开（**绝不能静默重建**）", false),
        Err(error) => {
            println!("拒绝的理由：{error}");
            check.is("坏掉的库拒绝打开", true);
            let after = std::fs::read(&path).expect("再读回");
            check.is(
                "文件没被静默重建（大小与内容都还是坏的）",
                after.len() == poisoned.len() && after[..4096] == poisoned[..4096],
            );
        }
    }
}

fn future(args: &[String], check: &mut Checks) {
    let lib = PathBuf::from(args.first().expect("库根"));
    let version: i64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(99);
    {
        let conn = rusqlite::Connection::open(lib.join("catalog.db")).expect("开库");
        conn.pragma_update(None, "user_version", version).expect("写版本");
    }
    let now = time::now_millis();
    match CatalogDb::open(&lib, OpenOpts::unbacked_up(now)) {
        Ok(_) => check.is("未来版本的库必须拒绝打开", false),
        Err(error) => {
            let text = error.to_string();
            println!("拒绝的理由：{text}");
            check.is("未来版本的库拒绝打开", true);
            check.is(
                "说清了该怎么办（提示里要有「升级」）",
                text.contains("升级") || text.contains("新版本"),
            );
        }
    }
}

fn restore(args: &[String], check: &mut Checks) {
    let snapshot = PathBuf::from(args.first().expect("快照文件"));
    let lib = PathBuf::from(args.get(1).expect("库根"));
    let work = lib.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let expected = std::fs::read_to_string(work.join("expected.txt")).expect("读期望值");

    std::fs::copy(&snapshot, lib.join("catalog.db")).expect("还原");
    let _ = std::fs::remove_file(lib.join("catalog.db-wal"));
    let _ = std::fs::remove_file(lib.join("catalog.db-shm"));

    let now = time::now_millis();
    let catalog = match CatalogDb::open(&lib, OpenOpts::unbacked_up(now)) {
        Ok(catalog) => catalog,
        Err(error) => {
            println!("❌ 还原之后打不开：{error}");
            std::process::exit(1);
        }
    };
    let (assets, files, imported) = counts_of(&catalog);
    let now_line = format!("assets={assets}\nfiles={files}\nimported={imported}\n");
    println!("还原之后：{now_line}");
    println!("损坏之前：{expected}");
    check.is("还原之后库能正常打开", true);
    check.is("还原之后的账与损坏前一致", now_line == expected);
}

// ── 共用 ────────────────────────────────────────────────

fn counts_of(catalog: &CatalogDb) -> (i64, i64, i64) {
    catalog
        .read(|conn| {
            let assets: i64 = conn.query_row("SELECT count(*) FROM assets", [], |row| row.get(0))?;
            let files: i64 =
                conn.query_row("SELECT count(*) FROM asset_files", [], |row| row.get(0))?;
            let imported: i64 = conn.query_row(
                "SELECT count(*) FROM import_items WHERE status = 'imported'",
                [],
                |row| row.get(0),
            )?;
            Ok((assets, files, imported))
        })
        .unwrap_or((0, 0, 0))
}

fn version_of(path: &PathBuf) -> i64 {
    rusqlite::Connection::open(path)
        .ok()
        .and_then(|conn| conn.query_row("PRAGMA user_version", [], |row| row.get(0)).ok())
        .unwrap_or(-1)
}

/// 从 backups/ 里已有的快照反推「这个库的分组名」（`_v` 之前那一段）。
fn latest_snapshot_group(dir: &Path, kind: DbKind) -> Option<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".db") && name.contains("_v"))
        .collect();
    names.sort();
    let last = names.last()?;
    let prefix = kind.prefix();
    last.starts_with(prefix)
        .then(|| last.split("_v").next().unwrap_or_default().to_string())
}

fn run_once(catalog: &CatalogDb, source: &Path, now: i64) -> (raybend::import::runner::RunCounts, usize) {
    let parsed = template::parse(TEMPLATE).expect("模版");
    let job = RunRequest {
        index: 0,
        source_root: source.to_path_buf(),
        photos_dir: "photos".to_string(),
        template: parsed,
        template_source: TEMPLATE.to_string(),
        include_subdirs: true,
        avoid_duplicates: true,
        excluded: std::sync::Arc::new(std::collections::HashSet::new()),
    };
    let mut sink = CatalogSink::new(catalog, now);
    let ops = RepoFs::new(catalog.root());
    let scanner = FsScanner;
    let control = Control::new();
    let handle = BatchHandle::new(BatchProgress::new("migration-drill", now));
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
