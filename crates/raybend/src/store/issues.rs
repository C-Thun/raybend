//! 不可变定稿：完整编辑 profile 的版本化快照；latest 仍由 `develop_stacks` 管理。
//! 选中态每次按当前 profile 的规范哈希匹配，不写选择 ID。

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::develop::{DevelopStack, EditBase};
use crate::error::{Error, Result};

pub const PROFILE_SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Issue {
    pub id: i64,
    pub asset_id: i64,
    pub name: String,
    pub profile_hash: String,
    pub source_base: EditBase,
    pub created_at: i64,
    pub stack: DevelopStack,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Selection {
    Sooc,
    Raw,
    Latest,
    Issue(i64),
}

/// BTreeMap 保证键序稳定；serde_json 保留浮点精度。名称和时间不进哈希。
pub fn profile_hash(stack: &DevelopStack) -> Result<String> {
    let mut appearance = stack.clone();
    appearance.auto_adjust = None;
    let bytes = serde_json::to_vec(&appearance)
        .map_err(|error| Error::Unsupported(format!("定稿配置序列化失败：{error}")))?;
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Ok(format!("{hash:016x}"))
}

pub fn list(conn: &Connection, asset_id: i64) -> Result<Vec<Issue>> {
    let mut statement = conn.prepare(
        "SELECT id, asset_id, name, profile_hash, source_base, created_at, schema_version, profile_json \
         FROM issues WHERE asset_id = ?1 ORDER BY created_at DESC, id DESC",
    )?;
    let rows = statement.query_map([asset_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, String>(7)?,
        ))
    })?;
    rows.map(|row| {
        let (id, asset_id, name, profile_hash, base, created_at, schema_version, json) = row?;
        if schema_version != PROFILE_SCHEMA_VERSION {
            return Err(Error::Unsupported(format!(
                "定稿 {id} 的配置版本 {schema_version} 尚不支持"
            )));
        }
        let source_base = EditBase::parse(&base)
            .ok_or_else(|| Error::Unsupported(format!("定稿 {id} 的编辑源无效")))?;
        let stack: DevelopStack = serde_json::from_str(&json)
            .map_err(|error| Error::Unsupported(format!("定稿 {id} 的配置损坏：{error}")))?;
        if stack.source_base != source_base || self::profile_hash(&stack)? != profile_hash {
            return Err(Error::Unsupported(format!("定稿 {id} 的配置指纹不一致")));
        }
        Ok(Issue {
            id,
            asset_id,
            name,
            profile_hash,
            source_base,
            created_at,
            stack,
        })
    })
    .collect()
}

pub fn get(conn: &Connection, asset_id: i64, issue_id: i64) -> Result<Option<Issue>> {
    Ok(list(conn, asset_id)?
        .into_iter()
        .find(|issue| issue.id == issue_id))
}

fn same_adjustments(first: &DevelopStack, second: &DevelopStack) -> bool {
    let mut first = first.clone();
    let mut second = second.clone();
    first.auto_adjust = None;
    second.auto_adjust = None;
    first == second
}

/// 只匹配不可变的命名定稿；latest 工作副本不参与去重。
fn matching_issue(stack: &DevelopStack, issues: &[Issue]) -> Result<Option<i64>> {
    let hash = profile_hash(stack)?;
    Ok(issues
        .iter()
        .find(|issue| issue.profile_hash == hash && same_adjustments(&issue.stack, stack))
        .map(|issue| issue.id))
}

pub fn selection(stack: &DevelopStack, issues: &[Issue]) -> Result<Selection> {
    // 优先命名定稿，其次原始源；latest 只是没有其它匹配时的兜底。
    if let Some(id) = matching_issue(stack, issues)? {
        return Ok(Selection::Issue(id));
    }
    if stack.is_empty() {
        return Ok(match stack.source_base {
            EditBase::Sooc => Selection::Sooc,
            EditBase::Raw => Selection::Raw,
        });
    }
    Ok(Selection::Latest)
}

pub fn can_finalize(stack: &DevelopStack, issues: &[Issue]) -> Result<bool> {
    // 自动保存使当前 profile 必然等于 latest；这不能阻止另存定稿。
    // 只有原始源或已经存在的不可变定稿才阻止保存。
    Ok(!stack.is_empty() && matching_issue(stack, issues)?.is_none())
}

pub fn create(
    conn: &Connection,
    asset_id: i64,
    raw_name: &str,
    stack: &DevelopStack,
    now_ms: i64,
) -> Result<Issue> {
    let name = raw_name.trim();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err(Error::Unsupported("定稿名称必须是 1–80 个可见字符".into()));
    }
    if ["sooc", "raw", "latest"]
        .iter()
        .any(|reserved| name.eq_ignore_ascii_case(reserved))
    {
        return Err(Error::Unsupported(
            "SOOC、RAW 与 latest 是保留定稿名".into(),
        ));
    }
    if !can_finalize(stack, &list(conn, asset_id)?)? {
        return Err(Error::Unsupported("当前配置与现有定稿或原始源相同".into()));
    }
    let hash = profile_hash(stack)?;
    let json = serde_json::to_string(stack)
        .map_err(|error| Error::Unsupported(format!("定稿配置序列化失败：{error}")))?;
    conn.execute(
        "INSERT INTO issues (asset_id, schema_version, name, profile_json, profile_hash, source_base, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![asset_id, PROFILE_SCHEMA_VERSION, name, json, hash, stack.source_base.as_str(), now_ms],
    )?;
    get(conn, asset_id, conn.last_insert_rowid())?
        .ok_or_else(|| Error::Unsupported("刚创建的定稿无法读回".into()))
}

pub fn delete(conn: &Connection, asset_id: i64, issue_id: i64) -> Result<bool> {
    Ok(conn.execute(
        "DELETE FROM issues WHERE asset_id = ?1 AND id = ?2",
        params![asset_id, issue_id],
    )? > 0)
}

/// 影调与颜色取十档；这只是可编辑的建议名，不参与配置或画面。
pub fn style_words(stack: &DevelopStack, english: bool) -> (&'static str, &'static str) {
    let value = |key: &str| stack.params.get(key).copied().unwrap_or(0.0);
    let exposure = value("exposure");
    let contrast = value("contrast");
    let blacks = value("blacks");
    let highlights = value("highlights");
    let saturation = value("saturation") + value("vibrance") * 0.5;
    let temp = value("temperature");
    let tint = value("tint");
    let tone = if exposure < -0.7 {
        0
    } else if exposure > 0.7 {
        1
    } else if contrast < -25.0 {
        2
    } else if contrast > 25.0 {
        3
    } else if blacks < -20.0 {
        4
    } else if blacks > 20.0 {
        5
    } else if highlights < -25.0 {
        6
    } else if highlights > 25.0 {
        7
    } else if contrast < -8.0 {
        8
    } else {
        9
    };
    let color = if saturation < -35.0 {
        0
    } else if saturation > 35.0 {
        1
    } else if temp < 4500.0 && temp > 0.0 {
        2
    } else if temp > 7500.0 {
        3
    } else if tint < -20.0 {
        4
    } else if tint > 20.0 {
        5
    } else if saturation < -10.0 {
        6
    } else if saturation > 10.0 {
        7
    } else if temp < 5500.0 && temp > 0.0 {
        8
    } else {
        9
    };
    const ZH_TONE: [&str; 10] = [
        "幽暗", "明朗", "柔和", "鲜明", "深邃", "轻盈", "含蓄", "通透", "淡雅", "自然",
    ];
    const ZH_COLOR: [&str; 10] = [
        "素净灰调",
        "浓彩",
        "清冷蓝调",
        "暖金调",
        "青绿调",
        "绯红调",
        "低饱和",
        "鲜艳色彩",
        "微冷调",
        "原色",
    ];
    const EN_TONE: [&str; 10] = [
        "Nocturne", "Luminous", "Soft", "Bold", "Deep", "Airy", "Muted", "Radiant", "Delicate",
        "Natural",
    ];
    const EN_COLOR: [&str; 10] = [
        "Monochrome",
        "Vivid",
        "Blue",
        "Amber",
        "Teal",
        "Crimson",
        "Pastel",
        "Color",
        "Cool",
        "Neutral",
    ];
    if english {
        (EN_TONE[tone], EN_COLOR[color])
    } else {
        (ZH_TONE[tone], ZH_COLOR[color])
    }
}

pub fn suggested_name(
    conn: &Connection,
    asset_id: i64,
    stack: &DevelopStack,
    english: bool,
) -> Result<String> {
    let (tone, color) = style_words(stack, english);
    let root = if english {
        format!("{tone} {color}")
    } else {
        format!("{tone}{color}")
    };
    for number in 1..=1296_u32 {
        let digit = |value: u32| char::from_digit(value, 36).unwrap().to_ascii_uppercase();
        let suffix = format!("{}{}", digit(number / 36), digit(number % 36));
        let candidate = format!("{root} {suffix}");
        let exists: Option<i64> = conn
            .query_row(
                "SELECT id FROM issues WHERE asset_id = ?1 AND name = ?2",
                params![asset_id, candidate],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_none() {
            return Ok(candidate);
        }
    }
    Err(Error::Unsupported("这张照片的自动定稿名称已用完".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::migration::{self, DbKind};

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        migration::apply(&mut conn, DbKind::Catalog, migration::Backups::none(), 1).unwrap();
        conn
    }

    #[test]
    fn automatic_provenance_is_saved_without_affecting_issue_matching_or_legacy_hash() {
        let conn = db();
        conn.execute_batch("INSERT INTO assets(id, imported_at, updated_at) VALUES(1,1,1);")
            .unwrap();
        let mut original = DevelopStack::default();
        original.params.insert("exposure".into(), 0.5);
        let hash = profile_hash(&original).unwrap();
        let mut with_auto = original.clone();
        with_auto.auto_adjust = Some(super::super::develop::AutoAdjustBaseline {
            values: [("exposure".to_owned(), 0.25)].into(),
            ..Default::default()
        });
        assert_eq!(hash, profile_hash(&with_auto).unwrap());
        let saved = create(&conn, 1, "自动结果", &with_auto, 1).unwrap();
        assert_eq!(saved.stack.auto_adjust, with_auto.auto_adjust);
        assert_eq!(
            selection(&original, &list(&conn, 1).unwrap()).unwrap(),
            Selection::Issue(saved.id)
        );
        assert!(!can_finalize(&original, &list(&conn, 1).unwrap()).unwrap());
        with_auto
            .auto_adjust
            .as_mut()
            .unwrap()
            .values
            .insert("exposure".into(), 0.4);
        assert_eq!(
            selection(&with_auto, &list(&conn, 1).unwrap()).unwrap(),
            Selection::Issue(saved.id)
        );
        assert!(!can_finalize(&with_auto, &list(&conn, 1).unwrap()).unwrap());
        with_auto.params.insert("exposure".into(), 0.8);
        assert!(can_finalize(&with_auto, &list(&conn, 1).unwrap()).unwrap());
    }

    #[test]
    fn immutable_issues_are_scoped_to_assets_and_match_by_profile() {
        let conn = db();
        conn.execute_batch(
            "INSERT INTO assets(id, imported_at, updated_at) VALUES (1, 1, 1), (2, 1, 1);",
        )
        .unwrap();
        let mut first = DevelopStack::default();
        first.params.insert("exposure".into(), 0.25);
        let saved = create(&conn, 1, "清冷蓝调 AA", &first, 100).unwrap();
        assert_eq!(
            selection(&first, &list(&conn, 1).unwrap()).unwrap(),
            Selection::Issue(saved.id)
        );
        assert!(!can_finalize(&first, &list(&conn, 1).unwrap()).unwrap());
        assert!(create(&conn, 1, "重复配置", &first, 101).is_err());
        assert!(list(&conn, 2).unwrap().is_empty());
        assert!(!delete(&conn, 2, saved.id).unwrap());
        assert!(delete(&conn, 1, saved.id).unwrap());
        assert!(can_finalize(&first, &list(&conn, 1).unwrap()).unwrap());
    }

    #[test]
    fn auto_saved_latest_does_not_block_finalize_or_hide_matching_issue() {
        use crate::store::develop::{load, save};
        let conn = db();
        conn.execute(
            "INSERT INTO assets(id, imported_at, updated_at) VALUES (1, 1, 1)",
            [],
        )
        .unwrap();
        for base in [EditBase::Raw, EditBase::Sooc] {
            let mut current = DevelopStack {
                source_base: base,
                ..Default::default()
            };
            current.params.insert("exposure".into(), 0.25);
            save(&conn, 1, &current, 1).unwrap();
            let latest = load(&conn, 1).unwrap();
            assert_eq!(latest, current);
            let before = list(&conn, 1).unwrap();
            assert_eq!(selection(&current, &before).unwrap(), Selection::Latest);
            assert!(can_finalize(&latest, &before).unwrap());

            let saved = create(
                &conn,
                1,
                &format!("已编辑版本 {}", base.as_str()),
                &latest,
                2,
            )
            .unwrap();
            let saved_issues = list(&conn, 1).unwrap();
            assert_eq!(
                selection(&latest, &saved_issues).unwrap(),
                Selection::Issue(saved.id)
            );
            assert!(!can_finalize(&latest, &saved_issues).unwrap());

            current.params.insert("contrast".into(), 12.0);
            save(&conn, 1, &current, 3).unwrap();
            let changed_latest = load(&conn, 1).unwrap();
            assert_eq!(
                selection(&changed_latest, &saved_issues).unwrap(),
                Selection::Latest
            );
            assert!(can_finalize(&changed_latest, &saved_issues).unwrap());

            save(&conn, 1, &saved.stack, 4).unwrap();
            assert_eq!(
                selection(&load(&conn, 1).unwrap(), &saved_issues).unwrap(),
                Selection::Issue(saved.id)
            );
            assert!(!can_finalize(&load(&conn, 1).unwrap(), &saved_issues).unwrap());
        }
    }

    #[test]
    fn named_issue_matches_before_source_fallback_and_hash_collision_is_checked() {
        let stack = DevelopStack::default();
        let issue = Issue {
            id: 7,
            asset_id: 1,
            name: "旧版原始快照".into(),
            profile_hash: profile_hash(&stack).unwrap(),
            source_base: EditBase::Raw,
            created_at: 1,
            stack: stack.clone(),
        };
        assert_eq!(selection(&stack, &[issue]).unwrap(), Selection::Issue(7));
        assert_eq!(selection(&stack, &[]).unwrap(), Selection::Raw);
        assert!(!can_finalize(&stack, &[]).unwrap());
        let sooc = DevelopStack {
            source_base: EditBase::Sooc,
            ..Default::default()
        };
        assert_eq!(selection(&sooc, &[]).unwrap(), Selection::Sooc);
        assert!(!can_finalize(&sooc, &[]).unwrap());

        let mut current = stack.clone();
        current.params.insert("exposure".into(), 0.5);
        let collision = Issue {
            id: 8,
            asset_id: 1,
            name: "另一份配置".into(),
            profile_hash: profile_hash(&current).unwrap(),
            source_base: EditBase::Raw,
            created_at: 1,
            stack,
        };
        assert_eq!(
            selection(&current, &[collision.clone()]).unwrap(),
            Selection::Latest
        );
        assert!(can_finalize(&current, &[collision]).unwrap());
    }

    #[test]
    fn names_and_invalid_profiles_do_not_pollute_the_catalog() {
        let conn = db();
        conn.execute(
            "INSERT INTO assets(id, imported_at, updated_at) VALUES (1, 1, 1)",
            [],
        )
        .unwrap();
        let mut first = DevelopStack::default();
        first.params.insert("exposure".into(), 0.25);
        assert!(create(&conn, 1, "  ", &first, 1).is_err());
        assert!(create(&conn, 1, "bad\nname", &first, 1).is_err());
        assert!(create(&conn, 1, &"长".repeat(81), &first, 1).is_err());
        assert!(create(&conn, 1, "RAW", &first, 1).is_err());
        assert!(create(&conn, 1, "Latest", &first, 1).is_err());
        let name = suggested_name(&conn, 1, &first, false).unwrap();
        assert!(name.ends_with(" 01"));
        create(&conn, 1, &name, &first, 1).unwrap();
        assert!(
            suggested_name(&conn, 1, &first, false)
                .unwrap()
                .ends_with(" 02")
        );
    }

    #[test]
    fn hash_and_selection_follow_complete_profile() {
        let mut stack = DevelopStack::default();
        assert_eq!(selection(&stack, &[]).unwrap(), Selection::Raw);
        stack.params.insert("exposure".into(), 0.5);
        assert_eq!(selection(&stack, &[]).unwrap(), Selection::Latest);
        let old = profile_hash(&stack).unwrap();
        stack.lut_id = Some("lut-1".into());
        stack.lut_enabled = Some(false);
        assert_ne!(old, profile_hash(&stack).unwrap());
    }

    #[test]
    fn names_have_hundred_style_pairs_and_separate_base36_suffix() {
        let mut stack = DevelopStack::default();
        stack.params.insert("contrast".into(), -30.0);
        stack.params.insert("saturation".into(), -40.0);
        assert_eq!(style_words(&stack, false), ("柔和", "素净灰调"));
        assert_eq!(style_words(&stack, true), ("Soft", "Monochrome"));
    }
}
