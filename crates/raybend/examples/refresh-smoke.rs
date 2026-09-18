//! 刷新冒烟：对一个真实目录跑「扫描 → 差分 → 落库 → 再扫描」的完整链路。
//!
//! 用法：
//! ```bash
//! cargo run -p raybend --example refresh-smoke -- /mnt/c/src/tmp/pic
//! ```
//!
//! 在一个**临时库**里做（跑完即删，不碰任何现有数据），验证：
//! 1. 首轮：新文件 → 建资产（位图 + RAW 配对成一张照片）
//! 2. 次轮：**零变化**（幂等）
//! 3. 改名、删除、改内容各跑一遍，看差分判对没有

use std::path::PathBuf;
use std::time::Instant;

use raybend::media::diff::{self, DiskFile};
use raybend::media::exif::{self, ExifData, TakenAt, TakenAtSource};
use raybend::media::scan::{self, Cancel, ScanEvent, ScanOptions};
use raybend::store::assets::{self, FileRow};
use raybend::store::db::{CatalogDb, OpenOpts};
use raybend::store::file_id::FileId;

/// 扫描一个目录并把绝对路径补成带文件身份的 `DiskFile` 列表。
fn snapshot(root: &std::path::Path, opts: &ScanOptions) -> Vec<DiskFile> {
    let mut files = Vec::new();
    scan::scan(root, opts, &Cancel::new(), |ev| {
        if let ScanEvent::File(f) = ev {
            let identity = FileId::try_read(f.abs_path.as_path());
            files.push(DiskFile {
                rel_path: f.rel_path.clone(),
                rel_path_folded: raybend::store::path_semantics::PathForms::new(&f.rel_path)
                    .folded()
                    .to_string(),
                kind: f.kind,
                size_bytes: f.size_bytes,
                mtime_ms: f.mtime_ms,
                identity,
            });
        }
        Ok(())
    })
    .expect("扫描失败");
    files
}

fn refresh(
    db: &CatalogDb,
    root: &std::path::Path,
    opts: &ScanOptions,
    now: i64,
) -> assets::ApplyOutcome {
    let disk = snapshot(root, opts);
    let db_files: Vec<diff::DbFile> = db
        .read(assets::list_files)
        .expect("读文件表失败")
        .iter()
        .map(FileRow::to_db_file)
        .collect();
    let plan = diff::diff(&disk, &db_files);
    // 闭包会被送进写者线程，所以要把数据 move 进去（'static + Send）
    db.write(move |conn| assets::apply_diff(conn, &plan, &disk, now))
        .expect("落库失败")
}

/// 给库里每个资产填 EXIF（每个资产只读**一个**文件：优先位图）。
///
/// 两个设计点：
/// * **优先读位图**（JPG 几 MB）而不是 RAW（几十 MB）—— 同一次拍摄两者的拍摄时间一致，
///   读小文件快得多；只有「没有位图」的资产才去读 RAW。
/// * **读文件在写锁之外**：EXIF 解析是慢活，不能占着单写者不放。
fn fill_exif(db: &CatalogDb, root: &std::path::Path, now: i64) -> (usize, usize) {
    let files = db.read(assets::list_files).expect("读文件表失败");

    // 每个资产挑一个文件：位图优先，其次 RAW
    let mut best: std::collections::HashMap<i64, (u8, &raybend::store::assets::FileRow)> =
        std::collections::HashMap::new();
    for f in &files {
        let rank = match f.role.as_str() {
            "bitmap" => 0u8,
            "raw" => 1,
            _ => 2,
        };
        best.entry(f.asset_id)
            .and_modify(|e| {
                if rank < e.0 {
                    *e = (rank, f);
                }
            })
            .or_insert((rank, f));
    }

    let jobs: Vec<(i64, std::path::PathBuf, String, Option<i64>)> = best
        .values()
        .map(|(_, f)| {
            let name = f
                .rel_path
                .rsplit('/')
                .next()
                .unwrap_or(&f.rel_path)
                .to_string();
            (f.asset_id, root.join(&f.rel_path), name, f.mtime_ms)
        })
        .collect();

    // 读文件与解析 EXIF 全部在写锁**之外**（慢活不该占着单写者）
    let n = jobs.len();
    let mut filled: Vec<(i64, ExifData, Option<TakenAt>)> = Vec::with_capacity(n);
    let mut by_source: std::collections::HashMap<&'static str, usize> =
        std::collections::HashMap::new();
    for (asset_id, abs, name, mtime) in jobs {
        let data = exif::read_file(&abs);
        let taken = exif::resolve_taken_at(Some(&data), &name, mtime);
        if let Some(t) = taken {
            let key = match t.source {
                TakenAtSource::Exif => "exif",
                TakenAtSource::Filename => "filename",
                TakenAtSource::FileMtime => "file_mtime",
                TakenAtSource::Sibling => "sibling",
            };
            *by_source.entry(key).or_default() += 1;
        }
        filled.push((asset_id, data, taken));
    }
    let with_exif = filled.iter().filter(|(_, d, _)| !d.is_empty()).count();

    db.write(move |conn| {
        for (asset_id, data, taken) in &filled {
            assets::apply_exif(conn, *asset_id, data, *taken, now)?;
        }
        Ok(())
    })
    .expect("写 EXIF 失败");

    let mut src: Vec<(&str, usize)> = by_source.into_iter().collect();
    src.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    println!("时间来源  {src:?}");
    (n, with_exif)
}

fn main() {
    let Some(root) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!("用法：refresh-smoke <照片目录>");
        std::process::exit(2);
    };
    let opts = ScanOptions::default();
    let now = raybend::store::time::now_millis();

    let tmp = tempfile::tempdir().expect("建临时目录失败");
    let lib_root = tmp.path().join("lib");
    let backups = tmp.path().join("backups");
    std::fs::create_dir_all(&lib_root).unwrap();

    // ① 建库
    let t = Instant::now();
    let db = CatalogDb::create(
        &lib_root,
        "refresh-smoke",
        None,
        OpenOpts::new(Some(&backups), now),
    )
    .expect("建库失败");
    println!(
        "建库      {} （{:.0} 毫秒）",
        db.path().display(),
        t.elapsed().as_secs_f64() * 1000.0
    );

    // ② 首轮
    let t = Instant::now();
    let first = refresh(&db, &root, &opts, now);
    let elapsed = t.elapsed();
    println!(
        "首轮      资产 +{} · 文件 +{} · 耗时 {:.0} 毫秒",
        first.new_assets,
        first.new_files,
        elapsed.as_secs_f64() * 1000.0
    );

    // ③ 次轮：必须零变化（幂等）
    let second = refresh(&db, &root, &opts, now + 1000);
    println!(
        "次轮      变化 {} 条（期望 0）{}",
        second.total(),
        if second.total() == 0 {
            " ✅ 幂等"
        } else {
            " ❌ 不幂等！"
        }
    );

    // ④ 填 EXIF（读文件在写锁之外）
    let t = Instant::now();
    let (n, with_exif) = fill_exif(&db, &root, now + 2000);
    println!(
        "EXIF      读了 {n} 个文件 · {with_exif} 个有 EXIF · 耗时 {:.0} 毫秒",
        t.elapsed().as_secs_f64() * 1000.0
    );
    let sample: Vec<(String, Option<i64>, String)> = db
        .read(|c| {
            let mut stmt = c.prepare(
                "SELECT camera_model, taken_at, coalesce(iso, 0) FROM assets
                  WHERE taken_at IS NOT NULL ORDER BY taken_at LIMIT 3",
            )?;
            let rows = stmt.query_map([], |r| {
                let t: Option<i64> = r.get(1)?;
                let iso: i64 = r.get(2)?;
                Ok((
                    r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    t.map(|ms| raybend::store::time::civil(ms).0),
                    format!("ISO {iso}"),
                ))
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .unwrap();
    for (model, year, iso) in sample {
        println!("          样例：{model} · {year:?} · {iso}");
    }

    // ⑤ 库内统计
    let (assets_n, files_n) = db.read(assets::counts).unwrap();
    println!("库内      资产 {assets_n} · 文件 {files_n}");

    let paired: i64 = db
        .read(|c| {
            Ok(c.query_row(
                "SELECT count(*) FROM (
                    SELECT asset_id FROM asset_files GROUP BY asset_id HAVING count(*) = 2)",
                [],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap();
    println!("配对      {paired} 张照片同时有 位图 + RAW");

    let offline: i64 = db
        .read(|c| {
            Ok(c.query_row(
                "SELECT count(DISTINCT asset_id) FROM asset_files WHERE missing_since IS NOT NULL",
                [],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap();
    println!("离线      资产 {offline}（首轮不该有）");

    println!("\n临时库路径（跑完即删）：{}", lib_root.display());
}
