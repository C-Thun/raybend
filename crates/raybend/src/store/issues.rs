//! 不可变定稿：完整编辑 profile 的版本化快照；latest 仍由 `develop_stacks` 管理。
//! 选中态每次按当前 profile 的规范哈希匹配，不写选择 ID。

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::develop::{DevelopStack, EditBase};
use crate::error::{Error, Result};

pub const PROFILE_SCHEMA_VERSION: i64 = 2;

pub fn profile_schema_version(stack: &DevelopStack) -> i64 {
    if stack.color.is_some() { 2 } else { 1 }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Issue {
    pub id: i64,
    pub asset_id: i64,
    pub name: String,
    pub profile_hash: String,
    pub source_base: EditBase,
    pub created_at: i64,
    /// 导出尾号序号（I00–I99）；每资产唯一，分配见 `allocate_ordinal`。
    pub ordinal: i64,
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
        "SELECT id, asset_id, name, profile_hash, source_base, created_at, schema_version, profile_json, ordinal \
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
            row.get::<_, i64>(8)?,
        ))
    })?;
    rows.map(|row| {
        let (id, asset_id, name, profile_hash, base, created_at, schema_version, json, ordinal) = row?;
        if !(1..=PROFILE_SCHEMA_VERSION).contains(&schema_version) {
            return Err(Error::Unsupported(format!(
                "定稿 {id} 的配置版本 {schema_version} 尚不支持"
            )));
        }
        let source_base = EditBase::parse(&base)
            .ok_or_else(|| Error::Unsupported(format!("定稿 {id} 的编辑源无效")))?;
        let stack: DevelopStack = serde_json::from_str(&json)
            .map_err(|error| Error::Unsupported(format!("定稿 {id} 的配置损坏：{error}")))?;
        stack.validate()?;
        if schema_version != profile_schema_version(&stack) || stack.source_base != source_base || self::profile_hash(&stack)? != profile_hash {
            return Err(Error::Unsupported(format!("定稿 {id} 的配置指纹不一致")));
        }
        Ok(Issue {
            id,
            asset_id,
            name,
            profile_hash,
            source_base,
            created_at,
            ordinal,
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

/// 序号分配（specs/export-issue-ordinal.md §3）：从 assets.issue_counter 起步向上找
/// 第一个空位，99 后绕回 0；全满拒绝。写侧由单写者 actor 串行化，配合
/// UNIQUE(asset_id, ordinal) 索引双保险。
fn allocate_ordinal(conn: &Connection, asset_id: i64) -> Result<i64> {
    let counter: i64 = conn
        .query_row(
            "SELECT issue_counter FROM assets WHERE id = ?1",
            [asset_id],
            |row| row.get(0),
        )
        .map_err(|_| Error::Unsupported("照片不存在，无法分配定稿序号".into()))?;
    let mut statement = conn.prepare("SELECT ordinal FROM issues WHERE asset_id = ?1")?;
    let occupied: std::collections::BTreeSet<i64> = statement
        .query_map([asset_id], |row| row.get::<_, i64>(0))?
        .collect::<std::result::Result<_, _>>()?;
    if occupied.len() >= 100 {
        return Err(Error::Unsupported("已达 100 个定稿上限（序号 0–99 全部占用）".into()));
    }
    let mut candidate = counter.rem_euclid(100);
    while occupied.contains(&candidate) {
        candidate = (candidate + 1) % 100;
    }
    conn.execute(
        "UPDATE assets SET issue_counter = ?2 WHERE id = ?1",
        params![asset_id, (candidate + 1) % 100],
    )?;
    Ok(candidate)
}

pub fn create(
    conn: &Connection,
    asset_id: i64,
    raw_name: &str,
    stack: &DevelopStack,
    now_ms: i64,
) -> Result<Issue> {
    let name = validated_name(raw_name)?;
    ensure_name_free(conn, asset_id, name, None)?;
    if !can_finalize(stack, &list(conn, asset_id)?)? {
        return Err(Error::Unsupported("当前配置与现有定稿或原始源相同".into()));
    }
    let hash = profile_hash(stack)?;
    let json = serde_json::to_string(stack)
        .map_err(|error| Error::Unsupported(format!("定稿配置序列化失败：{error}")))?;
    let ordinal = allocate_ordinal(conn, asset_id)?;
    conn.execute(
        "INSERT INTO issues (asset_id, schema_version, name, profile_json, profile_hash, source_base, created_at, ordinal) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![asset_id, profile_schema_version(stack), name, json, hash, stack.source_base.as_str(), now_ms, ordinal],
    )?;
    get(conn, asset_id, conn.last_insert_rowid())?
        .ok_or_else(|| Error::Unsupported("刚创建的定稿无法读回".into()))
}

/// 定稿名称的合法性：1–80 个可见字符，且不能占保留名（`SOOC` / `RAW` / `latest`）。
///
/// [`create`] 与 [`rename`] 共用这一份 —— 两处各写一遍必然漂移：
/// 「能建出来的名字」与「能改成去的名字」看起来就该是同一套规则。
fn validated_name(raw_name: &str) -> Result<&str> {
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
    Ok(name)
}

/// 同一张照片里名字不能撞车（DB 有 `UNIQUE(asset_id, name)`）——
/// 在这里先拦住，是为了让用户看到「已有同名定稿」而不是一句 SQLite 约束错。
///
/// `ignore_id` = 改名时允许保留自己那个名字（改成同一个名字是无事发生）。
fn ensure_name_free(
    conn: &Connection,
    asset_id: i64,
    name: &str,
    ignore_id: Option<i64>,
) -> Result<()> {
    let taken: Option<i64> = conn
        .query_row(
            "SELECT id FROM issues WHERE asset_id = ?1 AND name = ?2",
            params![asset_id, name],
            |row| row.get(0),
        )
        .optional()?;
    match taken {
        Some(id) if Some(id) != ignore_id => {
            Err(Error::Unsupported(format!("已有同名定稿「{name}」")))
        }
        _ => Ok(()),
    }
}

/// 改一条定稿的**名称**（崔总 2026-10-08）。
///
/// 只动 `name` 一列：配置 JSON / 哈希 / 基准 / 创建时间 / 序号全部不动 ——
/// 改名前后是**同一份定稿**，选中态（按 `id`）与画面都不该受影响。
/// 名字撞车与建定稿同一条规矩（DB 的唯一约束，先拦一步给人话）。
/// 返回 `Ok(false)` = 这条定稿不存在（越权/踩空的 `id`，调用方按错误处理）。
pub fn rename(conn: &Connection, asset_id: i64, issue_id: i64, raw_name: &str) -> Result<bool> {
    let name = validated_name(raw_name)?;
    ensure_name_free(conn, asset_id, name, Some(issue_id))?;
    Ok(conn.execute(
        "UPDATE issues SET name = ?3 WHERE asset_id = ?1 AND id = ?2",
        params![asset_id, issue_id, name],
    )? > 0)
}

pub fn delete(conn: &Connection, asset_id: i64, issue_id: i64) -> Result<bool> {
    Ok(conn.execute(
        "DELETE FROM issues WHERE asset_id = ?1 AND id = ?2",
        params![asset_id, issue_id],
    )? > 0)
}

/// 采纳（sidecar 导入）一条定稿：保留 sidecar 里的**名称 / 基准 / 创建时间**；
/// 序号优先沿用，冲突或越界时重新分配；重复（同哈希同配置）或名称非法则跳过。
///
/// 与 [`create`] 的区别：不要求「与现有定稿不同」的交互语义（重复直接静默跳过），
/// 也不用当前 latest —— 栈来自 sidecar（共用 `DevelopStack::validate` 校验）。
/// 返回 `true` = 真的建了。
pub fn import_issue(
    conn: &Connection,
    asset_id: i64,
    raw_name: &str,
    source_base: EditBase,
    created_at_ms: i64,
    ordinal_hint: Option<i64>,
    stack: &DevelopStack,
) -> Result<bool> {
    if validated_name(raw_name).is_err() {
        return Ok(false);
    }
    let name = raw_name.trim();
    // 栈自身的基准与声明对齐（手改过的 sidecar 可能两边不一致；声明值胜出）——
    // 必须在去重判断**之前**做，否则两边基准不一致时哈希对不上、重复检测失效。
    let mut stack = stack.clone();
    stack.source_base = source_base;
    stack.validate()?;
    let existing = list(conn, asset_id)?;
    if matching_issue(&stack, &existing)?.is_some() {
        return Ok(false);
    }
    let occupied: std::collections::BTreeSet<i64> =
        existing.iter().map(|issue| issue.ordinal).collect();
    let ordinal = match ordinal_hint {
        Some(hint) if (0..100).contains(&hint) && !occupied.contains(&hint) => hint,
        _ => allocate_ordinal(conn, asset_id)?,
    };
    let hash = profile_hash(&stack)?;
    let json = serde_json::to_string(&stack)
        .map_err(|error| Error::Unsupported(format!("定稿配置序列化失败：{error}")))?;
    conn.execute(
        "INSERT INTO issues (asset_id, schema_version, name, profile_json, profile_hash, source_base, created_at, ordinal) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            asset_id,
            profile_schema_version(&stack),
            name,
            json,
            hash,
            source_base.as_str(),
            created_at_ms,
            ordinal
        ],
    )?;
    Ok(true)
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
    fn import_issue_keeps_fields_reuses_ordinal_and_skips_duplicates() {
        let conn = db();
        conn.execute_batch("INSERT INTO assets(id, imported_at, updated_at) VALUES(1,1,1);")
            .unwrap();
        let stack = |exposure: f64| {
            let mut stack = DevelopStack::default();
            stack.params.insert("exposure".into(), exposure);
            stack
        };
        // 序号沿用提示值；名称与创建时间保留
        assert!(import_issue(
            &conn,
            1,
            "暖调",
            EditBase::Sooc,
            1_759_000_000_000,
            Some(7),
            &stack(0.1)
        )
        .unwrap());
        let imported = list(&conn, 1).unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].name, "暖调");
        assert_eq!(imported[0].ordinal, 7);
        assert_eq!(imported[0].created_at, 1_759_000_000_000);
        assert_eq!(imported[0].source_base, EditBase::Sooc);

        // 同配置重复导入：静默跳过
        assert!(!import_issue(
            &conn,
            1,
            "另一个名字",
            EditBase::Sooc,
            2,
            Some(8),
            &stack(0.1)
        )
        .unwrap());

        // 序号冲突（7 已占）：重新分配
        assert!(import_issue(&conn, 1, "冷调", EditBase::Raw, 3, Some(7), &stack(0.2)).unwrap());
        let imported = list(&conn, 1).unwrap();
        assert_eq!(imported.len(), 2);
        // list 按 created_at DESC 排（第一条的时间戳更大），序号比集合不比顺序
        let mut ordinals: Vec<i64> = imported.iter().map(|i| i.ordinal).collect();
        ordinals.sort_unstable();
        assert_eq!(ordinals, vec![0, 7]);

        // 非法名称 / 越界序号：跳过 / 重分配
        assert!(!import_issue(&conn, 1, "  ", EditBase::Raw, 4, None, &stack(0.3)).unwrap());
        assert!(!import_issue(
            &conn,
            1,
            "SOOC",
            EditBase::Raw,
            5,
            None,
            &stack(0.4)
        )
        .unwrap(), "保留名不许用");
        assert!(import_issue(&conn, 1, "远端", EditBase::Raw, 6, Some(999), &stack(0.5)).unwrap());
        let imported = list(&conn, 1).unwrap();
        assert_eq!(imported.len(), 3);
        assert!(imported.iter().all(|i| (0..100).contains(&i.ordinal)));
    }

    #[test]
    fn ordinals_allocate_in_sequence_reuse_holes_and_wrap() {
        let conn = db();
        conn.execute_batch("INSERT INTO assets(id, imported_at, updated_at) VALUES(1,1,1);")
            .unwrap();
        let stack = |exposure: f64| {
            let mut stack = DevelopStack::default();
            stack.params.insert("exposure".into(), exposure);
            stack
        };
        let a = create(&conn, 1, "甲", &stack(0.1), 1).unwrap();
        let b = create(&conn, 1, "乙", &stack(0.2), 2).unwrap();
        let c = create(&conn, 1, "丙", &stack(0.3), 3).unwrap();
        assert_eq!((a.ordinal, b.ordinal, c.ordinal), (0, 1, 2));
        // 删中间留洞：分配从游标顺找（不急着补洞），接着 3、4
        assert!(delete(&conn, 1, b.id).unwrap());
        let d = create(&conn, 1, "丁", &stack(0.4), 4).unwrap();
        assert_eq!(d.ordinal, 3);
        let e = create(&conn, 1, "戊", &stack(0.5), 5).unwrap();
        assert_eq!(e.ordinal, 4);
        // 游标指到 99：下一个拿 99，再下一个绕回后跳过 0、复用洞 1
        conn.execute("UPDATE assets SET issue_counter = 99 WHERE id = 1", [])
            .unwrap();
        let f = create(&conn, 1, "己", &stack(0.6), 6).unwrap();
        assert_eq!(f.ordinal, 99);
        let g = create(&conn, 1, "庚", &stack(0.7), 7).unwrap();
        assert_eq!(g.ordinal, 1);
    }

    #[test]
    fn ordinal_cap_at_one_hundred_rejects_new_issues_and_reuses_freed_slot() {
        let conn = db();
        conn.execute_batch("INSERT INTO assets(id, imported_at, updated_at) VALUES(1,1,1);")
            .unwrap();
        for number in 0..100 {
            let mut stack = DevelopStack::default();
            stack.params.insert("exposure".into(), -2.0 + number as f64 / 25.0);
            create(&conn, 1, &format!("第{number}"), &stack, number + 1).unwrap();
        }
        let mut ordinals: Vec<i64> = list(&conn, 1)
            .unwrap()
            .iter()
            .map(|issue| issue.ordinal)
            .collect();
        ordinals.sort_unstable();
        assert_eq!(ordinals, (0..100).collect::<Vec<_>>());
        let mut stack = DevelopStack::default();
        stack.params.insert("exposure".into(), 1.999);
        let full = create(&conn, 1, "第一百零一", &stack, 999).unwrap_err();
        assert!(full.to_string().contains("100"));
        // 删掉一个，位置立刻可用
        let victim = list(&conn, 1)
            .unwrap()
            .into_iter()
            .find(|issue| issue.ordinal == 42)
            .unwrap();
        assert!(delete(&conn, 1, victim.id).unwrap());
        let again = create(&conn, 1, "补位", &stack, 1000).unwrap();
        assert_eq!(again.ordinal, 42);
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
    fn color_issues_have_schema_two_and_legacy_issues_stay_immutable() {
        let conn = db();
        conn.execute_batch("INSERT INTO assets(id, imported_at, updated_at) VALUES(1,1,1);").unwrap();
        let mut legacy = DevelopStack::default();
        legacy.params.insert("exposure".into(), 0.5);
        let first = create(&conn, 1, "旧定稿", &legacy, 1).unwrap();
        let mut modern = legacy.clone();
        modern.color = Some(crate::color::PhotoColorState::new_pipeline(crate::color::SourceColor::AssumedSrgb));
        let second = create(&conn, 1, "色彩定稿", &modern, 2).unwrap();
        assert_ne!(first.profile_hash, second.profile_hash);
        let rows: Vec<(i64, i64)> = conn.prepare("SELECT id,schema_version FROM issues ORDER BY id").unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
        assert_eq!(rows, vec![(first.id, 1), (second.id, 2)]);
        let saved = list(&conn, 1).unwrap();
        let saved_first = saved.iter().find(|issue| issue.id == first.id).unwrap();
        let saved_second = saved.iter().find(|issue| issue.id == second.id).unwrap();
        assert_eq!(saved_first.stack, legacy);
        assert_eq!(saved_first.profile_hash, first.profile_hash);
        assert_eq!(saved_second.stack, modern);
        assert_eq!(selection(&modern, &saved).unwrap(), Selection::Issue(second.id));
        conn.execute("UPDATE issues SET schema_version=1 WHERE id=?1", [second.id]).unwrap();
        assert!(list(&conn, 1).is_err());
    }

    #[test]
    fn rename_changes_only_the_name() {
        let conn = db();
        conn.execute_batch("INSERT INTO assets(id, imported_at, updated_at) VALUES (1,1,1), (2,1,1);")
            .unwrap();
        let mut stack = DevelopStack::default();
        stack.params.insert("exposure".into(), 0.5);
        let saved = create(&conn, 1, "旧名字", &stack, 100).unwrap();

        assert!(rename(&conn, 1, saved.id, "  新名字  ").unwrap());
        let renamed = get(&conn, 1, saved.id).unwrap().unwrap();
        assert_eq!(renamed.name, "新名字", "首尾空白要去掉");
        // 改成自己的名字：无事发生，不算撞车
        assert!(rename(&conn, 1, saved.id, "新名字").unwrap());
        // 改的只有名字：同一份定稿的一切都原样
        assert_eq!(renamed.profile_hash, saved.profile_hash);
        assert_eq!(renamed.stack, saved.stack);
        assert_eq!(renamed.created_at, saved.created_at);
        assert_eq!(renamed.ordinal, saved.ordinal);
        assert_eq!(renamed.source_base, saved.source_base);
        // 选中态按 id：改名不换选中
        assert_eq!(
            selection(&stack, &list(&conn, 1).unwrap()).unwrap(),
            Selection::Issue(saved.id)
        );
        // 撞名要拦住（DB 有 UNIQUE(asset_id, name)），且要说人话
        let mut other = stack.clone();
        other.params.insert("contrast".into(), 10.0);
        let second = create(&conn, 1, "另一个", &other, 200).unwrap();
        let conflict = rename(&conn, 1, second.id, "新名字").unwrap_err();
        assert!(conflict.to_string().contains("已有同名定稿"), "{conflict}");
        assert_eq!(get(&conn, 1, second.id).unwrap().unwrap().name, "另一个");
        // 另一张照片的同名不算撞（作用域是「每张照片」）
        assert!(create(&conn, 2, "新名字", &stack, 300).is_ok());

        // 误传的 id：改不到（不是「默默成功」）
        assert!(!rename(&conn, 1, saved.id + 999, "无主名字").unwrap());
        assert_eq!(get(&conn, 2, saved.id).unwrap(), None);
    }

    #[test]
    fn rename_rejects_invalid_and_reserved_names() {
        let conn = db();
        conn.execute_batch("INSERT INTO assets(id, imported_at, updated_at) VALUES (1,1,1);")
            .unwrap();
        let mut stack = DevelopStack::default();
        stack.params.insert("exposure".into(), 0.5);
        let saved = create(&conn, 1, "正常名字", &stack, 100).unwrap();

        for bad in ["", "   ", "bad\nname", "sooC", "RAW", "Latest"] {
            assert!(rename(&conn, 1, saved.id, bad).is_err(), "非法名不许通过：{bad:?}");
        }
        assert!(rename(&conn, 1, saved.id, &"长".repeat(81)).is_err());
        // 失败不许改到原有名字
        assert_eq!(get(&conn, 1, saved.id).unwrap().unwrap().name, "正常名字");
        // 边界：80 个字符正好可以
        let name = "长".repeat(80);
        assert!(rename(&conn, 1, saved.id, &name).unwrap());
        assert_eq!(get(&conn, 1, saved.id).unwrap().unwrap().name, name);
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
            ordinal: 0,
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
            ordinal: 1,
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

    #[test]
    fn invalid_imported_issue_is_rejected_before_insertion() {
        let conn = db();
        conn.execute_batch("INSERT INTO assets(id, imported_at, updated_at) VALUES(1,1,1);").unwrap();
        let mut stack = DevelopStack::default();
        stack.curves.insert("rgb".into(), vec![[0.2, 0.0], [0.2, 1.0]]);
        assert!(import_issue(&conn, 1, "非法曲线", EditBase::Raw, 1, None, &stack).is_err());
        assert!(list(&conn, 1).unwrap().is_empty());
    }

}
