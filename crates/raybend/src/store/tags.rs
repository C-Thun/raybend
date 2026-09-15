//! 标签：**全局词典**（`app.db`）+ **每库关联**（`catalog.db`）。
//!
//! 规格出处：`BROWSE.md` §7（标签体系）。设计要点：
//!
//! * **词典与关联分居两库**：标签名是跨库公用的（在 A 库建的词，在 B 库也该能选到），
//!   所以名字住 `app.db` 的 `tags`，而「哪张照片打了哪个标签」住各库的 `asset_tags`。
//! * **跨库引用故意不加外键**：`catalog.db` 必须能**单独搬走**（`REPOSITORY.md` §2）。
//!   由此可能出现「关联里的 tag_id 在词典里查不到」——这不是错误，UI 按「待命名」显示；
//!   库打开时用 [`distinct_tag_ids`] + [`ensure_tags_by_id`] 做一次**词典对齐**即可。
//! * **相等判据 = 折叠名**（NFC + 小写，与 `store::path_semantics` 同一套思路）：
//!   所以 `Trip` 与 `trip` 是同一个标签，不会并存两条。
//! * **所有写操作都假定跑在单写者上**（`AGENTS.md` §6.4 的单一写者 actor），
//!   因此「先查后写」不需要额外的事务竞争处理；本模块的函数只接 `&Connection`，
//!   调用方决定是单条还是放进批量事务里。
//!
//! `use_count`（使用次数）**不由本模块自动跨库维护** —— 它属于 `app.db`，而关联在
//! `catalog.db`。上层在打/去标签后调用 [`bump_use_count`]，或定期用 [`tag_usage`]
//! 重算，两条路都留了。

use rusqlite::{Connection, OptionalExtension, params};
use unicode_normalization::UnicodeNormalization;

use crate::error::{Error, Result};

/// 标签名长度上限（**字符数**，不是字节数）。
///
/// 64 已经远超实际需要（标签是人手输入的短词），但它挡住「粘贴一整段文字当标签」
/// 这种把 UI 撑爆的输入；超限返回 [`Error::Unsupported`]，由 UI 提示用户。
pub const MAX_TAG_NAME_CHARS: usize = 64;

/// 一个标签（词典侧）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub id: i64,
    /// 展示名：保留用户输入的大小写与写法（不还原成折叠形式）。
    pub name: String,
    /// NFC + 小写折叠；相等与唯一性判据。
    pub name_folded: String,
    /// 使用次数（在多少张照片上用过；由上层同步）。
    pub use_count: i64,
    pub created_at: i64,
}

/// 标签名的折叠形式：NFC 规范化 + 小写。
///
/// 与路径折叠同一套思路，但**不做路径那套处理**（去分隔符、合并斜杠等）——
/// 标签名里出现 `/` 是合法的，不该被改写。
#[must_use]
pub fn fold_name(name: &str) -> String {
    name.nfc().collect::<String>().to_lowercase()
}

/// 规范化并校验用户输入的标签名：去首尾空白 → 空则报错 → 超长则报错。
///
/// 返回**清理后的展示名**（内部空白保留原样，不做压缩）。
pub fn clean_name(input: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(Error::Unsupported("标签名不能为空".into()));
    }
    let len = trimmed.chars().count();
    if len > MAX_TAG_NAME_CHARS {
        return Err(Error::Unsupported(format!(
            "标签名太长（{len} 个字符，上限 {MAX_TAG_NAME_CHARS}）"
        )));
    }
    Ok(trimmed.to_string())
}

// ══════════════════════════════════════════════════════════════════
// 词典侧（app.db）
// ══════════════════════════════════════════════════════════════════

const TAG_COLS: &str = "id, name, name_folded, use_count, created_at";

fn row_to_tag(row: &rusqlite::Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: row.get(0)?,
        name: row.get(1)?,
        name_folded: row.get(2)?,
        use_count: row.get(3)?,
        created_at: row.get(4)?,
    })
}

/// 按名字取标签（按折叠名比较）。
pub fn tag_by_name(conn: &Connection, name: &str) -> Result<Option<Tag>> {
    let folded = fold_name(name.trim());
    Ok(conn
        .query_row(
            &format!("SELECT {TAG_COLS} FROM tags WHERE name_folded = ?1"),
            [&folded],
            row_to_tag,
        )
        .optional()?)
}

/// 按 id 取标签。
pub fn tag_by_id(conn: &Connection, id: i64) -> Result<Option<Tag>> {
    Ok(conn
        .query_row(
            &format!("SELECT {TAG_COLS} FROM tags WHERE id = ?1"),
            [id],
            row_to_tag,
        )
        .optional()?)
}

/// 取一批 id 对应的标签；**返回的顺序与入参一致**，查不到的 id 直接跳过。
///
/// 词典对齐用：拿 `catalog.db` 里的 tag_id 来查名字。
pub fn tags_by_ids(conn: &Connection, ids: &[i64]) -> Result<Vec<Tag>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut by_id = std::collections::HashMap::with_capacity(ids.len());
    for t in list_all(conn)? {
        by_id.insert(t.id, t);
    }
    Ok(ids.iter().filter_map(|id| by_id.get(id).cloned()).collect())
}

/// 词典全量（按使用次数倒序、同次数按名字）。
pub fn list_all(conn: &Connection) -> Result<Vec<Tag>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {TAG_COLS} FROM tags ORDER BY use_count DESC, name COLLATE NOCASE ASC"
    ))?;
    let rows = stmt.query_map([], row_to_tag)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 常用标签（给「标签输入框」的提示列表）。
pub fn list_tags(conn: &Connection, limit: usize) -> Result<Vec<Tag>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(&format!(
        "SELECT {TAG_COLS} FROM tags ORDER BY use_count DESC, name COLLATE NOCASE ASC LIMIT ?1"
    ))?;
    let rows = stmt.query_map([limit as i64], row_to_tag)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 词典里有多少个标签。
pub fn count_tags(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT count(*) FROM tags", [], |r| r.get(0))?)
}

/// **取或建**：按折叠名找，找不到就建一个。返回 tag id。
///
/// 名字会先经 [`clean_name`]（去空白、查空、查长度）。
pub fn ensure_tag(conn: &Connection, name: &str, now_ms: i64) -> Result<i64> {
    let name = clean_name(name)?;
    let folded = fold_name(&name);
    if let Some(id) = conn
        .query_row(
            "SELECT id FROM tags WHERE name_folded = ?1",
            [&folded],
            |r| r.get::<_, i64>(0),
        )
        .optional()?
    {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO tags (name, name_folded, use_count, created_at) VALUES (?1, ?2, 0, ?3)",
        params![name, folded, now_ms],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 批量 [`ensure_tag`]；返回与入参**一一对应**的 id 列表（含重复项）。
pub fn ensure_tags(conn: &Connection, names: &[String], now_ms: i64) -> Result<Vec<i64>> {
    names.iter().map(|n| ensure_tag(conn, n, now_ms)).collect()
}

/// 按 id **补登记**词典条目（已知名字时用）。
///
/// 词典对齐用：`catalog.db` 里用到的 tag_id 若在 `app.db` 缺失，就按此补上。
/// 返回是否真的插入了。
pub fn register_tag_with_id(conn: &Connection, id: i64, name: &str, now_ms: i64) -> Result<bool> {
    let name = clean_name(name)?;
    let folded = fold_name(&name);
    let existing: Option<i64> = conn
        .query_row("SELECT id FROM tags WHERE id = ?1", [id], |r| r.get(0))
        .optional()?;
    if existing.is_some() {
        return Ok(false);
    }
    conn.execute(
        "INSERT INTO tags (id, name, name_folded, use_count, created_at) VALUES (?1, ?2, ?3, 0, ?4)",
        params![id, name, folded, now_ms],
    )?;
    Ok(true)
}

/// 改名。目标折叠名已被**别的**标签占用 → [`Error::Unsupported`]。
pub fn rename_tag(conn: &Connection, id: i64, new_name: &str) -> Result<()> {
    let name = clean_name(new_name)?;
    let folded = fold_name(&name);
    let owner: Option<i64> = conn
        .query_row(
            "SELECT id FROM tags WHERE name_folded = ?1",
            [&folded],
            |r| r.get(0),
        )
        .optional()?;
    match owner {
        Some(other) if other != id => Err(Error::Unsupported(format!(
            "已存在同名标签（大小写不敏感）：{name}"
        ))),
        _ => {
            conn.execute(
                "UPDATE tags SET name = ?1, name_folded = ?2 WHERE id = ?3",
                params![name, folded, id],
            )?;
            Ok(())
        }
    }
}

/// 删除词典条目。返回是否删掉了（不存在则 `false`）。
///
/// **注意**：各库的 `asset_tags` 关联不会被连带删除（跨库）；上层应在删词典前
/// 先问清楚，或删完把残留关联当孤儿处理。
pub fn delete_tag(conn: &Connection, id: i64) -> Result<bool> {
    Ok(conn.execute("DELETE FROM tags WHERE id = ?1", [id])? > 0)
}

/// 使用次数增减（**下限 0**，不会变负）。返回新值。
pub fn bump_use_count(conn: &Connection, id: i64, delta: i64) -> Result<i64> {
    conn.execute(
        "UPDATE tags SET use_count = max(0, use_count + ?1) WHERE id = ?2",
        params![delta, id],
    )?;
    Ok(
        conn.query_row("SELECT use_count FROM tags WHERE id = ?1", [id], |r| {
            r.get(0)
        })?,
    )
}

/// 直接设定使用次数（重算用，比 `+1/-1` 更不容易漂移）。
pub fn set_use_count(conn: &Connection, id: i64, value: i64) -> Result<()> {
    conn.execute(
        "UPDATE tags SET use_count = ?1 WHERE id = ?2",
        params![value.max(0), id],
    )?;
    Ok(())
}

// ══════════════════════════════════════════════════════════════════
// 关联侧（catalog.db）
// ══════════════════════════════════════════════════════════════════

/// 一次 `set_tags` 的差量：新增了哪些、去掉了哪些（都给调用方去同步 `use_count`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagDelta {
    /// 本次**新挂上**的 tag id（此前没有的）。
    pub added: Vec<i64>,
    /// 本次**摘掉**的 tag id。
    pub removed: Vec<i64>,
}

impl TagDelta {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

/// 给一张照片挂一个标签。返回是否**新挂上**（重复挂返回 `false`）。
pub fn attach_tag(conn: &Connection, asset_id: i64, tag_id: i64, now_ms: i64) -> Result<bool> {
    let n = conn.execute(
        "INSERT OR IGNORE INTO asset_tags (asset_id, tag_id, tagged_at) VALUES (?1, ?2, ?3)",
        params![asset_id, tag_id, now_ms],
    )?;
    Ok(n > 0)
}

/// 批量挂标签；返回**新挂上**的那些 id（用于同步 `use_count`）。
pub fn attach_tags(
    conn: &Connection,
    asset_id: i64,
    tag_ids: &[i64],
    now_ms: i64,
) -> Result<Vec<i64>> {
    let mut added = Vec::new();
    for &tag_id in tag_ids {
        if attach_tag(conn, asset_id, tag_id, now_ms)? {
            added.push(tag_id);
        }
    }
    Ok(added)
}

/// 摘掉一个标签；返回是否**真的摘掉**（本来就没挂则 `false`）。
pub fn detach_tag(conn: &Connection, asset_id: i64, tag_id: i64) -> Result<bool> {
    Ok(conn.execute(
        "DELETE FROM asset_tags WHERE asset_id = ?1 AND tag_id = ?2",
        params![asset_id, tag_id],
    )? > 0)
}

/// 批量摘标签；返回**真的摘掉**的那些 id。
pub fn detach_tags(conn: &Connection, asset_id: i64, tag_ids: &[i64]) -> Result<Vec<i64>> {
    let mut removed = Vec::new();
    for &tag_id in tag_ids {
        if detach_tag(conn, asset_id, tag_id)? {
            removed.push(tag_id);
        }
    }
    Ok(removed)
}

/// 把一张照片的标签**设成给定集合**（UI 里「编辑标签」的语义）。
///
/// 入参里的重复 id 会去重；返回差量。
pub fn set_tags(
    conn: &Connection,
    asset_id: i64,
    tag_ids: &[i64],
    now_ms: i64,
) -> Result<TagDelta> {
    let mut wanted: Vec<i64> = tag_ids.to_vec();
    wanted.sort_unstable();
    wanted.dedup();

    let current = tags_of_asset(conn, asset_id)?;
    let added: Vec<i64> = wanted
        .iter()
        .copied()
        .filter(|id| !current.contains(id))
        .collect();
    let removed: Vec<i64> = current
        .iter()
        .copied()
        .filter(|id| !wanted.contains(id))
        .collect();

    for &id in &added {
        attach_tag(conn, asset_id, id, now_ms)?;
    }
    for &id in &removed {
        detach_tag(conn, asset_id, id)?;
    }
    Ok(TagDelta { added, removed })
}

/// 一张照片身上的 tag id（**升序**，便于比较与测试）。
pub fn tags_of_asset(conn: &Connection, asset_id: i64) -> Result<Vec<i64>> {
    let mut stmt =
        conn.prepare("SELECT tag_id FROM asset_tags WHERE asset_id = ?1 ORDER BY tag_id")?;
    let rows = stmt.query_map([asset_id], |r| r.get::<_, i64>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 一个标签下的 asset id（分页）。`limit` 为 0 表示不限。
pub fn assets_with_tag(
    conn: &Connection,
    tag_id: i64,
    limit: usize,
    offset: usize,
) -> Result<Vec<i64>> {
    let sql =
        "SELECT asset_id FROM asset_tags WHERE tag_id = ?1 ORDER BY asset_id LIMIT ?2 OFFSET ?3";
    let limit = if limit == 0 { -1 } else { limit as i64 };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![tag_id, limit, offset as i64], |r| {
        r.get::<_, i64>(0)
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 本库**用到了哪些** tag id（升序）。词典对齐用。
pub fn distinct_tag_ids(conn: &Connection) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare("SELECT DISTINCT tag_id FROM asset_tags ORDER BY tag_id")?;
    let rows = stmt.query_map([], |r| r.get::<_, i64>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 本库每个标签的**实际用量**（tag_id, 张数），升序。
///
/// 这是 `use_count` 的**真相源**（关联在本地表，一次 GROUP BY 就全出来）；
/// 拿它去 `app.db` 用 [`set_use_count`] 重算，比上下增减更不容易漂移。
pub fn tag_usage(conn: &Connection) -> Result<Vec<(i64, i64)>> {
    let mut stmt =
        conn.prepare("SELECT tag_id, count(*) FROM asset_tags GROUP BY tag_id ORDER BY tag_id")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 摘掉本库里**指定 tag ids 之外**的关联（词典里删了词之后清残留）。
///
/// 返回清掉的关联条数。`keep` 为空 = 清掉全部关联（谨慎使用）。
pub fn prune_tags_except(conn: &Connection, keep: &[i64]) -> Result<usize> {
    if keep.is_empty() {
        return Ok(conn.execute("DELETE FROM asset_tags", [])?);
    }
    // SQLite 的参数个数有上限，分批删：按「不在白名单里」筛。
    let mut removed = 0usize;
    let mut stmt = conn.prepare("SELECT DISTINCT tag_id FROM asset_tags")?;
    let used: Vec<i64> = stmt
        .query_map([], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for id in used {
        if !keep.contains(&id) {
            removed += conn.execute("DELETE FROM asset_tags WHERE tag_id = ?1", [id])?;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::migration::{self, DbKind};
    use crate::store::pragma;

    const T0: i64 = 1_789_516_800_000; // 2026-09-15 前后，固定值便于断言

    fn app() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::App, migration::Backups::none(), T0).unwrap();
        conn
    }

    fn catalog() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::Catalog, migration::Backups::none(), T0).unwrap();
        conn
    }

    /// 造一张最小资产（`assets` 只有 imported_at / updated_at 是必填）。
    fn asset(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO assets (imported_at, updated_at) VALUES (?1, ?1)",
            [T0],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    // ---------- 折叠与命名 ----------

    #[test]
    fn fold_is_nfc_and_lowercase() {
        assert_eq!(fold_name("Trip"), "trip");
        assert_eq!(fold_name("  TRIP "), "  trip ");
        // NFC：组合字符与预组合字符折叠后相等（macOS 上可能给 NFD）
        assert_eq!(fold_name("cafe\u{301}"), fold_name("café"));
        // 中文不受影响
        assert_eq!(fold_name("旅行"), "旅行");
    }

    #[test]
    fn clean_name_rejects_empty_and_too_long() {
        assert!(clean_name("").is_err());
        assert!(clean_name("   ").is_err());
        assert!(clean_name("\t\n").is_err());
        assert_eq!(clean_name("  trip  ").unwrap(), "trip");
        assert_eq!(clean_name("旅行").unwrap(), "旅行");

        let ok = "字".repeat(MAX_TAG_NAME_CHARS);
        assert!(clean_name(&ok).is_ok(), "刚好到上限应当接受");
        let too_long = "字".repeat(MAX_TAG_NAME_CHARS + 1);
        assert!(clean_name(&too_long).is_err(), "超一个字符就该拒绝");
        // 长度按**字符**算，不是字节：63 个汉字 = 189 字节，仍然合法
        assert!(clean_name(&"汉".repeat(63)).is_ok());
    }

    // ---------- 词典 ----------

    #[test]
    fn ensure_tag_is_idempotent_and_case_insensitive() {
        let conn = app();
        let a = ensure_tag(&conn, "Travel", T0).unwrap();
        let b = ensure_tag(&conn, "travel", T0 + 1).unwrap();
        let c = ensure_tag(&conn, "TRAVEL", T0 + 2).unwrap();
        assert_eq!((a, b, c), (a, a, a), "大小写不同不应新建");
        assert_eq!(count_tags(&conn).unwrap(), 1);

        // 展示名保留**第一次**输入的写法
        let t = tag_by_id(&conn, a).unwrap().unwrap();
        assert_eq!(t.name, "Travel");
        assert_eq!(t.use_count, 0);
        assert_eq!(t.created_at, T0);
    }

    #[test]
    fn ensure_tag_dedupes_nfc_variants() {
        let conn = app();
        let composed = ensure_tag(&conn, "café", T0).unwrap(); // é 预组合
        let decomposed = ensure_tag(&conn, "cafe\u{301}", T0).unwrap(); // e + 组合重音
        assert_eq!(composed, decomposed);
        assert_eq!(count_tags(&conn).unwrap(), 1, "NFC 变体应归一");
    }

    #[test]
    fn ensure_tag_trims_but_keeps_inner_spaces() {
        let conn = app();
        let id = ensure_tag(&conn, "  golden hour  ", T0).unwrap();
        assert_eq!(tag_by_id(&conn, id).unwrap().unwrap().name, "golden hour");
        // 再输入带不同首尾空白的形式 → 同一个
        assert_eq!(ensure_tag(&conn, "golden hour", T0).unwrap(), id);
    }

    #[test]
    fn ensure_tag_rejects_bad_names() {
        let conn = app();
        assert!(ensure_tag(&conn, "   ", T0).is_err());
        assert!(ensure_tag(&conn, &"x".repeat(65), T0).is_err());
        assert_eq!(count_tags(&conn).unwrap(), 0, "被拒绝的输入不该留下记录");
    }

    #[test]
    fn list_orders_by_use_count_then_name() {
        let conn = app();
        let a = ensure_tag(&conn, "beta", T0).unwrap();
        let b = ensure_tag(&conn, "Alpha", T0).unwrap();
        let c = ensure_tag(&conn, "zeta", T0).unwrap();
        bump_use_count(&conn, b, 3).unwrap();
        bump_use_count(&conn, c, 3).unwrap();
        bump_use_count(&conn, a, 1).unwrap();

        let all: Vec<String> = list_all(&conn)
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(all, vec!["Alpha", "zeta", "beta"], "先按次数，再按名字");
        assert_eq!(list_tags(&conn, 2).unwrap().len(), 2);
        assert!(
            list_tags(&conn, 0).unwrap().is_empty(),
            "limit=0 返回空而不是全部"
        );
    }

    #[test]
    fn rename_moves_name_and_rejects_collision() {
        let conn = app();
        let a = ensure_tag(&conn, "trip", T0).unwrap();
        let b = ensure_tag(&conn, "vacation", T0).unwrap();

        rename_tag(&conn, a, "  JOURNEY ").unwrap();
        let t = tag_by_id(&conn, a).unwrap().unwrap();
        assert_eq!(
            (t.name.as_str(), t.name_folded.as_str()),
            ("JOURNEY", "journey")
        );

        // 与别的标签撞折叠名 → 拒绝
        let err = rename_tag(&conn, a, "Vacation").unwrap_err();
        assert!(matches!(err, Error::Unsupported(_)), "应报冲突：{err:?}");
        assert_eq!(tag_by_id(&conn, a).unwrap().unwrap().name, "JOURNEY");

        // 改成自己（不同大小写）是允许的
        rename_tag(&conn, a, "journey").unwrap();
        assert_eq!(tag_by_id(&conn, a).unwrap().unwrap().name, "journey");
        assert!(tag_by_id(&conn, b).unwrap().is_some());
    }

    #[test]
    fn bump_use_count_never_goes_negative() {
        let conn = app();
        let id = ensure_tag(&conn, "x", T0).unwrap();
        assert_eq!(bump_use_count(&conn, id, 2).unwrap(), 2);
        assert_eq!(bump_use_count(&conn, id, -1).unwrap(), 1);
        assert_eq!(bump_use_count(&conn, id, -5).unwrap(), 0, "下限是 0");
        set_use_count(&conn, id, -3).unwrap();
        assert_eq!(tag_by_id(&conn, id).unwrap().unwrap().use_count, 0);
        set_use_count(&conn, id, 7).unwrap();
        assert_eq!(tag_by_id(&conn, id).unwrap().unwrap().use_count, 7);
    }

    #[test]
    fn delete_tag_reports_whether_it_existed() {
        let conn = app();
        let id = ensure_tag(&conn, "temp", T0).unwrap();
        assert!(delete_tag(&conn, id).unwrap());
        assert!(!delete_tag(&conn, id).unwrap(), "再删就没有了");
        assert!(tag_by_id(&conn, id).unwrap().is_none());
    }

    #[test]
    fn tags_by_ids_keeps_order_and_skips_missing() {
        let conn = app();
        let a = ensure_tag(&conn, "a", T0).unwrap();
        let b = ensure_tag(&conn, "b", T0).unwrap();
        let got = tags_by_ids(&conn, &[b, 9999, a]).unwrap();
        assert_eq!(
            got.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            vec!["b", "a"],
            "顺序跟随入参，缺失的跳过"
        );
        assert!(tags_by_ids(&conn, &[]).unwrap().is_empty());
    }

    #[test]
    fn ensure_tags_maps_one_to_one() {
        let conn = app();
        let ids = ensure_tags(
            &conn,
            &["a".to_string(), "A".to_string(), "b".to_string()],
            T0,
        )
        .unwrap();
        assert_eq!(ids.len(), 3);
        assert_eq!(ids[0], ids[1], "折叠相同的两个输入给同一个 id");
        assert_ne!(ids[0], ids[2]);
        assert_eq!(count_tags(&conn).unwrap(), 2);
    }

    // ---------- 关联 ----------

    #[test]
    fn attach_is_idempotent_and_reports_new() {
        let conn = catalog();
        let a = asset(&conn);
        assert!(attach_tag(&conn, a, 7, T0).unwrap(), "第一次是真挂上");
        assert!(!attach_tag(&conn, a, 7, T0 + 9).unwrap(), "重复挂不算新增");
        assert_eq!(tags_of_asset(&conn, a).unwrap(), vec![7]);

        // 时间戳保留第一次的（不因重复挂而刷新）
        let at: i64 = conn
            .query_row(
                "SELECT tagged_at FROM asset_tags WHERE asset_id = ?1 AND tag_id = 7",
                [a],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(at, T0);

        let added = attach_tags(&conn, a, &[7, 8, 8, 9], T0).unwrap();
        assert_eq!(added, vec![8, 9], "只报新挂上的");
        assert_eq!(tags_of_asset(&conn, a).unwrap(), vec![7, 8, 9]);
    }

    #[test]
    fn detach_reports_removed() {
        let conn = catalog();
        let a = asset(&conn);
        attach_tags(&conn, a, &[1, 2, 3], T0).unwrap();
        assert!(detach_tag(&conn, a, 2).unwrap());
        assert!(!detach_tag(&conn, a, 2).unwrap(), "再摘就没有了");
        assert!(!detach_tag(&conn, a, 42).unwrap(), "本来就没挂");
        assert_eq!(detach_tags(&conn, a, &[1, 42]).unwrap(), vec![1]);
        assert_eq!(tags_of_asset(&conn, a).unwrap(), vec![3]);
    }

    #[test]
    fn set_tags_computes_delta() {
        let conn = catalog();
        let a = asset(&conn);
        attach_tags(&conn, a, &[1, 2, 3], T0).unwrap();

        // 1,2,3 → 2,3,4：新增 4，去掉 1
        let d = set_tags(&conn, a, &[2, 3, 4], T0 + 1).unwrap();
        assert_eq!(d.added, vec![4]);
        assert_eq!(d.removed, vec![1]);
        assert!(!d.is_empty());
        assert_eq!(tags_of_asset(&conn, a).unwrap(), vec![2, 3, 4]);

        // 入参去重：4 写两遍只算一次；2、3 不在目标集合里，要摘掉
        let d2 = set_tags(&conn, a, &[4, 4], T0 + 2).unwrap();
        assert!(d2.added.is_empty(), "4 已在身上，不该算新增：{d2:?}");
        assert_eq!(d2.removed, vec![2, 3]);
        assert_eq!(tags_of_asset(&conn, a).unwrap(), vec![4]);

        // 幂等：同一个集合再设一次，应当完全无变化
        let again = set_tags(&conn, a, &[4], T0 + 3).unwrap();
        assert!(again.is_empty(), "重复设置同一集合应无差量：{again:?}");

        // 清空
        let d3 = set_tags(&conn, a, &[], T0 + 4).unwrap();
        assert_eq!(d3.removed, vec![4]);
        assert!(tags_of_asset(&conn, a).unwrap().is_empty());

        // 空 → 空：无事发生
        assert!(set_tags(&conn, a, &[], T0 + 5).unwrap().is_empty());
    }

    #[test]
    fn tags_are_per_asset() {
        let conn = catalog();
        let a = asset(&conn);
        let b = asset(&conn);
        attach_tags(&conn, a, &[1, 2], T0).unwrap();
        attach_tags(&conn, b, &[2, 3], T0).unwrap();
        assert_eq!(tags_of_asset(&conn, a).unwrap(), vec![1, 2]);
        assert_eq!(tags_of_asset(&conn, b).unwrap(), vec![2, 3]);
        assert_eq!(assets_with_tag(&conn, 2, 0, 0).unwrap(), vec![a, b]);
        assert_eq!(assets_with_tag(&conn, 1, 0, 0).unwrap(), vec![a]);
        assert!(assets_with_tag(&conn, 99, 0, 0).unwrap().is_empty());
    }

    #[test]
    fn assets_with_tag_pages() {
        let conn = catalog();
        let ids: Vec<i64> = (0..5).map(|_| asset(&conn)).collect();
        for id in &ids {
            attach_tag(&conn, *id, 5, T0).unwrap();
        }
        assert_eq!(
            assets_with_tag(&conn, 5, 2, 0).unwrap(),
            vec![ids[0], ids[1]]
        );
        assert_eq!(
            assets_with_tag(&conn, 5, 2, 2).unwrap(),
            vec![ids[2], ids[3]]
        );
        assert_eq!(assets_with_tag(&conn, 5, 2, 4).unwrap(), vec![ids[4]]);
        assert!(assets_with_tag(&conn, 5, 2, 9).unwrap().is_empty());
        assert_eq!(
            assets_with_tag(&conn, 5, 0, 0).unwrap().len(),
            5,
            "limit=0 不限"
        );
    }

    #[test]
    fn deleting_asset_cascades_associations() {
        let conn = catalog();
        let a = asset(&conn);
        let b = asset(&conn);
        attach_tags(&conn, a, &[1, 2], T0).unwrap();
        attach_tags(&conn, b, &[2], T0).unwrap();

        conn.execute("DELETE FROM assets WHERE id = ?1", [a])
            .unwrap();
        assert!(
            tags_of_asset(&conn, a).unwrap().is_empty(),
            "关联应随资产级联删除"
        );
        assert_eq!(assets_with_tag(&conn, 2, 0, 0).unwrap(), vec![b]);
    }

    #[test]
    fn usage_and_distinct_ids() {
        let conn = catalog();
        let a = asset(&conn);
        let b = asset(&conn);
        attach_tags(&conn, a, &[10, 20], T0).unwrap();
        attach_tags(&conn, b, &[20], T0).unwrap();
        assert_eq!(distinct_tag_ids(&conn).unwrap(), vec![10, 20]);
        assert_eq!(tag_usage(&conn).unwrap(), vec![(10, 1), (20, 2)]);
    }

    #[test]
    fn prune_keeps_only_whitelisted_tags() {
        let conn = catalog();
        let a = asset(&conn);
        attach_tags(&conn, a, &[1, 2, 3], T0).unwrap();

        assert_eq!(
            prune_tags_except(&conn, &[2, 3]).unwrap(),
            1,
            "清掉 1 一处关联"
        );
        assert_eq!(tags_of_asset(&conn, a).unwrap(), vec![2, 3]);

        assert_eq!(
            prune_tags_except(&conn, &[2, 3]).unwrap(),
            0,
            "已干净则是 0"
        );
        assert_eq!(prune_tags_except(&conn, &[]).unwrap(), 2, "空白名单 = 全清");
        assert!(tags_of_asset(&conn, a).unwrap().is_empty());
    }

    // ---------- 跨库：词典对齐 ----------

    #[test]
    fn dictionary_alignment_flow() {
        // 库里有 7 / 8 两个关联，但 app.db 词典是空的（catalog 被单独搬过来的情形）
        let cat = catalog();
        let a = asset(&cat);
        attach_tags(&cat, a, &[7, 8], T0).unwrap();

        let app_conn = app();
        assert!(tag_by_id(&app_conn, 7).unwrap().is_none(), "本来查不到");

        // 对齐：按 id 补登记
        let used = distinct_tag_ids(&cat).unwrap();
        assert_eq!(used, vec![7, 8]);
        for id in &used {
            register_tag_with_id(&app_conn, *id, &format!("tag-{id}"), T0).unwrap();
        }
        assert_eq!(tag_by_id(&app_conn, 7).unwrap().unwrap().name, "tag-7");
        // 重复对齐是幂等的
        assert!(!register_tag_with_id(&app_conn, 7, "别的名字", T0).unwrap());
        assert_eq!(tag_by_id(&app_conn, 7).unwrap().unwrap().name, "tag-7");

        // 用本库用量重算 use_count
        for (id, n) in tag_usage(&cat).unwrap() {
            set_use_count(&app_conn, id, n).unwrap();
        }
        let all = list_all(&app_conn).unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().all(|t| t.use_count == 1));
    }

    #[test]
    fn missing_dictionary_entry_is_not_an_error() {
        // 词典里删过词、库里还留着关联：这是**允许**的状态，不该报错
        let cat = catalog();
        let a = asset(&cat);
        attach_tag(&cat, a, 4242, T0).unwrap();
        let app_conn = app();
        assert!(tags_by_ids(&app_conn, &[4242]).unwrap().is_empty());
        assert_eq!(tags_of_asset(&cat, a).unwrap(), vec![4242]);
    }

    // ---------- 与 assets 的其它字段共存 ----------

    #[test]
    fn v2_marking_columns_have_sane_defaults() {
        let conn = catalog();
        let a = asset(&conn);
        let (label, like_state, lock): (Option<String>, Option<String>, i64) = conn
            .query_row(
                "SELECT color_label, like_state, lock_level FROM assets WHERE id = ?1",
                [a],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert!(label.is_none(), "色标默认未设");
        assert!(like_state.is_none(), "喜欢默认未设");
        assert_eq!(lock, 0, "锁默认是 0（无锁）");

        // 写进去再读回来（含中文与非法范围值之外的合法值）
        conn.execute(
            "UPDATE assets SET color_label = 'purple', like_state = 'like', lock_level = 2,
             author = '张三', description = '海边的清晨', gps_lat = 22.3, gps_lon = 114.2,
             country = '中国', province_state = '香港', city = '西贡', sublocation = '码头',
             taken_at_offset_min = 480 WHERE id = ?1",
            [a],
        )
        .unwrap();
        let (author, desc, off): (String, String, i64) = conn
            .query_row(
                "SELECT author, description, taken_at_offset_min FROM assets WHERE id = ?1",
                [a],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            (author.as_str(), desc.as_str(), off),
            ("张三", "海边的清晨", 480)
        );
    }

    #[test]
    fn v2_fts_indexes_description_but_not_tags() {
        let conn = catalog();
        let a = asset(&conn);
        conn.execute(
            "INSERT INTO assets_fts(rowid, file_name, camera, lens, description)
             VALUES (?1, 'MYP0001.jpg', 'Panasonic DC-S5', 'Lumix 20-60', ?2)",
            params![a, "海边清晨的码头"],
        )
        .unwrap();

        // 中文 ≥3 字能搜到（trigram 的分词下限）
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM assets_fts WHERE assets_fts MATCH '清晨的'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "description 应当进得了索引");

        // 短词（<3 字符）搜不到 —— 这是 trigram 的已知限制，搜索实现时要 LIKE 兜底
        let short: i64 = conn
            .query_row(
                "SELECT count(*) FROM assets_fts WHERE assets_fts MATCH '海边'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(short, 0, "已记录的限制：2 字查询搜不到");

        // v1 的老列（file_name / camera / lens）照旧可用
        let f: i64 = conn
            .query_row(
                "SELECT count(*) FROM assets_fts WHERE assets_fts MATCH 'MYP0001'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(f, 1);
    }

    #[test]
    fn v2_migration_carries_old_fts_rows_over() {
        // 模拟「已经是 v1 且有数据」的库升级到 v2：FTS 重建后旧行要还在
        let mut conn = Connection::open_in_memory().unwrap();
        pragma::apply(&conn, false).unwrap();

        // 只跑 v1
        let v1 = &crate::store::migration::CATALOG_MIGRATIONS[..1];
        conn.execute_batch(v1[0].sql).unwrap();
        conn.execute(
            "INSERT INTO assets (imported_at, updated_at) VALUES (?1, ?1)",
            [T0],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO assets_fts(rowid, file_name, camera, lens) VALUES (?1, 'OLD0001.jpg', 'Nikon Z6', '50mm')",
            [id],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 1_i64).unwrap();

        // 再走正常迁移路径升到 v2
        let out =
            migration::apply(&mut conn, DbKind::Catalog, migration::Backups::none(), T0).unwrap();
        assert_eq!(out.applied, vec![2]);

        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM assets_fts WHERE assets_fts MATCH 'OLD0001'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "v1 的全文索引内容应被搬到 v2 的表里");
        // 新列可写
        conn.execute(
            "UPDATE assets_fts SET description = '补写的描述' WHERE rowid = ?1",
            [id],
        )
        .unwrap();
    }
}
