//! 全局机型基础曲线档案。只存稳定的拟合曲线，照片引用时另存快照。

use crate::develop::curve::Curve;
use crate::error::{Error, Result};
use rusqlite::{Connection, OptionalExtension};

#[derive(Debug, Clone, PartialEq)]
pub struct BaseCurveProfile {
    pub id: i64,
    pub camera_make: String,
    pub camera_model: String,
    pub name: String,
    pub points: Vec<[f32; 2]>,
    pub sample_count: i64,
}

/// 空品牌或型号不构成可靠的机型键。
#[must_use]
pub fn camera_key(make: &str, model: &str) -> Option<(String, String)> {
    let make = make.trim().to_lowercase();
    let model = model.trim().to_lowercase();
    (!make.is_empty() && !model.is_empty()).then_some((make, model))
}

fn read_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<BaseCurveProfile> {
    let json: String = row.get(4)?;
    let points = serde_json::from_str(&json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(BaseCurveProfile {
        id: row.get(0)?,
        camera_make: row.get(1)?,
        camera_model: row.get(2)?,
        name: row.get(3)?,
        points,
        sample_count: row.get(5)?,
    })
}

/// 只列当前机型的档案，按建档顺序稳定显示。
pub fn list(conn: &Connection, make: &str, model: &str) -> Result<Vec<BaseCurveProfile>> {
    let Some((make_key, model_key)) = camera_key(make, model) else {
        return Ok(Vec::new());
    };
    let mut stmt = conn.prepare(
        "SELECT id, camera_make, camera_model, name, points, sample_count FROM camera_base_curves \
         WHERE make_key = ?1 AND model_key = ?2 ORDER BY id",
    )?;
    let profiles = stmt
        .query_map([make_key, model_key], read_profile)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for profile in &profiles {
        Curve::from_points(profile.points.clone()).map_err(Error::Unsupported)?;
    }
    Ok(profiles)
}

/// 建一份不可变档案；后续命中只增加样本数，不悄悄改变旧照片的曲线。
pub fn create(
    conn: &Connection,
    make: &str,
    model: &str,
    points: &[[f32; 2]],
    now_ms: i64,
) -> Result<BaseCurveProfile> {
    let Some((make_key, model_key)) = camera_key(make, model) else {
        return Err(Error::Unsupported("缺少相机品牌或型号".into()));
    };
    Curve::from_points(points.to_vec()).map_err(Error::Unsupported)?;
    let number: i64 = conn.query_row(
        "SELECT count(*) + 1 FROM camera_base_curves WHERE make_key = ?1 AND model_key = ?2",
        [&make_key, &model_key],
        |row| row.get(0),
    )?;
    let name = format!("拟合档案 {number}");
    let json = serde_json::to_string(points).map_err(|e| Error::Unsupported(e.to_string()))?;
    conn.execute(
        "INSERT INTO camera_base_curves (camera_make,camera_model,make_key,model_key,name,points,created_at) \
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        rusqlite::params![make.trim(), model.trim(), make_key, model_key, name, json, now_ms],
    )?;
    Ok(BaseCurveProfile {
        id: conn.last_insert_rowid(),
        camera_make: make.trim().into(),
        camera_model: model.trim().into(),
        name,
        points: points.to_vec(),
        sample_count: 1,
    })
}

/// 命中已有档案的统计计数；曲线保持不可变。
pub fn record_match(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE camera_base_curves SET sample_count = sample_count + 1 WHERE id = ?1",
        [id],
    )?;
    Ok(())
}

/// 通过稳定 ID 取档案。
pub fn get(conn: &Connection, id: i64) -> Result<Option<BaseCurveProfile>> {
    let profile = conn.query_row(
        "SELECT id, camera_make, camera_model, name, points, sample_count FROM camera_base_curves WHERE id = ?1",
        [id], read_profile,
    ).optional()?;
    if let Some(ref profile) = profile {
        Curve::from_points(profile.points.clone()).map_err(Error::Unsupported)?;
    }
    Ok(profile)
}

/// 改名只更新展示名称；ID、拟合结果和照片引用保持稳定。
pub fn rename(
    conn: &Connection,
    id: i64,
    make: &str,
    model: &str,
    name: &str,
) -> Result<BaseCurveProfile> {
    let name = name.trim();
    if id <= 0 || name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control)
    {
        return Err(Error::Unsupported(
            "档案名称需要 1–80 个字符，且不能包含控制字符".into(),
        ));
    }
    let mut profile =
        get(conn, id)?.ok_or_else(|| Error::Unsupported("基础曲线档案不存在".into()))?;
    let key =
        camera_key(make, model).ok_or_else(|| Error::Unsupported("缺少相机品牌或型号".into()))?;
    if camera_key(&profile.camera_make, &profile.camera_model).as_ref() != Some(&key) {
        return Err(Error::Unsupported("基础曲线档案不属于当前机型".into()));
    }
    conn.execute(
        "UPDATE camera_base_curves SET name = ?2 WHERE id = ?1",
        rusqlite::params![id, name],
    )?;
    profile.name = name.to_owned();
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::migration::{self, Backups, DbKind};
    #[test]
    fn rename_preserves_identity_curve_samples_and_creation_time() {
        let mut db = Connection::open_in_memory().unwrap();
        migration::apply(&mut db, DbKind::App, Backups::none(), 1).unwrap();
        let profile = create(
            &db,
            "Panasonic",
            "DC-G9",
            &[[0.0, 0.0], [0.5, 0.65], [1.0, 1.0]],
            42,
        )
        .unwrap();
        record_match(&db, profile.id).unwrap();
        let renamed = rename(&db, profile.id, " PANASONIC ", " dc-g9 ", "  清冷蓝调 🌊  ").unwrap();
        assert_eq!(renamed.name, "清冷蓝调 🌊");
        assert_eq!(renamed.id, profile.id);
        assert_eq!(renamed.points, profile.points);
        assert_eq!(renamed.sample_count, 2);
        assert_eq!(get(&db, profile.id).unwrap().unwrap(), renamed);
        let time: i64 = db
            .query_row(
                "SELECT created_at FROM camera_base_curves WHERE id=?1",
                [profile.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(time, 42);
        let limit = "🌊".repeat(80);
        assert_eq!(
            rename(&db, profile.id, "Panasonic", "DC-G9", &limit)
                .unwrap()
                .name,
            limit
        );
        let second = create(&db, "Panasonic", "DC-G9", &[[0.0, 0.0], [1.0, 1.0]], 43).unwrap();
        assert!(
            rename(&db, second.id, "Panasonic", "DC-G9", &limit).is_ok(),
            "展示名允许相同，按 ID 区分"
        );
    }

    #[test]
    fn rename_rejects_bad_names_missing_id_and_other_models_without_mutation() {
        let mut db = Connection::open_in_memory().unwrap();
        migration::apply(&mut db, DbKind::App, Backups::none(), 1).unwrap();
        let profile = create(&db, "Panasonic", "DC-G9", &[[0.0, 0.0], [1.0, 1.0]], 42).unwrap();
        for name in ["", " ", "坏\n名称", "坏\u{7f}名称", &"中".repeat(81)] {
            assert!(rename(&db, profile.id, "Panasonic", "DC-G9", name).is_err());
        }
        for id in [-1, 0, 999] {
            assert!(rename(&db, id, "Panasonic", "DC-G9", "测试").is_err());
        }
        assert!(rename(&db, profile.id, "Panasonic", "GH5", "测试").is_err());
        assert!(rename(&db, profile.id, "", "DC-G9", "测试").is_err());
        assert_eq!(get(&db, profile.id).unwrap().unwrap(), profile);
    }

    #[test]
    fn profile_library_is_per_model_and_rejects_missing_camera() {
        let mut db = Connection::open_in_memory().unwrap();
        migration::apply(&mut db, DbKind::App, Backups::none(), 1).unwrap();
        assert!(list(&db, " ", "X").unwrap().is_empty());
        assert!(create(&db, "", "X", &[[0.0, 0.0], [1.0, 1.0]], 1).is_err());
        let first = create(
            &db,
            "Panasonic",
            "DC-G9",
            &[[0.0, 0.0], [0.5, 0.65], [1.0, 1.0]],
            1,
        )
        .unwrap();
        let second = create(
            &db,
            "panasonic",
            "dc-g9",
            &[[0.0, 0.0], [0.5, 0.55], [1.0, 1.0]],
            2,
        )
        .unwrap();
        assert_eq!(list(&db, "PANASONIC", "DC-G9").unwrap().len(), 2);
        assert!(list(&db, "Panasonic", "DC-GH5").unwrap().is_empty());
        record_match(&db, first.id).unwrap();
        assert_eq!(get(&db, first.id).unwrap().unwrap().sample_count, 2);
        assert_eq!(second.name, "拟合档案 2");
    }
}
