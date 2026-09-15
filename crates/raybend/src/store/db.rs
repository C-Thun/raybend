//! 对外门面：`AppDb`（全局库）与 `CatalogDb`（每库）。
//!
//! 这一层把「打开文件 → 套 PRAGMA → 跑迁移 → 备份 → 起写线程 + 读池」这套流程收成一个动作，
//! 让上层（`src-tauri` 的命令、将来的 CLI）只需要：
//!
//! ```no_run
//! # use raybend::store::db::{AppDb, CatalogDb, OpenOpts};
//! # fn main() -> raybend::Result<()> {
//! let now = raybend::store::time::now_millis();
//! let app = AppDb::open("/tmp/raybend", now)?;
//! app.set_setting("theme", "dark")?;
//!
//! // 建一个新库并登记进去（快照统一放 app data 的 backups/ 下）
//! let backups = std::path::Path::new("/tmp/raybend/backups");
//! let opts = OpenOpts::new(Some(backups), now);
//! let lib = CatalogDb::create("/tmp/照片库", "我的照片", None, opts)?;
//! app.register_library(lib.meta(), std::path::Path::new("/tmp/照片库"), now)?;
//! # Ok(()) }
//! ```
//!
//! **目录布局**（`AGENTS.md` §6.4）：
//!
//! ```text
//! <data_dir>/
//!   app.db            ← AppDb
//!   backups/          ← 迁移前快照（保留 7 份）
//!
//! <库根>/
//!   catalog.db        ← CatalogDb
//!   photos/           ← 导入落地目录
//! ```

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::{Error, Result};

use super::library::{self, LibraryMeta};
use super::migration::{self, Backups, DbKind, MigrationOutcome};
use super::pool::ReadPool;
use super::writer::Writer;
use super::{pragma, time};

/// 全局库文件名。
pub const APP_DB_FILE: &str = "app.db";
/// 快照子目录名。
pub const BACKUPS_DIR: &str = "backups";

/// 打开一个库时的选项。
///
/// 备份目录**不在库目录里**（`LIBRARY.md` §1 规定库根只有 `catalog.db` 与 `photos/`），
/// 而是由调用方指定 —— 生产环境一律是 `<app data>/backups/`。
#[derive(Debug, Clone, Copy)]
pub struct OpenOpts<'a> {
    /// 迁移前快照的存放目录；`None` = 不备份（只该在测试里这么用）。
    pub backups: Option<&'a Path>,
    /// 当前时间（Unix 毫秒）。由调用方注入：一次会话里时间一致，测试里可控制。
    pub now_ms: i64,
}

impl<'a> OpenOpts<'a> {
    /// 指定备份目录与时间。
    #[must_use]
    pub const fn new(backups: Option<&'a Path>, now_ms: i64) -> Self {
        Self { backups, now_ms }
    }

    /// 不备份（测试用）。
    #[must_use]
    pub const fn unbacked_up(now_ms: i64) -> Self {
        Self {
            backups: None,
            now_ms,
        }
    }
}

/// 一次「打开」的结果里，上层可能关心的元信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenReport {
    /// 库文件路径。
    pub path: PathBuf,
    /// 迁移情况（`None` 表示这次走的是「已是最新」的路径）。
    pub migration: Option<MigrationOutcome>,
}

/// 全局库：应用设置、库注册表、任务队列。
pub struct AppDb {
    pool: ReadPool,
    writer: Writer,
    report: OpenReport,
}

impl AppDb {
    /// 打开（必要时创建并迁移）`<data_dir>/app.db`。
    ///
    /// 首次运行会创建文件；升级时会先在 `<data_dir>/backups/` 留快照。
    pub fn open(data_dir: impl AsRef<Path>, now_ms: i64) -> Result<Self> {
        let data_dir = data_dir.as_ref();
        std::fs::create_dir_all(data_dir)?;
        let path = data_dir.join(APP_DB_FILE);
        let backups = data_dir.join(BACKUPS_DIR);
        // 目录布局是约定的一部分：即使这次不需要备份，也把 backups/ 建好
        std::fs::create_dir_all(&backups)?;

        let outcome = migrate_file(&path, DbKind::App, Backups::at(&backups), now_ms, true)?;

        let pool = ReadPool::open(&path)?;
        let writer = Writer::open(&path)?;
        Ok(Self {
            pool,
            writer,
            report: OpenReport {
                path,
                migration: Some(outcome),
            },
        })
    }

    /// 库文件路径。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.report.path
    }

    /// 打开时的迁移情况。
    #[must_use]
    pub fn report(&self) -> &OpenReport {
        &self.report
    }

    /// 读（走读连接池）。
    pub fn read<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        self.pool.with(f)
    }

    /// 写（走单写者线程，串行化）。
    pub fn write<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    {
        self.writer.run(f)
    }

    /// 在一个事务里写。
    pub fn write_tx<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&rusqlite::Transaction<'_>) -> Result<T> + Send + 'static,
    {
        self.writer.transaction(f)
    }

    /// 等所有排队的写落盘。
    pub fn flush(&self) -> Result<()> {
        self.writer.flush()
    }

    // ---------- 设置（KV，值是 JSON）----------

    /// 读一条设置（原始字符串）。
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        self.read(|conn| {
            Ok(conn
                .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                    r.get::<_, String>(0)
                })
                .ok())
        })
    }

    /// 写一条设置（原始字符串）。
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let key = key.to_string();
        let value = value.to_string();
        let now = time::now_millis();
        self.write(move |conn| {
            conn.execute(
                "INSERT INTO settings(key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value,
                                               updated_at = excluded.updated_at",
                rusqlite::params![key, value, now],
            )?;
            Ok(())
        })
    }

    /// 读一条设置并反序列化（JSON）。
    pub fn get_setting_json<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        match self.get_setting(key)? {
            None => Ok(None),
            Some(raw) => Ok(Some(serde_json::from_str(&raw)?)),
        }
    }

    /// 把值序列化成 JSON 存进设置。
    pub fn set_setting_json<T: serde::Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.set_setting(key, &serde_json::to_string(value)?)
    }

    // ---------- 库注册表 ----------

    /// 登记（或更新）一个库。
    pub fn register_library(&self, meta: &LibraryMeta, root: &Path, now_ms: i64) -> Result<()> {
        let meta = meta.clone();
        let root = root.to_string_lossy().into_owned();
        self.write_tx(move |tx| {
            library::register_library(tx, &meta, now_ms)?;
            library::add_library_path(tx, &meta.id, &root, now_ms)?;
            Ok(())
        })
    }

    /// 列出所有已登记的库。
    pub fn list_libraries(&self) -> Result<Vec<library::LibraryRow>> {
        self.read(library::list_libraries)
    }

    /// 解析一个库当前在不在线。
    pub fn resolve_library(&self, library_id: &str) -> Result<library::LibraryState> {
        self.read(|conn| library::resolve_library(conn, library_id))
    }
}

impl std::fmt::Debug for AppDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppDb")
            .field("path", &self.report.path)
            .finish_non_exhaustive()
    }
}

/// 一个库：`catalog.db` 的读池 + 写者 + 元信息。
pub struct CatalogDb {
    pool: ReadPool,
    writer: Writer,
    meta: LibraryMeta,
    report: OpenReport,
}

impl CatalogDb {
    /// 打开一个**已存在**的库（`root/catalog.db`）。
    ///
    /// 缺文件 / 不是库 → 明确的错误；库更新 → [`Error::SchemaTooNew`]（拒绝打开）。
    pub fn open(root: impl AsRef<Path>, opts: OpenOpts<'_>) -> Result<Self> {
        let root = root.as_ref();
        let path = root.join(library::CATALOG_FILE_NAME);
        if !path.is_file() {
            return Err(Error::NotALibrary {
                path: root.to_path_buf(),
                reason: format!("目录下没有 {}", library::CATALOG_FILE_NAME),
            });
        }
        // 先在只读连接上确认身份（避免对「不是库」的目录做迁移这种重活）
        let meta = library::read_library_meta(&path)?;

        let outcome = migrate_file(
            &path,
            DbKind::Catalog,
            backups_for(&opts, &meta.id),
            opts.now_ms,
            false,
        )?;
        let pool = ReadPool::open(&path)?;
        let writer = Writer::open(&path)?;
        Ok(Self {
            pool,
            writer,
            meta,
            report: OpenReport {
                path,
                migration: Some(outcome),
            },
        })
    }

    /// 建一个新库（已存在则原样打开，**不会换 ID**）。详见 [`library::create_or_open_catalog`]。
    pub fn create(
        root: impl AsRef<Path>,
        name: &str,
        template: Option<&str>,
        opts: OpenOpts<'_>,
    ) -> Result<Self> {
        let root = root.as_ref();
        let path = root.join(library::CATALOG_FILE_NAME);
        let meta = library::create_or_open_catalog(&path, name, template, opts.now_ms)?;

        let outcome = migrate_file(
            &path,
            DbKind::Catalog,
            backups_for(&opts, &meta.id),
            opts.now_ms,
            false,
        )?;
        let pool = ReadPool::open(&path)?;
        let writer = Writer::open(&path)?;
        Ok(Self {
            pool,
            writer,
            meta,
            report: OpenReport {
                path,
                migration: Some(outcome),
            },
        })
    }

    /// 库元信息。
    #[must_use]
    pub fn meta(&self) -> &LibraryMeta {
        &self.meta
    }

    /// 库根目录（`catalog.db` 所在目录）。
    #[must_use]
    pub fn root(&self) -> &Path {
        self.report
            .path
            .parent()
            .unwrap_or_else(|| Path::new("."))
    }

    /// 库文件路径。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.report.path
    }

    /// 打开报告（迁移情况）。
    #[must_use]
    pub fn report(&self) -> &OpenReport {
        &self.report
    }

    /// 读。
    pub fn read<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        self.pool.with(f)
    }

    /// 写。
    pub fn write<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    {
        self.writer.run(f)
    }

    /// 事务写。
    pub fn write_tx<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&rusqlite::Transaction<'_>) -> Result<T> + Send + 'static,
    {
        self.writer.transaction(f)
    }

    /// 等写落盘。
    pub fn flush(&self) -> Result<()> {
        self.writer.flush()
    }

    /// 改导入模版（写进库内元信息；`app.db` 的缓存由调用方决定是否同步）。
    pub fn set_import_template(&self, template: &str) -> Result<()> {
        let template = template.to_string();
        self.write(move |conn| library::write_meta(conn, library::META_IMPORT_TEMPLATE, &template))
    }
}

impl std::fmt::Debug for CatalogDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CatalogDb")
            .field("path", &self.report.path)
            .field("library_id", &self.meta.id)
            .finish_non_exhaustive()
    }
}

/// 打开文件 → 套 PRAGMA → 跑迁移（带快照与版本闸门）→ 关掉临时连接。
///
/// `create_if_missing` 为假时要求文件已存在（库目录不能凭函数调用凭空长出来）。
fn migrate_file(
    path: &Path,
    kind: DbKind,
    backups: Backups<'_>,
    now_ms: i64,
    create_if_missing: bool,
) -> Result<MigrationOutcome> {
    let _ = create_if_missing;
    if !create_if_missing && !path.is_file() {
        return Err(Error::PathNotFound(path.to_path_buf()));
    }
    let mut conn = Connection::open(path)?;
    pragma::apply(&conn, false)?;
    let outcome = migration::apply(&mut conn, kind, backups, now_ms)?;
    Ok(outcome)
}

/// 库快照的文件名标签：库 ID 前 8 位（同一个备份目录里要能区分是哪个库）。
fn label_for(library_id: &str) -> &str {
    &library_id[..library_id.len().min(8)]
}

/// 按选项给某个库组织快照策略。
fn backups_for<'a>(opts: &OpenOpts<'a>, library_id: &'a str) -> Backups<'a> {
    match opts.backups {
        Some(dir) => Backups::labelled(dir, label_for(library_id)),
        None => Backups::none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::ids;

    const T0: i64 = 1_789_430_400_000;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    /// 测试用的打开选项：快照放到给定目录（与生产一致的行为）。
    fn opts(dir: &Path, now: i64) -> OpenOpts<'_> {
        OpenOpts::new(Some(dir), now)
    }

    // ---------- AppDb ----------

    #[test]
    fn opens_and_creates_app_db() {
        let dir = tmp();
        let app = AppDb::open(dir.path(), T0).unwrap();
        assert_eq!(app.path(), dir.path().join(APP_DB_FILE));
        assert!(app.path().is_file());
        assert!(dir.path().join(BACKUPS_DIR).is_dir(), "备份目录要建好");
        let m = app.report().migration.as_ref().unwrap();
        assert_eq!((m.from, m.to), (0, 1));
    }

    #[test]
    fn settings_roundtrip_and_persist() {
        let dir = tmp();
        {
            let app = AppDb::open(dir.path(), T0).unwrap();
            assert_eq!(app.get_setting("theme").unwrap(), None);
            app.set_setting("theme", "dark").unwrap();
            app.set_setting("theme", "light").unwrap(); // 覆盖
            app.set_setting_json("recent", &vec!["D:\\a", "D:\\b"]).unwrap();
            assert_eq!(app.get_setting("theme").unwrap().as_deref(), Some("light"));
        }
        // 重开：数据还在（说明写线程的队列在 Drop 时跑干了）
        let app = AppDb::open(dir.path(), T0 + 1000).unwrap();
        assert_eq!(app.get_setting("theme").unwrap().as_deref(), Some("light"));
        let recent: Vec<String> = app.get_setting_json("recent").unwrap().unwrap();
        assert_eq!(recent, vec!["D:\\a", "D:\\b"]);
        // 已经是当前版本 → 不再迁移
        assert_eq!(app.report().migration.as_ref().unwrap().to, 1);
    }

    #[test]
    fn settings_json_handles_unicode_and_bad_values() {
        let dir = tmp();
        let app = AppDb::open(dir.path(), T0).unwrap();
        app.set_setting_json("名字", &"照片库📷").unwrap();
        let back: String = app.get_setting_json("名字").unwrap().unwrap();
        assert_eq!(back, "照片库📷");

        // 手动塞入坏 JSON → 报错而不是 panic
        app.set_setting("bad", "{不是 json").unwrap();
        let err = app.get_setting_json::<Vec<String>>("bad");
        assert!(matches!(err, Err(Error::Json(_))), "{err:?}");
    }

    #[test]
    fn rejects_app_db_from_a_newer_version() {
        let dir = tmp();
        {
            let app = AppDb::open(dir.path(), T0).unwrap();
            app.flush().unwrap();
        }
        // 模拟「新版程序建的库」
        {
            let conn = Connection::open(dir.path().join(APP_DB_FILE)).unwrap();
            conn.pragma_update(None, "user_version", 99_i64).unwrap();
        }
        let err = AppDb::open(dir.path(), T0).unwrap_err();
        assert!(matches!(err, Error::SchemaTooNew { found: 99, .. }), "{err:?}");
        assert!(err.to_string().contains("升级"), "提示要说清怎么办：{err}");
    }

    #[test]
    fn app_db_path_that_is_a_file_fails() {
        let dir = tmp();
        let blocker = dir.path().join("挡路的文件");
        std::fs::write(&blocker, b"x").unwrap();
        assert!(AppDb::open(&blocker, T0).is_err(), "路径被文件占住时必须报错");
    }

    // ---------- CatalogDb ----------

    #[test]
    fn creates_catalog_and_reads_it_back() {
        let dir = tmp();
        let root = dir.path().join("照片库");
        let meta = {
            let lib = CatalogDb::create(&root, "我的照片", None, opts(&root, T0)).unwrap();
            assert!(ids::is_valid(lib.meta().id.as_str()));
            assert_eq!(lib.root(), root);
            assert_eq!(lib.path(), root.join("catalog.db"));
            assert!(root.join("photos").is_dir());
            lib.meta().clone()
        };

        // 重新打开：身份不变
        let lib = CatalogDb::open(&root, opts(&root, T0 + 10)).unwrap();
        assert_eq!(lib.meta(), &meta);
    }

    #[test]
    fn opening_a_directory_without_catalog_is_an_error() {
        let dir = tmp();
        let err = CatalogDb::open(dir.path(), opts(dir.path(), T0)).unwrap_err();
        match err {
            Error::NotALibrary { reason, .. } => {
                assert!(reason.contains("catalog.db"), "{reason}");
            }
            other => panic!("应当是 NotALibrary，实际 {other:?}"),
        }
    }

    #[test]
    fn catalog_writes_are_visible_after_reopen() {
        let dir = tmp();
        let root = dir.path().join("库");
        {
            let lib = CatalogDb::create(&root, "库", None, opts(&root, T0)).unwrap();
            lib.write(|conn| {
                conn.execute(
                    "INSERT INTO assets(taken_at, camera_model, imported_at, updated_at)
                     VALUES (?1, 'NIKON Z 7II', ?2, ?2)",
                    rusqlite::params![T0, T0],
                )?;
                Ok(())
            })
            .unwrap();
            lib.set_import_template(":CYEAR/:FILENAME").unwrap();
        }

        let lib = CatalogDb::open(&root, opts(&root, T0 + 1)).unwrap();
        let n: i64 = lib
            .read(|c| Ok(c.query_row("SELECT count(*) FROM assets", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(lib.meta().import_template, ":CYEAR/:FILENAME");
    }

    #[test]
    fn rejects_catalog_from_a_newer_version() {
        let dir = tmp();
        let root = dir.path().join("库");
        {
            let lib = CatalogDb::create(&root, "库", None, opts(&root, T0)).unwrap();
            lib.flush().unwrap();
        }
        {
            let conn = Connection::open(root.join("catalog.db")).unwrap();
            conn.pragma_update(None, "user_version", 42_i64).unwrap();
        }
        let err = CatalogDb::open(&root, opts(&root, T0)).unwrap_err();
        assert!(matches!(err, Error::SchemaTooNew { found: 42, .. }), "{err:?}");
    }

    #[test]
    fn create_never_replaces_an_existing_library() {
        let dir = tmp();
        let root = dir.path().join("库");
        let first = CatalogDb::create(&root, "原来的库", None, opts(&root, T0)).unwrap();
        let id = first.meta().id.clone();
        first.flush().unwrap();
        drop(first);

        // 用别的名字再 create：应当打开原有库（ID 不变），而不是重建成新库
        let again = CatalogDb::create(&root, "换个名字", None, opts(&root, T0 + 5)).unwrap();
        assert_eq!(again.meta().id, id, "已存在的库绝不能被换掉身份");
    }

    #[test]
    fn catalogue_snapshots_do_not_pollute_the_library_root() {
        // LIBRARY.md §1：库根只有 catalog.db 与 photos/。
        // 快照统一放调用方给的目录（生产是 app data 下的 backups/）。
        let dir = tmp();
        let backups = dir.path().join("app数据").join(BACKUPS_DIR);
        let root = dir.path().join("库");
        let lib = CatalogDb::create(&root, "库", None, OpenOpts::new(Some(&backups), T0)).unwrap();
        lib.flush().unwrap();

        let entries: Vec<String> = std::fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(!entries.contains(&BACKUPS_DIR.to_string()), "库根不该出现 backups：{entries:?}");
        assert!(entries.contains(&"catalog.db".to_string()));
        assert!(entries.contains(&"photos".to_string()));
    }

    #[test]
    fn snapshot_label_is_the_library_id_prefix() {
        assert_eq!(label_for("Ab3xY9zQ1mNp7Kd2"), "Ab3xY9zQ");
        assert_eq!(label_for("短"), "短");
        assert_eq!(label_for(""), "");

        let dir = Path::new("/tmp/backups");
        let o = opts(dir, 0);
        let b = backups_for(&o, "Ab3xY9zQ1mNp7Kd2");
        assert_eq!(b.dir, Some(dir));
        assert_eq!(b.label, Some("Ab3xY9zQ"), "catalog 的快照文件名要能区分是哪个库");

        let unbacked = backups_for(&OpenOpts::unbacked_up(0), "Ab3xY9zQ1mNp7Kd2");
        assert!(unbacked.dir.is_none() && unbacked.label.is_none());
    }

    // ---------- 两者配合：注册 → 解析 ----------

    #[test]
    fn register_then_resolve_online() {
        let dir = tmp();
        let app = AppDb::open(dir.path().join("app 数据"), T0).unwrap();
        let root = dir.path().join("库");
        let lib = CatalogDb::create(&root, "我的照片", None, opts(&root, T0)).unwrap();

        app.register_library(lib.meta(), &root, T0).unwrap();
        let list = app.list_libraries().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, lib.meta().id);
        assert_eq!(list[0].name, "我的照片");
        assert_eq!(list[0].paths.len(), 1);

        let state = app.resolve_library(&lib.meta().id).unwrap();
        assert!(state.is_online(), "库就在那儿，应当在线：{state:?}");
        if let library::LibraryState::Online { root: found } = state {
            assert_eq!(found, root);
        }
    }

    #[test]
    fn resolve_reports_offline_after_the_drive_goes_away() {
        let dir = tmp();
        let app = AppDb::open(dir.path().join("app 数据"), T0).unwrap();
        let root = dir.path().join("库");
        let lib = CatalogDb::create(&root, "我的照片", None, opts(&root, T0)).unwrap();
        app.register_library(lib.meta(), &root, T0).unwrap();
        lib.flush().unwrap();
        drop(lib);

        // 模拟「移动硬盘被拔掉」：库目录整个不见了
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            app.resolve_library("Ab3xY9zQ1mNp7Kd2").unwrap(),
            library::LibraryState::Offline { tried: 0 },
            "没登记过的库当然是离线"
        );
        // 真正登记过的那个库
        let after = AppDb::open(dir.path().join("app 数据"), T0).unwrap();
        let n = after.list_libraries().unwrap().len();
        assert_eq!(n, 1);
    }

    #[test]
    fn two_libraries_same_path_are_kept_separate() {
        // 同路径不同库：把 A 的 catalog.db 换成 B 的，两条登记都要在
        let dir = tmp();
        let app = AppDb::open(dir.path().join("app"), T0).unwrap();
        let root = dir.path().join("共用目录");

        let a = CatalogDb::create(&root, "库A", None, opts(&root, T0)).unwrap();
        let a_id = a.meta().id.clone();
        app.register_library(a.meta(), &root, T0).unwrap();
        a.flush().unwrap();
        drop(a);

        // 把目录里换成另一个库（模拟用户换了 catalog.db）
        std::fs::remove_file(root.join("catalog.db")).unwrap();
        let b = CatalogDb::create(&root, "库B", None, opts(&root, T0 + 1)).unwrap();
        app.register_library(b.meta(), &root, T0 + 1).unwrap();

        let list = app.list_libraries().unwrap();
        assert_eq!(list.len(), 2, "同一路径下的两个库都要留着");
        // 现在这个路径上是 B → A 离线、B 在线（判据是文件里的 ID，不是路径）
        assert!(app.resolve_library(&b.meta().id).unwrap().is_online());
        assert!(!app.resolve_library(&a_id).unwrap().is_online());
    }
}
