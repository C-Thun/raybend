//! Reviewed photo interpretation changes reuse the catalog transaction and undo.
use super::{
    develop::{self, DevelopStack, Setting},
    marking::{self, ChangeSet, Op},
};
use crate::{Error, Result, color::PhotoColorState};
use rusqlite::{Connection, OptionalExtension};
use std::collections::BTreeSet;

pub const MAX_PHOTOS: usize = 256;
#[derive(Clone, Debug)]
pub struct PreparedChange {
    pub asset_id: i64,
    pub before: DevelopStack,
    pub after: PhotoColorState,
}

pub fn selection(ids: &[i64]) -> Result<Vec<i64>> {
    if ids.is_empty() || ids.len() > MAX_PHOTOS || ids.iter().any(|id| *id <= 0) {
        return Err(Error::Unsupported("批量指定需要 1–256 张有效照片".into()));
    }
    Ok(ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

/// Call inside the existing single-writer transaction. Check every reviewed entry
/// before changing any of them; a concurrent edit/lock invalidates the whole review.
pub fn apply_reviewed(conn: &Connection, entries: &[PreparedChange]) -> Result<ChangeSet> {
    if entries.is_empty() {
        return Ok(ChangeSet::new("色彩管理", Vec::new()));
    }
    let ids = selection(
        &entries
            .iter()
            .map(|entry| entry.asset_id)
            .collect::<Vec<_>>(),
    )?;
    if ids.len() != entries.len() {
        return Err(Error::Unsupported("批量指定包含重复照片".into()));
    }
    let mut ops = Vec::with_capacity(entries.len());
    for entry in entries {
        entry
            .after
            .validate_frozen()
            .map_err(|e| Error::Unsupported(e.into()))?;
        let lock: Option<i64> = conn
            .query_row(
                "SELECT lock_level FROM assets WHERE id=?1",
                [entry.asset_id],
                |row| row.get(0),
            )
            .optional()?;
        if lock.is_none_or(|level| level >= i64::from(marking::LOCK_NO_EDIT))
            || develop::load(conn, entry.asset_id)? != entry.before
        {
            return Err(Error::Unsupported(
                "预检后照片或编辑状态已改变，请重新预检".into(),
            ));
        }
        let after = Some(serde_json::to_string(&entry.after)?);
        let before = entry.before.setting_value(Setting::Color);
        if before != after {
            ops.push(Op::DevelopSetting {
                asset_id: entry.asset_id,
                key: Setting::Color.key().into(),
                before,
                after,
            });
        }
    }
    let change = ChangeSet::new("色彩管理", ops);
    marking::apply(conn, &change)?;
    Ok(change)
}

#[cfg(test)]
mod tests {
    use super::super::migration::{Backups, DbKind, apply};
    use super::*;
    fn catalog() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn, DbKind::Catalog, Backups::none(), 0).unwrap();
        for _ in 0..2 {
            conn.execute(
                "INSERT INTO assets(rating,flag,imported_at,updated_at) VALUES(0,'none',0,0)",
                [],
            )
            .unwrap();
        }
        conn
    }
    fn changes(conn: &Connection) -> Vec<PreparedChange> {
        (1..=2)
            .map(|asset_id| PreparedChange {
                asset_id,
                before: develop::load(conn, asset_id).unwrap(),
                after: PhotoColorState::new_pipeline(crate::color::SourceColor::AssumedSrgb),
            })
            .collect()
    }
    #[test]
    fn bounded_selection_rejects_invalid_ids_and_deduplicates() {
        assert_eq!(selection(&[2, 1, 2]).unwrap(), vec![1, 2]);
        for ids in [vec![], vec![0], vec![-1], vec![1; 257]] {
            assert!(selection(&ids).is_err());
        }
        assert_eq!(
            selection(&(1..=256).collect::<Vec<_>>()).unwrap().len(),
            256
        );
    }
    #[test]
    fn existing_transaction_undo_restores_both_photos_and_preserves_adjustments() {
        let mut conn = catalog();
        develop::set_param(&conn, 1, "exposure", Some(0.5), 0).unwrap();
        let entries = changes(&conn);
        let tx = conn.transaction().unwrap();
        let change = apply_reviewed(&tx, &entries).unwrap();
        tx.commit().unwrap();
        assert_eq!(change.ops.len(), 2);
        assert_eq!(
            develop::load(&conn, 1).unwrap().params.get("exposure"),
            Some(&0.5)
        );
        marking::apply(&conn, &change.invert()).unwrap();
        for entry in entries {
            assert_eq!(develop::load(&conn, entry.asset_id).unwrap(), entry.before);
        }
    }
    #[test]
    fn stale_last_photo_or_lock_cannot_partially_modify_first_photo() {
        let conn = catalog();
        let entries = changes(&conn);
        develop::set_param(&conn, 2, "exposure", Some(1.0), 0).unwrap();
        assert!(apply_reviewed(&conn, &entries).is_err());
        assert!(develop::load(&conn, 1).unwrap().color.is_none());
        let entries = changes(&conn);
        conn.execute("UPDATE assets SET lock_level=2 WHERE id=2", [])
            .unwrap();
        assert!(apply_reviewed(&conn, &entries).is_err());
        assert!(develop::load(&conn, 1).unwrap().color.is_none());
    }
    #[test]
    fn duplicate_and_unresolved_states_are_rejected_before_changes() {
        let conn = catalog();
        let mut entries = changes(&conn);
        entries[1] = entries[0].clone();
        assert!(apply_reviewed(&conn, &entries).is_err());
        entries.truncate(1);
        entries[0].after.source = crate::color::SourceColor::Auto;
        assert!(apply_reviewed(&conn, &entries).is_err());
        assert!(develop::load(&conn, 1).unwrap().color.is_none());
    }
}
