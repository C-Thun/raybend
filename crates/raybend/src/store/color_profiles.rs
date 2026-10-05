//! 全局 RGB ICC 索引。原件以内容身份保存在 app data；本表只管选择入口。

use crate::error::{Error, Result};
use lcms2::ProfileClassSignature;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorProfileClass {
    Input,
    Display,
    Output,
    ColorSpace,
}

impl ColorProfileClass {
    pub fn from_icc(class: ProfileClassSignature) -> Option<Self> {
        match class {
            ProfileClassSignature::InputClass => Some(Self::Input),
            ProfileClassSignature::DisplayClass => Some(Self::Display),
            ProfileClassSignature::OutputClass => Some(Self::Output),
            ProfileClassSignature::ColorSpaceClass => Some(Self::ColorSpace),
            _ => None,
        }
    }

    fn as_db(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Display => "display",
            Self::Output => "output",
            Self::ColorSpace => "color_space",
        }
    }

    pub fn allows_input(self) -> bool {
        matches!(self, Self::Input | Self::Display | Self::ColorSpace)
    }

    pub fn allows_output(self) -> bool {
        matches!(self, Self::Output | Self::Display | Self::ColorSpace)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorProfileRecord {
    pub profile_id: String,
    pub name: String,
    pub original_filename: String,
    pub profile_class: ColorProfileClass,
    pub hidden: bool,
    pub created_at: i64,
}

fn record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ColorProfileRecord> {
    let class: String = row.get(3)?;
    let profile_class = match class.as_str() {
        "input" => ColorProfileClass::Input,
        "display" => ColorProfileClass::Display,
        "output" => ColorProfileClass::Output,
        "color_space" => ColorProfileClass::ColorSpace,
        _ => {
            return Err(rusqlite::Error::InvalidColumnType(
                3,
                "profile_class".into(),
                rusqlite::types::Type::Text,
            ));
        }
    };
    Ok(ColorProfileRecord {
        profile_id: row.get(0)?,
        name: row.get(1)?,
        original_filename: row.get(2)?,
        profile_class,
        hidden: row.get::<_, i64>(4)? != 0,
        created_at: row.get(5)?,
    })
}

const COLUMNS: &str = "profile_id,name,original_filename,profile_class,hidden,created_at";

pub fn entries(conn: &Connection, include_hidden: bool) -> Result<Vec<ColorProfileRecord>> {
    let mut statement = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM color_profiles WHERE hidden=0 OR ?1=1 ORDER BY created_at,profile_id"
    ))?;
    Ok(statement
        .query_map([i64::from(include_hidden)], record)?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<ColorProfileRecord>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM color_profiles WHERE profile_id=?1"),
            [id],
            record,
        )
        .optional()?)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admission {
    Imported,
    Duplicate,
    Restored,
}

/// 同内容去重与隐藏恢复在一次事务中完成；不改变旧引用的身份或分类。
pub fn admit(conn: &mut Connection, entry: &ColorProfileRecord) -> Result<Admission> {
    validate(entry)?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let outcome = match get(&tx, &entry.profile_id)? {
        Some(existing) if existing.hidden => {
            tx.execute(
                "UPDATE color_profiles SET hidden=0 WHERE profile_id=?1",
                [&entry.profile_id],
            )?;
            Admission::Restored
        }
        Some(_) => Admission::Duplicate,
        None => {
            tx.execute(
                "INSERT INTO color_profiles (profile_id,name,original_filename,profile_class,hidden,created_at) VALUES (?1,?2,?3,?4,0,?5)",
                params![entry.profile_id, entry.name, entry.original_filename, entry.profile_class.as_db(), entry.created_at],
            )?;
            Admission::Imported
        }
    };
    tx.commit()?;
    Ok(outcome)
}

pub fn hide(conn: &Connection, id: &str) -> Result<bool> {
    Ok(conn.execute(
        "UPDATE color_profiles SET hidden=1 WHERE profile_id=?1 AND hidden=0",
        [id],
    )? > 0)
}

fn validate(entry: &ColorProfileRecord) -> Result<()> {
    if entry.profile_id.len() != 64
        || !entry
            .profile_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || entry.name.trim().is_empty()
        || entry.name.chars().count() > 160
        || entry.name.chars().any(char::is_control)
        || entry.original_filename.trim().is_empty()
        || entry.original_filename.chars().count() > 512
        || entry.original_filename.chars().any(char::is_control)
    {
        return Err(Error::Unsupported("ICC 配置文件索引无效".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::migration::{self, DbKind};

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        migration::apply(&mut conn, DbKind::App, migration::Backups::none(), 1).unwrap();
        conn
    }

    fn entry(hash: char) -> ColorProfileRecord {
        ColorProfileRecord {
            profile_id: hash.to_string().repeat(64),
            name: "摄影棚 色彩.icc".into(),
            original_filename: "摄影棚 色彩.icc".into(),
            profile_class: ColorProfileClass::Display,
            hidden: false,
            created_at: 1,
        }
    }

    #[test]
    fn duplicate_restore_and_same_name_different_content() {
        let mut conn = db();
        assert_eq!(admit(&mut conn, &entry('a')).unwrap(), Admission::Imported);
        let mut renamed = entry('a');
        renamed.name = "new name.icc".into();
        assert_eq!(admit(&mut conn, &renamed).unwrap(), Admission::Duplicate);
        assert_eq!(
            get(&conn, &renamed.profile_id).unwrap().unwrap().name,
            "摄影棚 色彩.icc"
        );
        assert!(hide(&conn, &renamed.profile_id).unwrap());
        assert!(!hide(&conn, &renamed.profile_id).unwrap());
        assert!(entries(&conn, false).unwrap().is_empty());
        assert_eq!(admit(&mut conn, &renamed).unwrap(), Admission::Restored);
        assert_eq!(admit(&mut conn, &entry('b')).unwrap(), Admission::Imported);
        assert_eq!(entries(&conn, false).unwrap().len(), 2);
    }

    #[test]
    fn invalid_hash_and_name_do_not_touch_db() {
        let mut conn = db();
        for bad in [
            "A".repeat(64),
            "f".repeat(63),
            "g".repeat(64),
            "色".repeat(64),
        ] {
            let mut entry = entry('a');
            entry.profile_id = bad;
            assert!(admit(&mut conn, &entry).is_err());
        }
        let mut entry = entry('a');
        entry.name = "\n".into();
        assert!(admit(&mut conn, &entry).is_err());
        assert!(entries(&conn, true).unwrap().is_empty());
    }

    #[test]
    fn concurrent_admission_keeps_one_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        let mut conn = Connection::open(&path).unwrap();
        migration::apply(&mut conn, DbKind::App, migration::Backups::none(), 1).unwrap();
        drop(conn);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut conn = Connection::open(path).unwrap();
                    conn.busy_timeout(std::time::Duration::from_secs(2))
                        .unwrap();
                    barrier.wait();
                    admit(&mut conn, &entry('c')).unwrap()
                })
            })
            .collect();
        let outcomes: Vec<_> = handles
            .into_iter()
            .map(|join| join.join().unwrap())
            .collect();
        assert_eq!(
            outcomes
                .iter()
                .filter(|value| **value == Admission::Imported)
                .count(),
            1
        );
        assert_eq!(
            entries(&Connection::open(path).unwrap(), false)
                .unwrap()
                .len(),
            1
        );
    }
}
