//! 标记写入与撤销：评分 / 色标 / 喜欢 / 锁 / 标签。
//!
//! 出处：`BROWSE.md` §3.2（标记系列）、§3.3（标签）、§3.4（锁）。
//!
//! # 三件容易做错的事，这里显式处理
//!
//! 1. **撤销要能精确还原**：所以每次改动都先读旧值，落成一组 [`Op`]
//!    （`before` / `after` 都在里面），撤销就是把这组操作**反过来**执行一遍。
//!    只记「改了哪些 id」是不够的 —— 批量打星时每张的旧值可能都不一样。
//! 2. **锁要真的挡住写**（`BROWSE.md` §3.4）：二级锁（不可编辑）挡住标记与标签；
//!    一级锁（不可删）只挡删除。**锁自己的修改不受锁限制** —— 否则锁上就解不开了。
//! 3. **批量里有一半被锁住时**不能整批失败：能改的改掉，被挡的如实报回来
//!    （返回值里有 `skipped_locked`，UI 才能提示「有 3 张被锁着没改」）。
//!
//! # 撤销栈为什么在 Rust 侧
//!
//! 它必须与「真的写进去了多少」一致：前端只发「标 3 星」，Rust 侧才知道哪几张因为
//! 锁被跳过了。栈放在 Rust 侧，UI 只调 `undo` / `redo` 两个命令，不需要自己维护补丁。
//! 栈是**内存态**（关软件即清）—— 与 `BROWSE.md` 对旗标的取舍一致。

use std::collections::BTreeSet;

use rusqlite::Connection;

use crate::error::{Error, Result};
use crate::store::time::now_millis;

/// 合法色标（与 `DESIGN.md` 的五个色标令牌一一对应）。
pub const COLORS: [&str; 5] = ["red", "yellow", "green", "blue", "purple"];
/// 合法的「喜欢」取值。
pub const LIKES: [&str; 2] = ["like", "dislike"];

/// 无锁。
pub const LOCK_NONE: u8 = 0;
/// 一级锁：**不能删**。
pub const LOCK_NO_DELETE: u8 = 1;
/// 二级锁：**不能编辑**（比一级更严，含标记与标签）。
pub const LOCK_NO_EDIT: u8 = 2;

/// 撤销栈的默认深度。100 步足够覆盖「刚发现打错了」这类场景，内存也几乎为零。
pub const DEFAULT_UNDO_DEPTH: usize = 100;

/// 一次改动里的**一步**。`before` / `after` 都在里面 —— 这是撤销能精确还原的前提。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Rating {
        asset_id: i64,
        before: u8,
        after: u8,
    },
    Color {
        asset_id: i64,
        before: Option<String>,
        after: Option<String>,
    },
    Like {
        asset_id: i64,
        before: Option<String>,
        after: Option<String>,
    },
    Lock {
        asset_id: i64,
        before: u8,
        after: u8,
    },
    TagAttach {
        asset_id: i64,
        tag_id: i64,
    },
    TagDetach {
        asset_id: i64,
        tag_id: i64,
    },
}

impl Op {
    /// 这一步动的是哪张照片。
    #[must_use]
    pub const fn asset_id(&self) -> i64 {
        match self {
            Self::Rating { asset_id, .. }
            | Self::Color { asset_id, .. }
            | Self::Like { asset_id, .. }
            | Self::Lock { asset_id, .. }
            | Self::TagAttach { asset_id, .. }
            | Self::TagDetach { asset_id, .. } => *asset_id,
        }
    }

    /// 这一步要不要受「二级锁」约束？
    ///
    /// **`Lock` 自己不受约束** —— 否则锁上之后就再也解不开（人会把自己关在门外）。
    #[must_use]
    pub const fn respects_edit_lock(&self) -> bool {
        !matches!(self, Self::Lock { .. })
    }

    /// 反过来执行的那一步。
    #[must_use]
    pub fn invert(&self) -> Self {
        match self {
            Self::Rating {
                asset_id,
                before,
                after,
            } => Self::Rating {
                asset_id: *asset_id,
                before: *after,
                after: *before,
            },
            Self::Color {
                asset_id,
                before,
                after,
            } => Self::Color {
                asset_id: *asset_id,
                before: after.clone(),
                after: before.clone(),
            },
            Self::Like {
                asset_id,
                before,
                after,
            } => Self::Like {
                asset_id: *asset_id,
                before: after.clone(),
                after: before.clone(),
            },
            Self::Lock {
                asset_id,
                before,
                after,
            } => Self::Lock {
                asset_id: *asset_id,
                before: *after,
                after: *before,
            },
            Self::TagAttach { asset_id, tag_id } => Self::TagDetach {
                asset_id: *asset_id,
                tag_id: *tag_id,
            },
            Self::TagDetach { asset_id, tag_id } => Self::TagAttach {
                asset_id: *asset_id,
                tag_id: *tag_id,
            },
        }
    }
}

/// 一次「用户动作」产生的全部改动（批量打星 = 很多个 [`Op`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeSet {
    /// 给 UI 显示的名字（撤销菜单里写「撤销：标 3 星」）。
    pub label: String,
    pub ops: Vec<Op>,
}

impl ChangeSet {
    #[must_use]
    pub fn new(label: impl Into<String>, ops: Vec<Op>) -> Self {
        Self {
            label: label.into(),
            ops,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// 反转后的补丁（撤销就是把它应用一遍）。
    #[must_use]
    pub fn invert(&self) -> Self {
        Self {
            label: self.label.clone(),
            ops: self.ops.iter().map(Op::invert).collect(),
        }
    }
}

/// 一次应用的结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Applied {
    /// 真的改了几张（**按照片计，不是按操作计**）。
    pub changed: usize,
    /// 因为二级锁被跳过的照片（升序、去重）。
    pub skipped_locked: Vec<i64>,
}

impl Applied {
    #[must_use]
    pub fn touched_anything(&self) -> bool {
        self.changed > 0
    }
}

/// 应用一组改动。
///
/// * 被二级锁挡住的操作**跳过而不是报错**（批量里夹着锁住的照片是常态）；
/// * 标签的重复挂/摘是**幂等**的：`attach_tag` 返回 false 时不算改动。
pub fn apply(conn: &Connection, change: &ChangeSet) -> Result<Applied> {
    let now = now_millis();
    let mut locked: BTreeSet<i64> = BTreeSet::new();
    {
        // 先把「哪些照片是二级锁」一次问清楚，避免每个 op 都查一遍
        let mut stmt = conn.prepare("SELECT id FROM assets WHERE lock_level >= ?1")?;
        let mut rows = stmt.query([i64::from(LOCK_NO_EDIT)])?;
        while let Some(row) = rows.next()? {
            locked.insert(row.get::<_, i64>(0)?);
        }
    }

    let mut changed: BTreeSet<i64> = BTreeSet::new();
    let mut skipped: BTreeSet<i64> = BTreeSet::new();

    for op in &change.ops {
        let asset_id = op.asset_id();
        if op.respects_edit_lock() && locked.contains(&asset_id) {
            skipped.insert(asset_id);
            continue;
        }
        match op {
            Op::Rating { after, .. } => {
                conn.execute(
                    "UPDATE assets SET rating = ?1, updated_at = ?2 WHERE id = ?3",
                    rusqlite::params![i64::from(*after), now, asset_id],
                )?;
                changed.insert(asset_id);
            }
            Op::Color { after, .. } => {
                conn.execute(
                    "UPDATE assets SET color_label = ?1, updated_at = ?2 WHERE id = ?3",
                    rusqlite::params![after, now, asset_id],
                )?;
                changed.insert(asset_id);
            }
            Op::Like { after, .. } => {
                conn.execute(
                    "UPDATE assets SET like_state = ?1, updated_at = ?2 WHERE id = ?3",
                    rusqlite::params![after, now, asset_id],
                )?;
                changed.insert(asset_id);
            }
            Op::Lock { after, .. } => {
                conn.execute(
                    "UPDATE assets SET lock_level = ?1, updated_at = ?2 WHERE id = ?3",
                    rusqlite::params![i64::from(*after), now, asset_id],
                )?;
                changed.insert(asset_id);
            }
            Op::TagAttach { tag_id, .. } => {
                let inserted = conn.execute(
                    "INSERT OR IGNORE INTO asset_tags (asset_id, tag_id, tagged_at)
                     VALUES (?1, ?2, ?3)",
                    rusqlite::params![asset_id, tag_id, now],
                )?;
                if inserted > 0 {
                    changed.insert(asset_id);
                }
            }
            Op::TagDetach { tag_id, .. } => {
                let removed = conn.execute(
                    "DELETE FROM asset_tags WHERE asset_id = ?1 AND tag_id = ?2",
                    rusqlite::params![asset_id, tag_id],
                )?;
                if removed > 0 {
                    changed.insert(asset_id);
                }
            }
        }
    }

    Ok(Applied {
        changed: changed.len(),
        skipped_locked: skipped.into_iter().collect(),
    })
}

/// 一张照片当前的标记值（读旧值用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marking {
    pub rating: u8,
    pub color_label: Option<String>,
    pub like_state: Option<String>,
    pub lock_level: u8,
}

/// 批量读标记（**保持入参顺序**，查不到的 id 直接跳过）。
pub fn read_markings(conn: &Connection, ids: &[i64]) -> Result<Vec<(i64, Marking)>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; ids.len()].join(", ");
    let sql = format!(
        "SELECT id, rating, color_label, like_state, lock_level FROM assets WHERE id IN ({placeholders})"
    );
    let params: Vec<rusqlite::types::Value> = ids
        .iter()
        .map(|id| rusqlite::types::Value::Integer(*id))
        .collect();
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(rusqlite::params_from_iter(params.iter()))?;
    let mut found: Vec<(i64, Marking)> = Vec::new();
    while let Some(row) = rows.next()? {
        found.push((
            row.get(0)?,
            Marking {
                rating: u8::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                color_label: row.get(2)?,
                like_state: row.get(3)?,
                lock_level: u8::try_from(row.get::<_, i64>(4)?).unwrap_or(0),
            },
        ));
    }
    // 按入参顺序排（调用方按顺序显示「这些照片的当前状态」）
    let mut ordered = Vec::with_capacity(found.len());
    for id in ids {
        if let Some(pos) = found.iter().position(|(fid, _)| fid == id) {
            ordered.push(found.remove(pos));
        }
    }
    Ok(ordered)
}

/// 校验评分（0–5）。
pub fn check_rating(rating: i64) -> Result<u8> {
    u8::try_from(rating)
        .ok()
        .filter(|r| *r <= 5)
        .ok_or_else(|| Error::Unsupported(format!("评分只能是 0–5，收到 {rating}")))
}

/// 校验色标（`None` = 无色；否则必须是 [`COLORS`] 之一）。
pub fn check_color(color: Option<&str>) -> Result<Option<String>> {
    match color {
        None | Some("") => Ok(None),
        Some(value) if COLORS.contains(&value) => Ok(Some(value.to_string())),
        Some(other) => Err(Error::Unsupported(format!(
            "不认识的颜色标记：{other}（只能是 {}）",
            COLORS.join(" / ")
        ))),
    }
}

/// 校验「喜欢」（`None` = 没表态）。
pub fn check_like(like: Option<&str>) -> Result<Option<String>> {
    match like {
        None | Some("") => Ok(None),
        Some(value) if LIKES.contains(&value) => Ok(Some(value.to_string())),
        Some(other) => Err(Error::Unsupported(format!(
            "不认识的喜欢状态：{other}（只能是 {}）",
            LIKES.join(" / ")
        ))),
    }
}

/// 校验锁级别（0–2）。
pub fn check_lock(level: i64) -> Result<u8> {
    u8::try_from(level)
        .ok()
        .filter(|l| *l <= LOCK_NO_EDIT)
        .ok_or_else(|| Error::Unsupported(format!("锁级别只能是 0–2，收到 {level}")))
}

// ─────────────────────────── 便捷入口（读旧值 → 生成补丁）───────────────────────────

/// 批量设评分，返回可以进撤销栈的补丁。
pub fn set_rating(conn: &Connection, ids: &[i64], rating: u8, label: &str) -> Result<ChangeSet> {
    let mut ops = Vec::new();
    for (id, marking) in read_markings(conn, ids)? {
        if marking.rating != rating {
            ops.push(Op::Rating {
                asset_id: id,
                before: marking.rating,
                after: rating,
            });
        }
    }
    Ok(ChangeSet::new(label, ops))
}

/// 批量设色标。
pub fn set_color(
    conn: &Connection,
    ids: &[i64],
    color: Option<String>,
    label: &str,
) -> Result<ChangeSet> {
    let mut ops = Vec::new();
    for (id, marking) in read_markings(conn, ids)? {
        if marking.color_label != color {
            ops.push(Op::Color {
                asset_id: id,
                before: marking.color_label,
                after: color.clone(),
            });
        }
    }
    Ok(ChangeSet::new(label, ops))
}

/// 批量设「喜欢」。
pub fn set_like(
    conn: &Connection,
    ids: &[i64],
    like: Option<String>,
    label: &str,
) -> Result<ChangeSet> {
    let mut ops = Vec::new();
    for (id, marking) in read_markings(conn, ids)? {
        if marking.like_state != like {
            ops.push(Op::Like {
                asset_id: id,
                before: marking.like_state,
                after: like.clone(),
            });
        }
    }
    Ok(ChangeSet::new(label, ops))
}

/// 批量设锁（**不受锁限制** —— 否则解不开）。
pub fn set_lock(conn: &Connection, ids: &[i64], level: u8, label: &str) -> Result<ChangeSet> {
    let mut ops = Vec::new();
    for (id, marking) in read_markings(conn, ids)? {
        if marking.lock_level != level {
            ops.push(Op::Lock {
                asset_id: id,
                before: marking.lock_level,
                after: level,
            });
        }
    }
    Ok(ChangeSet::new(label, ops))
}

/// 批量挂标签（批量只能加，`BROWSE.md` §3.3）。
pub fn attach_tags(
    conn: &Connection,
    ids: &[i64],
    tag_ids: &[i64],
    label: &str,
) -> Result<ChangeSet> {
    let mut ops = Vec::new();
    for id in ids {
        for tag_id in tag_ids {
            // 已经挂上的不再产生 op（幂等）
            let exists: i64 = conn.query_row(
                "SELECT count(*) FROM asset_tags WHERE asset_id = ?1 AND tag_id = ?2",
                rusqlite::params![id, tag_id],
                |r| r.get(0),
            )?;
            if exists == 0 {
                ops.push(Op::TagAttach {
                    asset_id: *id,
                    tag_id: *tag_id,
                });
            }
        }
    }
    Ok(ChangeSet::new(label, ops))
}

/// 批量摘标签（单张编辑标签弹窗里用）。
pub fn detach_tags(
    conn: &Connection,
    ids: &[i64],
    tag_ids: &[i64],
    label: &str,
) -> Result<ChangeSet> {
    let mut ops = Vec::new();
    for id in ids {
        for tag_id in tag_ids {
            let exists: i64 = conn.query_row(
                "SELECT count(*) FROM asset_tags WHERE asset_id = ?1 AND tag_id = ?2",
                rusqlite::params![id, tag_id],
                |r| r.get(0),
            )?;
            if exists > 0 {
                ops.push(Op::TagDetach {
                    asset_id: *id,
                    tag_id: *tag_id,
                });
            }
        }
    }
    Ok(ChangeSet::new(label, ops))
}

// ─────────────────────────── 撤销栈 ───────────────────────────

/// 撤销 / 重做栈（内存态，每库一份）。
#[derive(Debug)]
pub struct UndoStack {
    done: Vec<ChangeSet>,
    undone: Vec<ChangeSet>,
    capacity: usize,
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new(DEFAULT_UNDO_DEPTH)
    }
}

impl UndoStack {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            done: Vec::new(),
            undone: Vec::new(),
            capacity: capacity.max(1),
        }
    }

    /// 记一步（**会清空重做栈** —— 新动作之后「重做」就没有意义了）。
    pub fn push(&mut self, change: ChangeSet) {
        if change.is_empty() {
            return; // 空动作不进栈（否则用户会按到一个「什么都没干」的撤销）
        }
        self.undone.clear();
        self.done.push(change);
        while self.done.len() > self.capacity {
            self.done.remove(0);
        }
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    /// 下一次撤销会撤掉什么（UI 显示「撤销：标 3 星」）。
    #[must_use]
    pub fn undo_label(&self) -> Option<&str> {
        self.done.last().map(|c| c.label.as_str())
    }

    #[must_use]
    pub fn redo_label(&self) -> Option<&str> {
        self.undone.last().map(|c| c.label.as_str())
    }

    /*
     * 分成「取出 → 执行 → 归位」三步，是为了让**外壳侧**能在数据库写事务里执行：
     * 写事务要求闭包 `Send + 'static`，闭包里不能借用栈；所以先把要执行的补丁
     * 取出来（拥有一份），执行完再把记录归位。
     *
     * 中途失败（数据库写不进去）要把记录**放回原位** —— 否则用户会看到
     * 「撤销栈空了一步、照片却没变」这种对不上的状态。
     */

    /// 取出「下次撤销要执行的反向补丁」以及原记录（还没执行）。
    pub fn take_undo(&mut self) -> Option<(ChangeSet, ChangeSet)> {
        let change = self.done.pop()?;
        let patch = change.invert();
        Some((patch, change))
    }

    /// 撤销执行成功：把原记录移进重做栈。
    pub fn commit_undo(&mut self, change: ChangeSet) {
        self.undone.push(change);
    }

    /// 撤销执行失败：把记录放回撤销栈。
    pub fn give_back_undo(&mut self, change: ChangeSet) {
        self.done.push(change);
    }

    /// 取出「下次重做要执行的补丁」以及原记录。
    pub fn take_redo(&mut self) -> Option<(ChangeSet, ChangeSet)> {
        let change = self.undone.pop()?;
        let patch = change.clone();
        Some((patch, change))
    }

    /// 重做成功：把记录移回撤销栈。
    pub fn commit_redo(&mut self, change: ChangeSet) {
        self.done.push(change);
    }

    /// 重做失败：放回重做栈。
    pub fn give_back_redo(&mut self, change: ChangeSet) {
        self.undone.push(change);
    }

    /// 撤销一步（**自带连接**的便捷入口：单测与批处理用）。
    ///
    /// 外壳侧走 `take_undo` / `commit_undo` —— 那里要在写事务里执行。
    pub fn undo(&mut self, conn: &Connection) -> Result<Option<String>> {
        let Some((patch, change)) = self.take_undo() else {
            return Ok(None);
        };
        match apply(conn, &patch) {
            Ok(_) => {
                let label = change.label.clone();
                self.commit_undo(change);
                Ok(Some(label))
            }
            Err(e) => {
                self.give_back_undo(change);
                Err(e)
            }
        }
    }

    /// 重做一步（同上）。
    pub fn redo(&mut self, conn: &Connection) -> Result<Option<String>> {
        let Some((patch, change)) = self.take_redo() else {
            return Ok(None);
        };
        match apply(conn, &patch) {
            Ok(_) => {
                let label = change.label.clone();
                self.commit_redo(change);
                Ok(Some(label))
            }
            Err(e) => {
                self.give_back_redo(change);
                Err(e)
            }
        }
    }

    pub fn clear(&mut self) {
        self.done.clear();
        self.undone.clear();
    }

    #[must_use]
    pub fn depth(&self) -> usize {
        self.done.len()
    }
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

    fn asset(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO assets (imported_at, updated_at) VALUES (?1, ?1)",
            [T0],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn value<T: rusqlite::types::FromSql>(conn: &Connection, id: i64, column: &str) -> T {
        conn.query_row(
            &format!("SELECT {column} FROM assets WHERE id = ?1"),
            [id],
            |r| r.get(0),
        )
        .unwrap()
    }

    // ---------- 基本写入 ----------

    #[test]
    fn set_rating_writes_and_reports_the_old_value() {
        let conn = catalog();
        let id = asset(&conn);
        conn.execute("UPDATE assets SET rating = 2 WHERE id = ?1", [id])
            .unwrap();

        let change = set_rating(&conn, &[id], 4, "标 4 星").unwrap();
        assert_eq!(change.ops.len(), 1);
        assert_eq!(
            change.ops[0],
            Op::Rating {
                asset_id: id,
                before: 2,
                after: 4
            }
        );
        let applied = apply(&conn, &change).unwrap();
        assert_eq!(applied.changed, 1);
        assert_eq!(value::<i64>(&conn, id, "rating"), 4);
    }

    #[test]
    fn setting_the_same_value_produces_no_ops() {
        let conn = catalog();
        let id = asset(&conn);
        let change = set_rating(&conn, &[id], 0, "标 0 星").unwrap();
        assert!(
            change.is_empty(),
            "值没变就不该产生补丁（否则撤销栈里全是空动作）"
        );
        assert!(!apply(&conn, &change).unwrap().touched_anything());
    }

    #[test]
    fn color_can_be_cleared_back_to_none() {
        let conn = catalog();
        let id = asset(&conn);
        let red = set_color(&conn, &[id], Some("red".to_string()), "标红").unwrap();
        apply(&conn, &red).unwrap();
        assert_eq!(
            value::<Option<String>>(&conn, id, "color_label").as_deref(),
            Some("red")
        );

        let none = set_color(&conn, &[id], None, "去掉颜色").unwrap();
        assert_eq!(none.ops.len(), 1);
        apply(&conn, &none).unwrap();
        assert_eq!(value::<Option<String>>(&conn, id, "color_label"), None);
    }

    #[test]
    fn like_has_three_states() {
        let conn = catalog();
        let id = asset(&conn);
        apply(
            &conn,
            &set_like(&conn, &[id], Some("like".into()), "喜欢").unwrap(),
        )
        .unwrap();
        assert_eq!(
            value::<Option<String>>(&conn, id, "like_state").as_deref(),
            Some("like")
        );
        apply(
            &conn,
            &set_like(&conn, &[id], Some("dislike".into()), "不喜欢").unwrap(),
        )
        .unwrap();
        assert_eq!(
            value::<Option<String>>(&conn, id, "like_state").as_deref(),
            Some("dislike")
        );
        apply(&conn, &set_like(&conn, &[id], None, "取消").unwrap()).unwrap();
        assert_eq!(value::<Option<String>>(&conn, id, "like_state"), None);
    }

    #[test]
    fn batch_sets_every_photo_and_keeps_per_photo_old_values() {
        let conn = catalog();
        let a = asset(&conn);
        let b = asset(&conn);
        conn.execute("UPDATE assets SET rating = 1 WHERE id = ?1", [a])
            .unwrap();
        conn.execute("UPDATE assets SET rating = 5 WHERE id = ?1", [b])
            .unwrap();

        let change = set_rating(&conn, &[a, b], 3, "标 3 星").unwrap();
        assert_eq!(change.ops.len(), 2);
        apply(&conn, &change).unwrap();
        assert_eq!(value::<i64>(&conn, a, "rating"), 3);
        assert_eq!(value::<i64>(&conn, b, "rating"), 3);

        // 撤销要分别还原成 1 和 5（不是都还原成某个统一值）
        apply(&conn, &change.invert()).unwrap();
        assert_eq!(value::<i64>(&conn, a, "rating"), 1);
        assert_eq!(value::<i64>(&conn, b, "rating"), 5);
    }

    // ---------- 锁 ----------

    #[test]
    fn a_no_edit_lock_blocks_marking() {
        let conn = catalog();
        let id = asset(&conn);
        conn.execute("UPDATE assets SET lock_level = 2 WHERE id = ?1", [id])
            .unwrap();

        let change = set_rating(&conn, &[id], 5, "标 5 星").unwrap();
        assert_eq!(change.ops.len(), 1, "补丁照常生成");
        let applied = apply(&conn, &change).unwrap();
        assert_eq!(applied.changed, 0);
        assert_eq!(applied.skipped_locked, vec![id]);
        assert_eq!(value::<i64>(&conn, id, "rating"), 0, "锁住的没被改");
    }

    #[test]
    fn a_no_delete_lock_does_not_block_marking() {
        let conn = catalog();
        let id = asset(&conn);
        conn.execute("UPDATE assets SET lock_level = 1 WHERE id = ?1", [id])
            .unwrap();
        let change = set_rating(&conn, &[id], 4, "标 4 星").unwrap();
        assert_eq!(apply(&conn, &change).unwrap().changed, 1, "一级锁只挡删除");
    }

    #[test]
    fn the_lock_itself_can_always_be_changed() {
        let conn = catalog();
        let id = asset(&conn);
        conn.execute("UPDATE assets SET lock_level = 2 WHERE id = ?1", [id])
            .unwrap();
        let change = set_lock(&conn, &[id], 0, "解锁").unwrap();
        assert_eq!(apply(&conn, &change).unwrap().changed, 1);
        assert_eq!(value::<i64>(&conn, id, "lock_level"), 0);
    }

    #[test]
    fn a_partly_locked_batch_changes_what_it_can() {
        let conn = catalog();
        let free = asset(&conn);
        let locked = asset(&conn);
        conn.execute("UPDATE assets SET lock_level = 2 WHERE id = ?1", [locked])
            .unwrap();

        let change = set_rating(&conn, &[free, locked], 5, "标 5 星").unwrap();
        let applied = apply(&conn, &change).unwrap();
        assert_eq!(applied.changed, 1);
        assert_eq!(applied.skipped_locked, vec![locked]);
        assert_eq!(value::<i64>(&conn, free, "rating"), 5);
    }

    // ---------- 标签 ----------

    #[test]
    fn attaching_tags_is_idempotent() {
        let conn = catalog();
        let id = asset(&conn);
        let change = attach_tags(&conn, &[id], &[7, 8], "加标签").unwrap();
        assert_eq!(change.ops.len(), 2);
        apply(&conn, &change).unwrap();
        assert_eq!(
            super::super::tags::tags_of_asset(&conn, id).unwrap(),
            vec![7, 8]
        );

        // 再挂一次：不产生 op
        let again = attach_tags(&conn, &[id], &[7], "加标签").unwrap();
        assert!(again.is_empty());
    }

    #[test]
    fn detaching_only_reports_tags_that_were_there() {
        let conn = catalog();
        let id = asset(&conn);
        apply(&conn, &attach_tags(&conn, &[id], &[7], "加").unwrap()).unwrap();
        let detach = detach_tags(&conn, &[id], &[7, 9], "摘").unwrap();
        assert_eq!(detach.ops.len(), 1, "没挂过的标签不该产生 op");
        apply(&conn, &detach).unwrap();
        assert!(
            super::super::tags::tags_of_asset(&conn, id)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn tag_ops_invert_to_the_opposite_operation() {
        let attach = Op::TagAttach {
            asset_id: 1,
            tag_id: 2,
        };
        assert_eq!(
            attach.invert(),
            Op::TagDetach {
                asset_id: 1,
                tag_id: 2
            }
        );
    }

    // ---------- 校验 ----------

    #[test]
    fn rating_bounds_are_checked() {
        assert_eq!(check_rating(0).unwrap(), 0);
        assert_eq!(check_rating(5).unwrap(), 5);
        assert!(check_rating(6).is_err());
        assert!(check_rating(-1).is_err());
    }

    #[test]
    fn color_and_like_reject_unknown_values() {
        assert_eq!(check_color(Some("red")).unwrap().as_deref(), Some("red"));
        assert_eq!(check_color(None).unwrap(), None);
        assert_eq!(check_color(Some("")).unwrap(), None, "空串当没设");
        assert!(check_color(Some("reddish")).is_err());
        assert_eq!(check_like(Some("like")).unwrap().as_deref(), Some("like"));
        assert!(check_like(Some("love")).is_err());
    }

    #[test]
    fn lock_bounds_are_checked() {
        for level in 0..=2 {
            assert_eq!(check_lock(level).unwrap(), u8::try_from(level).unwrap());
        }
        assert!(check_lock(3).is_err());
        assert!(check_lock(-1).is_err());
    }

    #[test]
    fn validating_an_empty_id_list_is_fine() {
        let conn = catalog();
        assert!(read_markings(&conn, &[]).unwrap().is_empty());
        assert!(set_rating(&conn, &[], 3, "空").unwrap().is_empty());
    }

    // ---------- 撤销栈 ----------

    #[test]
    fn undo_restores_and_redo_reapplies() {
        let conn = catalog();
        let id = asset(&conn);
        let mut stack = UndoStack::default();

        let change = set_rating(&conn, &[id], 3, "标 3 星").unwrap();
        apply(&conn, &change).unwrap();
        stack.push(change);
        assert_eq!(value::<i64>(&conn, id, "rating"), 3);

        assert_eq!(stack.undo(&conn).unwrap().as_deref(), Some("标 3 星"));
        assert_eq!(value::<i64>(&conn, id, "rating"), 0);
        assert!(stack.can_redo());

        assert_eq!(stack.redo(&conn).unwrap().as_deref(), Some("标 3 星"));
        assert_eq!(value::<i64>(&conn, id, "rating"), 3);
        assert!(!stack.can_redo());
    }

    #[test]
    fn undo_on_an_empty_stack_is_none_not_an_error() {
        let conn = catalog();
        let mut stack = UndoStack::default();
        assert_eq!(stack.undo(&conn).unwrap(), None);
        assert_eq!(stack.redo(&conn).unwrap(), None);
    }

    #[test]
    fn pushing_after_undo_drops_the_redo_branch() {
        let conn = catalog();
        let id = asset(&conn);
        let mut stack = UndoStack::default();
        let first = set_rating(&conn, &[id], 1, "一星").unwrap();
        apply(&conn, &first).unwrap();
        stack.push(first);
        stack.undo(&conn).unwrap();
        assert!(stack.can_redo());

        let second = set_rating(&conn, &[id], 5, "五星").unwrap();
        apply(&conn, &second).unwrap();
        stack.push(second);
        assert!(!stack.can_redo(), "新动作之后「重做」不该还能用");
    }

    #[test]
    fn empty_changes_do_not_enter_the_stack() {
        let mut stack = UndoStack::default();
        stack.push(ChangeSet::new("什么都没干", Vec::new()));
        assert!(!stack.can_undo());
    }

    #[test]
    fn the_stack_keeps_only_the_last_n_steps() {
        let conn = catalog();
        let id = asset(&conn);
        let mut stack = UndoStack::new(3);
        for rating in 1..=5u8 {
            let change = set_rating(&conn, &[id], rating, &format!("标 {rating} 星")).unwrap();
            apply(&conn, &change).unwrap();
            stack.push(change);
        }
        assert_eq!(stack.depth(), 3, "只留最后 3 步");
        assert_eq!(stack.undo_label(), Some("标 5 星"));
    }

    #[test]
    fn labels_describe_the_next_undo_and_redo() {
        let conn = catalog();
        let id = asset(&conn);
        let mut stack = UndoStack::default();
        assert_eq!(stack.undo_label(), None);
        let change = set_color(&conn, &[id], Some("blue".into()), "标蓝").unwrap();
        apply(&conn, &change).unwrap();
        stack.push(change);
        assert_eq!(stack.undo_label(), Some("标蓝"));
        assert_eq!(stack.redo_label(), None);
        stack.undo(&conn).unwrap();
        assert_eq!(stack.undo_label(), None);
        assert_eq!(stack.redo_label(), Some("标蓝"));
    }

    #[test]
    fn undoing_a_tag_change_removes_the_tag_again() {
        let conn = catalog();
        let id = asset(&conn);
        let mut stack = UndoStack::default();
        let change = attach_tags(&conn, &[id], &[5], "加标签").unwrap();
        apply(&conn, &change).unwrap();
        stack.push(change);
        assert_eq!(
            super::super::tags::tags_of_asset(&conn, id).unwrap(),
            vec![5]
        );
        stack.undo(&conn).unwrap();
        assert!(
            super::super::tags::tags_of_asset(&conn, id)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn clear_empties_both_stacks() {
        let conn = catalog();
        let id = asset(&conn);
        let mut stack = UndoStack::default();
        let change = set_rating(&conn, &[id], 2, "二星").unwrap();
        apply(&conn, &change).unwrap();
        stack.push(change);
        stack.undo(&conn).unwrap();
        stack.clear();
        assert!(!stack.can_undo() && !stack.can_redo());
    }

    #[test]
    fn ops_report_which_asset_they_touch() {
        let op = Op::Rating {
            asset_id: 42,
            before: 0,
            after: 1,
        };
        assert_eq!(op.asset_id(), 42);
        assert!(op.respects_edit_lock());
        assert!(
            !Op::Lock {
                asset_id: 42,
                before: 0,
                after: 2
            }
            .respects_edit_lock(),
            "锁自己要能改"
        );
    }
}
