//! 一张原片的识别租约。磁盘、UID、源版本和当前 token 同时匹配才允许替换 AI。
use crate::{
    Result,
    store::{organization::PhotoRef, photo_tags},
};
use rusqlite::{Connection, OptionalExtension};
use std::path::{Path, PathBuf};
#[derive(Debug, Clone)]
pub struct Lease {
    pub photo: PhotoRef,
    pub token: String,
    pub input: super::source::Input,
    pub path: PathBuf,
    pub disk_signature: String,
}
#[derive(Debug)]
pub enum Prepared {
    Ready(Lease),
    Known,
    Locked,
    Unavailable(String),
    Removed,
}
pub fn prepare(
    conn: &Connection,
    root: &Path,
    photo: PhotoRef,
    pipeline: &str,
    rerun: bool,
    now: i64,
) -> Result<Prepared> {
    let row: Option<(String, u8)> = conn
        .query_row(
            "SELECT organization_uid,lock_level FROM assets WHERE id=?1",
            [photo.asset_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((uid, lock)) = row else {
        return Ok(Prepared::Removed);
    };
    if uid != photo.photo_uid {
        return Ok(Prepared::Removed);
    }
    let Some(input) = super::source::fresh(conn, root, photo.asset_id, now)? else {
        return Ok(Prepared::Unavailable("缺少原始照片或 RAW 预览来源".into()));
    };
    if lock >= 2 {
        return Ok(Prepared::Locked);
    }
    if !rerun
        && photo_tags::snapshot(conn, photo.asset_id)?
            .result
            .is_some_and(|r| {
                r.valid && r.pipeline_sha256 == pipeline && r.source_key == input.source_key
            })
    {
        return Ok(Prepared::Known);
    }
    let path = root.join(&input.file.rel_path);
    let disk_signature = crate::media::source::source_signature(&path)?;
    let token = photo_tags::begin_attempt(conn, photo.asset_id, &uid, &input.source_key)?;
    Ok(Prepared::Ready(Lease {
        photo,
        token,
        input,
        path,
        disk_signature,
    }))
}
/// 必须与队列的取消守卫由同一个 coordinator 临界区包围；本函数不持推理锁。
pub fn commit(
    conn: &Connection,
    root: &Path,
    lease: &Lease,
    pack: &super::pack::Verified,
    features: &[f32],
    pipeline: &str,
    zh: bool,
    now: i64,
) -> Result<bool> {
    let current = super::source::fresh(conn, root, lease.photo.asset_id, now)?;
    let Some(current) = current else {
        photo_tags::cancel_attempt(conn, lease.photo.asset_id, &lease.token)?;
        return Ok(false);
    };
    let lock: u8 = conn.query_row(
        "SELECT lock_level FROM assets WHERE id=?1",
        [lease.photo.asset_id],
        |r| r.get(0),
    )?;
    let disk = crate::media::source::source_signature(&root.join(&current.file.rel_path))?;
    if lock >= 2 || current.source_key != lease.input.source_key || disk != lease.disk_signature {
        photo_tags::cancel_attempt(conn, lease.photo.asset_id, &lease.token)?;
        return Ok(false);
    }
    let allowed:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM photo_ai_attempts p JOIN assets a ON a.id=p.asset_id WHERE p.asset_id=?1 AND p.token=?2 AND p.photo_uid=a.organization_uid AND p.source_key=?3)",rusqlite::params![lease.photo.asset_id,lease.token,current.source_key],|r|r.get(0))?;
    if !allowed {
        return Ok(false);
    }
    let result =
        super::result::scored(conn, pack, features, &current.source_key, pipeline, zh, now)?;
    photo_tags::commit_ai(
        conn,
        lease.photo.asset_id,
        &lease.token,
        &current.source_key,
        &result,
        now,
    )
}
pub fn discard(conn: &Connection, lease: &Lease) -> Result<bool> {
    photo_tags::cancel_attempt(conn, lease.photo.asset_id, &lease.token)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_original_replacement_and_uid_reuse_revoke_lease_without_reading_pixels() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("原片.JPG"), b"first").unwrap();
        let mut c = Connection::open_in_memory().unwrap();
        crate::store::pragma::apply(&c, true).unwrap();
        crate::store::migration::apply(
            &mut c,
            crate::store::migration::DbKind::Catalog,
            crate::store::migration::Backups::none(),
            1,
        )
        .unwrap();
        c.execute_batch("INSERT INTO assets(id,organization_uid,imported_at,updated_at) VALUES(1,'uid',1,1);INSERT INTO asset_files(id,asset_id,rel_path,rel_path_folded,role,ext,created_at,updated_at) VALUES(1,1,'原片.JPG','原片.jpg','bitmap','jpg',1,1)").unwrap();
        let photo = PhotoRef {
            repository_id: "库".into(),
            asset_id: 1,
            photo_uid: "uid".into(),
        };
        let Prepared::Ready(a) =
            prepare(&c, dir.path(), photo.clone(), &"a".repeat(64), false, 1).unwrap()
        else {
            panic!()
        };
        std::fs::write(&a.path, b"replacement").unwrap();
        let Prepared::Ready(b) =
            prepare(&c, dir.path(), photo.clone(), &"a".repeat(64), false, 2).unwrap()
        else {
            panic!()
        };
        assert_ne!(a.input.source_key, b.input.source_key);
        assert!(!discard(&c, &a).unwrap());
        assert!(discard(&c, &b).unwrap());
        c.execute(
            "UPDATE assets SET organization_uid='replacement' WHERE id=1",
            [],
        )
        .unwrap();
        assert!(matches!(
            prepare(&c, dir.path(), photo, &"a".repeat(64), false, 3).unwrap(),
            Prepared::Removed
        ));
        let photo = PhotoRef {
            repository_id: "库".into(),
            asset_id: 1,
            photo_uid: "replacement".into(),
        };
        c.execute("UPDATE assets SET lock_level=2 WHERE id=1", [])
            .unwrap();
        assert!(matches!(
            prepare(&c, dir.path(), photo, &"a".repeat(64), false, 3).unwrap(),
            Prepared::Locked
        ));
    }
}
