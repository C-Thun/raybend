//! 相片整理的数据契约。标签文字与目录绑定属于 catalog，桶与成员属于 app。
//!
//! 所有写函数由现有单写者调用；跨库查询只接收已挂载库的租约。

use std::collections::BTreeSet;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use super::{query, tags};

pub const MAX_RULE_GROUPS: usize = 16;
pub const MAX_RULE_VALUES: usize = 64;
pub const MAX_BUCKET_NAME_CHARS: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuleGroup {
    pub repository_ids: Vec<String>,
    #[serde(default)]
    pub min_rating: Option<u8>,
    #[serde(default)]
    pub colors: Vec<String>,
    #[serde(default)]
    pub likes: Vec<String>,
    #[serde(default)]
    pub locks: Vec<u8>,
    #[serde(default)]
    pub tag_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuleSet {
    pub groups: Vec<RuleGroup>,
}

fn unique_sorted<T: Ord>(items: &mut Vec<T>) {
    items.sort();
    items.dedup();
}

/// 规则保存前规范化。库范围总是独立的 AND 条件，零组表示普通桶。
pub fn normalize_rules(mut rules: RuleSet) -> Result<RuleSet> {
    if rules.groups.len() > MAX_RULE_GROUPS {
        return Err(Error::Unsupported(format!("条件链最多 {MAX_RULE_GROUPS} 组")));
    }
    for group in &mut rules.groups {
        group.repository_ids = group.repository_ids.iter().map(|id| id.trim().to_string()).collect();
        if group.repository_ids.iter().any(String::is_empty) || group.repository_ids.is_empty() {
            return Err(Error::Unsupported("每组必须明确选择库".into()));
        }
        unique_sorted(&mut group.repository_ids);
        if group.min_rating.is_some_and(|rating| rating > 5) {
            return Err(Error::Unsupported("星级必须在 0–5 之间".into()));
        }
        if group.colors.iter().any(|color| !matches!(color.as_str(), "red" | "yellow" | "green" | "cyan" | "blue" | "purple" | "none")) {
            return Err(Error::Unsupported("规则包含未知标色".into()));
        }
        if group.likes.iter().any(|like| !matches!(like.as_str(), "like" | "dislike" | "none")) {
            return Err(Error::Unsupported("规则包含未知喜欢状态".into()));
        }
        if group.locks.iter().any(|lock| *lock > 2) {
            return Err(Error::Unsupported("锁级别必须在 0–2 之间".into()));
        }
        unique_sorted(&mut group.colors);
        unique_sorted(&mut group.likes);
        unique_sorted(&mut group.locks);
        group.tag_keys = group.tag_keys.iter().map(|key| {
            tags::clean_name(key).map(|name| tags::fold_name(&name))
        }).collect::<Result<Vec<_>>>()?;
        unique_sorted(&mut group.tag_keys);
        if group.repository_ids.len() > MAX_RULE_VALUES || group.tag_keys.len() > MAX_RULE_VALUES {
            return Err(Error::Unsupported("每类规则值最多 64 个".into()));
        }
    }
    rules.groups.sort_by_key(|group| serde_json::to_string(group).unwrap_or_default());
    rules.groups.dedup();
    Ok(rules)
}

/// 共用既有筛选查询编译器；组内 AND，组间 OR 由调用方逐组并集。
pub fn filter_for_group(group: &RuleGroup) -> query::Filter {
    query::Filter {
        min_rating: group.min_rating,
        colors: group.colors.clone(),
        likes: group.likes.clone(),
        locks: group.locks.clone(),
        tag_keys: group.tag_keys.clone(),
        combinator: query::Combinator::And,
        ..query::Filter::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoRef {
    pub repository_id: String,
    pub photo_uid: String,
    pub asset_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    pub id: i64,
    pub name: String,
    pub pinned: bool,
    pub paused: bool,
    pub rules: RuleSet,
    pub rule_revision: i64,
    pub member_count: i64,
}

fn bucket_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Bucket> {
    let json: Option<String> = row.get(4)?;
    let rules = json.and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
    Ok(Bucket {
        id: row.get(0)?,
        name: row.get(1)?,
        pinned: row.get::<_, i64>(2)? != 0,
        paused: row.get::<_, i64>(3)? != 0,
        rules,
        rule_revision: row.get(5)?,
        member_count: row.get(6)?,
    })
}

pub fn list_buckets(conn: &Connection) -> Result<Vec<Bucket>> {
    let mut stmt = conn.prepare(
        "SELECT b.id, b.name, b.pinned, b.paused, b.rule_json, b.rule_revision,
                (SELECT count(*) FROM photo_bucket_members m WHERE m.bucket_id = b.id)
         FROM photo_buckets b
         ORDER BY b.pinned DESC, (b.rule_json IS NOT NULL) DESC, b.name_folded, b.id"
    )?;
    let rows = stmt.query_map([], bucket_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn bucket_by_id(conn: &Connection, id: i64) -> Result<Option<Bucket>> {
    Ok(conn.query_row(
        "SELECT b.id, b.name, b.pinned, b.paused, b.rule_json, b.rule_revision,
                (SELECT count(*) FROM photo_bucket_members m WHERE m.bucket_id = b.id)
         FROM photo_buckets b WHERE b.id = ?1",
        [id], bucket_row,
    ).optional()?)
}

fn clean_bucket_name(input: &str) -> Result<String> {
    let name = input.trim();
    if name.is_empty() || name.chars().count() > MAX_BUCKET_NAME_CHARS {
        return Err(Error::Unsupported("相片桶名称不能为空且不得超过 80 个字符".into()));
    }
    Ok(name.into())
}

/// 创建桶，规则与待补扫范围同时提交。
pub fn create_bucket(conn: &Connection, name: &str, rules: RuleSet, now: i64) -> Result<Bucket> {
    let name = clean_bucket_name(name)?;
    let rules = normalize_rules(rules)?;
    let text = (!rules.groups.is_empty()).then(|| serde_json::to_string(&rules).expect("规则可序列化"));
    let transaction = conn.unchecked_transaction()?;
    transaction.execute(
        "INSERT INTO photo_buckets(name, name_folded, rule_json, rule_revision, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![name, tags::fold_name(&name), text, i64::from(!rules.groups.is_empty()), now],
    )?;
    let id = transaction.last_insert_rowid();
    if !rules.groups.is_empty() {
        let repos: BTreeSet<&str> = rules.groups.iter().flat_map(|group| group.repository_ids.iter().map(String::as_str)).collect();
        for repo in repos {
            transaction.execute(
                "INSERT INTO photo_bucket_scans(bucket_id, repository_id, rule_revision) VALUES (?1, ?2, 1)",
                params![id, repo],
            )?;
        }
    }
    transaction.commit()?;
    bucket_by_id(conn, id)?.ok_or_else(|| Error::Unsupported("相片桶创建后无法读取".into()))
}

pub fn rename_bucket(conn: &Connection, id: i64, name: &str, now: i64) -> Result<bool> {
    let name = clean_bucket_name(name)?;
    Ok(conn.execute(
        "UPDATE photo_buckets SET name = ?1, name_folded = ?2, updated_at = ?3 WHERE id = ?4",
        params![name, tags::fold_name(&name), now, id],
    )? > 0)
}

pub fn set_bucket_pinned(conn: &Connection, id: i64, pinned: bool, now: i64) -> Result<bool> {
    Ok(conn.execute("UPDATE photo_buckets SET pinned = ?1, updated_at = ?2 WHERE id = ?3", params![pinned, now, id])? > 0)
}

pub fn set_bucket_paused(conn: &Connection, id: i64, paused: bool, now: i64) -> Result<bool> {
    let transaction = conn.unchecked_transaction()?;
    let changed = transaction.execute(
        "UPDATE photo_buckets SET paused = ?1, updated_at = ?2 WHERE id = ?3",
        params![paused, now, id],
    )? > 0;
    if changed && !paused
        && let Some(bucket) = bucket_by_id(&transaction, id)? {
            let repos: BTreeSet<&str> = bucket.rules.groups.iter()
                .flat_map(|group| group.repository_ids.iter().map(String::as_str)).collect();
            for repo in repos {
                transaction.execute(
                    "INSERT INTO photo_bucket_scans(bucket_id, repository_id, rule_revision, after_asset_id)
                     VALUES (?1, ?2, ?3, 0)
                     ON CONFLICT(bucket_id, repository_id) DO UPDATE SET
                     rule_revision = excluded.rule_revision, after_asset_id = 0",
                    params![id, repo, bucket.rule_revision],
                )?;
            }
        }
    transaction.commit()?;
    Ok(changed)
}

pub fn delete_bucket(conn: &Connection, id: i64) -> Result<bool> {
    Ok(conn.execute("DELETE FROM photo_buckets WHERE id = ?1 AND pinned = 0", [id])? > 0)
}

pub fn set_bucket_rules(conn: &Connection, id: i64, name: &str, rules: RuleSet, now: i64) -> Result<Option<Bucket>> {
    let name = clean_bucket_name(name)?;
    let rules = normalize_rules(rules)?;
    let text = (!rules.groups.is_empty()).then(|| serde_json::to_string(&rules).expect("规则可序列化"));
    let transaction = conn.unchecked_transaction()?;
    let changed = transaction.execute(
        "UPDATE photo_buckets SET name = ?1, name_folded = ?2, rule_json = ?3,
         rule_revision = rule_revision + 1, updated_at = ?4 WHERE id = ?5",
        params![name, tags::fold_name(&name), text, now, id],
    )?;
    if changed == 0 { return Ok(None); }
    transaction.execute("DELETE FROM photo_bucket_scans WHERE bucket_id = ?1", [id])?;
    let revision: i64 = transaction.query_row("SELECT rule_revision FROM photo_buckets WHERE id = ?1", [id], |r| r.get(0))?;
    let repos: BTreeSet<&str> = rules.groups.iter().flat_map(|group| group.repository_ids.iter().map(String::as_str)).collect();
    for repo in repos {
        transaction.execute(
            "INSERT INTO photo_bucket_scans(bucket_id, repository_id, rule_revision) VALUES (?1, ?2, ?3)",
            params![id, repo, revision],
        )?;
    }
    transaction.commit()?;
    bucket_by_id(conn, id)
}

fn validate_refs(photos: &[PhotoRef]) -> Result<()> {
    if photos.iter().any(|p| p.repository_id.is_empty() || p.photo_uid.is_empty() || p.asset_id <= 0) {
        return Err(Error::Unsupported("相片引用无效".into()));
    }
    Ok(())
}

/// 手动加入自动桶会清除此前的排除；重复加入是幂等的。
pub fn add_members(conn: &Connection, bucket_id: i64, photos: &[PhotoRef], now: i64) -> Result<usize> {
    validate_refs(photos)?;
    let transaction = conn.unchecked_transaction()?;
    let mut added = 0;
    for photo in photos {
        transaction.execute(
            "DELETE FROM photo_bucket_exclusions WHERE bucket_id = ?1 AND repository_id = ?2 AND photo_uid = ?3",
            params![bucket_id, photo.repository_id, photo.photo_uid],
        )?;
        added += transaction.execute(
            "INSERT OR IGNORE INTO photo_bucket_members(bucket_id, repository_id, photo_uid, asset_id, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![bucket_id, photo.repository_id, photo.photo_uid, photo.asset_id, now],
        )?;
    }
    transaction.commit()?;
    Ok(added)
}

/// 自动加入先查 revision 与排除，再在同一事务里提交，旧规则结果不能越过新规则。
pub fn add_auto_member(conn: &Connection, bucket_id: i64, revision: i64, photo: &PhotoRef, now: i64) -> Result<bool> {
    validate_refs(std::slice::from_ref(photo))?;
    Ok(conn.execute(
        "INSERT OR IGNORE INTO photo_bucket_members(bucket_id, repository_id, photo_uid, asset_id, added_at)
         SELECT id, ?3, ?4, ?5, ?6 FROM photo_buckets
         WHERE id = ?1 AND rule_revision = ?2 AND paused = 0 AND rule_json IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM photo_bucket_exclusions e
                           WHERE e.bucket_id = ?1 AND e.repository_id = ?3 AND e.photo_uid = ?4)",
        params![bucket_id, revision, photo.repository_id, photo.photo_uid, photo.asset_id, now],
    )? > 0)
}

pub fn remove_member(conn: &Connection, bucket_id: i64, photo: &PhotoRef, now: i64) -> Result<bool> {
    validate_refs(std::slice::from_ref(photo))?;
    let transaction = conn.unchecked_transaction()?;
    let removed = transaction.execute(
        "DELETE FROM photo_bucket_members WHERE bucket_id = ?1 AND repository_id = ?2 AND photo_uid = ?3",
        params![bucket_id, photo.repository_id, photo.photo_uid],
    )? > 0;
    if removed {
        transaction.execute(
            "INSERT OR IGNORE INTO photo_bucket_exclusions(bucket_id, repository_id, photo_uid, excluded_at)
             SELECT id, ?2, ?3, ?4 FROM photo_buckets WHERE id = ?1 AND rule_json IS NOT NULL",
            params![bucket_id, photo.repository_id, photo.photo_uid, now],
        )?;
    }
    transaction.commit()?;
    Ok(removed)
}

pub fn member_refs(conn: &Connection, bucket_id: i64, repository_id: &str) -> Result<Vec<PhotoRef>> {
    let mut stmt = conn.prepare(
        "SELECT repository_id, photo_uid, asset_id FROM photo_bucket_members
         WHERE bucket_id = ?1 AND repository_id = ?2 ORDER BY asset_id"
    )?;
    let rows = stmt.query_map(params![bucket_id, repository_id], |row| Ok(PhotoRef {
        repository_id: row.get(0)?, photo_uid: row.get(1)?, asset_id: row.get(2)?,
    }))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// catalog 中仍然指向同一 UID 的成员才能出现在 tiles；离线/删除不清 app 记录。
pub fn valid_member_refs(catalog: &Connection, refs: Vec<PhotoRef>) -> Result<Vec<PhotoRef>> {
    let mut stmt = catalog.prepare("SELECT organization_uid FROM assets WHERE id = ?1")?;
    let mut valid = Vec::with_capacity(refs.len());
    for reference in refs {
        let current: Option<String> = stmt.query_row([reference.asset_id], |row| row.get::<_, Option<String>>(0)).optional()?.flatten();
        if current.as_deref() == Some(reference.photo_uid.as_str()) { valid.push(reference); }
    }
    Ok(valid)
}

/// 旧数字标签词典映射进 catalog，保留无法命名的旧关联等待之后补齐。
pub fn sync_legacy_terms(conn: &Connection, dictionary: &[tags::Tag]) -> Result<usize> {
    let transaction = conn.unchecked_transaction()?;
    let mut synced = 0;
    for tag in dictionary {
        synced += transaction.execute(
            "INSERT OR IGNORE INTO tag_terms(tag_key, display_name, legacy_tag_id) VALUES (?1, ?2, ?3)",
            params![tag.name_folded, tag.name, tag.id],
        )?;
        // A directory tag may have created the same text first. Attach the legacy
        // numeric identity so its existing photo associations become visible too.
        transaction.execute(
            "UPDATE tag_terms SET legacy_tag_id = ?2 WHERE tag_key = ?1 AND legacy_tag_id IS NULL",
            params![tag.name_folded, tag.id],
        )?;
    }
    transaction.execute(
        "INSERT OR IGNORE INTO asset_tag_sources(asset_id, tag_key, source, tagged_at)
         SELECT a.asset_id, t.tag_key, 'manual', a.tagged_at
         FROM asset_tags a JOIN tag_terms t ON t.legacy_tag_id = a.tag_id", [],
    )?;
    transaction.commit()?;
    Ok(synced)
}

pub fn ensure_term(conn: &Connection, name: &str, legacy_tag_id: Option<i64>) -> Result<String> {
    let name = tags::clean_name(name)?;
    let key = tags::fold_name(&name);
    conn.execute(
        "INSERT OR IGNORE INTO tag_terms(tag_key, display_name, legacy_tag_id) VALUES (?1, ?2, ?3)",
        params![key, name, legacy_tag_id],
    )?;
    Ok(key)
}

pub fn set_directory_tag(conn: &Connection, directory_key: &str, name: &str, enabled: bool, now: i64) -> Result<bool> {
    if !directory_key.starts_with("photos/")
        || directory_key.split('/').any(|part| part.is_empty() || part == "." || part == "..")
        || directory_key.contains('\\')
    {
        return Err(Error::Unsupported("目录必须是库内 photos 下的路径".into()));
    }
    let key = ensure_term(conn, name, None)?;
    if enabled {
        Ok(conn.execute(
            "INSERT OR IGNORE INTO directory_tags(directory_key, tag_key, tagged_at) VALUES (?1, ?2, ?3)",
            params![directory_key, key, now],
        )? > 0)
    } else {
        Ok(conn.execute("DELETE FROM directory_tags WHERE directory_key = ?1 AND tag_key = ?2", params![directory_key, key])? > 0)
    }
}

pub fn directories_for_tag(conn: &Connection, tag_key: &str, limit: usize, offset: usize) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT directory_key FROM directory_tags WHERE tag_key = ?1
         ORDER BY directory_key LIMIT ?2 OFFSET ?3"
    )?;
    let rows = stmt.query_map(params![tags::fold_name(tag_key), limit as i64, offset as i64], |row| row.get(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn directory_tag_counts(conn: &Connection) -> Result<std::collections::BTreeMap<String, i64>> {
    let mut stmt = conn.prepare("SELECT tag_key, count(*) FROM directory_tags GROUP BY tag_key")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<std::collections::BTreeMap<_, _>>>()?)
}

pub fn tags_for_directory(conn: &Connection, directory_key: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.display_name FROM directory_tags d JOIN tag_terms t ON t.tag_key = d.tag_key
         WHERE d.directory_key = ?1 ORDER BY t.display_name"
    )?;
    let rows = stmt.query_map([directory_key], |row| row.get(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn photo_tags(conn: &Connection, asset_id: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT tag_key FROM asset_tag_sources WHERE asset_id = ?1 ORDER BY tag_key"
    )?;
    let rows = stmt.query_map([asset_id], |row| row.get(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn photo_ref(conn: &Connection, repository_id: &str, asset_id: i64) -> Result<Option<PhotoRef>> {
    Ok(conn.query_row(
        "SELECT organization_uid FROM assets WHERE id = ?1",
        [asset_id],
        |row| row.get::<_, Option<String>>(0),
    ).optional()?.flatten().map(|photo_uid| PhotoRef {
        repository_id: repository_id.into(), photo_uid, asset_id,
    }))
}

/// 批量操作只租用每个 catalog 一次，并复用同一条已预编译的 UID 查询。
pub fn photo_refs(conn: &Connection, repository_id: &str, asset_ids: &[i64]) -> Result<Vec<PhotoRef>> {
    let mut stmt = conn.prepare("SELECT organization_uid FROM assets WHERE id = ?1")?;
    let mut refs = Vec::with_capacity(asset_ids.len());
    for &asset_id in asset_ids {
        if asset_id <= 0 { return Err(Error::Unsupported("相片引用无效".into())); }
        let uid: Option<String> = stmt.query_row([asset_id], |row| row.get(0)).optional()?.flatten();
        let Some(photo_uid) = uid else {
            return Err(Error::Unsupported(format!("照片已不在库中：{repository_id}/{asset_id}")));
        };
        refs.push(PhotoRef { repository_id: repository_id.into(), photo_uid, asset_id });
    }
    Ok(refs)
}

const RECONCILE_BATCH: usize = 256;

fn matching_ids(conn: &Connection, bucket: &Bucket, repository_id: &str, candidates: &[i64]) -> Result<BTreeSet<i64>> {
    let mut found = BTreeSet::new();
    if candidates.is_empty() { return Ok(found); }
    for group in &bucket.rules.groups {
        if !group.repository_ids.iter().any(|id| id == repository_id) { continue; }
        let query = query::Query {
            scope: query::Scope::AssetIds { ids: candidates.to_vec() },
            filter: filter_for_group(group),
            sort: query::Sort::default(),
        };
        for row in query::timeline(conn, &query, 0)? {
            found.insert(row.id);
        }
    }
    Ok(found)
}

/// 一个库的一小批待处理资产。可重复调用直到返回 false；游标与成员同事务提交。
/// 即使崩溃重放，成员主键与排除规则也保持幂等。
pub fn reconcile_scan_batch(app: &Connection, catalog: &Connection, repository_id: &str, now: i64) -> Result<bool> {
    let pending: Option<(i64, i64, i64)> = app.query_row(
        "SELECT s.bucket_id, s.rule_revision, s.after_asset_id
         FROM photo_bucket_scans s JOIN photo_buckets b ON b.id = s.bucket_id
         WHERE s.repository_id = ?1 AND b.paused = 0
         ORDER BY s.bucket_id LIMIT 1",
        [repository_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional()?;
    let Some((bucket_id, revision, after)) = pending else { return Ok(false); };
    let Some(bucket) = bucket_by_id(app, bucket_id)? else { return Ok(false); };
    if bucket.rule_revision != revision {
        app.execute(
            "DELETE FROM photo_bucket_scans WHERE bucket_id = ?1 AND repository_id = ?2 AND rule_revision = ?3",
            params![bucket_id, repository_id, revision],
        )?;
        return Ok(true);
    }
    let mut stmt = catalog.prepare("SELECT id FROM assets WHERE id > ?1 ORDER BY id LIMIT ?2")?;
    let ids = stmt.query_map(params![after, RECONCILE_BATCH as i64], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let matched = matching_ids(catalog, &bucket, repository_id, &ids)?;
    let refs = matched.into_iter().map(|id| photo_ref(catalog, repository_id, id)).collect::<Result<Vec<_>>>()?;
    let transaction = app.unchecked_transaction()?;
    for photo in refs.into_iter().flatten() {
        add_auto_member(&transaction, bucket_id, revision, &photo, now)?;
    }
    if ids.len() < RECONCILE_BATCH {
        transaction.execute(
            "DELETE FROM photo_bucket_scans WHERE bucket_id = ?1 AND repository_id = ?2 AND rule_revision = ?3",
            params![bucket_id, repository_id, revision],
        )?;
    } else if let Some(last) = ids.last() {
        transaction.execute(
            "UPDATE photo_bucket_scans SET after_asset_id = ?1
             WHERE bucket_id = ?2 AND repository_id = ?3 AND rule_revision = ?4",
            params![last, bucket_id, repository_id, revision],
        )?;
    }
    transaction.commit()?;
    Ok(true)
}

/// 写入日志之后从 app.db 的 checkpoint 重放。目录标签变化没有照片事件。
pub fn reconcile_events_batch(app: &Connection, catalog: &Connection, repository_id: &str, now: i64) -> Result<bool> {
    let after: i64 = app.query_row(
        "SELECT event_id FROM photo_organization_cursors WHERE repository_id = ?1",
        [repository_id], |row| row.get(0),
    ).optional()?.unwrap_or(0);
    let mut stmt = catalog.prepare(
        "SELECT id, asset_id FROM photo_organization_events WHERE id > ?1 ORDER BY id LIMIT ?2"
    )?;
    let events = stmt.query_map(params![after, RECONCILE_BATCH as i64], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if events.is_empty() { return Ok(false); }
    let ids: Vec<i64> = events.iter().map(|(_, id)| *id).collect::<BTreeSet<_>>().into_iter().collect();
    let buckets = list_buckets(app)?;
    let mut additions = Vec::new();
    for bucket in buckets.iter().filter(|bucket| !bucket.paused && !bucket.rules.groups.is_empty()) {
        for id in matching_ids(catalog, bucket, repository_id, &ids)? {
            if let Some(photo) = photo_ref(catalog, repository_id, id)? {
                additions.push((bucket.id, bucket.rule_revision, photo));
            }
        }
    }
    let transaction = app.unchecked_transaction()?;
    for (bucket_id, revision, photo) in additions {
        add_auto_member(&transaction, bucket_id, revision, &photo, now)?;
    }
    let last = events.last().map(|(event_id, _)| *event_id).unwrap_or(after);
    transaction.execute(
        "INSERT INTO photo_organization_cursors(repository_id, event_id) VALUES (?1, ?2)
         ON CONFLICT(repository_id) DO UPDATE SET event_id = excluded.event_id",
        params![repository_id, last],
    )?;
    transaction.commit()?;
    Ok(true)
}

/// app.db 游标已提交后才能清理 catalog 日志；重复清理或崩溃重试都安全。
pub fn event_cursor(app: &Connection, repository_id: &str) -> Result<i64> {
    Ok(app.query_row(
        "SELECT event_id FROM photo_organization_cursors WHERE repository_id = ?1",
        [repository_id], |row| row.get(0),
    ).optional()?.unwrap_or(0))
}

pub fn prune_events(catalog: &Connection, committed_cursor: i64) -> Result<usize> {
    if committed_cursor <= 0 { return Ok(0); }
    Ok(catalog.execute(
        "DELETE FROM photo_organization_events WHERE id IN (
           SELECT id FROM photo_organization_events WHERE id <= ?1 ORDER BY id LIMIT 1024
         )",
        [committed_cursor],
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{migration::{self, Backups, DbKind}, pragma};

    fn db(kind: DbKind) -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, kind, Backups::none(), 1).unwrap();
        conn
    }

    fn asset(conn: &Connection) -> i64 {
        conn.execute("INSERT INTO assets(imported_at, updated_at) VALUES (1, 1)", []).unwrap();
        conn.last_insert_rowid()
    }

    fn group(repos: &[&str]) -> RuleGroup {
        RuleGroup {
            repository_ids: repos.iter().map(|s| (*s).into()).collect(),
            min_rating: Some(4),
            ..RuleGroup::default()
        }
    }

    #[test]
    fn rule_groups_are_or_and_each_group_is_and_with_explicit_repos() {
        let first = group(&["B", "A", "A"]);
        let second = RuleGroup { repository_ids: vec!["C".into()], tag_keys: vec!["  旅行 ".into()], ..RuleGroup::default() };
        let rules = normalize_rules(RuleSet { groups: vec![first.clone(), second.clone(), first] }).unwrap();
        assert_eq!(rules.groups.len(), 2);
        assert_eq!(rules.groups.iter().find(|g| g.min_rating.is_some()).unwrap().repository_ids, ["A", "B"]);
        assert_eq!(rules.groups.iter().find(|g| !g.tag_keys.is_empty()).unwrap().tag_keys, ["旅行"]);
        assert_eq!(filter_for_group(&rules.groups[0]).combinator, query::Combinator::And);
        assert!(normalize_rules(RuleSet { groups: vec![group(&[])] }).is_err());
        assert!(normalize_rules(RuleSet { groups: vec![RuleGroup { min_rating: Some(6), ..group(&["A"]) }] }).is_err());
        assert!(normalize_rules(RuleSet { groups: vec![group(&["A"]); 17] }).is_err());
        assert!(normalize_rules(RuleSet { groups: vec![RuleGroup {
            tag_keys: (0..65).map(|i| format!("标签{i}")).collect(), ..group(&["A"])
        }] }).is_err());
    }

    #[test]
    fn catalog_uid_and_directory_tags_do_not_inherit_to_photo() {
        let cat = db(DbKind::Catalog);
        let id = asset(&cat);
        let photo = photo_ref(&cat, "repo", id).unwrap().unwrap();
        assert_eq!(photo.photo_uid.len(), 32);
        set_directory_tag(&cat, "photos/云南", "  旅行 ", true, 2).unwrap();
        assert_eq!(directories_for_tag(&cat, "旅行", 20, 0).unwrap(), ["photos/云南"]);
        assert_eq!(directory_tag_counts(&cat).unwrap().get("旅行"), Some(&1));
        assert!(photo_tags(&cat, id).unwrap().is_empty());
        assert!(set_directory_tag(&cat, "photos/云南", "旅行", true, 3).unwrap() == false);
        assert!(set_directory_tag(&cat, "photos/云南", "旅行", false, 4).unwrap());
        assert!(directories_for_tag(&cat, "旅行", 20, 0).unwrap().is_empty());
        assert!(set_directory_tag(&cat, "../escape", "旅行", true, 2).is_err());
    }

    #[test]
    fn legacy_photo_tag_text_is_mirrored_and_kept_separate_from_directory() {
        let app = db(DbKind::App);
        let cat = db(DbKind::Catalog);
        let id = asset(&cat);
        let tag_id = tags::ensure_tag(&app, "旅行", 1).unwrap();
        cat.execute("INSERT INTO asset_tags(asset_id, tag_id, tagged_at) VALUES (?1, ?2, 3)", params![id, tag_id]).unwrap();
        assert!(photo_tags(&cat, id).unwrap().is_empty());
        sync_legacy_terms(&cat, &tags::list_all(&app).unwrap()).unwrap();
        assert_eq!(photo_tags(&cat, id).unwrap(), ["旅行"]);
        set_directory_tag(&cat, "photos/云南", "旅行", true, 4).unwrap();
        cat.execute("DELETE FROM asset_tags WHERE asset_id = ?1 AND tag_id = ?2", params![id, tag_id]).unwrap();
        assert!(photo_tags(&cat, id).unwrap().is_empty());
        assert_eq!(directories_for_tag(&cat, "旅行", 20, 0).unwrap(), ["photos/云南"]);
    }

    #[test]
    fn bucket_members_are_cross_repo_idempotent_and_exclusion_wins() {
        let app = db(DbKind::App);
        let rules = RuleSet { groups: vec![group(&["A", "B"])] };
        let bucket = create_bucket(&app, "飞鸟", rules, 1).unwrap();
        let a = PhotoRef { repository_id: "A".into(), photo_uid: "same".into(), asset_id: 1 };
        let b = PhotoRef { repository_id: "B".into(), photo_uid: "same".into(), asset_id: 1 };
        assert!(add_auto_member(&app, bucket.id, bucket.rule_revision, &a, 2).unwrap());
        assert!(!add_auto_member(&app, bucket.id, bucket.rule_revision, &a, 2).unwrap());
        assert!(add_auto_member(&app, bucket.id, bucket.rule_revision, &b, 2).unwrap());
        assert_eq!(bucket_by_id(&app, bucket.id).unwrap().unwrap().member_count, 2);
        assert!(remove_member(&app, bucket.id, &a, 3).unwrap());
        assert!(!add_auto_member(&app, bucket.id, bucket.rule_revision, &a, 4).unwrap());
        assert_eq!(add_members(&app, bucket.id, std::slice::from_ref(&a), 5).unwrap(), 1);
        assert_eq!(member_refs(&app, bucket.id, "A").unwrap(), [a]);
        let edited = set_bucket_rules(&app, bucket.id, "飞鸟与海", RuleSet { groups: vec![group(&["B"])] }, 6).unwrap().unwrap();
        assert!(!add_auto_member(&app, bucket.id, bucket.rule_revision, &b, 7).unwrap());
        assert!(edited.rule_revision > bucket.rule_revision);
        set_bucket_paused(&app, bucket.id, true, 8).unwrap();
        assert!(!add_auto_member(&app, bucket.id, edited.rule_revision, &b, 9).unwrap());
        set_bucket_pinned(&app, bucket.id, true, 10).unwrap();
        assert!(!delete_bucket(&app, bucket.id).unwrap());
        set_bucket_pinned(&app, bucket.id, false, 11).unwrap();
        assert!(delete_bucket(&app, bucket.id).unwrap());
    }

    #[test]
    fn stale_member_cannot_point_at_a_reused_asset_number() {
        let cat = db(DbKind::Catalog);
        let id = asset(&cat);
        let original = photo_ref(&cat, "A", id).unwrap().unwrap();
        assert_eq!(valid_member_refs(&cat, vec![original.clone()]).unwrap(), [original.clone()]);
        cat.execute("DELETE FROM assets WHERE id = ?1", [id]).unwrap();
        cat.execute("INSERT INTO assets(id, imported_at, updated_at) VALUES (?1, 2, 2)", [id]).unwrap();
        assert_ne!(photo_ref(&cat, "A", id).unwrap().unwrap().photo_uid, original.photo_uid);
        assert!(valid_member_refs(&cat, vec![original]).unwrap().is_empty());
    }

    #[test]
    fn batched_photo_refs_keep_uid_identity_and_reject_missing_assets() {
        let cat = db(DbKind::Catalog);
        let first = asset(&cat);
        let second = asset(&cat);
        let refs = photo_refs(&cat, "库 A", &[first, second, first]).unwrap();
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[0], refs[2]);
        assert_ne!(refs[0].photo_uid, refs[1].photo_uid);
        assert!(photo_refs(&cat, "库 A", &[first, second + 1]).is_err());
        assert!(photo_refs(&cat, "库 A", &[0]).is_err());
    }

    #[test]
    fn scan_and_event_replay_add_once_without_readding_excluded_photo() {
        let app = db(DbKind::App);
        let cat = db(DbKind::Catalog);
        let first = asset(&cat);
        cat.execute("UPDATE assets SET rating = 5 WHERE id = ?1", [first]).unwrap();
        let rules = RuleSet { groups: vec![
            group(&["A"]),
            RuleGroup { repository_ids: vec!["A".into()], likes: vec!["like".into()], ..RuleGroup::default() },
        ] };
        let bucket = create_bucket(&app, "自动", rules, 1).unwrap();
        assert!(reconcile_scan_batch(&app, &cat, "A", 2).unwrap());
        assert!(!reconcile_scan_batch(&app, &cat, "A", 2).unwrap());
        assert_eq!(bucket_by_id(&app, bucket.id).unwrap().unwrap().member_count, 1);
        assert!(reconcile_events_batch(&app, &cat, "A", 3).unwrap());
        let committed = event_cursor(&app, "A").unwrap();
        assert!(prune_events(&cat, committed).unwrap() > 0);
        assert_eq!(prune_events(&cat, committed).unwrap(), 0);
        assert!(!reconcile_events_batch(&app, &cat, "A", 3).unwrap());
        assert_eq!(bucket_by_id(&app, bucket.id).unwrap().unwrap().member_count, 1);
        let photo = photo_ref(&cat, "A", first).unwrap().unwrap();
        remove_member(&app, bucket.id, &photo, 4).unwrap();
        cat.execute("UPDATE assets SET rating = 4 WHERE id = ?1", [first]).unwrap();
        reconcile_events_batch(&app, &cat, "A", 5).unwrap();
        assert_eq!(bucket_by_id(&app, bucket.id).unwrap().unwrap().member_count, 0);
        let second = asset(&cat);
        cat.execute("UPDATE assets SET like_state = 'like' WHERE id = ?1", [second]).unwrap();
        reconcile_events_batch(&app, &cat, "A", 6).unwrap();
        assert_eq!(bucket_by_id(&app, bucket.id).unwrap().unwrap().member_count, 1);
    }
}
