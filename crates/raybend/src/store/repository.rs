//! 库：身份、元信息与登记。
//!
//! 取代了早期设计里的 `repo.json` —— 用户 2026-09-15 明确：**库身份写在 `catalog.db` 里**，
//! 中央 `app.db` 只记「有哪些库、它们可能在哪些路径下」。这样：
//!
//! * 整个库目录可以整体搬走（身份跟着文件走，不跟着路径走）；
//! * 同一个路径可以先后/同时属于不同的库（看路径下那份 `catalog.db` 里的 ID 是谁）；
//! * 同一个库可以登记多条路径（换盘符、U 盘、镜像备份）。
//!
//! 详见 `REPOSITORY.md` §1–§2。

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{Error, Result};

use super::ids;
use super::migration::{self, Backups};
use super::path_semantics::PathForms;

/// `repository_meta` 里「库身份」的键名。
pub const META_REPOSITORY_ID: &str = "repository_id";
/// 导入模版键名。
pub const META_IMPORT_TEMPLATE: &str = "import_template";
/// 建库时间键名。
pub const META_CREATED_AT: &str = "created_at";
/// 库展示名键名（跟着库走，换台机器也认得）。
pub const META_NAME: &str = "name";

/// 默认导入模版（`REPOSITORY.md` §3.1）。
pub const DEFAULT_IMPORT_TEMPLATE: &str = ":CYEAR-:CMONTH-:CDAY/MY:FILENAME";

/// 库内导入落地目录名（`REPOSITORY.md` §1；FUTURE G14：将来可配）。
pub const DEFAULT_PHOTOS_DIR: &str = "photos";

/// catalog 的文件名（**必须在库根**）。
pub const CATALOG_FILE_NAME: &str = "catalog.db";

/// 一个库的元信息（存在该库的 `catalog.db` 里）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryMeta {
    /// 库唯一 ID（[`super::ids`] 生成）。
    pub id: String,
    /// 展示名。
    pub name: String,
    /// 导入模版。
    pub import_template: String,
    /// 建库时间（Unix 毫秒）。
    pub created_at: i64,
}

impl RepositoryMeta {
    /// 从 `catalog.db` 的 `repository_meta` 表读出。
    ///
    /// 缺 `repository_id` 或格式不合法 → [`Error::NotARepository`]（带上路径，便于提示用户）。
    pub fn read(conn: &Connection, db_path: &Path) -> Result<Self> {
        // 「目录里有个 catalog.db，但不是我们的库」要有可读的提示，
        // 而不是把 SQLite 的 `no such table: repository_meta` 抛给用户。
        if !has_meta_table(conn)? {
            return Err(Error::NotARepository {
                path: db_path.to_path_buf(),
                reason: "文件不是 raybend 的库（没有 repository_meta 表）".to_string(),
            });
        }
        let id = read_meta(conn, META_REPOSITORY_ID)?.ok_or_else(|| Error::NotARepository {
            path: db_path.to_path_buf(),
            reason: "catalog.db 里没有库身份（repository_id）".to_string(),
        })?;
        ids::require_valid(&id, db_path)?;
        Ok(Self {
            id,
            name: read_meta(conn, META_NAME)?.unwrap_or_else(|| "未命名库".to_string()),
            import_template: read_meta(conn, META_IMPORT_TEMPLATE)?
                .unwrap_or_else(|| DEFAULT_IMPORT_TEMPLATE.to_string()),
            created_at: read_meta(conn, META_CREATED_AT)?
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0),
        })
    }

    /// 写入（或更新）自身到 `repository_meta`。
    pub fn write(&self, conn: &Connection) -> Result<()> {
        write_meta(conn, META_REPOSITORY_ID, &self.id)?;
        write_meta(conn, META_NAME, &self.name)?;
        write_meta(conn, META_IMPORT_TEMPLATE, &self.import_template)?;
        write_meta(conn, META_CREATED_AT, &self.created_at.to_string())?;
        Ok(())
    }
}

/// 这个文件里有没有 `repository_meta` 表。
///
/// 文件根本不是 SQLite（`SQLITE_NOTADB`）时，同样返回 `false` —— 两种情况的结论一样：
/// **这不是一个 raybend 库**。
fn has_meta_table(conn: &Connection) -> Result<bool> {
    let res = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'repository_meta'",
        [],
        |r| r.get::<_, i64>(0),
    );
    match res {
        Ok(n) => Ok(n > 0),
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.extended_code == rusqlite::ffi::SQLITE_NOTADB =>
        {
            Ok(false)
        }
        Err(e) => Err(e.into()),
    }
}

/// 读一条库元信息。
pub fn read_meta(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT value FROM repository_meta WHERE key = ?1",
            [key],
            |r| r.get::<_, String>(0),
        )
        .optional()?)
}

/// 写一条库元信息（存在则覆盖）。
pub fn write_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO repository_meta(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// 只读地打开一个已存在的库，读它的元信息。
pub fn read_repository_meta(db_path: &Path) -> Result<RepositoryMeta> {
    if !db_path.exists() {
        return Err(Error::PathNotFound(db_path.to_path_buf()));
    }
    let conn = Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    RepositoryMeta::read(&conn, db_path)
}

/// 创建一个新库（`catalog.db` + 导入目录），或打开已存在的库。
///
/// * 目标不存在 → 建库：跑迁移、写库身份（**新生成的 ID**）、建 `photos/` 目录；
/// * 目标已存在且是合法库 → **原样返回**（绝不重新生成 ID —— 那会让旧登记全部失联）；
/// * 目标已存在但不是库（缺库身份）→ 报 [`Error::NotARepository`]，让调用方决定怎么办。
pub fn create_or_open_catalog(
    db_path: &Path,
    name: &str,
    template: Option<&str>,
    now_ms: i64,
) -> Result<RepositoryMeta> {
    if db_path.exists() {
        return read_repository_meta(db_path);
    }
    if let Some(dir) = db_path.parent() {
        std::fs::create_dir_all(dir)?;
    }

    let mut conn = Connection::open(db_path)?;
    super::pragma::apply(&conn, false)?;
    migration::apply(
        &mut conn,
        migration::DbKind::Catalog,
        Backups::none(),
        now_ms,
    )?;

    let meta = RepositoryMeta {
        id: ids::new_repository_id_at(now_ms)?,
        name: name.to_string(),
        import_template: template.unwrap_or(DEFAULT_IMPORT_TEMPLATE).to_string(),
        created_at: now_ms,
    };
    meta.write(&conn)?;

    // 落地目录：建库顺手建好，导入时不必再判
    if let Some(root) = db_path.parent() {
        std::fs::create_dir_all(root.join(DEFAULT_PHOTOS_DIR))?;
    }
    Ok(meta)
}

/// 库的在线状态（`REPOSITORY.md` §2.2）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryState {
    /// 在某条已登记路径下找到了它。
    Online {
        /// 库根目录（`catalog.db` 所在目录）。
        root: PathBuf,
    },
    /// 登记过的路径下都没有它的 `catalog.db`。
    Offline {
        /// 尝试过的路径数量（给用户提示用）。
        tried: usize,
    },
}

impl RepositoryState {
    /// 是否在线。
    #[must_use]
    pub fn is_online(&self) -> bool {
        matches!(self, Self::Online { .. })
    }
}

/// 在 `app.db` 里查这个库登记过哪些路径（折叠形式，用于存在性判断）。
fn registered_paths(conn: &Connection, repository_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT path FROM repository_paths WHERE repository_id = ?1 ORDER BY last_seen_at DESC NULLS LAST, path",
    )?;
    let rows = stmt.query_map([repository_id], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 探测一个路径下**是不是这个库**：读 `catalog.db` 的身份与期望比对。
///
/// 这正是「同路径不同库 / 同库多路径」得以成立的机制：判据是**文件里的 ID**，
/// 不是路径字符串。
#[must_use]
pub fn path_holds_repository(root: &Path, repository_id: &str) -> bool {
    let catalog = root.join(CATALOG_FILE_NAME);
    if !catalog.is_file() {
        return false;
    }
    read_repository_meta(&catalog).is_ok_and(|m| m.id == repository_id)
}

/// 解析一个库当前在不在线（`REPOSITORY.md` §2.2）。
///
/// 从**最近见过的路径开始**依次探测；都不在 → 离线。
/// 调用方（界面层）可以缓存结果；这里每次都真查，保证正确性。
pub fn resolve_repository(conn: &Connection, repository_id: &str) -> Result<RepositoryState> {
    let paths = registered_paths(conn, repository_id)?;
    let tried = paths.len();
    for p in paths {
        let root = PathBuf::from(&p);
        if path_holds_repository(&root, repository_id) {
            return Ok(RepositoryState::Online { root });
        }
    }
    Ok(RepositoryState::Offline { tried })
}

// ---------------------------------------------------------------------------
// app.db 侧：库与其路径的登记
// ---------------------------------------------------------------------------

/// 一个库的登记信息（含它登记过的路径）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryRow {
    /// 库 ID。
    pub id: String,
    /// 展示名（以库内记录为准，离线时可能来自缓存）。
    pub name: String,
    /// 导入模版缓存（离线时也能显示）。
    pub import_template: Option<String>,
    /// 建库时间。
    pub created_at: i64,
    /// 最近打开时间。
    pub last_opened_at: Option<i64>,
    /// 登记过的路径。
    pub paths: Vec<RepositoryPathRow>,
}

/// 库的一条登记路径及其最近状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryPathRow {
    /// 原始路径。
    pub path: String,
    /// 折叠路径（比较用）。
    pub path_folded: String,
    /// 状态：`online` / `offline` / `unknown`。
    pub status: String,
    /// 最近一次确认「它确实在这里」的时间。
    pub last_seen_at: Option<i64>,
}

/// 把一个库登记进 `app.db`（已存在则更新名称与模版缓存）。
pub fn register_repository(conn: &Connection, meta: &RepositoryMeta, now_ms: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO repositories(id, name, import_template, photos_dir, created_at, last_opened_at)
         VALUES (?1, ?2, ?3, ?4, ?5, NULL)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name,
                                       import_template = excluded.import_template",
        params![
            meta.id,
            meta.name,
            meta.import_template,
            DEFAULT_PHOTOS_DIR,
            meta.created_at
        ],
    )?;
    let _ = now_ms;
    Ok(())
}

/// 给一个库登记一条路径（`REPOSITORY.md` §2.4：同库多路径）。
///
/// 同一路径重复登记是安全的（幂等）。
pub fn add_repository_path(
    conn: &Connection,
    repository_id: &str,
    path: &str,
    now_ms: i64,
) -> Result<()> {
    let forms = PathForms::new(path);
    conn.execute(
        // 冲突时只刷新 last_seen：**保留第一次登记时的原始拼写**（那是用户认得的写法）
        "INSERT INTO repository_paths(repository_id, path, path_folded, added_at, last_seen_at, status)
         VALUES (?1, ?2, ?3, ?4, ?4, 'unknown')
         ON CONFLICT(repository_id, path_folded) DO UPDATE SET last_seen_at = excluded.last_seen_at",
        params![repository_id, forms.raw(), forms.folded(), now_ms],
    )?;
    Ok(())
}

/// 更新某条路径的探测状态。
pub fn set_path_status(
    conn: &Connection,
    repository_id: &str,
    path: &str,
    status: &str,
    now_ms: i64,
) -> Result<bool> {
    let folded = PathForms::new(path).folded().to_string();
    let seen = if status == "online" {
        Some(now_ms)
    } else {
        None
    };
    let n = conn.execute(
        "UPDATE repository_paths
            SET status = ?3,
                last_seen_at = COALESCE(?4, last_seen_at)
          WHERE repository_id = ?1 AND path_folded = ?2",
        params![repository_id, folded, status, seen],
    )?;
    Ok(n > 0)
}

/// 记录「刚刚打开过这个库」。
pub fn touch_repository_opened(conn: &Connection, repository_id: &str, now_ms: i64) -> Result<()> {
    conn.execute(
        "UPDATE repositories SET last_opened_at = ?2 WHERE id = ?1",
        params![repository_id, now_ms],
    )?;
    Ok(())
}

/// 列出所有已登记的库（含各自的路径），按最近打开时间倒序。
pub fn list_repositories(conn: &Connection) -> Result<Vec<RepositoryRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, import_template, created_at, last_opened_at
           FROM repositories
          ORDER BY last_opened_at DESC NULLS LAST, created_at DESC",
    )?;
    let mut rows: Vec<RepositoryRow> = stmt
        .query_map([], |r| {
            Ok(RepositoryRow {
                id: r.get(0)?,
                name: r.get(1)?,
                import_template: r.get(2)?,
                created_at: r.get(3)?,
                last_opened_at: r.get(4)?,
                paths: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut path_stmt = conn.prepare(
        "SELECT path, path_folded, status, last_seen_at
           FROM repository_paths WHERE repository_id = ?1 ORDER BY last_seen_at DESC NULLS LAST, path",
    )?;
    for row in &mut rows {
        row.paths = path_stmt
            .query_map([&row.id], |r| {
                Ok(RepositoryPathRow {
                    path: r.get(0)?,
                    path_folded: r.get(1)?,
                    status: r.get(2)?,
                    last_seen_at: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
    }
    Ok(rows)
}

/// 查一个库的登记信息（不存在返回 `None`）。
pub fn find_repository(conn: &Connection, repository_id: &str) -> Result<Option<RepositoryRow>> {
    Ok(list_repositories(conn)?
        .into_iter()
        .find(|l| l.id == repository_id))
}

/// 从 `app.db` 里**忘记**一个库：只删登记，**不动磁盘上的任何文件**。
pub fn forget_repository(conn: &Connection, repository_id: &str) -> Result<bool> {
    let n = conn.execute("DELETE FROM repositories WHERE id = ?1", [repository_id])?;
    Ok(n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{
        migration::{Backups, DbKind},
        pragma,
    };

    fn app_db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::App, Backups::none(), 0).unwrap();
        conn
    }

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    const T0: i64 = 1_789_430_400_000;

    // ---------- 建库与身份 ----------

    #[test]
    fn creates_a_library_with_identity_and_photos_dir() {
        let dir = tmp();
        let db = dir.path().join("库").join(CATALOG_FILE_NAME);
        let meta = create_or_open_catalog(&db, "我的照片", None, T0).unwrap();

        assert!(db.is_file(), "catalog.db 必须建出来");
        assert!(ids::is_valid(&meta.id));
        assert_eq!(meta.name, "我的照片");
        assert_eq!(meta.import_template, DEFAULT_IMPORT_TEMPLATE);
        assert_eq!(meta.created_at, T0);
        assert!(
            db.parent().unwrap().join(DEFAULT_PHOTOS_DIR).is_dir(),
            "photos/ 目录要顺手建好"
        );

        // 再打开：身份必须**一模一样**（重新生成 ID 会让旧登记全部失联）
        let again = create_or_open_catalog(&db, "改个名字", None, T0 + 999).unwrap();
        assert_eq!(again.id, meta.id, "已存在的库绝不能换 ID");
        assert_eq!(again.created_at, meta.created_at);
    }

    #[test]
    fn reads_meta_of_an_existing_library() {
        let dir = tmp();
        let db = dir.path().join(CATALOG_FILE_NAME);
        let meta = create_or_open_catalog(&db, "旅行", Some(":CYEAR/:FILENAME"), T0).unwrap();

        let read = read_repository_meta(&db).unwrap();
        assert_eq!(read, meta);
        assert_eq!(read.import_template, ":CYEAR/:FILENAME");
    }

    #[test]
    fn a_plain_sqlite_file_is_not_a_library() {
        let dir = tmp();
        let db = dir.path().join(CATALOG_FILE_NAME);
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch("CREATE TABLE 别的(x)").unwrap();
        drop(conn);

        let err = read_repository_meta(&db).unwrap_err();
        match err {
            Error::NotARepository { path, reason } => {
                assert_eq!(path, db);
                assert!(
                    reason.contains("不是 raybend 的库"),
                    "要能看出这是「长得像库但不是库」：{reason}"
                );
            }
            other => panic!("应当是 NotARepository，实际 {other:?}"),
        }
    }

    #[test]
    fn a_non_sqlite_file_is_not_a_library() {
        let dir = tmp();
        let db = dir.path().join(CATALOG_FILE_NAME);
        std::fs::write(&db, "这只是一段文本，不是数据库".as_bytes()).unwrap();

        let err = read_repository_meta(&db).unwrap_err();
        match err {
            Error::NotARepository { path, reason } => {
                assert_eq!(path, db);
                assert!(reason.contains("不是 raybend 的库"), "{reason}");
            }
            other => panic!("应当是 NotARepository，实际 {other:?}"),
        }
    }

    #[test]
    fn malformed_repository_id_is_rejected() {
        let dir = tmp();
        let db = dir.path().join(CATALOG_FILE_NAME);
        create_or_open_catalog(&db, "x", None, T0).unwrap();

        let conn = Connection::open(&db).unwrap();
        write_meta(&conn, META_REPOSITORY_ID, "这是被篡改的ID").unwrap();
        drop(conn);

        let err = read_repository_meta(&db).unwrap_err();
        assert!(matches!(err, Error::NotARepository { .. }), "{err:?}");
        assert!(err.to_string().contains("格式不合法"), "{err}");
    }

    #[test]
    fn unknown_extra_keys_are_ignored() {
        // 前向兼容：将来版本写的额外键，旧版本读了不能崩
        let dir = tmp();
        let db = dir.path().join(CATALOG_FILE_NAME);
        let meta = create_or_open_catalog(&db, "库", None, T0).unwrap();
        let conn = Connection::open(&db).unwrap();
        write_meta(&conn, "future_key", r#"{"a":1}"#).unwrap();
        drop(conn);

        assert_eq!(read_repository_meta(&db).unwrap(), meta);
    }

    #[test]
    fn missing_db_path_reports_not_found() {
        let dir = tmp();
        let err = read_repository_meta(&dir.path().join("没有.db")).unwrap_err();
        assert!(matches!(err, Error::PathNotFound(_)), "{err:?}");
    }

    #[test]
    fn meta_write_is_upsert() {
        let dir = tmp();
        let db = dir.path().join(CATALOG_FILE_NAME);
        create_or_open_catalog(&db, "旧名", None, T0).unwrap();
        let conn = Connection::open(&db).unwrap();
        write_meta(&conn, META_NAME, "新名").unwrap();
        write_meta(&conn, META_NAME, "更新名").unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM repository_meta WHERE key='name'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "同一个键只能有一条");
        assert_eq!(read_meta(&conn, META_NAME).unwrap().unwrap(), "更新名");
        assert!(read_meta(&conn, "不存在").unwrap().is_none());
    }

    // ---------- app.db 侧的登记 ----------

    #[test]
    fn registers_library_with_multiple_paths() {
        let app = app_db();
        let meta = RepositoryMeta {
            id: "Ab3xY9zQ1mNp7Kd2".to_string(),
            name: "照片库".to_string(),
            import_template: DEFAULT_IMPORT_TEMPLATE.to_string(),
            created_at: T0,
        };
        register_repository(&app, &meta, T0).unwrap();
        add_repository_path(&app, &meta.id, r"D:\照片", T0).unwrap();
        add_repository_path(&app, &meta.id, r"E:\备份\照片", T0 + 1).unwrap();
        // 同一个路径重复登记 → 幂等
        add_repository_path(&app, &meta.id, r"d:/照片", T0 + 2).unwrap();

        let list = list_repositories(&app).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(
            list[0].paths.len(),
            2,
            "两条路径（大小写不同的那条算同一条）"
        );
        assert!(list[0].paths.iter().any(|p| p.path == r"D:\照片"));
        assert!(list[0].import_template.is_some(), "模版要在 app.db 留缓存");
    }

    #[test]
    fn same_path_can_belong_to_two_libraries() {
        let app = app_db();
        for (id, name) in [("libA000000000000", "库A"), ("libB000000000000", "库B")] {
            let meta = RepositoryMeta {
                id: id.to_string(),
                name: name.to_string(),
                import_template: DEFAULT_IMPORT_TEMPLATE.to_string(),
                created_at: T0,
            };
            register_repository(&app, &meta, T0).unwrap();
            add_repository_path(&app, id, r"D:\同一个目录", T0).unwrap();
        }
        let n: i64 = app
            .query_row("SELECT count(*) FROM repository_paths", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2, "同路径不同库是允许的（REPOSITORY.md §2.1）");
    }

    #[test]
    fn path_status_and_last_seen() {
        let app = app_db();
        let meta = RepositoryMeta {
            id: "libX000000000000".to_string(),
            name: "X".to_string(),
            import_template: DEFAULT_IMPORT_TEMPLATE.to_string(),
            created_at: T0,
        };
        register_repository(&app, &meta, T0).unwrap();
        add_repository_path(&app, &meta.id, r"D:\照片", T0).unwrap();

        assert!(set_path_status(&app, &meta.id, r"d:/照片", "online", T0 + 5).unwrap());
        let row = find_repository(&app, &meta.id).unwrap().unwrap();
        assert_eq!(row.paths[0].status, "online");
        assert_eq!(row.paths[0].last_seen_at, Some(T0 + 5));

        // 离线时不该抹掉 last_seen（那是「最后一次见到它」的历史）
        assert!(set_path_status(&app, &meta.id, r"D:\照片", "offline", T0 + 9).unwrap());
        let row = find_repository(&app, &meta.id).unwrap().unwrap();
        assert_eq!(row.paths[0].status, "offline");
        assert_eq!(row.paths[0].last_seen_at, Some(T0 + 5));

        // 不存在的路径 → false（调用方能据此报错）
        assert!(!set_path_status(&app, &meta.id, r"Z:\没有", "online", T0).unwrap());
    }

    #[test]
    fn forget_repository_only_removes_registration() {
        let dir = tmp();
        let db = dir.path().join(CATALOG_FILE_NAME);
        let meta = create_or_open_catalog(&db, "库", None, T0).unwrap();

        let app = app_db();
        register_repository(&app, &meta, T0).unwrap();
        add_repository_path(&app, &meta.id, dir.path().to_string_lossy().as_ref(), T0).unwrap();

        assert!(forget_repository(&app, &meta.id).unwrap());
        assert!(list_repositories(&app).unwrap().is_empty());
        assert!(
            db.is_file(),
            "「忘记库」绝不能删磁盘上的东西（那是另一件事，需要用户明确同意）"
        );
        assert!(
            !forget_repository(&app, &meta.id).unwrap(),
            "重复忘记返回 false"
        );
    }

    #[test]
    fn touch_opened_moves_library_to_front() {
        let app = app_db();
        for (i, id) in ["lib1", "lib2"].iter().enumerate() {
            let meta = RepositoryMeta {
                id: (*id).to_string(),
                name: (*id).to_string(),
                import_template: DEFAULT_IMPORT_TEMPLATE.to_string(),
                created_at: T0 + i as i64,
            };
            register_repository(&app, &meta, T0).unwrap();
        }
        touch_repository_opened(&app, "lib1", T0 + 100).unwrap();
        let list = list_repositories(&app).unwrap();
        assert_eq!(list[0].id, "lib1", "最近打开的要排前面");
        assert_eq!(list[0].last_opened_at, Some(T0 + 100));
    }

    // ---------- 在线/离线解析 ----------

    #[test]
    fn resolves_library_by_identity_not_by_path() {
        let dir = tmp();
        // 库真实落在 A 目录
        let root_a = dir.path().join("A");
        let db_a = root_a.join(CATALOG_FILE_NAME);
        let meta = create_or_open_catalog(&db_a, "库", None, T0).unwrap();

        let app = app_db();
        register_repository(&app, &meta, T0).unwrap();
        add_repository_path(&app, &meta.id, root_a.to_string_lossy().as_ref(), T0).unwrap();

        match resolve_repository(&app, &meta.id).unwrap() {
            RepositoryState::Online { root } => assert_eq!(root, root_a),
            other => panic!("应当在线，实际 {other:?}"),
        }
    }

    #[test]
    fn offline_when_no_registered_path_holds_it() {
        let app = app_db();
        let meta = RepositoryMeta {
            id: "libOffline0000000".to_string(),
            name: "离线库".to_string(),
            import_template: DEFAULT_IMPORT_TEMPLATE.to_string(),
            created_at: T0,
        };
        register_repository(&app, &meta, T0).unwrap();
        add_repository_path(&app, &meta.id, r"Z:\不存在的盘", T0).unwrap();
        add_repository_path(&app, &meta.id, r"Y:\也不存在", T0).unwrap();

        assert_eq!(
            resolve_repository(&app, &meta.id).unwrap(),
            RepositoryState::Offline { tried: 2 }
        );
    }

    #[test]
    fn finds_library_after_it_moves_to_another_registered_path() {
        // 「离线库换挂载点」：登记过的另一条路径上有它 → 应当找到
        let dir = tmp();
        let root_old = dir.path().join("老的盘");
        let root_new = dir.path().join("新的盘");
        let db_old = root_old.join(CATALOG_FILE_NAME);
        let meta = create_or_open_catalog(&db_old, "库", None, T0).unwrap();

        let app = app_db();
        register_repository(&app, &meta, T0).unwrap();
        add_repository_path(&app, &meta.id, root_old.to_string_lossy().as_ref(), T0).unwrap();
        // 后来又在另一个位置见到过同一个库
        std::fs::create_dir_all(&root_new).unwrap();
        std::fs::copy(&db_old, root_new.join(CATALOG_FILE_NAME)).unwrap();
        add_repository_path(&app, &meta.id, root_new.to_string_lossy().as_ref(), T0 + 1).unwrap();

        // 老位置被拔掉了
        std::fs::remove_file(&db_old).unwrap();
        match resolve_repository(&app, &meta.id).unwrap() {
            RepositoryState::Online { root } => assert_eq!(root, root_new, "应当在新位置找到它"),
            other => panic!("应当在线，实际 {other:?}"),
        }
    }

    #[test]
    fn path_holding_a_different_library_is_not_a_match() {
        // 「同路径不同库」的另一面：路径下有 catalog.db，但不是要的那个库
        let dir = tmp();
        let root = dir.path().join("库目录");
        let db = root.join(CATALOG_FILE_NAME);
        let meta = create_or_open_catalog(&db, "库", None, T0).unwrap();

        assert!(path_holds_repository(&root, &meta.id));
        assert!(!path_holds_repository(&root, "anotherLibrary000"));
        assert!(!path_holds_repository(&dir.path().join("空目录"), &meta.id));
    }

    #[test]
    fn list_repositories_is_empty_when_nothing_registered() {
        let app = app_db();
        assert!(list_repositories(&app).unwrap().is_empty());
        assert!(find_repository(&app, "没有").unwrap().is_none());
    }
}
