//! AI 任务冻结在既有 app.db jobs 中。只存身份；图像与 catalog 写锁不进入队列事务。
use crate::{Error, Result, store::organization::PhotoRef};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
const RUN: &str = "photo_ai_run";
const IMAGE: &str = "photo_ai_image";
pub const MAX_PAGE: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Run {
    pub version: u8,
    pub label: String,
    pub pipeline_sha256: String,
    pub prepared: bool,
    #[serde(default)]
    pub rerun: bool,
    #[serde(default = "default_zh")]
    pub zh: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Item {
    pub version: u8,
    pub run_id: i64,
    pub photo: PhotoRef,
}
#[derive(Debug, Clone)]
pub struct Claim {
    pub id: i64,
    pub item: Item,
    pub attempts: i64,
}
fn invalid() -> Error {
    Error::Unsupported("AI 任务身份或状态无效".into())
}
fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn default_zh() -> bool {
    true
}
impl Run {
    fn validate(&self) -> Result<()> {
        if self.version != 1
            || self.label.trim().is_empty()
            || self.label.chars().count() > 256
            || !valid_hash(&self.pipeline_sha256)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl Item {
    fn validate(&self) -> Result<()> {
        if self.version != 1
            || self.run_id <= 0
            || self.photo.asset_id <= 0
            || self.photo.repository_id.is_empty()
            || self.photo.repository_id.len() > 128
            || self.photo.photo_uid.is_empty()
            || self.photo.photo_uid.len() > 128
        {
            return Err(invalid());
        }
        Ok(())
    }
}
pub fn create(conn: &Connection, label: &str, pipeline: &str, now: i64) -> Result<i64> {
    create_with_options(conn, label, pipeline, false, true, now)
}
pub fn create_with_options(
    conn: &Connection,
    label: &str,
    pipeline: &str,
    rerun: bool,
    zh: bool,
    now: i64,
) -> Result<i64> {
    let run = Run {
        version: 1,
        label: label.trim().into(),
        pipeline_sha256: pipeline.into(),
        prepared: false,
        rerun,
        zh,
    };
    run.validate()?;
    conn.execute("INSERT INTO jobs(kind,payload,state,created_at,updated_at) VALUES(?1,?2,'preparing',?3,?3)",params![RUN,serde_json::to_string(&run)?,now])?;
    Ok(conn.last_insert_rowid())
}
fn load(conn: &Connection, id: i64) -> Result<(Run, String)> {
    let (json, state): (String, String) = conn.query_row(
        "SELECT payload,state FROM jobs WHERE id=?1 AND kind=?2",
        params![id, RUN],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let run: Run = serde_json::from_str(&json)?;
    run.validate()?;
    Ok((run, state))
}
/// 重放同一页幂等；准备状态下只写范围，不启动推理。
pub fn append_page(conn: &Connection, run_id: i64, photos: &[PhotoRef], now: i64) -> Result<usize> {
    if photos.len() > MAX_PAGE {
        return Err(invalid());
    }
    let tx = conn.unchecked_transaction()?;
    let (run, state) = load(&tx, run_id)?;
    if run.prepared || !["preparing", "paused"].contains(&state.as_str()) {
        return Err(invalid());
    }
    let mut count = 0;
    for photo in photos {
        let item = Item {
            version: 1,
            run_id,
            photo: photo.clone(),
        };
        item.validate()?;
        count+=tx.execute("INSERT OR IGNORE INTO jobs(kind,payload,state,created_at,updated_at) VALUES(?1,?2,'pending',?3,?3)",params![IMAGE,serde_json::to_string(&item)?,now])?;
    }
    tx.commit()?;
    Ok(count)
}
pub fn finish_prepare(conn: &Connection, id: i64, now: i64) -> Result<()> {
    let (mut run, state) = load(conn, id)?;
    if run.prepared || !["preparing", "paused"].contains(&state.as_str()) {
        return Err(invalid());
    }
    run.prepared = true;
    conn.execute(
        "UPDATE jobs SET payload=?2,state=?3,updated_at=?4 WHERE id=?1",
        params![
            id,
            serde_json::to_string(&run)?,
            if state == "paused" {
                "paused"
            } else {
                "pending"
            },
            now
        ],
    )?;
    settle(conn, id, now)?;
    Ok(())
}
pub fn pause(conn: &Connection, id: i64, now: i64) -> Result<bool> {
    Ok(conn.execute("UPDATE jobs SET state='paused',updated_at=?2 WHERE id=?1 AND kind=?3 AND state IN ('preparing','pending','running')",params![id,now,RUN])?>0)
}
pub fn resume(conn: &Connection, id: i64, now: i64) -> Result<bool> {
    let (run, state) = load(conn, id)?;
    if state != "paused" {
        return Ok(false);
    }
    conn.execute(
        "UPDATE jobs SET state=?2,updated_at=?3 WHERE id=?1",
        params![id, if run.prepared { "pending" } else { "preparing" }, now],
    )?;
    Ok(true)
}
pub fn cancel(conn: &Connection, id: i64, now: i64) -> Result<bool> {
    let tx = conn.unchecked_transaction()?;
    let changed=tx.execute("UPDATE jobs SET state='cancelled',updated_at=?2 WHERE id=?1 AND kind=?3 AND state IN ('preparing','pending','running','paused')",params![id,now,RUN])?>0;
    if changed {
        tx.execute("UPDATE jobs SET state='cancelled',updated_at=?2 WHERE kind=?3 AND json_valid(payload) AND json_extract(CASE WHEN json_valid(payload) THEN payload ELSE '{}' END,'$.runId')=?1 AND state IN ('pending','running','waiting')",params![id,now,IMAGE])?;
    }
    tx.commit()?;
    Ok(changed)
}
/// 上层还必须撤销对应 catalog attempt。app 确认使用 compare-and-set，取消后不能复活。
pub fn active(conn: &Connection, claim: &Claim) -> Result<bool> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM jobs c JOIN jobs r ON r.id=?2 WHERE c.id=?1 AND c.kind='photo_ai_image' AND c.state='running' AND c.attempts=?3 AND r.kind='photo_ai_run' AND r.state IN ('running','pending','paused'))",params![claim.id,claim.item.run_id,claim.attempts],|r|r.get(0))?)
}
pub fn has_running(conn: &Connection) -> Result<bool> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM jobs WHERE (kind='photo_ai_image' AND state='running') OR (kind='photo_ai_run' AND state IN ('preparing','pending','running')))",[],|r|r.get(0))?)
}
pub fn fail_prepare(conn: &Connection, id: i64, message: &str, now: i64) -> Result<()> {
    if !cancel(conn, id, now)? {
        return Ok(());
    }
    conn.execute("UPDATE jobs SET state='failed',error=?2,updated_at=?3 WHERE id=?1 AND kind='photo_ai_run' AND state='cancelled'",params![id,message.chars().take(2048).collect::<String>(),now])?;
    Ok(())
}
pub fn context(conn: &Connection, id: i64) -> Result<Run> {
    load(conn, id).map(|v| v.0)
}
pub fn complete(conn: &Connection, claim: &Claim, now: i64) -> Result<bool> {
    finish_item(conn, claim, now, false)
}
pub fn skip(conn: &Connection, claim: &Claim, now: i64) -> Result<bool> {
    finish_item(conn, claim, now, true)
}
fn finish_item(conn: &Connection, claim: &Claim, now: i64, skipped: bool) -> Result<bool> {
    let n=conn.execute("UPDATE jobs SET state=?5,error=NULL,updated_at=?2 WHERE id=?1 AND kind=?3 AND state='running' AND attempts=?4",params![claim.id,now,IMAGE,claim.attempts,if skipped {"skipped"}else{"done"}])?;
    settle(conn, claim.item.run_id, now)?;
    Ok(n > 0)
}
fn settle(conn: &Connection, run_id: i64, now: i64) -> Result<()> {
    conn.execute("UPDATE jobs SET state=CASE WHEN EXISTS(SELECT 1 FROM jobs c WHERE c.kind=?3 AND json_valid(c.payload) AND json_extract(c.payload,'$.runId')=?1 AND c.state='failed') THEN 'failed' ELSE 'done' END,updated_at=?2
        WHERE id=?1 AND kind=?4 AND state IN ('pending','running','paused') AND json_extract(CASE WHEN json_valid(payload) THEN payload ELSE '{}' END,'$.prepared')=1
        AND NOT EXISTS(SELECT 1 FROM jobs c WHERE c.kind=?3 AND json_valid(c.payload) AND json_extract(c.payload,'$.runId')=?1 AND c.state IN ('pending','running','waiting'))",params![run_id,now,IMAGE,RUN])?;
    Ok(())
}
pub fn claim_next(conn: &Connection, now: i64) -> Result<Option<Claim>> {
    let tx = conn.unchecked_transaction()?;
    // JSON 无效/父任务丢失不能永久留在 pending。先隔离损坏的关联。
    tx.execute("UPDATE jobs SET state='failed',error='任务载荷或父任务无效',updated_at=?1 WHERE kind=?2 AND state IN ('pending','waiting') AND (NOT json_valid(payload) OR NOT EXISTS(SELECT 1 FROM jobs r WHERE r.id=json_extract(CASE WHEN json_valid(jobs.payload) THEN jobs.payload ELSE '{}' END,'$.runId') AND r.kind=?3))", params![now,IMAGE,RUN])?;
    let parents: Vec<i64> = {
        let mut q =
            tx.prepare("SELECT id FROM jobs WHERE kind=?1 AND state IN ('pending','running')")?;
        q.query_map([RUN], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    for id in parents {
        if load(&tx, id).is_err() {
            tx.execute(
                "UPDATE jobs SET state='failed',error='父任务载荷无效',updated_at=?2 WHERE id=?1",
                params![id, now],
            )?;
            tx.execute("UPDATE jobs SET state='failed',error='父任务载荷无效',updated_at=?2 WHERE kind=?3 AND json_valid(payload) AND json_extract(CASE WHEN json_valid(payload) THEN payload ELSE '{}' END,'$.runId')=?1 AND state IN ('pending','waiting')",params![id,now,IMAGE])?;
        }
    }
    loop {
        let row:Option<(i64,String,i64)>=tx.query_row("UPDATE jobs SET state='running',attempts=attempts+1,updated_at=?1 WHERE id=(
            SELECT c.id FROM jobs c JOIN jobs r ON r.id=json_extract(CASE WHEN json_valid(c.payload) THEN c.payload ELSE '{}' END,'$.runId')
            WHERE c.kind=?2 AND c.state='pending' AND c.updated_at<=?1 AND r.kind=?3 AND r.state IN ('pending','running')
            AND json_valid(r.payload) AND json_extract(r.payload,'$.prepared')=1 ORDER BY c.id LIMIT 1) RETURNING id,payload,attempts",params![now,IMAGE,RUN],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let Some((id, json, attempts)) = row else {
            tx.commit()?;
            return Ok(None);
        };
        match serde_json::from_str::<Item>(&json) {
            Ok(item) if item.validate().is_ok() => {
                tx.execute(
                    "UPDATE jobs SET state='running',updated_at=?2 WHERE id=?1 AND state='pending'",
                    params![item.run_id, now],
                )?;
                tx.commit()?;
                return Ok(Some(Claim { id, item, attempts }));
            }
            _ => {
                tx.execute(
                    "UPDATE jobs SET state='failed',error='任务载荷无效',updated_at=?2 WHERE id=?1",
                    params![id, now],
                )?;
                let parent: Option<i64> = tx.query_row("SELECT json_extract(CASE WHEN json_valid(payload) THEN payload ELSE '{}' END,'$.runId') FROM jobs WHERE id=?1", [id], |r|r.get(0))?;
                if let Some(parent) = parent {
                    settle(&tx, parent, now)?;
                }
            }
        }
    }
}
pub fn wait_offline(conn: &Connection, claim: &Claim, now: i64) -> Result<()> {
    conn.execute("UPDATE jobs SET state='waiting',attempts=MAX(attempts-1,0),updated_at=?2 WHERE id=?1 AND state='running' AND attempts=?3",params![claim.id,now,claim.attempts])?;
    Ok(())
}
pub fn wake_repository(conn: &Connection, repo: &str, now: i64) -> Result<usize> {
    Ok(conn.execute("UPDATE jobs SET state='pending',updated_at=?2 WHERE kind=?3 AND state='waiting' AND json_valid(payload) AND json_extract(CASE WHEN json_valid(payload) THEN payload ELSE '{}' END,'$.photo.repositoryId')=?1",params![repo,now,IMAGE])?)
}
pub fn fail(conn: &Connection, claim: &Claim, error: &str, now: i64) -> Result<bool> {
    let retry = claim.attempts < 3;
    let message: String = error.chars().take(2048).collect();
    let n=conn.execute("UPDATE jobs SET state=?2,error=?3,updated_at=?4 WHERE id=?1 AND kind=?5 AND state='running' AND attempts=?6",params![claim.id,if retry {"pending"} else {"failed"},message,if retry {now.saturating_add(30_000)} else {now},IMAGE,claim.attempts])?;
    settle(conn, claim.item.run_id, now)?;
    Ok(n > 0 && retry)
}
pub fn recover_paused(conn: &Connection, now: i64) -> Result<usize> {
    let tx = conn.unchecked_transaction()?;
    let n=tx.execute("UPDATE jobs SET state='paused',updated_at=?2 WHERE kind=?1 AND state IN ('preparing','pending','running')",params![RUN,now])?;
    tx.execute(
        "UPDATE jobs SET state='pending',updated_at=?2 WHERE kind=?1 AND state='running'",
        params![IMAGE, now],
    )?;
    tx.execute("UPDATE jobs SET state='failed',error='范围冻结未完成，请重新选择范围新建任务',updated_at=?2 WHERE kind=?1 AND state='paused' AND json_extract(CASE WHEN json_valid(payload) THEN payload ELSE '{}' END,'$.prepared')=0", params![RUN,now])?;
    tx.execute("UPDATE jobs SET state='cancelled',updated_at=?2 WHERE kind=?1 AND state IN ('pending','running','waiting') AND EXISTS(SELECT 1 FROM jobs r WHERE r.id=json_extract(CASE WHEN json_valid(jobs.payload) THEN jobs.payload ELSE '{}' END,'$.runId') AND r.kind='photo_ai_run' AND r.state='failed' AND json_extract(CASE WHEN json_valid(r.payload) THEN r.payload ELSE '{}' END,'$.prepared')=0)",params![IMAGE,now])?;
    tx.commit()?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        crate::store::pragma::apply(&c, false).unwrap();
        crate::store::migration::apply(
            &mut c,
            crate::store::migration::DbKind::App,
            crate::store::migration::Backups::none(),
            1,
        )
        .unwrap();
        c
    }
    fn photo(id: i64) -> PhotoRef {
        PhotoRef {
            repository_id: "库A".into(),
            photo_uid: format!("uid{id}"),
            asset_id: id,
        }
    }
    fn run(c: &Connection) -> i64 {
        let id = create(c, "公开样本", &"a".repeat(64), 1).unwrap();
        append_page(c, id, &[photo(1), photo(2)], 1).unwrap();
        finish_prepare(c, id, 1).unwrap();
        id
    }
    #[test]
    fn preparation_is_frozen_and_duplicate_pages_are_idempotent() {
        let c = db();
        let id = create(&c, "测试", &"a".repeat(64), 1).unwrap();
        assert!(claim_next(&c, 1).unwrap().is_none());
        assert_eq!(append_page(&c, id, &[photo(1), photo(1)], 1).unwrap(), 1);
        assert_eq!(append_page(&c, id, &[photo(1)], 1).unwrap(), 0);
        finish_prepare(&c, id, 2).unwrap();
        assert!(append_page(&c, id, &[photo(2)], 3).is_err());
        let claim = claim_next(&c, 3).unwrap().unwrap();
        assert!(complete(&c, &claim, 4).unwrap());
        assert_eq!(load(&c, id).unwrap().1, "done");
        assert!(create(&c, "", &"a".repeat(64), 1).is_err());
        assert!(create(&c, "测试", &"A".repeat(64), 1).is_err());
        let id = create(&c, "测试", &"a".repeat(64), 1).unwrap();
        assert!(append_page(&c, id, &vec![photo(1); 501], 1).is_err());
    }
    #[test]
    fn pause_finishes_current_cancel_discards_current_and_restart_stays_paused() {
        let c = db();
        let id = run(&c);
        let first = claim_next(&c, 1).unwrap().unwrap();
        pause(&c, id, 2).unwrap();
        assert!(claim_next(&c, 2).unwrap().is_none());
        assert!(complete(&c, &first, 2).unwrap());
        resume(&c, id, 3).unwrap();
        let next = claim_next(&c, 3).unwrap().unwrap();
        cancel(&c, id, 4).unwrap();
        assert!(!complete(&c, &next, 5).unwrap());
        assert_eq!(load(&c, id).unwrap().1, "cancelled");
        let id = run(&c);
        let first = claim_next(&c, 6).unwrap().unwrap();
        assert_eq!(recover_paused(&c, 7).unwrap(), 1);
        assert!(claim_next(&c, 8).unwrap().is_none());
        assert!(!complete(&c, &first, 8).unwrap());
        resume(&c, id, 9).unwrap();
        assert!(claim_next(&c, 9).unwrap().is_some());
    }
    #[test]
    fn offline_does_not_burn_attempts_and_failures_have_bounded_backoff() {
        let c = db();
        let id = run(&c);
        let first = claim_next(&c, 1).unwrap().unwrap();
        wait_offline(&c, &first, 2).unwrap();
        let second = claim_next(&c, 2).unwrap().unwrap();
        complete(&c, &second, 2).unwrap();
        assert!(claim_next(&c, 3).unwrap().is_none());
        wake_repository(&c, "库A", 4).unwrap();
        let mut item = claim_next(&c, 4).unwrap().unwrap();
        assert_eq!(item.attempts, 1);
        for (attempt, now) in [(1, 4), (2, 30_004), (3, 60_004)] {
            assert_eq!(item.attempts, attempt);
            assert_eq!(fail(&c, &item, "崩溃", now).unwrap(), attempt < 3);
            assert!(claim_next(&c, now).unwrap().is_none());
            if attempt < 3 {
                item = claim_next(&c, now + 30_000).unwrap().unwrap();
            }
        }
        assert_eq!(load(&c, id).unwrap().1, "failed");
    }
    #[test]
    fn old_completion_cannot_confirm_new_attempt_and_other_kinds_are_untouched() {
        let c = db();
        let id = run(&c);
        let old = claim_next(&c, 1).unwrap().unwrap();
        fail(&c, &old, "重试", 2).unwrap();
        let other = claim_next(&c, 2).unwrap().unwrap();
        complete(&c, &other, 2).unwrap();
        let new = claim_next(&c, 30_002).unwrap().unwrap();
        assert_eq!(new.id, old.id);
        assert!(!complete(&c, &old, 30_003).unwrap());
        assert!(complete(&c, &new, 30_004).unwrap());
        assert_eq!(load(&c, id).unwrap().1, "done");
        c.execute_batch(
            "INSERT INTO jobs(kind,state,created_at,updated_at) VALUES('thumbnail','running',1,1)",
        )
        .unwrap();
        recover_paused(&c, 6).unwrap();
        assert_eq!(
            c.query_row("SELECT state FROM jobs WHERE kind='thumbnail'", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            "running"
        );
    }
    #[test]
    fn interrupted_freeze_is_not_resumed_as_a_partial_range_and_cancel_is_preserved() {
        let c = db();
        let id = create(&c, "准备中", &"a".repeat(64), 1).unwrap();
        append_page(&c, id, &[photo(1)], 1).unwrap();
        recover_paused(&c, 2).unwrap();
        assert_eq!(load(&c, id).unwrap().1, "failed");
        assert!(super::super::status::retry(&c, id, 3).is_err());
        assert!(claim_next(&c, 3).unwrap().is_none());
        let id = create(&c, "取消准备", &"a".repeat(64), 1).unwrap();
        cancel(&c, id, 2).unwrap();
        fail_prepare(&c, id, "范围准备失败", 3).unwrap();
        assert_eq!(load(&c, id).unwrap().1, "cancelled");
    }
    #[test]
    fn empty_run_finishes_and_corrupt_jobs_never_wait_forever() {
        let c = db();
        let empty = create(&c, "空范围", &"a".repeat(64), 1).unwrap();
        finish_prepare(&c, empty, 2).unwrap();
        assert_eq!(load(&c, empty).unwrap().1, "done");
        c.execute("INSERT INTO jobs(kind,payload,state,created_at,updated_at) VALUES(?1,'{bad','pending',1,1)",[IMAGE]).unwrap();
        let broken = c.last_insert_rowid();
        let id = run(&c);
        c.execute("UPDATE jobs SET payload='{}' WHERE id=?1", [id])
            .unwrap();
        assert!(claim_next(&c, 3).unwrap().is_none());
        assert_eq!(
            c.query_row("SELECT state FROM jobs WHERE id=?1", [broken], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "failed"
        );
        assert_eq!(
            c.query_row("SELECT state FROM jobs WHERE id=?1", [id], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "failed"
        );
        let id = run(&c);
        c.execute("UPDATE jobs SET payload=json_set(payload,'$.version',2) WHERE kind=?1 AND json_extract(CASE WHEN json_valid(payload) THEN payload ELSE '{}' END,'$.runId')=?2",params![IMAGE,id]).unwrap();
        assert!(claim_next(&c, 4).unwrap().is_none());
        assert_eq!(load(&c, id).unwrap().1, "failed");
    }
}
