//! 编辑预设的目录与条目（`specs/editor-presets.md` §2）。
//!
//! 设备级资产：目录只有一级，`default` 目录由读取侧惰性保证存在且不可删；
//! `payload` 是大类快照 JSON，**这里只校验可解析且是对象**，语义归前端
//! （`src/lib/presets.ts`）。风格与命名照 [`crate::store::luts`]。

use crate::error::{Error, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

/// `默认` 目录的固定 id（名称走语言包，DB 里的 `name` 只是占位）。
pub const DEFAULT_DIRECTORY_ID: &str = "default";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PresetDirectory {
    pub id: String,
    pub name: String,
    pub sort_order: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PresetRecord {
    pub id: String,
    pub directory_id: String,
    pub name: String,
    /// 大类快照（`{version:1, tone:{...}, ...}`）；读到坏 JSON 的行**整行跳过**，
    /// 不让面板起不来（specs §4 `preset_library` 的语义）。
    pub payload: serde_json::Value,
    pub created_at: i64,
    pub updated_at: i64,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn valid_name(name: &str, max_chars: usize) -> bool {
    !name.is_empty() && name.chars().count() <= max_chars && !name.chars().any(char::is_control)
}

/** 校验 payload：必须是合法 JSON **对象**（语义校验在前端清洗层）。 */
fn valid_payload(payload: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(payload)
        .map(|value| value.is_object())
        .unwrap_or(false)
}

pub fn directories(conn: &Connection) -> Result<Vec<PresetDirectory>> {
    let mut statement = conn.prepare(
        "SELECT id, name, sort_order, created_at FROM preset_dirs ORDER BY sort_order, created_at, id",
    )?;
    Ok(statement
        .query_map([], |row| {
            Ok(PresetDirectory {
                id: row.get(0)?,
                name: row.get(1)?,
                sort_order: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 保证 `default` 始终存在（幂等；名称只是 DB 占位，界面走语言包）。
pub fn ensure_default_directory(conn: &Connection, now_ms: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO preset_dirs (id, name, sort_order, created_at) VALUES (?1, ?2, 0, ?3) \
         ON CONFLICT(id) DO NOTHING",
        params![DEFAULT_DIRECTORY_ID, "Default", now_ms],
    )?;
    Ok(())
}

pub fn directory_exists(conn: &Connection, id: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM preset_dirs WHERE id = ?1",
            [id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn directory_name_taken(conn: &Connection, name: &str) -> Result<bool> {
    // SQLite NOCASE 只覆盖 ASCII；与前端 toLowerCase 的 Unicode 口径保持一致。
    let wanted = name.to_lowercase();
    let mut statement = conn.prepare("SELECT name FROM preset_dirs")?;
    for candidate in statement.query_map([], |row| row.get::<_, String>(0))? {
        if candidate?.to_lowercase() == wanted { return Ok(true); }
    }
    Ok(false)
}

pub fn create_directory(
    conn: &Connection,
    id: &str,
    name: &str,
    now_ms: i64,
) -> Result<PresetDirectory> {
    let name = name.trim();
    if !valid_id(id) || !valid_name(name, 40) {
        return Err(Error::Unsupported("预设目录 ID 或名称无效".into()));
    }
    if id == DEFAULT_DIRECTORY_ID || directory_name_taken(conn, name)? {
        return Err(Error::Unsupported("同名目录已存在".into()));
    }
    conn.execute(
        "INSERT INTO preset_dirs (id, name, sort_order, created_at) \
         VALUES (?1, ?2, (SELECT COALESCE(MAX(sort_order) + 1, 0) FROM preset_dirs), ?3)",
        params![id, name, now_ms],
    )?;
    Ok(PresetDirectory {
        id: id.to_string(),
        name: name.to_string(),
        sort_order: 0,
        created_at: now_ms,
    })
}

/// 删除目录：`default` 拒绝；**非空拒绝**（前端本就不显示按钮，这里是兜底）。
pub fn delete_directory(conn: &Connection, id: &str) -> Result<()> {
    if id == DEFAULT_DIRECTORY_ID {
        return Err(Error::Unsupported("默认目录不可删除".into()));
    }
    let presets: i64 = conn.query_row(
        "SELECT COUNT(*) FROM presets WHERE directory_id = ?1",
        [id],
        |row| row.get(0),
    )?;
    if presets > 0 {
        return Err(Error::Unsupported("目录内还有预设，清空后才能删除".into()));
    }
    let removed = conn.execute("DELETE FROM preset_dirs WHERE id = ?1", [id])?;
    if removed == 0 {
        return Err(Error::Unsupported("目录不存在".into()));
    }
    Ok(())
}

fn preset_name_taken(conn: &Connection, directory_id: &str, name: &str) -> Result<bool> {
    let wanted = name.to_lowercase();
    let mut statement = conn.prepare("SELECT name FROM presets WHERE directory_id = ?1")?;
    for candidate in statement.query_map([directory_id], |row| row.get::<_, String>(0))? {
        if candidate?.to_lowercase() == wanted { return Ok(true); }
    }
    Ok(false)
}

const PRESET_COLUMNS: &str = "id, directory_id, name, payload, created_at, updated_at";

fn record(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<PresetRecord>> {
    let payload_text: String = row.get(3)?;
    let payload = match serde_json::from_str::<serde_json::Value>(&payload_text) {
        Ok(value) if value.is_object() => value,
        // 坏行跳过（查询层用 flat_map 过滤 None）
        _ => return Ok(None),
    };
    Ok(Some(PresetRecord {
        id: row.get(0)?,
        directory_id: row.get(1)?,
        name: row.get(2)?,
        payload,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    }))
}

/// 全部预设（按创建时间升序；坏 payload 的行跳过）。
pub fn presets(conn: &Connection) -> Result<Vec<PresetRecord>> {
    let mut statement = conn.prepare(&format!(
        "SELECT {PRESET_COLUMNS} FROM presets ORDER BY created_at, id"
    ))?;
    Ok(statement
        .query_map([], record)?
        .collect::<rusqlite::Result<Vec<Option<_>>>>()?
        .into_iter()
        .flatten()
        .collect())
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<PresetRecord>> {
    Ok(conn
        .query_row(
            &format!("SELECT {PRESET_COLUMNS} FROM presets WHERE id = ?1"),
            [id],
            record,
        )
        .optional()?
        .flatten())
}

pub fn create(
    conn: &Connection,
    id: &str,
    directory_id: &str,
    name: &str,
    payload: &str,
    now_ms: i64,
) -> Result<PresetRecord> {
    let name = name.trim();
    if !valid_id(id) || !valid_name(name, 80) || !valid_payload(payload) {
        return Err(Error::Unsupported("预设 ID、名称或内容无效".into()));
    }
    if !directory_exists(conn, directory_id)? {
        return Err(Error::Unsupported("目标目录不存在".into()));
    }
    if preset_name_taken(conn, directory_id, name)? {
        return Err(Error::Unsupported("同名预设已存在".into()));
    }
    conn.execute(
        "INSERT INTO presets (id, directory_id, name, payload, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![id, directory_id, name, payload, now_ms],
    )?;
    Ok(PresetRecord {
        id: id.to_string(),
        directory_id: directory_id.to_string(),
        name: name.to_string(),
        payload: serde_json::from_str(payload)
            .map_err(|error| Error::Unsupported(error.to_string()))?,
        created_at: now_ms,
        updated_at: now_ms,
    })
}

pub fn delete(conn: &Connection, id: &str) -> Result<()> {
    let removed = conn.execute("DELETE FROM presets WHERE id = ?1", [id])?;
    if removed == 0 {
        return Err(Error::Unsupported("预设不存在".into()));
    }
    Ok(())
}

/// 移到目标目录；**目标已有同名时自动加 ` 2` / ` 3` 后缀**（不打断拖拽流程）。
pub fn move_to(
    conn: &Connection,
    id: &str,
    directory_id: &str,
    now_ms: i64,
) -> Result<PresetRecord> {
    if !directory_exists(conn, directory_id)? {
        return Err(Error::Unsupported("目标目录不存在".into()));
    }
    let Some(current) = get(conn, id)? else {
        return Err(Error::Unsupported("预设不存在".into()));
    };
    if current.directory_id == directory_id {
        return Ok(current);
    }
    let mut name = current.name.clone();
    if preset_name_taken(conn, directory_id, &name)? {
        let base = current.name.clone();
        let mut suffix = 2;
        loop {
            name = format!("{base} {suffix}");
            if !preset_name_taken(conn, directory_id, &name)? {
                break;
            }
            suffix += 1;
            if suffix > 1000 {
                return Err(Error::Unsupported("无法为移动的预设分配可用名称".into()));
            }
        }
    }
    conn.execute(
        "UPDATE presets SET directory_id = ?2, name = ?3, updated_at = ?4 WHERE id = ?1",
        params![id, directory_id, name, now_ms],
    )?;
    Ok(PresetRecord {
        directory_id: directory_id.to_string(),
        name,
        updated_at: now_ms,
        ..current
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::migration::{self, DbKind};

    fn prepare(conn: &mut Connection) {
        migration::apply(conn, DbKind::App, migration::Backups::none(), 1).unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        ensure_default_directory(conn, 1).unwrap();
    }

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        prepare(&mut conn);
        conn
    }

    const PAYLOAD: &str = r#"{"version":1,"tone":{"exposure":0.35}}"#;

    #[test]
    fn directories_start_with_default_only() {
        let conn = db();
        let dirs = directories(&conn).unwrap();
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0].id, DEFAULT_DIRECTORY_ID);
    }

    #[test]
    fn ensure_default_is_idempotent() {
        let conn = db();
        ensure_default_directory(&conn, 5).unwrap();
        assert_eq!(directories(&conn).unwrap().len(), 1);
        conn.execute("DELETE FROM preset_dirs WHERE id = 'default'", []).unwrap();
        conn.execute("INSERT INTO preset_dirs (id, name, sort_order, created_at) VALUES ('d1', '人像', 1, 6)", []).unwrap();
        ensure_default_directory(&conn, 7).unwrap();
        assert!(directory_exists(&conn, DEFAULT_DIRECTORY_ID).unwrap());
    }

    #[test]
    fn create_directory_rejects_invalid_and_duplicate() {
        let conn = db();
        create_directory(&conn, "d1", "人像", 10).unwrap();
        // 同名（大小写不敏感）
        assert!(create_directory(&conn, "d2", "人像", 11).is_err());
        assert!(create_directory(&conn, "d3", "人像 ".trim(), 12).is_err());
        // 抢占 default id
        assert!(create_directory(&conn, DEFAULT_DIRECTORY_ID, "别的", 13).is_err());
        // 非法 id / 名字
        assert!(create_directory(&conn, "bad id", "x", 14).is_err());
        assert!(create_directory(&conn, "d4", "", 15).is_err());
        assert!(create_directory(&conn, "d5", &"长".repeat(41), 16).is_err());
        // 合法的 Unicode 名称
        create_directory(&conn, "d6", "旅行・2026", 17).unwrap();
        create_directory(&conn, "d7", "Äther", 18).unwrap();
        assert!(create_directory(&conn, "d8", "äther", 19).is_err());
    }

    #[test]
    fn delete_directory_guards() {
        let conn = db();
        // default 拒绝
        assert!(delete_directory(&conn, DEFAULT_DIRECTORY_ID).is_err());
        // 非空拒绝
        create_directory(&conn, "d1", "人像", 10).unwrap();
        create(&conn, "p1", "d1", "柔和", PAYLOAD, 20).unwrap();
        assert!(delete_directory(&conn, "d1").is_err());
        // 清空后可删
        delete(&conn, "p1").unwrap();
        delete_directory(&conn, "d1").unwrap();
        // 不存在的目录
        assert!(delete_directory(&conn, "nope").is_err());
    }

    #[test]
    fn create_preset_validates_payload_and_names() {
        let conn = db();
        create(&conn, "p1", DEFAULT_DIRECTORY_ID, "柔和", PAYLOAD, 20).unwrap();
        // 同目录同名（大小写不敏感）
        assert!(create(&conn, "p2", DEFAULT_DIRECTORY_ID, "柔和", PAYLOAD, 21).is_err());
        // 坏 payload：非法 JSON / 非对象
        assert!(create(&conn, "p3", DEFAULT_DIRECTORY_ID, "x", "{", 22).is_err());
        assert!(create(&conn, "p4", DEFAULT_DIRECTORY_ID, "x", "[1,2]", 23).is_err());
        // 目录不存在
        assert!(create(&conn, "p5", "nope", "x", PAYLOAD, 24).is_err());
        // 同名但不同目录 → 允许
        create_directory(&conn, "d1", "人像", 25).unwrap();
        create(&conn, "p6", "d1", "柔和", PAYLOAD, 26).unwrap();
        // 名称长度上限 80
        assert!(create(&conn, "p7", "d1", &"名".repeat(81), PAYLOAD, 27).is_err());
        create(&conn, "p8", "d1", "Äther", PAYLOAD, 28).unwrap();
        assert!(create(&conn, "p9", "d1", "äther", PAYLOAD, 29).is_err());
    }

    #[test]
    fn bad_payload_rows_are_skipped_on_read() {
        let conn = db();
        create(&conn, "p1", DEFAULT_DIRECTORY_ID, "好行", PAYLOAD, 20).unwrap();
        // 直接塞一行坏数据（模拟旧版本/手改库）
        conn.execute(
            "INSERT INTO presets (id, directory_id, name, payload, created_at, updated_at) \
             VALUES ('p2', ?1, '坏行', 'not json', 21, 21)",
            params![DEFAULT_DIRECTORY_ID],
        )
        .unwrap();
        let all = presets(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "p1");
        assert!(get(&conn, "p2").unwrap().is_none());
    }

    #[test]
    fn move_renames_on_collision_and_noops_on_same_dir() {
        let conn = db();
        create_directory(&conn, "d1", "人像", 10).unwrap();
        create_directory(&conn, "d2", "风景", 11).unwrap();
        create(&conn, "p1", "d1", "柔和", PAYLOAD, 20).unwrap();
        create(&conn, "p2", "d2", "柔和", PAYLOAD, 21).unwrap();
        // 同目录 no-op
        let same = move_to(&conn, "p1", "d1", 30).unwrap();
        assert_eq!(same.name, "柔和");
        // 冲突 → 自动后缀
        let moved = move_to(&conn, "p1", "d2", 31).unwrap();
        assert_eq!(moved.directory_id, "d2");
        assert_eq!(moved.name, "柔和 2");
        // 大小写不敏感冲突也撞得上
        create(&conn, "p3", "d1", "夜景", PAYLOAD, 32).unwrap();
        let moved2 = move_to(&conn, "p3", "d2", 33).unwrap();
        assert_eq!(moved2.name, "夜景");
        // 目标目录不存在
        assert!(move_to(&conn, "p3", "nope", 34).is_err());
        // 移动后 p1 仍在 d2、名字带后缀
        let got = get(&conn, "p1").unwrap().unwrap();
        assert_eq!((got.directory_id.as_str(), got.name.as_str()), ("d2", "柔和 2"));
    }

    #[test]
    fn delete_preset_requires_existence() {
        let conn = db();
        create(&conn, "p1", DEFAULT_DIRECTORY_ID, "x", PAYLOAD, 20).unwrap();
        delete(&conn, "p1").unwrap();
        assert!(delete(&conn, "p1").is_err());
    }
}
