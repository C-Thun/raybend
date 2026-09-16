//! 全文检索索引（`assets_fts`）的维护。
//!
//! # 为什么需要这个模块
//!
//! `catalog_0001_init.sql` 建了 `assets_fts`（FTS5 + trigram），但 **M1 从头到尾
//! 没有人往里写过一行** —— 结果就是「搜什么都是空」。检索必须有人维护索引，
//! 这里补上这件事。
//!
//! # 定位：派生索引，可整份重建
//!
//! 与缩略图缓存同一个口径（`AGENTS.md` §6.5）：`assets_fts` 里的东西**全部能从
//! `assets` + `asset_files` 再算出来**，所以：
//!
//! * **不挂进导入的热路径**（那里每多一次写就多一分出错与变慢的机会）；
//! * 用一枚**签名**（资产条数 + 最大 id）判断索引是否过期，过期就在查询前重建；
//! * 重建是幂等的：删光重填，中途失败下回再来一遍。
//!
//! 代价：导入之后第一次搜索要多花一次重建（10 万条约 1 秒）。换来的是「索引永远不会
//! 因为某条导入路径忘了写而悄悄缺数据」——这类 bug 在界面上表现为「搜不到」，
//! 极难被发现，也很难查。
//!
//! # 存的是什么
//!
//! | 列 | 来源 |
//! | --- | --- |
//! | `file_name` | 展示用文件的**文件名**（含扩展名；从库内相对路径取末段） |
//! | `camera` | `make + model` |
//! | `lens` | 镜头 |
//! | `description` | 用户写的描述（v2 新增） |
//!
//! ⚠️ **标签不进 FTS**：标签名住在 `app.db`，在每库冗余一份会漂移；搜标签时用
//! `asset_tags` 关联去查（`FUTURE.md` H8）。

use rusqlite::Connection;

use crate::error::Result;
use crate::store::time::now_millis;

/// `repository_meta` 里记签名的键。
pub const STAMP_KEY: &str = "fts_stamp";

/// 把用户输入变成一个**安全的 FTS5 MATCH 串**。
///
/// FTS5 的查询语法里 `"`、`*`、`-`、`(`、`)`、`:` 都有含义：用户随手输入的
/// `DC-G9` 会被当成「DC 且非 G9」，一个引号就能让整条 MATCH 变成语法错误。
/// 办法是把整串包成**一个带引号的短语**，内部的 `"` 双写转义。
#[must_use]
pub fn quote_for_match(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\"\""))
}

/// 当前数据的签名：**资产条数 + 最大 id**。
///
/// 为什么这两项就够：插入会改变条数或最大 id；删除会改变条数；改元数据（描述、镜头）
/// 在界面上是「正在编辑某一行的字段」，而那条路径**自己会调 [`refresh_asset`]**。
fn signature(conn: &Connection) -> Result<String> {
    let (count, max_id): (i64, Option<i64>) =
        conn.query_row("SELECT count(*), max(id) FROM assets", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    Ok(format!("{}:{}", count, max_id.unwrap_or(0)))
}

/// 把当前签名写进 `repository_meta`。
fn write_stamp(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT INTO repository_meta(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![STAMP_KEY, signature(conn)?],
    )?;
    Ok(())
}

/// 索引里记下的签名（从没建过 → `None`）。
pub fn stamp(conn: &Connection) -> Result<Option<String>> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM repository_meta WHERE key = ?1",
            [STAMP_KEY],
            |r| r.get(0),
        )
        .ok();
    Ok(value)
}

/// 索引是否是最新的。
pub fn is_fresh(conn: &Connection) -> Result<bool> {
    Ok(stamp(conn)?.as_deref() == Some(signature(conn)?.as_str()))
}

/// 过期就重建；返回是否真的重建过。
pub fn ensure_fresh(conn: &Connection) -> Result<bool> {
    if is_fresh(conn)? {
        return Ok(false);
    }
    rebuild(conn)?;
    Ok(true)
}

/// 整份重建索引（幂等）。
pub fn rebuild(conn: &Connection) -> Result<usize> {
    let now = now_millis();

    // ① 先把要进索引的数据取出来（含文件名，末段在 Rust 里取，SQL 里做这件事很难看）
    let mut rows: Vec<(i64, String, String, String, String)> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT a.id,
                    COALESCE((SELECT f.rel_path FROM asset_files f
                               WHERE f.asset_id = a.id AND f.role = 'bitmap' LIMIT 1),
                             (SELECT f.rel_path FROM asset_files f
                               WHERE f.asset_id = a.id ORDER BY f.role LIMIT 1), ''),
                    a.camera_make, a.camera_model, a.lens, a.description
               FROM assets a",
        )?;
        let mut query = stmt.query([])?;
        while let Some(row) = query.next()? {
            let id: i64 = row.get(0)?;
            let rel_path: String = row.get(1)?;
            let make: Option<String> = row.get(2)?;
            let model: Option<String> = row.get(3)?;
            let lens: Option<String> = row.get(4)?;
            let description: Option<String> = row.get(5)?;

            let file_name = rel_path
                .rsplit('/')
                .next()
                .unwrap_or(&rel_path)
                .to_string();
            let camera = format!(
                "{} {}",
                make.unwrap_or_default().trim(),
                model.unwrap_or_default().trim()
            )
            .trim()
            .to_string();
            rows.push((
                id,
                file_name,
                camera,
                lens.unwrap_or_default(),
                description.unwrap_or_default(),
            ));
        }
    }

    // ② 清空重填（放在同一个事务里：要么全是新索引，要么还是旧的）
    conn.execute("DELETE FROM assets_fts", [])?;
    {
        let mut insert = conn.prepare(
            "INSERT INTO assets_fts(rowid, file_name, camera, lens, description)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for (id, file_name, camera, lens, description) in &rows {
            insert.execute(rusqlite::params![id, file_name, camera, lens, description])?;
        }
    }

    // ③ 记下签名（用它决定下次还要不要重建）
    write_stamp(conn)?;

    let _ = now; // 时间戳留给将来做增量维护时的诊断字段
    Ok(rows.len())
}

/// 索引一行的一手材料：`(相对路径, 品牌, 型号, 镜头, 描述)`。
type IndexSource = (String, Option<String>, Option<String>, Option<String>, Option<String>);

/// 只刷新一张照片的索引行（改描述 / 镜头时用，比重建快得多）。
pub fn refresh_asset(conn: &Connection, asset_id: i64) -> Result<()> {
    let row: Option<IndexSource> =
        conn.query_row(
            "SELECT COALESCE((SELECT f.rel_path FROM asset_files f
                               WHERE f.asset_id = a.id AND f.role = 'bitmap' LIMIT 1),
                             (SELECT f.rel_path FROM asset_files f
                               WHERE f.asset_id = a.id ORDER BY f.role LIMIT 1), ''),
                    a.camera_make, a.camera_model, a.lens, a.description
               FROM assets a WHERE a.id = ?1",
            [asset_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .ok();

    conn.execute("DELETE FROM assets_fts WHERE rowid = ?1", [asset_id])?;
    let Some((rel_path, make, model, lens, description)) = row else {
        // 资产已经没了 → 索引里也不该有它。
        // ⚠️ 仍然要把签名重记一次：删资产会让签名变成「条数变少」，
        // 不重记的话下次查询会以为索引过期、白白重建一遍全库。
        write_stamp(conn)?;
        return Ok(());
    };
    let file_name = rel_path.rsplit('/').next().unwrap_or(&rel_path).to_string();
    let camera = format!(
        "{} {}",
        make.unwrap_or_default().trim(),
        model.unwrap_or_default().trim()
    )
    .trim()
    .to_string();
    conn.execute(
        "INSERT INTO assets_fts(rowid, file_name, camera, lens, description)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            asset_id,
            file_name,
            camera,
            lens.unwrap_or_default(),
            description.unwrap_or_default()
        ],
    )?;
    // 单行更新不改变签名（条数与最大 id 都没变），但把签名**重记一次**更稳妥：
    // 万一是「先删了一个资产再改另一个」，签名会自然对上。
    write_stamp(conn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::migration::{self, DbKind};
    use crate::store::pragma;

    const T0: i64 = 1_789_516_800_000;

    fn catalog() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::Catalog, migration::Backups::none(), T0).unwrap();
        conn
    }

    /// 加一张照片（一个文件）。
    fn add(conn: &Connection, rel_path: &str) -> i64 {
        conn.execute(
            "INSERT INTO assets (imported_at, updated_at) VALUES (?1, ?1)",
            [T0],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        let ext = rel_path.rsplit('.').next().unwrap_or("").to_string();
        conn.execute(
            "INSERT INTO asset_files
               (asset_id, role, rel_path, rel_path_folded, ext, created_at, updated_at)
             VALUES (?1, 'bitmap', ?2, ?3, ?4, ?5, ?5)",
            rusqlite::params![id, rel_path, rel_path.to_lowercase(), ext, T0],
        )
        .unwrap();
        id
    }

    fn hits(conn: &Connection, query: &str) -> Vec<i64> {
        let mut stmt = conn
            .prepare("SELECT rowid FROM assets_fts WHERE assets_fts MATCH ?1 ORDER BY rowid")
            .unwrap();
        stmt.query_map([quote_for_match(query)], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn an_empty_catalog_is_fresh_after_rebuild() {
        let conn = catalog();
        assert!(!is_fresh(&conn).unwrap(), "从没建过 → 过期");
        assert_eq!(rebuild(&conn).unwrap(), 0);
        assert!(is_fresh(&conn).unwrap(), "空库重建后就是新的");
        assert!(!ensure_fresh(&conn).unwrap(), "已经新了就不该再建");
    }

    #[test]
    fn indexing_takes_the_file_name_not_the_whole_path() {
        let conn = catalog();
        add(&conn, "photos/2026-08-15/婚礼现场.jpg");
        rebuild(&conn).unwrap();
        assert_eq!(hits(&conn, "婚礼现场"), vec![1], "搜到的应当是文件名");
        assert_eq!(
            hits(&conn, "photos"),
            Vec::<i64>::new(),
            "目录名不进索引（否则搜什么都能命中一堆）"
        );
    }

    #[test]
    fn adding_an_asset_makes_the_index_stale() {
        let conn = catalog();
        add(&conn, "a/one.jpg");
        rebuild(&conn).unwrap();
        assert!(is_fresh(&conn).unwrap());

        add(&conn, "a/two.jpg");
        assert!(!is_fresh(&conn).unwrap(), "多了资产就该过期");
        assert!(ensure_fresh(&conn).unwrap(), "ensure 应当把它重建");
        assert!(is_fresh(&conn).unwrap());
        assert_eq!(hits(&conn, "two"), vec![2]);
    }

    #[test]
    fn rebuilding_is_idempotent_and_does_not_duplicate() {
        let conn = catalog();
        add(&conn, "a/one.jpg");
        rebuild(&conn).unwrap();
        rebuild(&conn).unwrap();
        rebuild(&conn).unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM assets_fts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "重复重建不该翻倍");
    }

    #[test]
    fn refresh_one_asset_keeps_the_index_fresh() {
        let conn = catalog();
        let id = add(&conn, "a/one.jpg");
        rebuild(&conn).unwrap();

        conn.execute("UPDATE assets SET description = '海边的黄昏' WHERE id = ?1", [id])
            .unwrap();
        // 直接改库（绕过 refresh）会让签名仍然「看起来新」——
        // 所以单行编辑的路径**必须**自己调 refresh_asset
        refresh_asset(&conn, id).unwrap();
        assert!(is_fresh(&conn).unwrap());
        assert_eq!(hits(&conn, "海边的黄昏"), vec![id]);
    }

    #[test]
    fn refresh_after_delete_drops_the_row() {
        let conn = catalog();
        let id = add(&conn, "a/one.jpg");
        rebuild(&conn).unwrap();
        conn.execute("DELETE FROM assets WHERE id = ?1", [id]).unwrap();
        refresh_asset(&conn, id).unwrap();
        assert_eq!(hits(&conn, "one"), Vec::<i64>::new());
        assert!(is_fresh(&conn).unwrap(), "删掉之后签名应当重新对上");
    }

    #[test]
    fn camera_and_lens_are_searchable_without_paths() {
        let conn = catalog();
        let id = add(&conn, "a/IMG_0001.RW2");
        conn.execute(
            "UPDATE assets SET camera_make = 'Panasonic', camera_model = 'DC-G9', lens = 'LUMIX 12-35mm'
             WHERE id = ?1",
            [id],
        )
        .unwrap();
        rebuild(&conn).unwrap();
        assert_eq!(hits(&conn, "Panasonic"), vec![id]);
        assert_eq!(hits(&conn, "DC-G9"), vec![id]);
        assert_eq!(hits(&conn, "LUMIX"), vec![id]);
    }

    #[test]
    fn a_raw_only_asset_still_gets_indexed() {
        let conn = catalog();
        conn.execute(
            "INSERT INTO assets (imported_at, updated_at) VALUES (?1, ?1)",
            [T0],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO asset_files
               (asset_id, role, rel_path, rel_path_folded, ext, created_at, updated_at)
             VALUES (?1, 'raw', 'photos/IMG_0002.ORF', 'photos/img_0002.orf', 'orf', ?2, ?2)",
            rusqlite::params![id, T0],
        )
        .unwrap();
        rebuild(&conn).unwrap();
        assert_eq!(
            hits(&conn, "IMG_0002"),
            vec![id],
            "只有 RAW 的照片也要能被搜到"
        );
    }

    #[test]
    fn bitmap_wins_over_raw_when_both_exist() {
        let conn = catalog();
        conn.execute(
            "INSERT INTO assets (imported_at, updated_at) VALUES (?1, ?1)",
            [T0],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        for (role, rel) in [
            ("raw", "photos/2026-08-15/_RAW/MYP0001.ORF"),
            ("bitmap", "photos/2026-08-15/MYP0001.png"),
        ] {
            let ext = rel.rsplit('.').next().unwrap();
            conn.execute(
                "INSERT INTO asset_files
                   (asset_id, role, rel_path, rel_path_folded, ext, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                rusqlite::params![id, role, rel, rel.to_lowercase(), ext, T0],
            )
            .unwrap();
        }
        rebuild(&conn).unwrap();
        assert_eq!(hits(&conn, "MYP0001"), vec![id]);
        let name: String = conn
            .query_row("SELECT file_name FROM assets_fts WHERE rowid = ?1", [id], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(name, "MYP0001.png", "展示用文件名取位图那份");
    }

    #[test]
    fn unicode_and_quotes_in_names_survive_rebuild() {
        let conn = catalog();
        add(&conn, "a/海边\"日落\".jpg");
        rebuild(&conn).unwrap();
        // ⚠️ trigram 按 3 个字符切分 ⇒ **少于 3 个字符的查询永远搜不到**
        // （`AGENTS.md` §7.2；查询侧对短词会回退 LIKE，这里测的是索引本身）
        assert_eq!(hits(&conn, "海边\"日"), vec![1], "带引号的片段要能搜到");
        assert_eq!(hits(&conn, "\"日落\""), vec![1], "整段（含引号）也要能搜到");
        assert_eq!(hits(&conn, "落\".j"), vec![1], "跨引号与扩展名的三字串也命中");
        assert_eq!(hits(&conn, "日落"), Vec::<i64>::new(), "两字词确实搜不到（查询侧会回退 LIKE）");
    }
}
