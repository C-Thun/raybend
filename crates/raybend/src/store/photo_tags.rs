//! 照片文字标签的来源、屏蔽与 AI 成功提交。SQL 投影是所有消费者的唯一有效集合。
//! 数字 asset_tags 继续只表达手动关联，不能拿 AI 投影作为手动编辑差量。

use super::{organization, tags};
use crate::{Error, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const TAG_STATE_VERSION: u32 = 1;
pub const MAX_AI_CONCEPTS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiOrigin {
    Local,
    Sidecar,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AiEvidence {
    pub concept_key: String,
    pub tag_name: String,
    /// 原始余弦相似度，不是置信概率。
    pub score: f32,
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AiResult {
    pub source_key: String,
    pub model_sha256: String,
    pub pipeline_sha256: String,
    pub generated_at: i64,
    pub origin: AiOrigin,
    pub valid: bool,
    pub evidence: Vec<AiEvidence>,
}

fn invalid(reason: &str) -> Error {
    Error::Unsupported(format!("AI 标签：{reason}"))
}
fn digest_valid(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn source_valid(s: &str) -> bool {
    !s.is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control)
}

impl AiResult {
    pub fn validate(&self) -> Result<()> {
        if !source_valid(&self.source_key)
            || !digest_valid(&self.model_sha256)
            || !digest_valid(&self.pipeline_sha256)
            || self.generated_at < 0
            || self.evidence.len() > MAX_AI_CONCEPTS
        {
            return Err(invalid("结果身份或类别数无效"));
        }
        let mut concepts = BTreeSet::new();
        for item in &self.evidence {
            if item.concept_key.is_empty()
                || item.concept_key.len() > 64
                || !item
                    .concept_key
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
                || !concepts.insert(&item.concept_key)
                || !item.score.is_finite()
                || !(-1.0..=1.0).contains(&item.score)
            {
                return Err(invalid("类别或分数无效"));
            }
            tags::clean_name(&item.tag_name)?;
        }
        Ok(())
    }
}

/// 自家 XMP 的版本化载荷；AI 证据和屏蔽不混入标准 subject。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TagState {
    pub version: u32,
    pub manual: Vec<String>,
    pub ai: Vec<String>,
    pub masks: Vec<String>,
    pub result: Option<AiResult>,
}

impl TagState {
    pub fn validate(&self) -> Result<()> {
        if self.version != TAG_STATE_VERSION {
            return Err(invalid("标签来源版本不受支持"));
        }
        for group in [&self.manual, &self.ai, &self.masks] {
            if group.len() > 4096 {
                return Err(invalid("标签数超过上限"));
            }
            let mut keys = BTreeSet::new();
            for name in group {
                let clean = tags::clean_name(name)?;
                if !keys.insert(tags::fold_name(&clean)) {
                    return Err(invalid("来源含重复文字"));
                }
            }
        }
        if let Some(result) = &self.result {
            result.validate()?;
            let accepted: BTreeSet<_> = result
                .evidence
                .iter()
                .filter(|e| e.accepted)
                .map(|e| tags::fold_name(e.tag_name.trim()))
                .collect();
            let actual: BTreeSet<_> = self.ai.iter().map(|s| tags::fold_name(s.trim())).collect();
            if accepted != actual {
                return Err(invalid("AI 来源与证据不一致"));
            }
        } else if !self.ai.is_empty() {
            return Err(invalid("AI 来源缺成功证据"));
        }
        Ok(())
    }
    #[must_use]
    pub fn has_content(&self) -> bool {
        !self.manual.is_empty() || !self.masks.is_empty() || self.result.is_some()
    }
}

pub fn effective_names(conn: &Connection, asset_id: i64) -> Result<Vec<String>> {
    // 老词典尚未对齐时拒绝输出，而不是悄悄丢掉未知 ID 对应的关键词。
    let unresolved: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM asset_tags a WHERE a.asset_id=?1 AND NOT EXISTS(SELECT 1 FROM tag_terms t WHERE t.legacy_tag_id=a.tag_id))", [asset_id], |r| r.get(0))?;
    if unresolved {
        return Err(invalid("手动标签词典未对齐，保留原有关键词"));
    }
    let mut stmt = conn.prepare("SELECT t.display_name FROM effective_photo_tags e JOIN tag_terms t ON t.tag_key=e.tag_key WHERE e.asset_id=?1 ORDER BY e.tag_key")?;
    Ok(stmt
        .query_map([asset_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

pub fn effective_ids(conn: &Connection, asset_id: i64) -> Result<Vec<i64>> {
    let mut stmt = conn
        .prepare("SELECT tag_id FROM effective_photo_tag_ids WHERE asset_id=?1 ORDER BY tag_id")?;
    Ok(stmt
        .query_map([asset_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

pub fn snapshot(conn: &Connection, asset_id: i64) -> Result<TagState> {
    let source_names = |source: &str| -> Result<Vec<String>> {
        let mut stmt = conn.prepare("SELECT t.display_name FROM asset_tag_sources s JOIN tag_terms t ON t.tag_key=s.tag_key WHERE s.asset_id=?1 AND s.source=?2 ORDER BY s.tag_key")?;
        Ok(stmt
            .query_map(params![asset_id, source], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    };
    let mut stmt = conn.prepare("SELECT t.display_name FROM asset_tag_masks m JOIN tag_terms t ON t.tag_key=m.tag_key WHERE m.asset_id=?1 ORDER BY m.tag_key")?;
    let masks = stmt
        .query_map([asset_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let row: Option<(String, bool)> = conn
        .query_row(
            "SELECT result_json, valid FROM photo_ai_results WHERE asset_id=?1",
            [asset_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let result = row
        .map(|(json, valid)| -> Result<AiResult> {
            let mut result: AiResult = serde_json::from_str(&json)?;
            result.valid = valid;
            result.validate()?;
            Ok(result)
        })
        .transpose()?;
    Ok(TagState {
        version: TAG_STATE_VERSION,
        manual: source_names("manual")?,
        ai: source_names("ai")?,
        masks,
        result,
    })
}

pub fn mask(conn: &Connection, asset_id: i64, name: &str, enabled: bool, now: i64) -> Result<bool> {
    let key = organization::ensure_term(conn, name, None)?;
    Ok(if enabled {
        conn.execute(
            "INSERT OR IGNORE INTO asset_tag_masks(asset_id,tag_key,masked_at) VALUES(?1,?2,?3)",
            params![asset_id, key, now],
        )?
    } else {
        conn.execute(
            "DELETE FROM asset_tag_masks WHERE asset_id=?1 AND tag_key=?2",
            params![asset_id, key],
        )?
    } > 0)
}

/// 同文字手动保留。上层先把数字词典适配到 tag_terms，再调用本函数。
pub fn retain_manual(conn: &Connection, asset_id: i64, name: &str, now: i64) -> Result<bool> {
    let tx = conn.unchecked_transaction()?;
    let key = organization::ensure_term(&tx, name, None)?;
    let id: Option<i64> = tx.query_row(
        "SELECT legacy_tag_id FROM tag_terms WHERE tag_key=?1",
        [&key],
        |r| r.get(0),
    )?;
    let mut changed = tx.execute(
        "DELETE FROM asset_tag_masks WHERE asset_id=?1 AND tag_key=?2",
        params![asset_id, key],
    )? > 0;
    changed |= tx.execute("INSERT OR IGNORE INTO asset_tag_sources(asset_id,tag_key,source,tagged_at) VALUES(?1,?2,'manual',?3)", params![asset_id,key,now])? > 0;
    if let Some(id) = id {
        changed |= tags::attach_tag(&tx, asset_id, id, now)?;
    }
    tx.commit()?;
    Ok(changed)
}

/// 只去掉手动来源，AI 仍有效时继续显示。
pub fn remove_manual(conn: &Connection, asset_id: i64, name: &str) -> Result<bool> {
    let tx = conn.unchecked_transaction()?;
    let key = tags::fold_name(&tags::clean_name(name)?);
    let mut changed = tx.execute("DELETE FROM asset_tags WHERE asset_id=?1 AND tag_id IN (SELECT legacy_tag_id FROM tag_terms WHERE tag_key=?2)", params![asset_id,key])? > 0;
    changed |= tx.execute(
        "DELETE FROM asset_tag_sources WHERE asset_id=?1 AND tag_key=?2 AND source='manual'",
        params![asset_id, key],
    )? > 0;
    tx.commit()?;
    Ok(changed)
}

/// 冻结照片身份和原始输入身份；较新的 attempt 自动撤销旧提交资格。
pub fn begin_attempt(
    conn: &Connection,
    asset_id: i64,
    photo_uid: &str,
    source_key: &str,
) -> Result<String> {
    if !source_valid(source_key) {
        return Err(invalid("输入身份无效"));
    }
    let token = super::ids::new_repository_id()?;
    let n = conn.execute("INSERT INTO photo_ai_attempts(asset_id,token,photo_uid,source_key)
        SELECT id,?2,organization_uid,?4 FROM assets WHERE id=?1 AND organization_uid=?3
        ON CONFLICT(asset_id) DO UPDATE SET token=excluded.token, photo_uid=excluded.photo_uid, source_key=excluded.source_key",
        params![asset_id,token,photo_uid,source_key])?;
    if n == 0 {
        return Err(invalid("照片身份已变化"));
    }
    Ok(token)
}

pub fn cancel_attempt(conn: &Connection, asset_id: i64, token: &str) -> Result<bool> {
    Ok(conn.execute(
        "DELETE FROM photo_ai_attempts WHERE asset_id=?1 AND token=?2",
        params![asset_id, token],
    )? > 0)
}

/// 调用方提交前重读原始文件，传入此刻的 source key；取消或新 attempt 会令返回 false。
pub fn commit_ai(
    conn: &Connection,
    asset_id: i64,
    token: &str,
    current_source: &str,
    result: &AiResult,
    now: i64,
) -> Result<bool> {
    result.validate()?;
    if result.origin != AiOrigin::Local || !result.valid {
        return Err(invalid("本地提交结果无效"));
    }
    let transaction = if conn.is_autocommit() {
        Some(conn.unchecked_transaction()?)
    } else {
        None
    };
    let tx = transaction.as_deref().unwrap_or(conn);
    let allowed: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM photo_ai_attempts p JOIN assets a ON a.id=p.asset_id
        WHERE p.asset_id=?1 AND p.token=?2 AND p.photo_uid=a.organization_uid AND p.source_key=?3 AND p.source_key=?4)",
        params![asset_id,token,current_source,result.source_key], |r| r.get(0))?;
    if !allowed {
        return Ok(false);
    }
    replace_ai(tx, asset_id, result, now)?;
    tx.execute(
        "DELETE FROM photo_ai_attempts WHERE asset_id=?1 AND token=?2",
        params![asset_id, token],
    )?;
    if let Some(tx) = transaction {
        tx.commit()?;
    }
    Ok(true)
}

fn replace_ai(conn: &Connection, asset_id: i64, result: &AiResult, now: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM asset_tag_sources WHERE asset_id=?1 AND source='ai'",
        [asset_id],
    )?;
    for item in result.evidence.iter().filter(|e| e.accepted) {
        let key = organization::ensure_term(conn, &item.tag_name, None)?;
        conn.execute("INSERT OR IGNORE INTO asset_tag_sources(asset_id,tag_key,source,tagged_at) VALUES(?1,?2,'ai',?3)", params![asset_id,key,now])?;
    }
    conn.execute("INSERT INTO photo_ai_results(asset_id,source_key,result_json,valid) VALUES(?1,?2,?3,?4)
        ON CONFLICT(asset_id) DO UPDATE SET source_key=excluded.source_key,result_json=excluded.result_json,valid=excluded.valid",
        params![asset_id,result.source_key,serde_json::to_string(result)?,result.valid])?;
    Ok(())
}

pub fn invalidate_source(conn: &Connection, asset_id: i64, current_source: &str) -> Result<bool> {
    fn write(conn: &Connection, asset_id: i64, key: &str) -> Result<bool> {
        let changed = conn.execute(
            "UPDATE photo_ai_results SET valid=0 WHERE asset_id=?1 AND source_key<>?2 AND valid=1",
            params![asset_id, key],
        )? > 0;
        conn.execute(
            "DELETE FROM photo_ai_attempts WHERE asset_id=?1 AND source_key<>?2",
            params![asset_id, key],
        )?;
        Ok(changed)
    }
    if conn.is_autocommit() {
        let tx = conn.unchecked_transaction()?;
        let changed = write(&tx, asset_id, current_source)?;
        tx.commit()?;
        Ok(changed)
    } else {
        write(conn, asset_id, current_source)
    }
}

/// 人工纠错只记录本次改变的两项，不复制或还原 AI 结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManualState {
    pub manual: bool,
    pub masked: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Edit {
    pub name: String,
    pub manual: Option<bool>,
    pub masked: Option<bool>,
}
pub fn manual_state(conn: &Connection, asset_id: i64, name: &str) -> Result<ManualState> {
    let key = tags::fold_name(&tags::clean_name(name)?);
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM asset_tag_sources WHERE asset_id=?1 AND tag_key=?2 AND source='manual'), EXISTS(SELECT 1 FROM asset_tag_masks WHERE asset_id=?1 AND tag_key=?2)",params![asset_id,key],|r|Ok(ManualState{manual:r.get(0)?,masked:r.get(1)?}))?)
}
/// 调用方的写事务负责原子性。保留操作必须同时明确 mask 状态以支持撤销。
pub fn write_manual_state(
    conn: &Connection,
    asset_id: i64,
    name: &str,
    state: ManualState,
    now: i64,
) -> Result<()> {
    let key = organization::ensure_term(conn, name, None)?;
    let id: Option<i64> = conn.query_row(
        "SELECT legacy_tag_id FROM tag_terms WHERE tag_key=?1",
        [&key],
        |r| r.get(0),
    )?;
    if state.manual {
        conn.execute("INSERT OR IGNORE INTO asset_tag_sources(asset_id,tag_key,source,tagged_at) VALUES(?1,?2,'manual',?3)",params![asset_id,key,now])?;
        if let Some(id) = id {
            tags::attach_tag(conn, asset_id, id, now)?;
        }
    } else {
        if let Some(id) = id {
            conn.execute(
                "DELETE FROM asset_tags WHERE asset_id=?1 AND tag_id=?2",
                params![asset_id, id],
            )?;
        }
        conn.execute(
            "DELETE FROM asset_tag_sources WHERE asset_id=?1 AND tag_key=?2 AND source='manual'",
            params![asset_id, key],
        )?;
    }
    mask(conn, asset_id, name, state.masked, now)?;
    Ok(())
}
pub fn edits(
    conn: &Connection,
    ids: &[i64],
    edits: &[Edit],
    label: &str,
) -> Result<super::marking::ChangeSet> {
    if edits.len() > 256
        || ids.len() != 1
            && edits
                .iter()
                .any(|e| e.manual == Some(false) || e.masked.is_some())
    {
        return Err(invalid("批量标签只允许添加"));
    }
    let mut keys = BTreeSet::new();
    let mut ops = Vec::new();
    for edit in edits {
        let name = tags::clean_name(&edit.name)?;
        if !keys.insert(tags::fold_name(&name)) || edit.manual.is_none() && edit.masked.is_none() {
            return Err(invalid("标签草稿重复或冲突"));
        }
        for id in ids.iter().copied().collect::<BTreeSet<_>>() {
            let before = manual_state(conn, id, &name)?;
            let after = ManualState {
                manual: edit.manual.unwrap_or(before.manual),
                masked: edit.masked.unwrap_or(if edit.manual == Some(true) {
                    false
                } else {
                    before.masked
                }),
            };
            if before != after {
                ops.push(super::marking::Op::PhotoTag {
                    asset_id: id,
                    name: name.clone(),
                    before,
                    after,
                });
            }
        }
    }
    Ok(super::marking::ChangeSet::new(label, ops))
}

/// 仅在新资产采纳事务内调用；扩展存在时 subject 不再当作手动来源。
pub fn restore_sidecar(conn: &Connection, asset_id: i64, state: &TagState, now: i64) -> Result<()> {
    state.validate()?;
    for name in &state.manual {
        let key = organization::ensure_term(conn, name, None)?;
        conn.execute("INSERT OR IGNORE INTO asset_tag_sources(asset_id,tag_key,source,tagged_at) VALUES(?1,?2,'manual',?3)", params![asset_id,key,now])?;
        let id: Option<i64> = conn.query_row(
            "SELECT legacy_tag_id FROM tag_terms WHERE tag_key=?1",
            [&key],
            |r| r.get(0),
        )?;
        if let Some(id) = id {
            tags::attach_tag(conn, asset_id, id, now)?;
        }
    }
    if let Some(result) = &state.result {
        let mut restored = result.clone();
        restored.origin = AiOrigin::Sidecar;
        // Sidecar 是外部断言；采纳后绑定当前库中的原片，不能带入另一 catalog 的行 ID。
        if let Some(input) = crate::ai::source::selected(conn, asset_id)? {
            restored.source_key = input.source_key;
        } else {
            restored.valid = false;
        }
        replace_ai(conn, asset_id, &restored, now)?;
    }
    for name in &state.masks {
        mask(conn, asset_id, name, true, now)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{
        migration::{self, DbKind},
        query::{self, Query, Scope},
    };
    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::store::pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::Catalog, migration::Backups::none(), 1).unwrap();
        conn.execute_batch(
            "INSERT INTO assets(id,organization_uid,imported_at,updated_at) VALUES(1,'uid',1,1);",
        )
        .unwrap();
        conn
    }
    fn result(names: &[&str]) -> AiResult {
        AiResult {
            source_key: "original-v1".into(),
            model_sha256: "a".repeat(64),
            pipeline_sha256: "b".repeat(64),
            generated_at: 2,
            origin: AiOrigin::Local,
            valid: true,
            evidence: names
                .iter()
                .enumerate()
                .map(|(i, name)| AiEvidence {
                    concept_key: format!("class_{i}"),
                    tag_name: (*name).into(),
                    score: 0.3,
                    accepted: true,
                })
                .collect(),
        }
    }
    fn commit(conn: &Connection, names: &[&str]) {
        let token = begin_attempt(conn, 1, "uid", "original-v1").unwrap();
        assert!(commit_ai(conn, 1, &token, "original-v1", &result(names), 2).unwrap());
    }
    #[test]
    fn sources_merge_masks_cover_every_source_and_retaining_clears_mask() {
        let conn = db();
        organization::ensure_term(&conn, "鸟", Some(7)).unwrap();
        tags::attach_tag(&conn, 1, 7, 1).unwrap();
        commit(&conn, &["鸟", "海"]);
        assert_eq!(effective_names(&conn, 1).unwrap(), ["海", "鸟"]);
        mask(&conn, 1, " 鸟 ", true, 3).unwrap();
        assert_eq!(effective_names(&conn, 1).unwrap(), ["海"]);
        assert_eq!(effective_ids(&conn, 1).unwrap(), Vec::<i64>::new());
        assert!(snapshot(&conn, 1).unwrap().ai.contains(&"鸟".into()));
        retain_manual(&conn, 1, "鸟", 4).unwrap();
        commit(&conn, &[]);
        assert_eq!(effective_names(&conn, 1).unwrap(), ["鸟"]);
        assert!(snapshot(&conn, 1).unwrap().result.is_some());
        assert!(snapshot(&conn, 1).unwrap().ai.is_empty());
    }
    #[test]
    fn applied_counts_follow_effective_names_and_locked_edits_have_no_delta() {
        let conn = db();
        organization::ensure_term(&conn, "鸟", Some(7)).unwrap();
        commit(&conn, &["鸟"]);
        let retain = edits(
            &conn,
            &[1],
            &[Edit {
                name: "鸟".into(),
                manual: Some(true),
                masked: None,
            }],
            "保留",
        )
        .unwrap();
        assert!(
            super::super::marking::apply(&conn, &retain)
                .unwrap()
                .tag_delta
                .is_empty()
        );
        let block = edits(
            &conn,
            &[1],
            &[Edit {
                name: "鸟".into(),
                manual: None,
                masked: Some(true),
            }],
            "禁止",
        )
        .unwrap();
        assert_eq!(
            super::super::marking::apply(&conn, &block)
                .unwrap()
                .tag_delta
                .removed,
            [7]
        );
        assert_eq!(
            super::super::marking::apply(&conn, &block.invert())
                .unwrap()
                .tag_delta
                .added,
            [7]
        );
        conn.execute("UPDATE assets SET lock_level=2 WHERE id=1", [])
            .unwrap();
        let skipped = super::super::marking::apply(&conn, &block).unwrap();
        assert!(skipped.tag_delta.is_empty());
        assert_eq!(skipped.skipped_locked, [1]);
    }
    #[test]
    fn removal_only_drops_manual_and_unmask_never_creates_a_source() {
        let conn = db();
        retain_manual(&conn, 1, "森林", 1).unwrap();
        commit(&conn, &["森林"]);
        assert!(remove_manual(&conn, 1, "森林").unwrap());
        assert_eq!(effective_names(&conn, 1).unwrap(), ["森林"]);
        mask(&conn, 1, "夜景", true, 1).unwrap();
        mask(&conn, 1, "夜景", false, 1).unwrap();
        assert_eq!(effective_names(&conn, 1).unwrap(), ["森林"]);
    }
    #[test]
    fn cancelling_racing_attempts_changed_source_and_uid_reuse_reject_commit() {
        let conn = db();
        commit(&conn, &["海"]);
        let first = begin_attempt(&conn, 1, "uid", "original-v1").unwrap();
        let second = begin_attempt(&conn, 1, "uid", "original-v1").unwrap();
        assert!(!commit_ai(&conn, 1, &first, "original-v1", &result(&["山"]), 3).unwrap());
        cancel_attempt(&conn, 1, &second).unwrap();
        assert!(!commit_ai(&conn, 1, &second, "original-v1", &result(&[]), 3).unwrap());
        let third = begin_attempt(&conn, 1, "uid", "original-v1").unwrap();
        assert!(!commit_ai(&conn, 1, &third, "original-v2", &result(&[]), 3).unwrap());
        conn.execute(
            "UPDATE assets SET organization_uid='replacement' WHERE id=1",
            [],
        )
        .unwrap();
        assert!(!commit_ai(&conn, 1, &third, "original-v1", &result(&[]), 3).unwrap());
        assert_eq!(effective_names(&conn, 1).unwrap(), ["海"]);
        assert!(invalidate_source(&conn, 1, "original-v2").unwrap());
        assert!(effective_names(&conn, 1).unwrap().is_empty());
        assert!(!snapshot(&conn, 1).unwrap().result.unwrap().valid);
    }
    #[test]
    fn invalid_result_or_failed_transaction_preserves_last_success() {
        let conn = db();
        commit(&conn, &["海"]);
        let token = begin_attempt(&conn, 1, "uid", "original-v1").unwrap();
        let mut bad = result(&["山"]);
        bad.evidence[0].score = f32::NAN;
        assert!(commit_ai(&conn, 1, &token, "original-v1", &bad, 3).is_err());
        conn.execute_batch("CREATE TRIGGER fail_ai BEFORE UPDATE ON photo_ai_results BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
        assert!(commit_ai(&conn, 1, &token, "original-v1", &result(&["山"]), 3).is_err());
        assert_eq!(effective_names(&conn, 1).unwrap(), ["海"]);
    }
    #[test]
    fn normalization_limits_and_invalid_sidecar_sources_are_checked() {
        let conn = db();
        commit(&conn, &["Été", "旅行"]);
        mask(&conn, 1, " E\u{301}TE\u{301} ", true, 2).unwrap();
        assert_eq!(effective_names(&conn, 1).unwrap(), ["旅行"]);
        assert!(mask(&conn, 1, " ", true, 2).is_err());
        assert!(mask(&conn, 1, &"中".repeat(65), true, 2).is_err());
        let mut bad = result(&[]);
        bad.model_sha256 = "A".repeat(64);
        assert!(bad.validate().is_err());
        let mut state = snapshot(&conn, 1).unwrap();
        state.ai.push("车".into());
        assert!(state.validate().is_err());
        let mut bad = result(&vec!["鸟"; 33]);
        assert!(bad.validate().is_err());
        bad = result(&["鸟"]);
        bad.evidence[0].score = f32::INFINITY;
        assert!(bad.validate().is_err());
        bad = result(&["鸟"]);
        bad.source_key = "x".repeat(1025);
        assert!(bad.validate().is_err());
        assert!(begin_attempt(&conn, 9, "uid", "original-v1").is_err());
    }
    #[test]
    fn auto_bucket_events_share_masks_and_keep_the_existing_accumulating_semantics() {
        let conn = db();
        let mut app = Connection::open_in_memory().unwrap();
        migration::apply(&mut app, DbKind::App, migration::Backups::none(), 1).unwrap();
        let bucket = organization::create_bucket(
            &app,
            "海边",
            organization::RuleSet {
                groups: vec![organization::RuleGroup {
                    repository_ids: vec!["库".into()],
                    tag_keys: vec!["海".into()],
                    ..Default::default()
                }],
            },
            1,
        )
        .unwrap();
        mask(&conn, 1, "海", true, 1).unwrap();
        commit(&conn, &["海"]);
        organization::reconcile_events_batch(&app, &conn, "库", 2).unwrap();
        assert!(
            organization::member_refs(&app, bucket.id, "库")
                .unwrap()
                .is_empty()
        );
        mask(&conn, 1, "海", false, 3).unwrap();
        organization::reconcile_events_batch(&app, &conn, "库", 3).unwrap();
        assert_eq!(
            organization::member_refs(&app, bucket.id, "库")
                .unwrap()
                .len(),
            1
        );
        mask(&conn, 1, "海", true, 4).unwrap();
        organization::reconcile_events_batch(&app, &conn, "库", 4).unwrap();
        assert_eq!(
            organization::member_refs(&app, bucket.id, "库")
                .unwrap()
                .len(),
            1,
            "自动桶保留已归集照片"
        );
        let photo = organization::photo_ref(&conn, "库", 1).unwrap().unwrap();
        organization::remove_member(&app, bucket.id, &photo, 5).unwrap();
        mask(&conn, 1, "海", false, 6).unwrap();
        organization::reconcile_events_batch(&app, &conn, "库", 6).unwrap();
        assert!(
            organization::member_refs(&app, bucket.id, "库")
                .unwrap()
                .is_empty(),
            "排除项不能被新 AI 事件复活"
        );
    }
    #[test]
    fn sidecar_rebases_to_current_catalog_source_and_missing_original_is_invalid() {
        let conn = db();
        commit(&conn, &["海"]);
        let state = snapshot(&conn, 1).unwrap();
        let other = db();
        other.execute_batch("INSERT INTO asset_files(asset_id,rel_path,rel_path_folded,role,ext,size_bytes,mtime_ms,created_at,updated_at) VALUES(1,'photos/新库.jpg','photos/新库.jpg','bitmap','jpg',100,1,1,1)").unwrap();
        restore_sidecar(&other, 1, &state, 2).unwrap();
        let restored = snapshot(&other, 1).unwrap().result.unwrap();
        assert_eq!(restored.origin, AiOrigin::Sidecar);
        assert_eq!(
            restored.source_key,
            crate::ai::source::selected(&other, 1)
                .unwrap()
                .unwrap()
                .source_key
        );
        assert!(restored.valid);
        assert_eq!(crate::ai::source::reconcile(&other, &[1]).unwrap(), 0);
        let missing = db();
        restore_sidecar(&missing, 1, &state, 2).unwrap();
        assert!(!snapshot(&missing, 1).unwrap().result.unwrap().valid);
        assert!(effective_names(&missing, 1).unwrap().is_empty());
    }
    #[test]
    fn navigation_numeric_filter_and_directory_tags_agree_without_inheritance() {
        let conn = db();
        organization::ensure_term(&conn, "海", Some(7)).unwrap();
        commit(&conn, &["海"]);
        organization::set_directory_tag(&conn, "photos/trip", "森林", true, 2).unwrap();
        assert_eq!(
            query::count(&conn, &Query::new(Scope::TagKey { key: "海".into() })).unwrap(),
            1
        );
        let mut q = Query::new(Scope::Repository);
        q.filter.tags = vec![7];
        assert_eq!(query::count(&conn, &q).unwrap(), 1);
        mask(&conn, 1, "海", true, 3).unwrap();
        assert_eq!(query::count(&conn, &q).unwrap(), 0);
        q.filter.tags.clear();
        q.filter.tag_keys = vec!["森林".into()];
        assert_eq!(query::count(&conn, &q).unwrap(), 0);
        assert_eq!(
            organization::directories_for_tag(&conn, "森林", 10, 0).unwrap(),
            ["photos/trip"]
        );
    }
    #[test]
    fn sidecar_origin_restores_without_promoting_ai_to_manual() {
        let conn = db();
        retain_manual(&conn, 1, "旅行", 1).unwrap();
        commit(&conn, &["海"]);
        mask(&conn, 1, "海", true, 3).unwrap();
        let state = snapshot(&conn, 1).unwrap();
        state.validate().unwrap();
        let other = db();
        restore_sidecar(&other, 1, &state, 4).unwrap();
        assert_eq!(snapshot(&other, 1).unwrap().manual, ["旅行"]);
        assert_eq!(
            snapshot(&other, 1).unwrap().result.unwrap().origin,
            AiOrigin::Sidecar
        );
        assert_eq!(effective_names(&other, 1).unwrap(), ["旅行"]);
    }
    #[test]
    fn human_undo_preserves_concurrent_ai_and_restores_mask() {
        let conn = db();
        mask(&conn, 1, "海", true, 1).unwrap();
        let change = edits(
            &conn,
            &[1],
            &[Edit {
                name: "海".into(),
                manual: Some(true),
                masked: None,
            }],
            "标签",
        )
        .unwrap();
        super::super::marking::apply(&conn, &change).unwrap();
        commit(&conn, &["海", "鸟"]);
        super::super::marking::apply(&conn, &change.invert()).unwrap();
        assert_eq!(effective_names(&conn, 1).unwrap(), ["鸟"]);
        assert!(snapshot(&conn, 1).unwrap().manual.is_empty());
        assert_eq!(snapshot(&conn, 1).unwrap().ai.len(), 2);
        assert!(
            edits(
                &conn,
                &[1, 2],
                &[Edit {
                    name: "海".into(),
                    manual: None,
                    masked: Some(true)
                }],
                "标签"
            )
            .is_err()
        );
        assert!(
            edits(
                &conn,
                &[1],
                &[Edit {
                    name: "海".into(),
                    manual: Some(true),
                    masked: Some(true)
                }],
                "标签"
            )
            .is_ok()
        );
    }
    #[test]
    fn legacy_missing_dictionary_refuses_lossy_keywords_and_keyset_does_not_shift() {
        let conn = db();
        conn.execute(
            "INSERT INTO asset_tags(asset_id,tag_id,tagged_at) VALUES(1,888,1)",
            [],
        )
        .unwrap();
        assert!(effective_names(&conn, 1).is_err());
        conn.execute_batch("INSERT INTO assets(id,organization_uid,imported_at,updated_at) VALUES(3,'uid3',1,1),(6,'uid6',1,1)").unwrap();
        let bound = query::identity_upper_bound(&conn).unwrap();
        let q = Query::new(Scope::Repository);
        assert_eq!(
            query::identity_page(&conn, &q, 0, bound, 1).unwrap()[0].0,
            1
        );
        conn.execute_batch("DELETE FROM assets WHERE id=1;INSERT INTO assets(id,organization_uid,imported_at,updated_at) VALUES(7,'uid7',1,1)").unwrap();
        assert_eq!(
            query::identity_page(&conn, &q, 1, bound, 500)
                .unwrap()
                .iter()
                .map(|p| p.0)
                .collect::<Vec<_>>(),
            [3, 6]
        );
        assert!(query::identity_page(&conn, &q, -1, bound, 1).is_err());
        assert!(query::identity_page(&conn, &q, 0, bound, 501).is_err());
        assert!(
            query::identity_page(&conn, &q, 0, bound, 0)
                .unwrap()
                .is_empty()
        );
    }
}
