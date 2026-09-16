//! 删除：**移到系统回收站**，不提供永久删除。
//!
//! 出处：`BROWSE.md` §5.9（`Delete` 有确认、`Shift+Delete` 免确认、支持批量）与
//! `plans/M2.md` §3 第 4 条（用户口述明确要删；`AGENTS.md` §11.3 的「第一阶段不做真删」
//! 就此解除，但**只到回收站**）。
//!
//! # 三条纪律
//!
//! 1. **只到回收站**：可恢复是底线。没有「永久删除」这个选项 —— 那是文件管理器的事。
//! 2. **一级锁挡住删除**（`BROWSE.md` §3.4：「一级不能删」）；二级锁自然也挡（更严）。
//! 3. **文件先走、记录后走**：只有当一个资产的**所有**文件都进了回收站
//!    （或本来就已经不在磁盘上），才删它的库记录。否则会出现「记录没了、文件还在」
//!    这种谁也不知道的残留 —— 那种状态比留着一条错的记录更糟。
//!
//! # 为什么删除要经过一个 trait
//!
//! 真的调 `trash` 会往系统回收站里丢东西。单元测试不该碰系统环境（CI 里也可能没有
//! 回收站目录），所以真正的「移走」这一个动作被抽成 [`Trasher`]：
//! 生产用 [`SystemTrash`]，测试用把文件挪到临时目录的假实现。
//! 这样**别的逻辑（锁、成组、记录清理）全都能被真测**。

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::Result;
use crate::store::marking::LOCK_NO_DELETE;

/// 把一个文件移进回收站。
pub trait Trasher {
    /// 成功返回 `Ok(())`；失败返回人话原因（会进报告给用户看）。
    fn trash(&self, path: &Path) -> std::result::Result<(), String>;
}

/// 真的调用系统回收站。
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemTrash;

impl Trasher for SystemTrash {
    fn trash(&self, path: &Path) -> std::result::Result<(), String> {
        trash::delete(path).map_err(|e| format!("{e}"))
    }
}

/// 一个资产的删除计划（**还没动任何文件**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletePlan {
    pub asset_id: i64,
    /// 该资产的全部文件（库内相对路径）。
    pub rel_paths: Vec<String>,
}

/// 删除结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeleteReport {
    /// 真的删掉的资产数（记录也跟着删了）。
    pub deleted: usize,
    /// 因为锁被挡下的资产（一级或二级锁都算）。
    pub blocked_locked: Vec<i64>,
    /// 文件已经不在磁盘上（当成「已消失」处理，记录照删）。
    pub already_gone: usize,
    /// 移进回收站失败的文件 + 原因；**这些资产不会删记录**。
    pub failed: Vec<(String, String)>,
}

impl DeleteReport {
    #[must_use]
    pub fn touched_anything(&self) -> bool {
        self.deleted > 0 || self.already_gone > 0
    }
}

/// 算出「删这些照片」要动哪些文件（不含锁判断的结果 —— 锁在 [`delete_assets_with`] 里判）。
pub fn plan(conn: &Connection, ids: &[i64]) -> Result<Vec<DeletePlan>> {
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let mut stmt = conn.prepare(
            "SELECT rel_path FROM asset_files WHERE asset_id = ?1 ORDER BY role, rel_path",
        )?;
        let rel_paths: Vec<String> = stmt
            .query_map([id], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        out.push(DeletePlan {
            asset_id: *id,
            rel_paths,
        });
    }
    Ok(out)
}

/// 用指定的 [`Trasher`] 删除一批照片（测试用这个入口）。
///
/// `root` 是库根目录（`rel_path` 是相对它的路径）。
pub fn delete_assets_with(
    trasher: &dyn Trasher,
    conn: &Connection,
    root: &Path,
    ids: &[i64],
) -> Result<DeleteReport> {
    let mut report = DeleteReport::default();
    if ids.is_empty() {
        return Ok(report);
    }

    // ① 锁：一级锁就挡住删除（BROWSE.md §3.4）
    let mut deletable: Vec<i64> = Vec::new();
    for id in ids {
        let lock: Option<i64> = conn
            .query_row("SELECT lock_level FROM assets WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .ok();
        match lock {
            None => continue, // 记录已经不在了（别的地方删过）—— 当作无事发生
            Some(level) if level >= i64::from(LOCK_NO_DELETE) => {
                report.blocked_locked.push(*id);
            }
            Some(_) => deletable.push(*id),
        }
    }

    // ② 文件先走
    let plans = plan(conn, &deletable)?;
    let mut to_forget: Vec<i64> = Vec::new();
    for plan in &plans {
        let mut all_moved = true;
        for rel in &plan.rel_paths {
            let abs = root.join(rel);
            if !abs.exists() {
                report.already_gone += 1;
                continue;
            }
            match trasher.trash(&abs) {
                Ok(()) => {}
                Err(reason) => {
                    report.failed.push((rel.clone(), reason));
                    all_moved = false;
                }
            }
        }
        if all_moved {
            to_forget.push(plan.asset_id);
        } else {
            // 有文件没移走 → 保留记录（否则会「记录没了、文件还在」）
        }
    }

    // ③ 记录后走（asset_files / asset_tags 由外键级联）
    for id in &to_forget {
        conn.execute("DELETE FROM assets WHERE id = ?1", [id])?;
        // 全文索引跟着清（删了照片还搜得到它是很怪的）
        crate::store::fts::refresh_asset(conn, *id)?;
    }
    report.deleted = to_forget.len();

    Ok(report)
}

/// 生产入口：真的移进系统回收站。
pub fn delete_assets(conn: &Connection, root: &Path, ids: &[i64]) -> Result<DeleteReport> {
    delete_assets_with(&SystemTrash, conn, root, ids)
}

/// 把库内相对路径拼成绝对路径（**只做拼接**，不检查存在性）。
#[must_use]
pub fn absolute(root: &Path, rel_path: &str) -> PathBuf {
    root.join(rel_path.replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::migration::{self, DbKind};
    use crate::store::pragma;
    use std::sync::Mutex;

    const T0: i64 = 1_789_516_800_000;

    fn catalog() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::Catalog, migration::Backups::none(), T0).unwrap();
        conn
    }

    /// 把文件「移走」到别处 —— 不碰系统回收站，行为与真身一致（文件从原位置消失）。
    struct MovingTrasher {
        moved: Mutex<Vec<PathBuf>>,
        fail_on: Vec<String>,
    }

    impl MovingTrasher {
        fn new() -> Self {
            Self {
                moved: Mutex::new(Vec::new()),
                fail_on: Vec::new(),
            }
        }

        fn failing_on(names: &[&str]) -> Self {
            Self {
                moved: Mutex::new(Vec::new()),
                fail_on: names.iter().map(|s| (*s).to_string()).collect(),
            }
        }

        fn moved(&self) -> Vec<PathBuf> {
            self.moved.lock().unwrap().clone()
        }
    }

    impl Trasher for MovingTrasher {
        fn trash(&self, path: &Path) -> std::result::Result<(), String> {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if self.fail_on.contains(&name) {
                return Err("模拟的失败（文件被占用）".to_string());
            }
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
            self.moved.lock().unwrap().push(path.to_path_buf());
            Ok(())
        }
    }

    fn asset_with_files(conn: &Connection, root: &Path, rels: &[&str]) -> i64 {
        conn.execute(
            "INSERT INTO assets (imported_at, updated_at) VALUES (?1, ?1)",
            [T0],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        for (i, rel) in rels.iter().enumerate() {
            let role = if i == 0 { "bitmap" } else { "raw" };
            let ext = rel.rsplit('.').next().unwrap().to_lowercase();
            conn.execute(
                "INSERT INTO asset_files
                   (asset_id, role, rel_path, rel_path_folded, ext, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                rusqlite::params![id, role, rel, rel.to_lowercase(), ext, T0],
            )
            .unwrap();
            let abs = root.join(rel);
            std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
            std::fs::write(&abs, b"photo").unwrap();
        }
        id
    }

    fn asset_exists(conn: &Connection, id: i64) -> bool {
        conn.query_row("SELECT count(*) FROM assets WHERE id = ?1", [id], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap()
            > 0
    }

    #[test]
    fn deletes_files_and_the_record() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let id = asset_with_files(&conn, root, &["photos/a.jpg", "photos/_RAW/a.orf"]);

        let trasher = MovingTrasher::new();
        let report = delete_assets_with(&trasher, &conn, root, &[id]).unwrap();

        assert_eq!(report.deleted, 1);
        assert!(report.failed.is_empty());
        assert_eq!(trasher.moved().len(), 2, "两个文件都移走了");
        assert!(!asset_exists(&conn, id), "记录也删了");
        assert!(!root.join("photos/a.jpg").exists());
    }

    #[test]
    fn a_first_level_lock_blocks_deletion() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let id = asset_with_files(&conn, root, &["photos/a.jpg"]);
        conn.execute("UPDATE assets SET lock_level = 1 WHERE id = ?1", [id])
            .unwrap();

        let trasher = MovingTrasher::new();
        let report = delete_assets_with(&trasher, &conn, root, &[id]).unwrap();

        assert_eq!(report.deleted, 0);
        assert_eq!(report.blocked_locked, vec![id]);
        assert!(trasher.moved().is_empty(), "锁住的照片文件一个都不能动");
        assert!(asset_exists(&conn, id));
        assert!(root.join("photos/a.jpg").exists());
    }

    #[test]
    fn a_second_level_lock_also_blocks_deletion() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let id = asset_with_files(&conn, root, &["photos/a.jpg"]);
        conn.execute("UPDATE assets SET lock_level = 2 WHERE id = ?1", [id])
            .unwrap();
        let report = delete_assets_with(&MovingTrasher::new(), &conn, root, &[id]).unwrap();
        assert_eq!(report.blocked_locked, vec![id]);
    }

    #[test]
    fn a_partly_locked_batch_deletes_the_free_ones() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let free = asset_with_files(&conn, root, &["photos/free.jpg"]);
        let locked = asset_with_files(&conn, root, &["photos/locked.jpg"]);
        conn.execute("UPDATE assets SET lock_level = 1 WHERE id = ?1", [locked])
            .unwrap();

        let report = delete_assets_with(&MovingTrasher::new(), &conn, root, &[free, locked]).unwrap();
        assert_eq!(report.deleted, 1);
        assert_eq!(report.blocked_locked, vec![locked]);
        assert!(!asset_exists(&conn, free));
        assert!(asset_exists(&conn, locked));
    }

    #[test]
    fn a_file_that_is_already_gone_does_not_block_the_cleanup() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let id = asset_with_files(&conn, root, &["photos/a.jpg"]);
        std::fs::remove_file(root.join("photos/a.jpg")).unwrap();

        let trasher = MovingTrasher::new();
        let report = delete_assets_with(&trasher, &conn, root, &[id]).unwrap();
        assert_eq!(report.already_gone, 1);
        assert_eq!(report.deleted, 1, "文件本来就不在了，记录也要清掉");
        assert!(!asset_exists(&conn, id));
    }

    #[test]
    fn a_failed_move_keeps_the_record() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let id = asset_with_files(&conn, root, &["photos/a.jpg", "photos/_RAW/a.orf"]);

        let trasher = MovingTrasher::failing_on(&["a.orf"]);
        let report = delete_assets_with(&trasher, &conn, root, &[id]).unwrap();

        assert_eq!(report.deleted, 0, "有文件没移走就不该删记录");
        assert_eq!(report.failed.len(), 1);
        assert!(report.failed[0].1.contains("被占用"));
        assert!(asset_exists(&conn, id));
        assert!(
            !root.join("photos/a.jpg").exists(),
            "已经移走的那个文件不会回来（报告里有，人去回收站找）"
        );
    }

    #[test]
    fn deleting_removes_the_search_index_row_too() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let id = asset_with_files(&conn, root, &["photos/婚礼现场.jpg"]);
        crate::store::fts::rebuild(&conn).unwrap();
        let hits: i64 = conn
            .query_row(
                "SELECT count(*) FROM assets_fts WHERE assets_fts MATCH ?1",
                [crate::store::fts::quote_for_match("婚礼现场")],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1);

        delete_assets_with(&MovingTrasher::new(), &conn, root, &[id]).unwrap();
        let hits: i64 = conn
            .query_row(
                "SELECT count(*) FROM assets_fts WHERE assets_fts MATCH ?1",
                [crate::store::fts::quote_for_match("婚礼现场")],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 0, "删掉的照片不该还能被搜到");
    }

    #[test]
    fn an_empty_or_unknown_id_list_is_a_noop() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let report = delete_assets_with(&MovingTrasher::new(), &conn, dir.path(), &[]).unwrap();
        assert_eq!(report, DeleteReport::default());

        let report = delete_assets_with(&MovingTrasher::new(), &conn, dir.path(), &[999]).unwrap();
        assert_eq!(report, DeleteReport::default(), "不存在的 id 不该报错");
    }

    #[test]
    fn plan_lists_every_file_of_the_asset() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let id = asset_with_files(&conn, dir.path(), &["photos/a.jpg", "photos/_RAW/a.orf"]);
        let plans = plan(&conn, &[id]).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].rel_paths.len(), 2);
    }

    #[test]
    fn absolute_paths_handle_separators() {
        let root = Path::new("/lib");
        assert_eq!(absolute(root, "photos/a.jpg"), PathBuf::from("/lib/photos/a.jpg"));
        assert_eq!(
            absolute(root, "photos\\a.jpg"),
            PathBuf::from("/lib/photos/a.jpg")
        );
    }

    /// 真的往系统回收站里丢一个文件（**手动跑**：`cargo test -p raybend -- --ignored`）。
    ///
    /// 不放进默认测试：它碰系统环境（CI 里可能没有回收站目录），而且「文件去哪了」
    /// 没法自动断言。这条只用来确认 `trash` crate 在本机确实能工作。
    #[test]
    #[ignore = "碰系统回收站，手动跑"]
    fn system_trash_actually_moves_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("raybend-trash-smoke.txt");
        std::fs::write(&path, b"raybend trash smoke").unwrap();
        SystemTrash.trash(&path).expect("移进回收站失败");
        assert!(!path.exists(), "移走之后原位置不该还有它");
    }
}
