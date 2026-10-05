//! AI 任务面板的有界读取与明确对象动作；状态仍在既有 jobs。
use crate::{Error, Result};
use rusqlite::{Connection, params};
use serde::Serialize;
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: i64,
    pub label: String,
    pub state: String,
    pub done: i64,
    pub skipped: i64,
    pub failed: i64,
    pub waiting: i64,
    pub total: i64,
    pub error: Option<String>,
}
pub fn tasks(conn: &Connection) -> Result<Vec<Task>> {
    let mut q=conn.prepare("SELECT id,payload,state,error FROM jobs WHERE kind='photo_ai_run' ORDER BY id DESC LIMIT 100")?;
    let runs = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out = Vec::new();
    for (id, payload, state, error) in runs {
        let label = serde_json::from_str::<super::jobs::Run>(&payload)
            .map_or_else(|_| "损坏任务".into(), |r| r.label);
        let(total,done,skipped,failed,waiting)=conn.query_row("SELECT COUNT(*),COALESCE(SUM(state='done'),0),COALESCE(SUM(state='skipped'),0),COALESCE(SUM(state='failed'),0),COALESCE(SUM(state='waiting'),0) FROM jobs WHERE kind='photo_ai_image' AND json_extract(CASE WHEN json_valid(payload) THEN payload ELSE '{}' END,'$.runId')=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
        out.push(Task {
            id,
            label,
            state,
            done,
            skipped,
            failed,
            waiting,
            total,
            error,
        });
    }
    Ok(out)
}
pub fn retry(conn: &Connection, id: i64, now: i64) -> Result<bool> {
    if !super::jobs::context(conn, id)?.prepared {
        return Err(Error::Unsupported(
            "范围尚未冻结完成，请新建识别任务".into(),
        ));
    }
    let tx = conn.unchecked_transaction()?;
    let changed=tx.execute("UPDATE jobs SET state='paused',error=NULL,updated_at=?2 WHERE id=?1 AND kind='photo_ai_run' AND state='failed'",params![id,now])?>0;
    if changed {
        tx.execute("UPDATE jobs SET state='pending',attempts=0,error=NULL,updated_at=?2 WHERE kind='photo_ai_image' AND state='failed' AND json_extract(CASE WHEN json_valid(payload) THEN payload ELSE '{}' END,'$.runId')=?1",params![id,now])?;
    }
    tx.commit()?;
    Ok(changed)
}
pub fn action(conn: &Connection, id: i64, action: &str, now: i64) -> Result<bool> {
    match action {
        "pause" => super::jobs::pause(conn, id, now),
        "resume" => super::jobs::resume(conn, id, now),
        "cancel" => super::jobs::cancel(conn, id, now),
        "retry" => retry(conn, id, now),
        _ => Err(Error::Unsupported("AI 任务动作无效".into())),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn summaries_retry_only_failures_and_keep_successful_members() {
        let mut c = Connection::open_in_memory().unwrap();
        crate::store::migration::apply(
            &mut c,
            crate::store::migration::DbKind::App,
            crate::store::migration::Backups::none(),
            1,
        )
        .unwrap();
        let id = super::super::jobs::create(&c, "七类测试", &"a".repeat(64), 1).unwrap();
        let items = (1..=2)
            .map(|asset_id| crate::store::organization::PhotoRef {
                repository_id: "库".into(),
                photo_uid: format!("uid{asset_id}"),
                asset_id,
            })
            .collect::<Vec<_>>();
        super::super::jobs::append_page(&c, id, &items, 1).unwrap();
        super::super::jobs::finish_prepare(&c, id, 2).unwrap();
        let claim = super::super::jobs::claim_next(&c, 3).unwrap().unwrap();
        super::super::jobs::complete(&c, &claim, 4).unwrap();
        c.execute_batch(
            "UPDATE jobs SET state='failed' WHERE state='pending' OR kind='photo_ai_run'",
        )
        .unwrap();
        assert!(retry(&c, id, 5).unwrap());
        let row = tasks(&c).unwrap().remove(0);
        assert_eq!(
            (row.done, row.failed, row.total, row.state),
            (1, 0, 2, "paused".into())
        );
        assert!(action(&c, id, "arbitrary", 6).is_err());
    }
}
