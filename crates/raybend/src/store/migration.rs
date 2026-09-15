//! 迁移执行器：**版本闸门 + 迁移前快照 + 逐条事务 + 完整性检查**。
//!
//! 为什么这四件事必须一开始就有（`AGENTS.md` §8 第 5 条：catalog 损坏/升级丢数据
//! 是用户最不可原谅的失败）：
//!
//! 1. **版本闸门**：库的 schema 比程序新 → 拒绝打开。强行打开会写坏新版的表结构。
//! 2. **迁移前快照**：`VACUUM INTO` 出一份完整副本（保留最近 [`BACKUP_KEEP`] 份），
//!    迁移失败或被新版本程序改坏时还有退路 —— 快照本身是原子的、不阻塞读。
//! 3. **逐条事务**：每个迁移单独一个事务，失败整条回滚且 `user_version` 不前进；
//!    绝不允许「迁移跑了一半」的库存在。
//! 4. **完整性检查**：迁移后跑 `foreign_key_check`，用外键把「迁移写坏数据」暴露出来。
//!
//! 迁移是**只向前**的：没有 down 迁移。要回退就用快照。

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::{Error, Result};

use super::time;

/// 库的种类 —— 决定快照文件名前缀与适用哪一套迁移。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbKind {
    /// 全局库 `app.db`（设置、库注册表、任务队列）。
    App,
    /// 每库的 `catalog.db`。
    Catalog,
    /// 缩略图缓存库 `thumbs.db`（派生数据：**删掉即可重建**，见 AGENTS.md §6.5）。
    Thumbs,
}

impl DbKind {
    /// 快照文件名前缀。
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Catalog => "catalog",
            Self::Thumbs => "thumbs",
        }
    }

    /// 中文名（用于提示与日志）。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::App => "应用库",
            Self::Catalog => "照片库",
            Self::Thumbs => "缩略图缓存",
        }
    }

    /// 这一类库的迁移列表。
    #[must_use]
    pub const fn migrations(self) -> &'static [Migration] {
        match self {
            Self::App => APP_MIGRATIONS,
            Self::Catalog => CATALOG_MIGRATIONS,
            Self::Thumbs => THUMBS_MIGRATIONS,
        }
    }
}

/// 一条迁移：版本号 + 名字（日志用）+ 内嵌的 SQL。
///
/// SQL 用 `include_str!` 嵌进二进制 —— 单文件分发时不会漏带 `.sql`。
#[derive(Debug)]
pub struct Migration {
    /// 目标版本号（执行成功后写入 `PRAGMA user_version`）。
    pub version: i64,
    /// 简短名字（日志、错误信息用）。
    pub name: &'static str,
    /// 迁移 SQL（可含多条语句）。
    pub sql: &'static str,
}

/// `app.db` 的迁移列表。**版本必须从 1 开始连续递增**（有测试守着）。
///
/// * v1 `init`：库注册表 / 路径 / 设置 / 任务队列 / 应用元信息
/// * v2 `tags`：**标签词典**（跨库公用，BROWSE.md §7.1）
pub const APP_MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "init",
        sql: include_str!("migrations/app_0001_init.sql"),
    },
    Migration {
        version: 2,
        name: "tags",
        sql: include_str!("migrations/app_0002_tags.sql"),
    },
];

/// `catalog.db` 的迁移列表。**版本必须从 1 开始连续递增**。
///
/// * v1 `init`：库元信息 / 资产 / 文件 / 全文索引 / 序号 / 导入批次
/// * v2 `marking_tags_geo`：色标·喜欢·锁 / 作者·描述·地理 / EXIF 时区 /
///   资产↔标签关联 / 全文索引加 `description`（BROWSE.md §3·§7·§9）
pub const CATALOG_MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "init",
        sql: include_str!("migrations/catalog_0001_init.sql"),
    },
    Migration {
        version: 2,
        name: "marking_tags_geo",
        sql: include_str!("migrations/catalog_0002_marking_tags_geo.sql"),
    },
];

/// 缩略图缓存库 `thumbs.db` 的迁移列表。
///
/// 缓存**不做快照**（派生数据，最坏情况就是重新生成一遍）。
pub const THUMBS_MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "init",
    sql: include_str!("migrations/thumbs_0001_init.sql"),
}];

/// 每类库保留的快照份数（`AGENTS.md` §6.4：保留 7 份）。**按库分别保留**。
pub const BACKUP_KEEP: usize = 7;

/// 快照策略：往哪放、以及文件名里怎么区分。
///
/// `app.db` 与所有 `catalog.db` 的快照**共用一个备份目录**（`<app data>/backups/`，
/// 见 `plans/M1-2.md` §3.5），所以 catalog 必须带上库里身份的短标识 ——
/// 否则轮转时会把别的库的备份挤掉，那是最不该发生的事之一。
#[derive(Debug, Clone, Copy, Default)]
pub struct Backups<'a> {
    /// 备份目录；`None` = 不备份（只该在测试里这么用）。
    pub dir: Option<&'a Path>,
    /// 文件名里的区分标签（例如库 ID 前缀）。
    pub label: Option<&'a str>,
}

impl<'a> Backups<'a> {
    /// 不备份。
    #[must_use]
    pub const fn none() -> Self {
        Self {
            dir: None,
            label: None,
        }
    }

    /// 备份到 `dir`（`app.db` 用）。
    #[must_use]
    pub const fn at(dir: &'a Path) -> Self {
        Self {
            dir: Some(dir),
            label: None,
        }
    }

    /// 备份到 `dir`，文件名带 `label`（`catalog.db` 用，label 取库 ID 前缀）。
    #[must_use]
    pub const fn labelled(dir: &'a Path, label: &'a str) -> Self {
        Self {
            dir: Some(dir),
            label: Some(label),
        }
    }

    /// 文件名前缀（形如 `app` / `catalog` / `catalog_Ab3xY9zQ`）。
    fn group(&self, kind: DbKind) -> String {
        match self.label {
            Some(l) if !l.is_empty() => format!("{}_{}", kind.prefix(), l),
            _ => kind.prefix().to_string(),
        }
    }
}

/// 一次迁移的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationOutcome {
    /// 迁移前的版本。
    pub from: i64,
    /// 迁移后的版本。
    pub to: i64,
    /// 实际执行的迁移版本号（升序）。
    pub applied: Vec<i64>,
    /// 迁移前生成的快照文件（全新库没有快照）。
    pub snapshot: Option<PathBuf>,
}

impl MigrationOutcome {
    /// 是否真的做了升级。
    #[must_use]
    pub fn changed(&self) -> bool {
        self.from != self.to
    }
}

/// 读库当前的 schema 版本（`PRAGMA user_version`）。
pub fn schema_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

/// 本程序对该类库支持的 schema 版本（迁移列表里的最大值）。
#[must_use]
pub fn supported_version(kind: DbKind) -> i64 {
    kind.migrations()
        .iter()
        .map(|m| m.version)
        .max()
        .unwrap_or(0)
}

/// 按种类执行迁移。这是生产路径的唯一入口。
///
/// `backup_dir` 为 `None` 时**不做快照** —— 只应在测试里这么用。
pub fn apply(
    conn: &mut Connection,
    kind: DbKind,
    backups: Backups<'_>,
    now_ms: i64,
) -> Result<MigrationOutcome> {
    apply_list(conn, kind, kind.migrations(), backups, now_ms)
}

/// 用一份**给定的**迁移列表执行（测试用；生产走 [`apply`]）。
fn apply_list(
    conn: &mut Connection,
    kind: DbKind,
    migrations: &[Migration],
    backups: Backups<'_>,
    now_ms: i64,
) -> Result<MigrationOutcome> {
    let current = schema_version(conn)?;
    let target = migrations.iter().map(|m| m.version).max().unwrap_or(0);

    // ① 版本闸门：库比程序新 → 拒绝打开
    if current > target {
        return Err(Error::SchemaTooNew {
            found: current,
            supported: target,
        });
    }

    if current == target {
        return Ok(MigrationOutcome {
            from: current,
            to: target,
            applied: Vec::new(),
            snapshot: None,
        });
    }

    // ② 迁移前快照：只在「已有数据」时做（全新库没什么可备份的）
    let snapshot = match (current, backups.dir) {
        (from @ 1.., Some(dir)) => Some(create_snapshot(conn, dir, kind, &backups, from, now_ms)?),
        _ => None,
    };

    // ③ 逐条事务。迁移期间关掉外键（允许重建表），结束后再打开并检查。
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let applied = run_all(conn, migrations, current);
    conn.pragma_update(None, "foreign_keys", "ON")?;
    let applied = applied?;

    // ④ 完整性检查：外键约束没被迁移破坏
    let violations = foreign_key_violations(conn)?;
    if !violations.is_empty() {
        return Err(Error::IntegrityCheck(violations.join("; ")));
    }

    Ok(MigrationOutcome {
        from: current,
        to: target,
        applied,
        snapshot,
    })
}

/// 从 `current` 之后开始逐条执行；任一条失败即返回错误（该条已回滚）。
fn run_all(conn: &mut Connection, migrations: &[Migration], current: i64) -> Result<Vec<i64>> {
    let mut applied = Vec::new();
    for m in migrations.iter().filter(|m| m.version > current) {
        let tx = conn.transaction()?;
        tx.execute_batch(m.sql).map_err(|e| Error::Migration {
            version: m.version,
            source: e,
        })?;
        tx.pragma_update(None, "user_version", m.version)?;
        tx.commit()?;
        applied.push(m.version);
    }
    Ok(applied)
}

/// 生成一份 `VACUUM INTO` 快照，并按 [`BACKUP_KEEP`] 轮转。
fn create_snapshot(
    conn: &Connection,
    dir: &Path,
    kind: DbKind,
    backups: &Backups<'_>,
    from: i64,
    now_ms: i64,
) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let name = format!(
        "{group}_v{from}_{stamp}.db",
        group = backups.group(kind),
        stamp = time::format_compact(now_ms)
    );
    let path = dir.join(name);
    // 同一秒内迁移两次会撞名：`VACUUM INTO` 拒绝覆盖已存在的文件，所以再加个后缀
    let path = unique_path(&path);
    // VACUUM INTO 接受绑定参数；它要求目标文件不存在、且不能在事务里执行
    conn.execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()])?;
    rotate_backups(dir, kind, &backups.group(kind), BACKUP_KEEP)?;
    Ok(path)
}

/// 若目标已存在，追加 `-2`、`-3`…… 直到不冲突。
fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let ext = path
        .extension()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    for i in 2..1000 {
        let candidate = path.with_file_name(format!("{stem}-{i}.{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    path.to_path_buf()
}

/// 轮转快照：**每一组**（同一个库）只保留文件名最新的 `keep` 份，返回删除数量。
///
/// 文件名形如 `catalog_Ab3xY9zQ_v1_20260915-143002Z.db`：
/// 分组 = `_v` 之前的部分，时间戳定宽 ⇒ 组内字典序即时间序。
///
/// **必须分组轮转**：所有库共用一个备份目录，若按总量淘汰，
/// 一个频繁升级的库会把别的库唯一的备份挤掉。
pub fn rotate_backups(dir: &Path, kind: DbKind, group: &str, keep: usize) -> Result<usize> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(0); // 目录不存在 = 没什么可轮转的
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_snapshot_of(p, kind, group))
        .collect();
    if files.len() <= keep {
        return Ok(0);
    }
    files.sort(); // 时间戳定宽 ⇒ 字典序 = 时间序
    let mut removed = 0;
    for old in &files[..files.len() - keep] {
        if std::fs::remove_file(old).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// 解析快照文件名 → `(库种类, 分组名)`；不是快照则 `None`。
///
/// 文件名形如 `app_v1_20260915-143002Z.db`，或 `catalog_Ab3xY9zQ_v2_20260915-143002Z.db`
/// （多库共用一个备份目录时靠分组名区分）。
fn snapshot_group(path: &Path) -> Option<(DbKind, String)> {
    let name = path.file_name()?.to_string_lossy();
    let rest = name.strip_suffix(".db")?;
    let (group, tail) = rest.split_once("_v")?;
    let (version, _stamp) = tail.split_once('_')?;
    if version.is_empty() || !version.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let kind = if group == "app" || group.starts_with("app_") {
        DbKind::App
    } else if group == "catalog" || group.starts_with("catalog_") {
        DbKind::Catalog
    } else {
        return None;
    };
    Some((kind, group.to_string()))
}

/// 这个文件是不是 `kind` 这一类里 `group` 那一组的快照。
fn is_snapshot_of(path: &Path, kind: DbKind, group: &str) -> bool {
    path.is_file() && snapshot_group(path).is_some_and(|(k, g)| k == kind && g == group)
}

/// 列出某类库的**所有**快照文件（新的在前）——诊断/设置界面用。
pub fn list_backups(dir: &Path, kind: DbKind) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| snapshot_group(p).is_some_and(|(k, _)| k == kind))
        .collect();
    files.sort();
    files.reverse();
    files
}

/// `PRAGMA foreign_key_check` 的结果（每项形如 `asset_files(rowid=12)`）。
fn foreign_key_violations(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
    let rows = stmt.query_map([], |r| {
        let table: String = r.get(0)?;
        let rowid: i64 = r.get(1)?;
        Ok(format!("{table}(rowid={rowid})"))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::pragma;

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().expect("内存库");
        pragma::apply(&conn, false).expect("PRAGMA");
        conn
    }

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().expect("临时目录")
    }

    // ---------- 真实迁移列表 ----------

    #[test]
    fn migration_lists_are_contiguous_from_one() {
        for kind in [DbKind::App, DbKind::Catalog, DbKind::Thumbs] {
            let versions: Vec<i64> = kind.migrations().iter().map(|m| m.version).collect();
            let expected: Vec<i64> = (1..=versions.len() as i64).collect();
            assert_eq!(versions, expected, "{:?} 的版本号必须从 1 连续递增", kind);
            assert_eq!(supported_version(kind), versions.len() as i64);
        }
    }

    #[test]
    fn fresh_app_db_applies_all_migrations() {
        let mut conn = mem();
        let out = apply(&mut conn, DbKind::App, Backups::none(), 1_789_516_800_000).unwrap();
        assert_eq!((out.from, out.to), (0, 2));
        assert_eq!(out.applied, vec![1, 2]);
        assert!(out.snapshot.is_none(), "全新库不需要快照");
        assert!(out.changed());

        // 表都建出来了
        for table in [
            "repositories",
            "repository_paths",
            "settings",
            "jobs",
            "app_meta",
            "tags",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "缺表：{table}");
        }
    }

    #[test]
    fn fresh_catalog_db_applies_all_migrations() {
        let mut conn = mem();
        let out = apply(
            &mut conn,
            DbKind::Catalog,
            Backups::none(),
            1_789_516_800_000,
        )
        .unwrap();
        assert_eq!((out.from, out.to), (0, 2));
        assert_eq!(out.applied, vec![1, 2]);
        for table in [
            "repository_meta",
            "assets",
            "asset_files",
            "seq_counters",
            "import_runs",
            "import_items",
            "assets_fts",
            "asset_tags",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "缺表：{table}");
        }
    }

    #[test]
    fn applying_twice_is_a_noop() {
        let mut conn = mem();
        apply(&mut conn, DbKind::Catalog, Backups::none(), 0).unwrap();
        let again = apply(&mut conn, DbKind::Catalog, Backups::none(), 0).unwrap();
        assert_eq!(again.applied, Vec::<i64>::new());
        assert!(!again.changed());
        // 不写死版本号：以后再加迁移，这条测试不该跟着改
        assert_eq!(
            schema_version(&conn).unwrap(),
            supported_version(DbKind::Catalog)
        );
    }

    #[test]
    fn rejects_database_from_a_newer_program() {
        let mut conn = mem();
        apply(&mut conn, DbKind::Catalog, Backups::none(), 0).unwrap();
        conn.pragma_update(None, "user_version", 99_i64).unwrap();

        let err = apply(&mut conn, DbKind::Catalog, Backups::none(), 0).unwrap_err();
        match err {
            Error::SchemaTooNew { found, supported } => {
                assert_eq!((found, supported), (99, supported_version(DbKind::Catalog)));
            }
            other => panic!("应当是 SchemaTooNew，实际是 {other:?}"),
        }
    }

    // ---------- 用假迁移列表测升级路径 ----------

    const FAKE_V1: Migration = Migration {
        version: 1,
        name: "v1",
        sql: "CREATE TABLE t1(id INTEGER PRIMARY KEY, name TEXT);",
    };
    const FAKE_V2: Migration = Migration {
        version: 2,
        name: "v2",
        sql: "ALTER TABLE t1 ADD COLUMN extra TEXT;",
    };
    const FAKE_V3: Migration = Migration {
        version: 3,
        name: "v3",
        sql: "CREATE INDEX idx_t1_name ON t1(name);",
    };
    const FAKE_BAD: Migration = Migration {
        version: 99,
        name: "bad",
        sql: "CREATE TABLE half_done(x); THIS IS NOT SQL;",
    };

    #[test]
    fn upgrades_step_by_step_and_snapshots_first() {
        let dir = tmp();
        let db = dir.path().join("catalog.db");
        let backups = dir.path().join("backups");

        let mut conn = Connection::open(&db).unwrap();
        pragma::apply(&conn, false).unwrap();
        apply_list(&mut conn, DbKind::Catalog, &[FAKE_V1], Backups::none(), 0).unwrap();
        conn.execute("INSERT INTO t1(name) VALUES ('a')", [])
            .unwrap();

        let out = apply_list(
            &mut conn,
            DbKind::Catalog,
            &[FAKE_V1, FAKE_V2, FAKE_V3],
            Backups::at(&backups),
            1_789_516_800_000,
        )
        .unwrap();

        assert_eq!((out.from, out.to), (1, 3));
        assert_eq!(out.applied, vec![2, 3]);
        // 快照存在、且是**升级前**的状态（没有 extra 列）
        let snap = out.snapshot.expect("应当有快照");
        assert!(snap.exists());
        let snap_conn = Connection::open(&snap).unwrap();
        let cols: Vec<String> = snap_conn
            .prepare("PRAGMA table_info(t1)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(cols, vec!["id", "name"], "快照应是升级前的结构");
        // 原库已升级，且数据还在
        let extra: i64 = conn
            .query_row(
                "SELECT count(*) FROM pragma_table_info('t1') WHERE name='extra'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(extra, 1);
        let n: i64 = conn
            .query_row("SELECT count(*) FROM t1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn failed_migration_rolls_back_completely() {
        let dir = tmp();
        let db = dir.path().join("catalog.db");
        let mut conn = Connection::open(&db).unwrap();
        pragma::apply(&conn, false).unwrap();
        apply_list(&mut conn, DbKind::Catalog, &[FAKE_V1], Backups::none(), 0).unwrap();

        let err = apply_list(
            &mut conn,
            DbKind::Catalog,
            &[
                FAKE_V1,
                Migration {
                    version: 2,
                    name: "bad",
                    sql: FAKE_BAD.sql,
                },
            ],
            Backups::none(),
            0,
        )
        .unwrap_err();
        match err {
            Error::Migration { version, .. } => assert_eq!(version, 2),
            other => panic!("应当是 Migration 错误，实际是 {other:?}"),
        }

        // 版本没前进，半成品表也没留下
        assert_eq!(schema_version(&conn).unwrap(), 1);
        let half: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='half_done'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(half, 0, "失败的迁移必须整条回滚");
    }

    #[test]
    fn foreign_key_violation_after_migration_is_reported() {
        let mut conn = mem();
        const V1: Migration = Migration {
            version: 1,
            name: "v1",
            sql: "CREATE TABLE parent(id INTEGER PRIMARY KEY);
                  CREATE TABLE child(id INTEGER PRIMARY KEY, pid INTEGER REFERENCES parent(id));",
        };
        apply_list(&mut conn, DbKind::Catalog, &[V1], Backups::none(), 0).unwrap();

        const V2: Migration = Migration {
            version: 2,
            name: "v2",
            sql: "INSERT INTO child(id, pid) VALUES (1, 4242);", // 指向不存在的 parent
        };
        let err =
            apply_list(&mut conn, DbKind::Catalog, &[V1, V2], Backups::none(), 0).unwrap_err();
        assert!(
            matches!(err, Error::IntegrityCheck(_)),
            "应当报外键完整性错误，实际是 {err:?}"
        );
        // 提示里要能看出是哪张表
        assert!(err.to_string().contains("child"), "{err}");
    }

    #[test]
    fn foreign_keys_are_off_during_migration_and_on_after() {
        let mut conn = mem();
        const V1: Migration = Migration {
            version: 1,
            name: "v1",
            sql: "CREATE TABLE parent(id INTEGER PRIMARY KEY);
                  CREATE TABLE child(id INTEGER PRIMARY KEY, pid INTEGER REFERENCES parent(id));",
        };
        apply_list(&mut conn, DbKind::Catalog, &[V1], Backups::none(), 0).unwrap();
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1, "迁移结束后外键必须重新打开");
    }

    #[test]
    fn user_version_advances_one_step_per_migration() {
        let mut conn = mem();
        let out = apply_list(
            &mut conn,
            DbKind::Catalog,
            &[FAKE_V1, FAKE_V2, FAKE_V3],
            Backups::none(),
            0,
        )
        .unwrap();
        assert_eq!(out.applied, vec![1, 2, 3]);
        assert_eq!(schema_version(&conn).unwrap(), 3);
    }

    // ---------- 快照轮转 ----------

    #[test]
    fn rotation_keeps_newest_and_deletes_oldest() {
        let dir = tmp();
        // 造 9 份快照（时间递增）
        for i in 0..9 {
            let stamp = format!("2026091{i}-000000Z");
            std::fs::write(dir.path().join(format!("catalog_v1_{stamp}.db")), b"x").unwrap();
        }
        // 混入不该被动的文件
        std::fs::write(dir.path().join("app_v1_20260910-000000Z.db"), b"x").unwrap();
        std::fs::write(dir.path().join("随便一个文件.txt"), b"x").unwrap();

        let removed = rotate_backups(dir.path(), DbKind::Catalog, "catalog", 7).unwrap();
        assert_eq!(removed, 2);

        let left = list_backups(dir.path(), DbKind::Catalog);
        assert_eq!(left.len(), 7);
        // 新的还在，最老的两份没了
        assert!(left[0].to_string_lossy().contains("20260918"));
        assert!(
            left.iter()
                .all(|p| !p.to_string_lossy().contains("20260910-000000Z"))
        );
        // 别的种类与无关文件不受影响
        assert!(dir.path().join("app_v1_20260910-000000Z.db").exists());
        assert!(dir.path().join("随便一个文件.txt").exists());
    }

    #[test]
    fn rotation_is_per_library_not_global() {
        // 所有库共用一个备份目录，所以轮转必须按库分组：
        // 库 A 频繁升级不该把库 B 唯一的备份挤掉。
        let dir = tmp();
        for group in ["catalog_AAAA", "catalog_BBBB"] {
            for i in 0..3 {
                std::fs::write(
                    dir.path().join(format!("{group}_v1_2026090{i}-000000Z.db")),
                    b"x",
                )
                .unwrap();
            }
        }
        // 只轮转 A 组，保留 2 份
        assert_eq!(
            rotate_backups(dir.path(), DbKind::Catalog, "catalog_AAAA", 2).unwrap(),
            1
        );

        let a = list_backups(dir.path(), DbKind::Catalog)
            .into_iter()
            .filter(|p| p.to_string_lossy().contains("AAAA"))
            .count();
        let b = list_backups(dir.path(), DbKind::Catalog)
            .into_iter()
            .filter(|p| p.to_string_lossy().contains("BBBB"))
            .count();
        assert_eq!(a, 2, "A 组保留 2 份");
        assert_eq!(b, 3, "B 组完全不受影响");
    }

    #[test]
    fn snapshot_file_names_are_parsed_for_grouping() {
        let p = Path::new("/tmp/catalog_Ab3xY9zQ_v2_20260915-143002Z.db");
        assert_eq!(
            snapshot_group(p),
            Some((DbKind::Catalog, "catalog_Ab3xY9zQ".to_string()))
        );
        assert_eq!(
            snapshot_group(Path::new("/tmp/app_v1_20260915-143002Z.db")),
            Some((DbKind::App, "app".to_string()))
        );
        // 不是快照的东西一律不认
        for bad in [
            "/tmp/catalog.db",
            "/tmp/catalog_Ab3xY9zQ.db",
            "/tmp/随便_v1_20260915-143002Z.db",
            "/tmp/catalog_vX_20260915-143002Z.db",
        ] {
            assert_eq!(snapshot_group(Path::new(bad)), None, "不该认：{bad}");
        }
    }

    #[test]
    fn rotation_is_a_noop_when_under_limit() {
        let dir = tmp();
        std::fs::write(dir.path().join("catalog_v1_20260915-000000Z.db"), b"x").unwrap();
        assert_eq!(
            rotate_backups(dir.path(), DbKind::Catalog, "catalog", 7).unwrap(),
            0
        );
        assert_eq!(list_backups(dir.path(), DbKind::Catalog).len(), 1);
    }

    #[test]
    fn rotation_on_missing_directory_is_fine() {
        let dir = tmp();
        let nope = dir.path().join("不存在");
        assert_eq!(rotate_backups(&nope, DbKind::App, "app", 7).unwrap(), 0);
        assert!(list_backups(&nope, DbKind::App).is_empty());
    }

    #[test]
    fn snapshots_in_the_same_second_do_not_collide() {
        let dir = tmp();
        let db = dir.path().join("catalog.db");
        let backups = dir.path().join("backups");
        let mut conn = Connection::open(&db).unwrap();
        pragma::apply(&conn, false).unwrap();
        apply_list(&mut conn, DbKind::Catalog, &[FAKE_V1], Backups::none(), 0).unwrap();

        // 同一时刻（同一毫秒）连做三次升级 → 不能因为同名而失败
        const V2A: Migration = Migration {
            version: 2,
            name: "a",
            sql: "CREATE TABLE a(x);",
        };
        const V2B: Migration = Migration {
            version: 2,
            name: "b",
            sql: "CREATE TABLE b(x);",
        };
        const V2C: Migration = Migration {
            version: 2,
            name: "c",
            sql: "CREATE TABLE c(x);",
        };
        for v in [V2A, V2B, V2C] {
            // 每次先把版本退回去，模拟「另一个库」
            conn.pragma_update(None, "user_version", 1_i64).unwrap();
            apply_list(
                &mut conn,
                DbKind::Catalog,
                &[FAKE_V1, v],
                Backups::at(&backups),
                1_789_516_800_000,
            )
            .unwrap();
        }
        let snaps = list_backups(&backups, DbKind::Catalog);
        assert_eq!(snaps.len(), 3, "同一秒的三次快照都要保留：{snaps:?}");
    }

    // ---------- catalog schema 的实际可用性 ----------

    #[test]
    fn catalog_accepts_a_full_asset_with_bitmap_and_raw() {
        let mut conn = mem();
        apply(&mut conn, DbKind::Catalog, Backups::none(), 0).unwrap();
        conn.execute(
            "INSERT INTO repository_meta(key, value) VALUES ('repository_id', 'Ab3xY9zQ1mNp7Kd2')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO assets(taken_at, camera_model, lens, imported_at, updated_at)
             VALUES (1789516800000, 'NIKON Z 7II', 'NIKKOR Z 50/1.8 S', 1, 1)",
            [],
        )
        .unwrap();
        let asset_id = conn.last_insert_rowid();
        for (role, rel, ext) in [
            ("bitmap", "photos/2026-08-15/MYP0001.png", "png"),
            ("raw", "photos/2026-08-15/_RAW/MYP0001.ORF", "orf"),
        ] {
            conn.execute(
                "INSERT INTO asset_files(asset_id, role, rel_path, rel_path_folded, ext,
                                         size_bytes, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 100, 1, 1)",
                rusqlite::params![asset_id, role, rel, rel.to_lowercase(), ext],
            )
            .unwrap();
        }
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM asset_files WHERE asset_id=?1",
                [asset_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 2, "位图与 RAW 都要在，且挂在同一个资产下");
    }

    #[test]
    fn catalog_rejects_duplicate_path_regardless_of_case() {
        let mut conn = mem();
        apply(&mut conn, DbKind::Catalog, Backups::none(), 0).unwrap();
        conn.execute(
            "INSERT INTO assets(imported_at, updated_at) VALUES (1, 1)",
            [],
        )
        .unwrap();
        let insert = |rel: &str, folded: &str| {
            conn.execute(
                "INSERT INTO asset_files(asset_id, role, rel_path, rel_path_folded, ext, created_at, updated_at)
                 VALUES (1, 'bitmap', ?1, ?2, 'jpg', 1, 1)",
                rusqlite::params![rel, folded],
            )
        };
        insert("photos/a.JPG", "photos/a.jpg").unwrap();
        // 大小写不同的同一路径必须被拒绝（因为折叠后相同）
        assert!(insert("photos/A.jpg", "photos/a.jpg").is_err());
    }

    #[test]
    fn catalogue_marks_missing_files_and_identity() {
        let mut conn = mem();
        apply(&mut conn, DbKind::Catalog, Backups::none(), 0).unwrap();
        conn.execute(
            "INSERT INTO assets(imported_at, updated_at) VALUES (1,1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO asset_files(asset_id, role, rel_path, rel_path_folded, ext,
                                     volume_serial, file_id, missing_since, created_at, updated_at)
             VALUES (1, 'bitmap', 'photos/a.jpg', 'photos/a.jpg', 'jpg', 7, x'0102030405060708090a0b0c0d0e0f10', 123, 1, 1)",
            [],
        )
        .unwrap();
        let (vol, blob): (i64, Vec<u8>) = conn
            .query_row("SELECT volume_serial, file_id FROM asset_files", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        use crate::store::file_id::FileId;
        let id = FileId::from_blob(u64::try_from(vol).unwrap(), &blob).expect("BLOB 应当能还原");
        assert_eq!(id.as_blob()[0], 1);
        assert_eq!(id.as_blob()[15], 0x10);
    }

    #[test]
    fn seq_counters_are_independent_per_width() {
        // REPOSITORY.md §3.3：同一目录下不同位数各自计数
        let mut conn = mem();
        apply(&mut conn, DbKind::Catalog, Backups::none(), 0).unwrap();
        for w in [3, 4, 5] {
            conn.execute(
                "INSERT INTO seq_counters(directory, width, value, updated_at) VALUES ('photos/2026-08-15', ?1, 7, 1)",
                [w],
            )
            .unwrap();
        }
        assert!(conn.execute(
            "INSERT INTO seq_counters(directory, width, value, updated_at) VALUES ('photos/2026-08-15', 4, 8, 1)",
            []
        ).is_err(), "同一目录同一宽度只能有一条计数");
        let n: i64 = conn
            .query_row("SELECT count(*) FROM seq_counters", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 3);
    }

    #[test]
    fn fts5_trigram_finds_chinese_substrings() {
        // AGENTS.md §7.2：中文必须用 trigram 才搜得到
        let mut conn = mem();
        apply(&mut conn, DbKind::Catalog, Backups::none(), 0).unwrap();
        conn.execute(
            "INSERT INTO assets_fts(rowid, file_name, camera, lens)
             VALUES (1, '婚礼现场合影.jpg', 'NIKON Z 7II', 'NIKKOR Z 50/1.8 S'),
                    (2, 'product-shot.png', 'Canon EOS R5', 'RF 85mm F2')",
            [],
        )
        .unwrap();
        let hit = |q: &str| -> i64 {
            conn.query_row(
                "SELECT count(*) FROM assets_fts WHERE assets_fts MATCH ?1",
                [q],
                |r| r.get(0),
            )
            .unwrap_or(0)
        };
        assert_eq!(hit("婚礼现场"), 1, "中文子串应当搜得到");
        assert_eq!(hit("婚礼现"), 1, "三字是 trigram 的下限");
        assert_eq!(hit("婚礼现场合影"), 1, "更长的子串也搜得到");
        assert_eq!(
            hit("合影"),
            0,
            "**两字查询用 trigram 搜不到** → 短词必须走 LIKE 兜底"
        );
        assert_eq!(hit("现场"), 0, "同上");
        assert_eq!(hit("EOS"), 1, "英文型号应当搜得到");
        assert_eq!(hit("eos"), 1, "大小写不敏感");
        assert_eq!(hit("RF 85mm"), 1, "带空格的查询也行");
    }

    #[test]
    fn fts5_index_stays_in_sync_on_delete() {
        let mut conn = mem();
        apply(&mut conn, DbKind::Catalog, Backups::none(), 0).unwrap();
        conn.execute(
            "INSERT INTO assets_fts(rowid, file_name) VALUES (1, '婚礼现场.jpg')",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM assets_fts WHERE rowid = 1", [])
            .unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM assets_fts WHERE assets_fts MATCH '婚礼现场'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn app_db_cascades_repository_paths() {
        let mut conn = mem();
        apply(&mut conn, DbKind::App, Backups::none(), 0).unwrap();
        conn.execute(
            "INSERT INTO repositories(id, name, created_at) VALUES ('lib1', '照片库', 1)",
            [],
        )
        .unwrap();
        for p in ["D:\\照片", "E:\\备份\\照片"] {
            conn.execute(
                "INSERT INTO repository_paths(repository_id, path, path_folded, added_at)
                 VALUES ('lib1', ?1, ?2, 1)",
                rusqlite::params![p, p.to_lowercase()],
            )
            .unwrap();
        }
        // 同一个库的两条不同路径都记得住（同库多路径）
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM repository_paths WHERE repository_id='lib1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);
        // 删库时路径记录跟着走（外键级联）
        conn.execute("DELETE FROM repositories WHERE id='lib1'", [])
            .unwrap();
        let left: i64 = conn
            .query_row("SELECT count(*) FROM repository_paths", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0, "删库必须级联清掉路径记录");
    }

    #[test]
    fn app_db_allows_same_path_for_different_libraries() {
        // 「同路径不同库」是明确需求（REPOSITORY.md §2.1）
        let mut conn = mem();
        apply(&mut conn, DbKind::App, Backups::none(), 0).unwrap();
        for id in ["libA", "libB"] {
            conn.execute(
                "INSERT INTO repositories(id, name, created_at) VALUES (?1, ?1, 1)",
                [id],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO repository_paths(repository_id, path, path_folded, added_at)
                 VALUES (?1, 'D:\\照片', 'd:\\照片', 1)",
                [id],
            )
            .unwrap();
        }
        let n: i64 = conn
            .query_row("SELECT count(*) FROM repository_paths", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2, "同一路径可以属于两个库");
    }
}
