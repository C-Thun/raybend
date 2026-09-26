//! 最近导入过的目录（`app.db` 侧，`REPOSITORY.md` / `design/main.md` §3.1.1）。
//!
//! 「最近」不是一个需要用户维护的收藏夹：每次用户**挑中**一个目录（勾选进「已选目录」），
//! 这里就记一条，并按上限裁剪。记录的是**完整路径**，界面上显示的是 `shortpath`。
//!
//! 三条约定：
//!
//! * **同一目录只留一条** —— 判据是 [`PathForms::folded`]（NFC + 小写），
//!   所以 `D:\Photos` 与 `d:/photos/` 是同一个目录（`AGENTS.md` §7.3）；
//! * **保留第一次记下时的拼写**（`path`）—— 那是用户认得的写法，后面改大小写不覆盖它；
//! * **`include_subdirs` 跟最新的选择走** —— 它是用户此刻的意图，不是历史事实。
//!
//! 这一层是纯数据库逻辑：不认识 Tauri，也不碰文件系统（目录是否还存在由上层判定）。

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::Result;

use super::path_semantics::PathForms;

/// 默认保留条数（用户要求「最近 50 条」）。
///
/// 上限本身可以是配置项（`specs/M1-5.md` §8）：调用方从设置里读到值就传进来，
/// 读不到就用它。**这里不写死上限**，只提供 [`prune`]。
pub const DEFAULT_LIMIT: usize = 50;

/// `recent_dirs` 的一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentDir {
    /// 原始路径（展示与打开用；保留第一次记下时的拼写）。
    pub path: String,
    /// NFC + 小写折叠（比较与唯一性判据）。
    pub path_folded: String,
    /// 勾选这条目录时的「包含子目录」状态。
    pub include_subdirs: bool,
    /// 最近一次使用（Unix 毫秒）。
    pub used_at: i64,
    /// 累计使用次数。
    pub use_count: i64,
}

/// 记一条（已存在则刷新时间、更新「包含子目录」、使用次数 +1）。
///
/// 空路径会被**静默忽略**：往列表里塞一条空路径只会得到一个点不动的行，
/// 而上游（界面）本来就不该产出一个空目录。
pub fn remember(conn: &Connection, path: &str, include_subdirs: bool, now_ms: i64) -> Result<()> {
    let forms = PathForms::new(path);
    if forms.folded().is_empty() {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO recent_dirs(path, path_folded, include_subdirs, used_at, use_count)
         VALUES (?1, ?2, ?3, ?4, 1)
         ON CONFLICT(path_folded) DO UPDATE SET
             include_subdirs = excluded.include_subdirs,
             used_at         = excluded.used_at,
             use_count       = recent_dirs.use_count + 1",
        params![
            forms.raw(),
            forms.folded(),
            i64::from(include_subdirs),
            now_ms
        ],
    )?;
    Ok(())
}

/// 最近使用过的目录，最新的在前。
///
/// `limit` 为 0 时返回空列表（而不是「不限量」）—— 界面上「0 条」就是 0 条，
/// 把 0 当成无限会让调用方的意图反过来。
pub fn list(conn: &Connection, limit: usize) -> Result<Vec<RecentDir>> {
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let mut stmt = conn.prepare(
        "SELECT path, path_folded, include_subdirs, used_at, use_count
           FROM recent_dirs
          ORDER BY used_at DESC, path_folded ASC
          LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |row| {
        Ok(RecentDir {
            path: row.get(0)?,
            path_folded: row.get(1)?,
            include_subdirs: row.get::<_, i64>(2)? != 0,
            used_at: row.get(3)?,
            use_count: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 移除一条（按折叠路径比较）。返回是否真的删掉了。
pub fn forget(conn: &Connection, path: &str) -> Result<bool> {
    let folded = PathForms::new(path).folded().to_string();
    if folded.is_empty() {
        return Ok(false);
    }
    let removed = conn.execute("DELETE FROM recent_dirs WHERE path_folded = ?1", [folded])?;
    Ok(removed > 0)
}

/// 表里现有多少条（统计与测试用）。
pub fn count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT count(*) FROM recent_dirs", [], |r| r.get(0))?)
}

/// 是否存在（按折叠路径比较）。
pub fn contains(conn: &Connection, path: &str) -> Result<bool> {
    let folded = PathForms::new(path).folded().to_string();
    let hit: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM recent_dirs WHERE path_folded = ?1",
            [folded],
            |r| r.get(0),
        )
        .optional()?;
    Ok(hit.is_some())
}

/// 裁剪：只保留最近使用的 `keep` 条，返回删掉的条数。
///
/// `keep = 0` 表示清空。裁剪与 [`remember`] 在同一个事务里做（调用方用
/// `AppDb::write_tx`），所以不会出现「列表先超上限、下一次才收」的中间态。
pub fn prune(conn: &Connection, keep: usize) -> Result<usize> {
    let keep = i64::try_from(keep).unwrap_or(i64::MAX);
    let removed = conn.execute(
        "DELETE FROM recent_dirs
          WHERE path_folded NOT IN (
                SELECT path_folded FROM recent_dirs
                 ORDER BY used_at DESC, path_folded ASC
                 LIMIT ?1
          )",
        [keep],
    )?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::migration::{self, Backups, DbKind};
    use crate::store::pragma;

    const T0: i64 = 1_789_516_800_000;

    fn app() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::App, Backups::none(), T0).unwrap();
        conn
    }

    fn paths(conn: &Connection, limit: usize) -> Vec<String> {
        list(conn, limit).unwrap().into_iter().map(|r| r.path).collect()
    }

    // ---------- 记住 / 列表 ----------

    #[test]
    fn remember_then_list_returns_it() {
        let conn = app();
        remember(&conn, r"D:\Photos\2024", false, T0).unwrap();
        let rows = list(&conn, DEFAULT_LIMIT).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, r"D:\Photos\2024");
        assert_eq!(rows[0].path_folded, "d:/photos/2024");
        assert!(!rows[0].include_subdirs);
        assert_eq!(rows[0].used_at, T0);
        assert_eq!(rows[0].use_count, 1);
    }

    #[test]
    fn empty_or_blank_path_is_ignored() {
        let conn = app();
        remember(&conn, "", true, T0).unwrap();
        // 只有分隔符的路径折叠后是空串（`/` 的 folded 是 "/"，见 path_semantics）：
        // 这里钉的是「空串绝不入表」，不是「所有奇怪路径都拒绝」
        remember(&conn, "", false, T0).unwrap();
        assert_eq!(count(&conn).unwrap(), 0);
        assert!(list(&conn, DEFAULT_LIMIT).unwrap().is_empty());
    }

    #[test]
    fn case_and_separator_variants_collapse_into_one_entry() {
        let conn = app();
        remember(&conn, r"D:\Photos\Trip", false, T0).unwrap();
        remember(&conn, "d:/photos/trip/", false, T0 + 1).unwrap();
        remember(&conn, r"D:\PHOTOS\TRIP", false, T0 + 2).unwrap();

        assert_eq!(count(&conn).unwrap(), 1, "同一目录只留一条");
        let rows = list(&conn, DEFAULT_LIMIT).unwrap();
        // 保留第一次记下时的拼写
        assert_eq!(rows[0].path, r"D:\Photos\Trip");
        assert_eq!(rows[0].used_at, T0 + 2, "时间跟最新一次走");
        assert_eq!(rows[0].use_count, 3);
    }

    #[test]
    fn unicode_paths_are_folded_to_the_same_entry() {
        let conn = app();
        // NFC 的 é 与 NFD 的 e + 组合重音：折叠后必须是同一条（AGENTS.md §7.3）
        let nfc = "/照片/café";
        let nfd = "/照片/cafe\u{0301}";
        remember(&conn, nfc, false, T0).unwrap();
        remember(&conn, nfd, false, T0 + 1).unwrap();
        assert_eq!(count(&conn).unwrap(), 1);
        assert!(contains(&conn, nfc).unwrap());
        assert!(contains(&conn, nfd).unwrap());
    }

    #[test]
    fn remember_updates_include_subdirs_to_the_latest_choice() {
        let conn = app();
        remember(&conn, "/photos", false, T0).unwrap();
        remember(&conn, "/photos", true, T0 + 1).unwrap();
        let rows = list(&conn, DEFAULT_LIMIT).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].include_subdirs, "开关跟用户此刻的选择走");
        // 再关回去也要生效
        remember(&conn, "/photos", false, T0 + 2).unwrap();
        assert!(!list(&conn, DEFAULT_LIMIT).unwrap()[0].include_subdirs);
    }

    #[test]
    fn list_is_newest_first_and_deterministic_on_ties() {
        let conn = app();
        remember(&conn, "/b", false, T0).unwrap();
        remember(&conn, "/a", false, T0).unwrap(); // 同一时刻两条
        remember(&conn, "/c", false, T0 + 5).unwrap();
        assert_eq!(paths(&conn, 10), vec!["/c", "/a", "/b"]);
    }

    #[test]
    fn list_respects_limit_and_zero_means_empty() {
        let conn = app();
        for i in 0..5 {
            remember(&conn, &format!("/dir{i}"), false, T0 + i).unwrap();
        }
        assert_eq!(paths(&conn, 3), vec!["/dir4", "/dir3", "/dir2"]);
        assert_eq!(paths(&conn, 0), Vec::<String>::new());
        assert_eq!(paths(&conn, 99).len(), 5);
    }

    #[test]
    fn very_long_path_is_stored_and_listed() {
        let conn = app();
        let long = format!("/{}", "深/".repeat(2000));
        remember(&conn, &long, false, T0).unwrap();
        assert_eq!(count(&conn).unwrap(), 1);
        assert_eq!(list(&conn, 1).unwrap()[0].path, long);
    }

    // ---------- 移除 ----------

    #[test]
    fn forget_removes_by_folded_path() {
        let conn = app();
        remember(&conn, r"D:\Photos", false, T0).unwrap();
        assert!(forget(&conn, "d:/photos/").unwrap(), "写法不同也是同一条");
        assert_eq!(count(&conn).unwrap(), 0);
        assert!(!forget(&conn, r"D:\Photos").unwrap(), "再删一次不再报成功");
    }

    #[test]
    fn forget_with_blank_path_is_false() {
        let conn = app();
        assert!(!forget(&conn, "").unwrap());
    }

    // ---------- 裁剪 ----------

    #[test]
    fn prune_keeps_only_the_newest_entries() {
        let conn = app();
        for i in 0..60 {
            remember(&conn, &format!("/dir{i:02}"), false, T0 + i).unwrap();
        }
        assert_eq!(count(&conn).unwrap(), 60);

        let removed = prune(&conn, DEFAULT_LIMIT).unwrap();
        assert_eq!(removed, 10);
        assert_eq!(count(&conn).unwrap(), 50);

        let kept = paths(&conn, DEFAULT_LIMIT);
        assert_eq!(kept[0], "/dir59");
        assert_eq!(kept[49], "/dir10");
        assert!(!kept.contains(&"/dir09".to_string()));
    }

    #[test]
    fn prune_with_zero_clears_everything() {
        let conn = app();
        remember(&conn, "/a", false, T0).unwrap();
        remember(&conn, "/b", false, T0 + 1).unwrap();
        assert_eq!(prune(&conn, 0).unwrap(), 2);
        assert_eq!(count(&conn).unwrap(), 0);
    }

    #[test]
    fn prune_under_the_limit_is_a_noop() {
        let conn = app();
        remember(&conn, "/a", false, T0).unwrap();
        assert_eq!(prune(&conn, DEFAULT_LIMIT).unwrap(), 0);
        assert_eq!(count(&conn).unwrap(), 1);
    }

    #[test]
    fn remember_then_prune_keeps_the_list_bounded() {
        let conn = app();
        // 模拟「每次都记一条再收一次」的真实用法
        for i in 0..120 {
            remember(&conn, &format!("/d{i}"), false, T0 + i).unwrap();
            prune(&conn, DEFAULT_LIMIT).unwrap();
        }
        assert_eq!(count(&conn).unwrap(), 50);
        assert_eq!(paths(&conn, 1)[0], "/d119");
    }
}
