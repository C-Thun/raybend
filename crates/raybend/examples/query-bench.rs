//! 查询引擎基准：10 万张合成照片上的筛选 / 排序 / 分页 / 检索。
//!
//! ```bash
//! cargo run -q --release -p raybend --example query-bench
//! cargo run -q --release -p raybend --example query-bench -- --rows 300000
//! cargo run -q --release -p raybend --example query-bench -- --plan   # 附带 EXPLAIN QUERY PLAN
//! ```
//!
//! 判据（M2 完成定义）：**10 万张库中筛选响应 < 100ms**（感知即时）。
//! 这里量的是「从 SQLite 拿到数据」的净时间 —— 不含 IPC 与渲染，
//! 那两段另有冒烟与人类体感来覆盖。
//!
//! 顺带产出：`EXPLAIN QUERY PLAN` 输出。**索引不是拍脑袋加的**，
//! 而是看这里有没有 `SCAN`（全表扫）再决定（见 `plans/M2.md` 步骤 2.4）。

use std::time::Instant;

use raybend::store::migration::{self, DbKind};
use raybend::store::pragma;
use raybend::store::query::{Combinator, Query, Scope, Sort, SortKey};
use rusqlite::Connection;

const T0: i64 = 1_789_516_800_000; // 2026-09 前后

fn main() {
    let mut rows = 100_000usize;
    let mut show_plan = false;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--rows" => {
                i += 1;
                rows = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(rows);
            }
            "--plan" => show_plan = true,
            other => eprintln!("忽略未知参数：{other}"),
        }
        i += 1;
    }

    let conn = open_and_fill(rows);

    println!("合成库：{rows} 张照片\n");

    // 每条用例：名字 + 查询 + 期望的「合理量级」
    let cases: Vec<(&str, Query)> = vec![
        ("整库计数", Query::new(Scope::Repository)),
        (
            "目录子树计数（一个导入日）",
            Query::new(Scope::subtree("photos/2026-08-15")),
        ),
        ("筛选：3 星及以上", star_filter()),
        ("筛选：红标", color_filter()),
        ("筛选：3 星 且 红标", and_filter()),
        ("筛选：近 30 天", recent_filter()),
        ("筛选：某机型 + ISO ≥ 3200", gear_filter()),
        ("检索：中文文件名", text_filter("婚礼现场")),
        ("检索：短词（LIKE 兜底）", text_filter("我的")),
    ];

    let mut worst = 0.0f64;
    let mut worst_name = String::new();

    for (name, query) in &cases {
        let total = raybend::store::query::count(&conn, query).expect("计数失败");
        let (p50, p95) = time_it(12, || {
            let _ = raybend::store::query::count(&conn, query).expect("计数失败");
        });
        let _ = total;
        if p95 > worst {
            worst = p95;
            worst_name = (*name).to_string();
        }
        println!(
            "{:<28} 命中 {:>7} 张  计数 P50 {:>7.2} ms  P95 {:>7.2} ms",
            name, total, p50, p95
        );
    }

    // 分页：第一页（网格打开时）与深翻（滚动到中段）
    let query = Query::new(Scope::Repository);
    for (label, offset) in [("第一页", 0usize), ("深翻到第 5 万张", 50_000)] {
        let (p50, p95) = time_it(12, || {
            let _ = raybend::store::query::page(&conn, &query, offset, 120).expect("取页失败");
        });
        if p95 > worst {
            worst = p95;
            worst_name = format!("分页：{label}");
        }
        println!(
            "{:<28} 120 行          取页 P50 {:>7.2} ms  P95 {:>7.2} ms",
            format!("分页：{label}"),
            p50,
            p95
        );
    }

    // 时间线（前端算分组与键盘导航顺序用）
    let (p50, p95) = time_it(6, || {
        let _ = raybend::store::query::timeline(&conn, &query, 0).expect("取时间线失败");
    });
    println!(
        "{:<28} id+时间 全量      P50 {:>7.2} ms  P95 {:>7.2} ms",
        "时间线", p50, p95
    );

    // 分面（筛选面板要用）
    let (p50, p95) = time_it(6, || {
        let _ = raybend::store::query::facets(&conn, &query).expect("分面失败");
    });
    println!(
        "{:<28} 4 个维度          P50 {:>7.2} ms  P95 {:>7.2} ms",
        "分面统计", p50, p95
    );

    // 排序开销（换个排序键）
    let mut by_name = Query::new(Scope::Repository);
    by_name.sort = Sort::new(SortKey::FileName, false);
    let (p50, p95) = time_it(8, || {
        let _ = raybend::store::query::page(&conn, &by_name, 0, 120).expect("按名取页失败");
    });
    println!(
        "{:<28} 120 行           P50 {:>7.2} ms  P95 {:>7.2} ms",
        "排序：按文件名", p50, p95
    );

    println!("\n最慢的一条：{worst_name}（P95 {worst:.2} ms）");
    if worst < 100.0 {
        println!("✅ 全部远低于 100ms 的判据");
    } else {
        println!("⚠️ 有查询超过 100ms —— 看 `--plan` 的 EXPLAIN 输出决定加什么索引");
    }

    if show_plan {
        println!("\n── EXPLAIN QUERY PLAN ──");
        for (name, query) in &cases {
            println!("\n[{name}]");
            plan(&conn, query);
        }
    }
}

fn open_and_fill(rows: usize) -> Connection {
    let mut conn = Connection::open_in_memory().expect("内存库");
    pragma::apply(&conn, false).expect("PRAGMA");
    migration::apply(&mut conn, DbKind::Catalog, migration::Backups::none(), T0).expect("迁移");

    let started = Instant::now();
    // 造一张 1..rows 的种子表（临时表也要提交，否则回滚后就不见了）
    {
        let tx = conn.transaction().expect("事务");
        tx.execute_batch("CREATE TEMP TABLE seed(n INTEGER PRIMARY KEY);")
            .expect("建种子表");
        tx.execute(
            "WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM seq WHERE n < ?1)
             INSERT INTO seed(n) SELECT n FROM seq",
            [i64::try_from(rows).unwrap_or(i64::MAX)],
        )
        .expect("填种子表");
        tx.commit().expect("提交种子表");
    }

    {
        let tx = conn.transaction().expect("事务");
        tx.execute(
            &format!(
                "INSERT INTO assets
                   (taken_at, taken_at_source, camera_make, camera_model, lens, focal_mm, f_number,
                    exposure_ms, iso, width, height, orientation, rating, color_label, like_state,
                    lock_level, imported_at, updated_at)
                 SELECT
                   {T0} + n * 60000,                       -- 每张差 1 分钟
                   'exif',
                   CASE n % 3 WHEN 0 THEN 'Panasonic' WHEN 1 THEN 'Canon' ELSE 'Fujifilm' END,
                   CASE n % 3 WHEN 0 THEN 'DC-G9' WHEN 1 THEN 'EOS R5' ELSE 'X-T5' END,
                   CASE n % 5 WHEN 0 THEN 'LUMIX 12-35mm' WHEN 1 THEN 'RF 24-105mm'
                              WHEN 2 THEN 'XF 16-55mm' WHEN 3 THEN 'LUMIX 35-100mm'
                              ELSE NULL END,
                   CASE n % 7 WHEN 0 THEN 12.0 WHEN 1 THEN 24.0 WHEN 2 THEN 35.0
                              WHEN 3 THEN 50.0 WHEN 4 THEN 85.0 WHEN 5 THEN 200.0 ELSE NULL END,
                   2.8,
                   1.0 / 250.0 * 1000.0,
                   CASE n % 6 WHEN 0 THEN 100 WHEN 1 THEN 200 WHEN 2 THEN 400
                              WHEN 3 THEN 800 WHEN 4 THEN 3200 ELSE 6400 END,
                   5184, 3888, 1,
                   CASE n % 11 WHEN 0 THEN 5 WHEN 1 THEN 4 WHEN 2 THEN 3 ELSE 0 END,
                   CASE n % 9 WHEN 0 THEN 'red' WHEN 1 THEN 'yellow' WHEN 2 THEN 'green'
                               WHEN 3 THEN 'blue' WHEN 4 THEN 'purple' ELSE NULL END,
                   CASE n % 13 WHEN 0 THEN 'like' WHEN 1 THEN 'dislike' ELSE NULL END,
                   CASE n % 37 WHEN 0 THEN 2 WHEN 1 THEN 1 ELSE 0 END,
                   {T0}, {T0}
                 FROM seed WHERE n <= ?1",
                T0 = T0
            ),
            [i64::try_from(rows).unwrap_or(i64::MAX)],
        )
        .expect("插资产");

        tx.execute(
            &format!(
                "INSERT INTO asset_files
                   (asset_id, role, rel_path, rel_path_folded, ext, size_bytes, created_at, updated_at)
                 SELECT a.id + 0, 'bitmap',
                   'photos/2026-' || printf('%02d', 1 + (a.id % 12)) || '-' ||
                   printf('%02d', 1 + (a.id % 28)) || '/MY' || printf('%06d', a.id) || '.jpg',
                   'photos/2026-' || printf('%02d', 1 + (a.id % 12)) || '-' ||
                   printf('%02d', 1 + (a.id % 28)) || '/my' || printf('%06d', a.id) || '.jpg',
                   'jpg', 3000000, {T0}, {T0}
                 FROM assets a"
            ),
            [],
        )
        .expect("插文件");

        // 中文文件名：每 100 张一张，用来测 FTS
        tx.execute(
            "UPDATE asset_files SET rel_path = 'photos/2026-08-15/婚礼现场-' || id || '.jpg',
                                    rel_path_folded = 'photos/2026-08-15/婚礼现场-' || id || '.jpg'
             WHERE id % 100 = 0",
            [],
        )
        .expect("改中文名");

        tx.commit().expect("提交");
    }

    // 标签：每张挂一个（测标签筛选）
    {
        let tx = conn.transaction().expect("事务");
        tx.execute(
            "INSERT INTO asset_tags (asset_id, tag_id, tagged_at)
             SELECT id, (id % 20) + 1, ?1 FROM assets",
            [T0],
        )
        .expect("插标签");
        tx.commit().expect("提交");
    }

    // 全文索引：整份重建（与生产路径一致）
    let n = raybend::store::fts::rebuild(&conn).expect("建索引");
    println!(
        "建库耗时 {:.1} 秒（含全文索引 {n} 行）",
        started.elapsed().as_secs_f64()
    );
    conn
}

fn star_filter() -> Query {
    let mut q = Query::new(Scope::Repository);
    q.filter.ratings = vec![3, 4, 5];
    q.filter.combinator = Combinator::And;
    q
}

fn color_filter() -> Query {
    let mut q = Query::new(Scope::Repository);
    q.filter.colors = vec!["red".to_string()];
    q
}

fn and_filter() -> Query {
    let mut q = Query::new(Scope::Repository);
    q.filter.ratings = vec![5];
    q.filter.colors = vec!["red".to_string()];
    q.filter.combinator = Combinator::And;
    q
}

fn recent_filter() -> Query {
    let mut q = Query::new(Scope::Repository);
    q.filter.combinator = Combinator::And;
    // 从最长的一头开始：10 万张 × 1 分钟 ≈ 69 天，取最后 30 天
    q.filter.taken_from = Some(T0 + 60 * 24 * 60 * 60 * 1000);
    q
}

fn gear_filter() -> Query {
    let mut q = Query::new(Scope::Repository);
    q.filter.combinator = Combinator::And;
    q.filter.cameras = vec!["Panasonic|DC-G9".to_string()];
    q.filter.iso_from = Some(3200);
    q
}

fn text_filter(text: &str) -> Query {
    let mut q = Query::new(Scope::Repository);
    q.filter.text = Some(text.to_string());
    q
}

/// 跑 `runs` 次，返回 (P50, P95)，单位毫秒。
fn time_it<F: FnMut()>(runs: usize, mut f: F) -> (f64, f64) {
    // 先热身一次（页面缓存、语句编译）
    f();
    let mut samples: Vec<f64> = Vec::with_capacity(runs);
    for _ in 0..runs {
        let started = Instant::now();
        f();
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    (percentile(&samples, 50.0), percentile(&samples, 95.0))
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = ((p / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

fn plan(conn: &Connection, query: &Query) {
    // 借用内部：where_clause 是私有的，这里用与它同样的方式重建一次 SQL
    let sql = format!(
        "SELECT a.id FROM assets a LEFT JOIN asset_files f ON f.asset_id = a.id \
         AND f.role = (SELECT MIN(role) FROM asset_files WHERE asset_id = a.id) {} ORDER BY {}",
        query_sql_fragment(query),
        ""
    );
    let mut stmt = match conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")) {
        Ok(s) => s,
        Err(e) => {
            println!("  （无法解释：{e}）");
            return;
        }
    };
    let rows = stmt.query_map([], |r| r.get::<_, String>(3));
    match rows {
        Ok(rows) => {
            for row in rows.flatten() {
                println!("  {row}");
            }
        }
        Err(e) => println!("  （无法解释：{e}）"),
    }
}

/// 只给 `--plan` 用：把筛选拼成人能读的片段（**不参与实际查询**）。
fn query_sql_fragment(query: &Query) -> String {
    if query.filter.is_empty() {
        return String::new();
    }
    let mut bits: Vec<String> = Vec::new();
    if !query.filter.ratings.is_empty() {
        bits.push("rating IN (…)".to_string());
    }
    if !query.filter.colors.is_empty() {
        bits.push("color_label IN (…)".to_string());
    }
    if !query.filter.cameras.is_empty() {
        bits.push("camera IN (…)".to_string());
    }
    if query.filter.iso_from.is_some() {
        bits.push("iso >= ?".to_string());
    }
    if query.filter.taken_from.is_some() {
        bits.push("taken_at >= ?".to_string());
    }
    if query.filter.text.is_some() {
        bits.push("id IN (SELECT rowid FROM assets_fts WHERE assets_fts MATCH ?)".to_string());
    }
    format!("WHERE {}", bits.join(" AND "))
}
