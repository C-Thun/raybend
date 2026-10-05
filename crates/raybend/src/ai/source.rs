//! 原始识别来源：稳定文件身份/版本，不包含路径、编辑栈或缩略图缓存。
use crate::{
    Result,
    store::{assets, photo_tags},
};
use rusqlite::Connection;
#[derive(Debug, Clone)]
pub struct Input {
    pub file: assets::FileRow,
    pub source_key: String,
}
/// 位图优先，RAW 仅供 worker 提取内嵌预览。同类选择用固定文件行 ID，改名不换来源。
pub fn selected(conn: &Connection, asset_id: i64) -> Result<Option<Input>> {
    let mut files = assets::files_of_asset(conn, asset_id)?;
    files.retain(|f| !f.missing && matches!(f.role.as_str(), "bitmap" | "raw"));
    files.sort_by_key(|f| (f.role != "bitmap", f.row_id));
    files
        .into_iter()
        .next()
        .map(|file| {
            let bytes = serde_json::to_vec(&(
                "original-v1",
                file.row_id,
                &file.role,
                file.identity,
                file.size_bytes,
                file.mtime_ms,
            ))?;
            Ok(Input {
                source_key: crate::fs_asset::hash_bytes(&bytes),
                file,
            })
        })
        .transpose()
}
/// 调度与提交各重读原片的磁盘状态；调用方须放进现有 catalog 写事务。
/// 仅检查这张照片的候选文件，位图暂缺时可退到 RAW，不扫描目录。
pub fn fresh(
    conn: &Connection,
    root: &std::path::Path,
    asset_id: i64,
    now: i64,
) -> Result<Option<Input>> {
    let mut files = assets::files_of_asset(conn, asset_id)?;
    files.retain(|f| matches!(f.role.as_str(), "bitmap" | "raw"));
    files.sort_by_key(|f| (f.role != "bitmap", f.row_id));
    for file in files {
        let relative = std::path::Path::new(&file.rel_path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(crate::Error::Unsupported("AI 原片路径超出库范围".into()));
        }
        let path = root.join(relative);
        match std::fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => {
                assets::update_stat(
                    conn,
                    file.row_id,
                    metadata.len(),
                    metadata
                        .modified()
                        .ok()
                        .map(crate::store::time::from_system_time),
                    metadata
                        .created()
                        .ok()
                        .map(crate::store::time::from_system_time),
                    now,
                )?;
                assets::update_identity(
                    conn,
                    file.row_id,
                    crate::store::file_id::FileId::try_read(&path),
                )?;
                assets::clear_missing(conn, file.row_id, now)?;
                break;
            }
            Ok(_) => return Err(crate::Error::Unsupported("AI 原片不是普通文件".into())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                assets::mark_missing(conn, file.row_id, now)?
            }
            Err(error) => return Err(error.into()),
        }
    }
    let input = selected(conn, asset_id)?;
    let key = input
        .as_ref()
        .map_or("missing-original-v1", |s| s.source_key.as_str());
    photo_tags::invalidate_source(conn, asset_id, key)?;
    Ok(input)
}

/// 在现有差分事务里调用，仅复查受影响且有 AI 证据/租约的资产。
pub fn reconcile(conn: &Connection, ids: &[i64]) -> Result<usize> {
    let mut changed = 0;
    for id in ids
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
    {
        let has:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM photo_ai_results WHERE asset_id=?1) OR EXISTS(SELECT 1 FROM photo_ai_attempts WHERE asset_id=?1)",[id],|r|r.get(0))?;
        if !has {
            continue;
        }
        let key =
            selected(conn, id)?.map_or_else(|| "missing-original-v1".into(), |s| s.source_key);
        changed += usize::from(photo_tags::invalidate_source(conn, id, &key)?);
    }
    Ok(changed)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        crate::store::pragma::apply(&c, true).unwrap();
        crate::store::migration::apply(
            &mut c,
            crate::store::migration::DbKind::Catalog,
            crate::store::migration::Backups::none(),
            1,
        )
        .unwrap();
        c.execute_batch("INSERT INTO assets(id,organization_uid,imported_at,updated_at) VALUES(1,'uid',1,1);INSERT INTO asset_files(id,asset_id,rel_path,rel_path_folded,role,ext,size_bytes,mtime_ms,created_at,updated_at) VALUES(1,1,'照片.NEF','照片.nef','raw','nef',20,1,1,1),(2,1,'照片.JPG','照片.jpg','bitmap','jpg',10,1,1,1)").unwrap();
        c
    }
    #[test]
    fn rename_does_not_change_version_but_replacement_and_source_choice_do() {
        let c = db();
        let first = selected(&c, 1).unwrap().unwrap();
        assert_eq!(first.file.row_id, 2);
        assets::update_path(&c, 2, "长中文目录/海边.JPG", 2).unwrap();
        assert_eq!(
            selected(&c, 1).unwrap().unwrap().source_key,
            first.source_key
        );
        assets::update_stat(&c, 2, 11, Some(3), None, 3).unwrap();
        assert_ne!(
            selected(&c, 1).unwrap().unwrap().source_key,
            first.source_key
        );
        assets::mark_missing(&c, 2, 4).unwrap();
        assert_eq!(selected(&c, 1).unwrap().unwrap().file.row_id, 1);
        assets::mark_missing(&c, 1, 4).unwrap();
        assert!(selected(&c, 1).unwrap().is_none());
        assert!(selected(&c, 999).unwrap().is_none());
    }
    #[test]
    fn source_change_revokes_attempt_but_path_change_keeps_it() {
        let c = db();
        let uid: String = c
            .query_row("SELECT organization_uid FROM assets WHERE id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        let key = selected(&c, 1).unwrap().unwrap().source_key;
        let token = photo_tags::begin_attempt(&c, 1, &uid, &key).unwrap();
        assets::update_path(&c, 2, "重命名.JPG", 2).unwrap();
        assert_eq!(reconcile(&c, &[1, 1]).unwrap(), 0);
        assets::update_stat(&c, 2, 12, Some(4), None, 4).unwrap();
        reconcile(&c, &[1]).unwrap();
        assert!(!photo_tags::cancel_attempt(&c, 1, &token).unwrap());
    }
}
