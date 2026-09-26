//! 应用级 LUT 分类与条目。文件名只作显示；内部路径由 ID 决定。

use crate::error::{Error, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LutCategory {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LutRecord {
    pub id: String,
    pub category_id: String,
    pub name: String,
    pub original_filename: String,
    pub original_path: String,
    pub file_rel_path: String,
    pub cover_rel_path: String,
    pub format: String,
    pub hidden: bool,
    pub file_hash: Option<String>,
    pub created_at: i64,
}

pub fn categories(conn: &Connection) -> Result<Vec<LutCategory>> {
    let mut statement =
        conn.prepare("SELECT id, name FROM lut_categories ORDER BY sort_order, created_at, id")?;
    Ok(statement
        .query_map([], |row| {
            Ok(LutCategory {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn create_category(
    conn: &Connection,
    id: &str,
    name: &str,
    now_ms: i64,
) -> Result<LutCategory> {
    let name = name.trim();
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        || name.is_empty()
        || name.chars().count() > 40
        || name.chars().any(char::is_control)
    {
        return Err(Error::Unsupported("LUT 分类 ID 或名称无效".into()));
    }
    conn.execute("INSERT INTO lut_categories (id,name,sort_order,created_at) VALUES (?1,?2,(SELECT COALESCE(MAX(sort_order)+1,0) FROM lut_categories),?3)",
        params![id,name,now_ms])?;
    Ok(LutCategory {
        id: id.to_string(),
        name: name.to_string(),
    })
}

const COLUMNS: &str = "id,category_id,name,original_filename,original_path,file_rel_path,cover_rel_path,format,hidden,created_at,file_hash";

fn record(row: &rusqlite::Row<'_>) -> rusqlite::Result<LutRecord> {
    Ok(LutRecord {
        id: row.get(0)?,
        category_id: row.get(1)?,
        name: row.get(2)?,
        original_filename: row.get(3)?,
        original_path: row.get(4)?,
        file_rel_path: row.get(5)?,
        cover_rel_path: row.get(6)?,
        format: row.get(7)?,
        hidden: row.get::<_, i64>(8)? != 0,
        created_at: row.get(9)?,
        file_hash: row.get(10)?,
    })
}

pub fn entries(conn: &Connection, include_hidden: bool) -> Result<Vec<LutRecord>> {
    let mut statement = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM luts WHERE hidden=0 OR ?1=1 ORDER BY created_at,id"
    ))?;
    Ok(statement
        .query_map([i64::from(include_hidden)], record)?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<LutRecord>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM luts WHERE id=?1"),
            [id],
            record,
        )
        .optional()?)
}

pub fn find_by_hash(conn: &Connection, hash: &str) -> Result<Option<LutRecord>> {
    Ok(conn.query_row(&format!("SELECT {COLUMNS} FROM luts WHERE file_hash=?1 ORDER BY hidden,created_at,id LIMIT 1"),
        [hash], record).optional()?)
}

/// 仅补旧条目的 NULL，不覆盖其它并行补录已经确定的身份。
pub fn backfill_hash(conn: &Connection, id: &str, hash: &str) -> Result<bool> {
    Ok(conn.execute(
        "UPDATE luts SET file_hash=?2 WHERE id=?1 AND file_hash IS NULL",
        params![id, hash],
    )? > 0)
}

fn insert(conn: &Connection, entry: &LutRecord) -> Result<()> {
    conn.execute("INSERT INTO luts (id,category_id,name,original_filename,original_path,file_rel_path,cover_rel_path,format,hidden,created_at,file_hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![entry.id,entry.category_id,entry.name,entry.original_filename,entry.original_path,
            entry.file_rel_path,entry.cover_rel_path,entry.format,i64::from(entry.hidden),entry.created_at,entry.file_hash])?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admission {
    Imported,
    Duplicate(String),
    Restored(String),
}

/// 最后一道去重闸门和恢复在同一 IMMEDIATE 事务内；不依赖导入前的快筛。
pub fn admit(conn: &mut Connection, entry: &LutRecord) -> Result<Admission> {
    let hash = entry
        .file_hash
        .as_deref()
        .filter(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        .ok_or_else(|| Error::Unsupported("LUT 缺少有效 SHA-256".into()))?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let outcome = if let Some(existing) = find_by_hash(&tx, hash)? {
        if existing.hidden {
            tx.execute(
                "UPDATE luts SET hidden=0, category_id=?2 WHERE id=?1",
                params![existing.id, entry.category_id],
            )?;
            Admission::Restored(existing.id)
        } else {
            Admission::Duplicate(existing.id)
        }
    } else {
        insert(&tx, entry)?;
        Admission::Imported
    };
    tx.commit()?;
    Ok(outcome)
}

pub fn hide(conn: &Connection, id: &str) -> Result<bool> {
    Ok(conn.execute("UPDATE luts SET hidden=1 WHERE id=?1 AND hidden=0", [id])? > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::migration::{self, DbKind};

    fn prepare(conn: &mut Connection) {
        migration::apply(conn, DbKind::App, migration::Backups::none(), 1).unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        create_category(conn, "first", "默认分类", 1).unwrap();
        create_category(conn, "second", "风景", 2).unwrap();
    }

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        prepare(&mut conn);
        conn
    }

    fn entry(id: &str, hash: char) -> LutRecord {
        LutRecord {
            id: id.into(),
            category_id: "first".into(),
            name: "测试曲线.cube".into(),
            original_filename: "测试曲线.cube".into(),
            original_path: "C:\\资源\\测试曲线.cube".into(),
            file_rel_path: format!("{id}/source.cube"),
            cover_rel_path: format!("{id}/cover.webp"),
            format: "cube".into(),
            hidden: false,
            file_hash: Some(hash.to_string().repeat(64)),
            created_at: 1,
        }
    }

    #[test]
    fn same_content_is_global_duplicate_despite_name_path_and_category() {
        let mut conn = db();
        let original = entry("one", 'a');
        assert_eq!(admit(&mut conn, &original).unwrap(), Admission::Imported);
        let mut renamed = entry("two", 'a');
        renamed.category_id = "second".into();
        renamed.name = "Other.CUBE".into();
        renamed.original_path = "/other/Other.CUBE".into();
        assert_eq!(
            admit(&mut conn, &renamed).unwrap(),
            Admission::Duplicate("one".into())
        );
        assert_eq!(entries(&conn, true).unwrap(), vec![original]);
        // 同名而字节不同是两个 LUT。
        assert_eq!(
            admit(&mut conn, &entry("three", 'b')).unwrap(),
            Admission::Imported
        );
        assert_eq!(entries(&conn, false).unwrap().len(), 2);
    }

    #[test]
    fn hidden_reimport_restores_original_identity_and_selected_category() {
        let mut conn = db();
        let original = entry("one", 'a');
        admit(&mut conn, &original).unwrap();
        assert!(hide(&conn, "one").unwrap());
        assert!(!hide(&conn, "one").unwrap());
        assert!(!hide(&conn, "missing").unwrap());
        assert!(entries(&conn, false).unwrap().is_empty());
        assert_eq!(
            get(&conn, "one").unwrap().unwrap().file_rel_path,
            original.file_rel_path
        );
        let mut renamed = entry("candidate", 'a');
        renamed.category_id = "second".into();
        renamed.name = "新文件名.cube".into();
        assert_eq!(
            admit(&mut conn, &renamed).unwrap(),
            Admission::Restored("one".into())
        );
        let mut restored = original;
        restored.category_id = "second".into();
        assert_eq!(entries(&conn, false).unwrap(), vec![restored]);
        assert!(get(&conn, "candidate").unwrap().is_none());
    }

    #[test]
    fn legacy_duplicates_keep_ids_and_prefer_visible_then_oldest() {
        let mut conn = db();
        let first = entry("a", 'a');
        let mut later = entry("b", 'a');
        later.created_at = 2;
        insert(&conn, &first).unwrap();
        insert(&conn, &later).unwrap();
        hide(&conn, "a").unwrap();
        assert_eq!(
            find_by_hash(&conn, &"a".repeat(64)).unwrap().unwrap().id,
            "b"
        );
        assert_eq!(
            admit(&mut conn, &entry("candidate", 'a')).unwrap(),
            Admission::Duplicate("b".into())
        );
        hide(&conn, "b").unwrap();
        assert_eq!(
            admit(&mut conn, &entry("candidate", 'a')).unwrap(),
            Admission::Restored("a".into())
        );
        assert_eq!(entries(&conn, true).unwrap().len(), 2);
    }

    #[test]
    fn legacy_backfill_is_idempotent_and_hash_values_are_validated() {
        let mut conn = db();
        let mut legacy = entry("legacy", 'a');
        legacy.file_hash = None;
        insert(&conn, &legacy).unwrap();
        assert!(backfill_hash(&conn, "legacy", &"a".repeat(64)).unwrap());
        assert!(!backfill_hash(&conn, "legacy", &"b".repeat(64)).unwrap());
        assert!(!backfill_hash(&conn, "missing", &"a".repeat(64)).unwrap());
        assert_eq!(
            get(&conn, "legacy").unwrap().unwrap().file_hash,
            Some("a".repeat(64))
        );
        for bad in [
            None,
            Some("".into()),
            Some("f".repeat(63)),
            Some("f".repeat(65)),
            Some("F".repeat(64)),
            Some("g".repeat(64)),
            Some("色".repeat(64)),
        ] {
            let mut candidate = entry("invalid", 'b');
            candidate.file_hash = bad;
            assert!(admit(&mut conn, &candidate).is_err());
            if candidate.file_hash.is_some() {
                assert!(insert(&conn, &candidate).is_err());
            }
        }
        assert_eq!(entries(&conn, true).unwrap().len(), 1);
    }

    #[test]
    fn invalid_category_rolls_back_restoration_and_new_import() {
        let mut conn = db();
        admit(&mut conn, &entry("one", 'a')).unwrap();
        hide(&conn, "one").unwrap();
        let mut candidate = entry("candidate", 'a');
        candidate.category_id = "missing".into();
        assert!(admit(&mut conn, &candidate).is_err());
        assert!(get(&conn, "one").unwrap().unwrap().hidden);
        candidate.file_hash = Some("b".repeat(64));
        assert!(admit(&mut conn, &candidate).is_err());
        assert!(get(&conn, "candidate").unwrap().is_none());
    }

    #[test]
    fn concurrent_connections_admit_only_one_copy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        let mut conn = Connection::open(&path).unwrap();
        prepare(&mut conn);
        drop(conn);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
        let handles: Vec<_> = (0..4)
            .map(|index| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut conn = Connection::open(path).unwrap();
                    conn.busy_timeout(std::time::Duration::from_secs(2))
                        .unwrap();
                    barrier.wait();
                    admit(&mut conn, &entry(&format!("entry-{index}"), 'a')).unwrap()
                })
            })
            .collect();
        let results: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        assert_eq!(
            results
                .iter()
                .filter(|result| **result == Admission::Imported)
                .count(),
            1
        );
        let conn = Connection::open(path).unwrap();
        let records = entries(&conn, true).unwrap();
        assert_eq!(records.len(), 1);
        for result in results {
            if let Admission::Duplicate(id) = result {
                assert_eq!(id, records[0].id);
            }
        }
    }
}
