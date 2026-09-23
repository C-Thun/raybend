//! **编辑栈**（M3-W3）：一张照片的调整参数与曲线 —— 无损编辑的真相源。
//!
//! # 三个 issue 里只有 `latest` 进库
//!
//! 界面上能看到三个「版本」，但只有一个是**存下来的**（人类 2026-09-24 定）：
//!
//! | 版本 | 是什么 | 存哪 |
//! | --- | --- | --- |
//! | `SOOC` | 相机直出的 JPG（该资产有 JPG 时才有） | **不存**：它就是那个文件 |
//! | `RAW` | RAW 完整解码（该资产有 RAW 时才有） | **不存**：解码即得 |
//! | `latest` | 当前编辑结果 | **本模块**：`develop_stacks` 三张表 |
//!
//! 所以「这张照片编辑过吗」= `develop_stacks` 里有没有这一行；
//! 「重置全部」= 删掉这三张表里属于它的所有行。
//!
//! # 只存非默认值
//!
//! `develop_params` 里没有的行 = 这一项没动过（默认值的真相在
//! `src/api/develop-params.json`，**不进库**）—— 这样改默认值不需要数据迁移，
//! 而「重置这一项」就是删一行。
//!
//! # 校验
//!
//! 写入前逐条校验（未知参数 id / 值超范围 / 未知曲线通道 / 控制点不合法一律拒绝）：
//! 库里存着一份算不出来的参数，比当场报错难查得多。
//! 校验用的是**管线那套定义**（[`crate::develop`]），不是这里另抄一份。

use std::collections::BTreeMap;

use rusqlite::Connection;

use crate::develop::curve::{Curve, CurveChannel};
use crate::develop::params::spec;
use crate::error::{Error, Result};

/// 一张照片的编辑栈（`latest`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DevelopStack {
    /// 参数 id → 值（**只装非默认项**）
    pub params: BTreeMap<String, f64>,
    /// 通道 → 控制点（归一化 0..1）；缺省 = 恒等
    pub curves: BTreeMap<String, Vec<[f32; 2]>>,
}

impl DevelopStack {
    /// 什么都没动过吗（没动过 = 与 SOOC 一样，不必渲染）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.params.is_empty() && self.curves.is_empty()
    }

    /// 动过的项数（参数 + 曲线通道）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.params.len() + self.curves.len()
    }
}

/// 读一张照片的编辑栈（没有就是空栈 —— **不是错误**）。
///
/// # Errors
/// 数据库读失败（含坏数据：曲线 JSON 解不开）。
pub fn load(conn: &Connection, asset_id: i64) -> Result<DevelopStack> {
    let mut stack = DevelopStack::default();

    let mut statement = conn.prepare("SELECT param_id, value FROM develop_params WHERE asset_id = ?1")?;
    let rows = statement.query_map([asset_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
    })?;
    for row in rows {
        let (id, value) = row?;
        stack.params.insert(id, value);
    }

    let mut statement =
        conn.prepare("SELECT channel, points FROM develop_curves WHERE asset_id = ?1")?;
    let rows = statement.query_map([asset_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (channel, json) = row?;
        let points: Vec<[f32; 2]> = serde_json::from_str(&json)
            .map_err(|e| Error::Unsupported(format!("曲线 {channel} 的数据坏了：{e}")))?;
        stack.curves.insert(channel, points);
    }

    Ok(stack)
}

/// **覆盖式写入**一张照片的编辑栈（前端那份载荷的语义）。
///
/// 语义：栈里没有的项**删掉**、有的项写入 —— 于是「重置这一项」与「拖回默认」
/// 在数据上是同一件事（前端把该发的都发过来，落库就是这一份）。
///
/// # Errors
/// 校验不过（未知 id / 值非法 / 坏曲线）或数据库写失败。
pub fn save(conn: &Connection, asset_id: i64, stack: &DevelopStack, now_ms: i64) -> Result<usize> {
    // ① 先校验（**写之前**，别写一半才发现有错）
    for (id, value) in &stack.params {
        let Some(spec) = spec(id) else {
            return Err(Error::Unsupported(format!("未知的显影参数：{id}")));
        };
        if !spec.accepts(*value) {
            return Err(Error::Unsupported(format!(
                "参数 {id} 的值非法：{value}（允许 {}..{}）",
                spec.min, spec.max
            )));
        }
    }
    for (channel, points) in &stack.curves {
        if CurveChannel::parse(channel).is_none() {
            return Err(Error::Unsupported(format!("未知的曲线通道：{channel}")));
        }
        Curve::from_points(points.clone())
            .map_err(|e| Error::Unsupported(format!("曲线 {channel} 不合法：{e}")))?;
    }

    // ② 栈本体（没有就建一个）
    ensure_stack(conn, asset_id, now_ms)?;

    // ③ 参数：先删掉「这一份里没有的」，再 upsert 有的
    let mut changed = 0usize;
    {
        let mut statement =
            conn.prepare("SELECT param_id FROM develop_params WHERE asset_id = ?1")?;
        let existing: Vec<String> = statement
            .query_map([asset_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for id in existing {
            if !stack.params.contains_key(&id) {
                conn.execute(
                    "DELETE FROM develop_params WHERE asset_id = ?1 AND param_id = ?2",
                    rusqlite::params![asset_id, id],
                )?;
                changed += 1;
            }
        }
    }
    for (id, value) in &stack.params {
        conn.execute(
            "INSERT INTO develop_params (asset_id, param_id, value) VALUES (?1, ?2, ?3)
             ON CONFLICT(asset_id, param_id) DO UPDATE SET value = excluded.value",
            rusqlite::params![asset_id, id, value],
        )?;
        changed += 1;
    }

    // ④ 曲线：同一套「先删后写」
    {
        let mut statement =
            conn.prepare("SELECT channel FROM develop_curves WHERE asset_id = ?1")?;
        let existing: Vec<String> = statement
            .query_map([asset_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for channel in existing {
            if !stack.curves.contains_key(&channel) {
                conn.execute(
                    "DELETE FROM develop_curves WHERE asset_id = ?1 AND channel = ?2",
                    rusqlite::params![asset_id, channel],
                )?;
                changed += 1;
            }
        }
    }
    for (channel, points) in &stack.curves {
        let json = serde_json::to_string(points)
            .map_err(|e| Error::Unsupported(format!("曲线序列化失败：{e}")))?;
        conn.execute(
            "INSERT INTO develop_curves (asset_id, channel, points) VALUES (?1, ?2, ?3)
             ON CONFLICT(asset_id, channel) DO UPDATE SET points = excluded.points",
            rusqlite::params![asset_id, channel, json],
        )?;
        changed += 1;
    }

    // ⑤ 全空就把栈本体也删掉（「没编辑过」要能回到干净状态）
    if stack.is_empty() {
        conn.execute("DELETE FROM develop_stacks WHERE asset_id = ?1", [asset_id])?;
    }

    Ok(changed)
}

/// 写**一项**参数（`value = None` ⇒ 删掉这一项 = 回到基线）。
///
/// 撤销栈走的就是这一条：它一次只还原一项（不像 [`save`] 那样覆盖整份）。
///
/// # Errors
/// 未知参数 id / 值非法 / 数据库写失败。
pub fn set_param(
    conn: &Connection,
    asset_id: i64,
    param_id: &str,
    value: Option<f64>,
    now_ms: i64,
) -> Result<()> {
    let Some(spec) = spec(param_id) else {
        return Err(Error::Unsupported(format!("未知的显影参数：{param_id}")));
    };
    if let Some(value) = value
        && !spec.accepts(value)
    {
        return Err(Error::Unsupported(format!(
            "参数 {param_id} 的值非法：{value}（允许 {}..{}）",
            spec.min, spec.max
        )));
    }
    ensure_stack(conn, asset_id, now_ms)?;
    match value {
        Some(value) => {
            conn.execute(
                "INSERT INTO develop_params (asset_id, param_id, value) VALUES (?1, ?2, ?3)
                 ON CONFLICT(asset_id, param_id) DO UPDATE SET value = excluded.value",
                rusqlite::params![asset_id, param_id, value],
            )?;
        }
        None => {
            conn.execute(
                "DELETE FROM develop_params WHERE asset_id = ?1 AND param_id = ?2",
                rusqlite::params![asset_id, param_id],
            )?;
        }
    }
    prune_empty_stack(conn, asset_id)?;
    Ok(())
}

/// 写**一条**曲线（`points = None` ⇒ 删掉 = 回到恒等）。
///
/// # Errors
/// 未知通道 / 控制点不合法 / 数据库写失败。
pub fn set_curve(
    conn: &Connection,
    asset_id: i64,
    channel: &str,
    points: Option<&[[f32; 2]]>,
    now_ms: i64,
) -> Result<()> {
    if CurveChannel::parse(channel).is_none() {
        return Err(Error::Unsupported(format!("未知的曲线通道：{channel}")));
    }
    if let Some(points) = points {
        Curve::from_points(points.to_vec())
            .map_err(|e| Error::Unsupported(format!("曲线 {channel} 不合法：{e}")))?;
    }
    ensure_stack(conn, asset_id, now_ms)?;
    match points {
        Some(points) => {
            let json = serde_json::to_string(points)
                .map_err(|e| Error::Unsupported(format!("曲线序列化失败：{e}")))?;
            conn.execute(
                "INSERT INTO develop_curves (asset_id, channel, points) VALUES (?1, ?2, ?3)
                 ON CONFLICT(asset_id, channel) DO UPDATE SET points = excluded.points",
                rusqlite::params![asset_id, channel, json],
            )?;
        }
        None => {
            conn.execute(
                "DELETE FROM develop_curves WHERE asset_id = ?1 AND channel = ?2",
                rusqlite::params![asset_id, channel],
            )?;
        }
    }
    prune_empty_stack(conn, asset_id)?;
    Ok(())
}

/// 建栈本体（没有就建，有就更新 `updated_at`）。
fn ensure_stack(conn: &Connection, asset_id: i64, now_ms: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO develop_stacks (asset_id, created_at, updated_at) VALUES (?1, ?2, ?2)
         ON CONFLICT(asset_id) DO UPDATE SET updated_at = ?2",
        rusqlite::params![asset_id, now_ms],
    )?;
    Ok(())
}

/// 栈空了就把本体删掉（「没编辑过」要能回到干净状态）。
fn prune_empty_stack(conn: &Connection, asset_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM develop_stacks
          WHERE asset_id = ?1
            AND NOT EXISTS (SELECT 1 FROM develop_params WHERE asset_id = ?1)
            AND NOT EXISTS (SELECT 1 FROM develop_curves WHERE asset_id = ?1)",
        [asset_id],
    )?;
    Ok(())
}

/// 清掉一张照片的编辑栈（重置全部）。
///
/// # Errors
/// 数据库写失败。
pub fn clear(conn: &Connection, asset_id: i64) -> Result<usize> {
    // 外键是 ON DELETE CASCADE，删栈本体就够了
    Ok(conn.execute("DELETE FROM develop_stacks WHERE asset_id = ?1", [asset_id])?)
}

/// 这张照片编辑过吗（缩略图要不要走编辑管线）。
///
/// # Errors
/// 数据库读失败。
pub fn has_edits(conn: &Connection, asset_id: i64) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT
            (SELECT count(*) FROM develop_params WHERE asset_id = ?1)
          + (SELECT count(*) FROM develop_curves WHERE asset_id = ?1)",
        [asset_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::migration::{Backups, DbKind, apply};
    use crate::store::time::now_millis;

    /// 建一个跑过全部迁移的 catalog，并塞进一张资产。
    fn catalog_with_asset() -> (Connection, i64) {
        let mut conn = Connection::open_in_memory().expect("内存库");
        apply(&mut conn, DbKind::Catalog, Backups::none(), 0).expect("迁移");
        conn.execute(
            "INSERT INTO assets (rating, flag, imported_at, updated_at) VALUES (0, 'none', 0, 0)",
            [],
        )
        .expect("插资产");
        let asset_id = conn.last_insert_rowid();
        (conn, asset_id)
    }

    fn stack(params: &[(&str, f64)], curves: &[(&str, Vec<[f32; 2]>)]) -> DevelopStack {
        DevelopStack {
            params: params
                .iter()
                .map(|(id, value)| ((*id).to_string(), *value))
                .collect(),
            curves: curves
                .iter()
                .map(|(channel, points)| ((*channel).to_string(), points.clone()))
                .collect(),
        }
    }

    #[test]
    fn empty_catalog_has_no_edits() {
        let (conn, asset_id) = catalog_with_asset();
        let loaded = load(&conn, asset_id).expect("读");
        assert!(loaded.is_empty());
        assert!(!has_edits(&conn, asset_id).expect("查"));
    }

    #[test]
    fn save_and_load_round_trip() {
        let (conn, asset_id) = catalog_with_asset();
        let written = stack(
            &[("exposure", 0.5), ("temperature", 4200.0)],
            &[("rgb", vec![[0.0, 0.0], [0.5, 0.6], [1.0, 1.0]])],
        );
        save(&conn, asset_id, &written, now_millis()).expect("写");
        let loaded = load(&conn, asset_id).expect("读");
        assert_eq!(loaded, written);
        assert!(has_edits(&conn, asset_id).expect("查"));
    }

    #[test]
    fn save_is_replace_all() {
        let (conn, asset_id) = catalog_with_asset();
        save(
            &conn,
            asset_id,
            &stack(&[("exposure", 1.0), ("contrast", 20.0)], &[]),
            now_millis(),
        )
        .expect("第一次");
        // 第二次只带一个参数：另一个必须**被删掉**（这就是「重置这一项」）
        save(&conn, asset_id, &stack(&[("exposure", 0.25)], &[]), now_millis()).expect("第二次");
        let loaded = load(&conn, asset_id).expect("读");
        assert_eq!(loaded.params.len(), 1);
        assert_eq!(loaded.params.get("exposure"), Some(&0.25));
        assert!(!loaded.params.contains_key("contrast"));
    }

    #[test]
    fn saving_an_empty_stack_cleans_the_row_up() {
        let (conn, asset_id) = catalog_with_asset();
        save(&conn, asset_id, &stack(&[("exposure", 1.0)], &[]), now_millis()).expect("写");
        assert!(has_edits(&conn, asset_id).expect("查"));
        save(&conn, asset_id, &DevelopStack::default(), now_millis()).expect("清空");
        assert!(!has_edits(&conn, asset_id).expect("查"), "全空就该回到没编辑过");
        let stacks: i64 = conn
            .query_row("SELECT count(*) FROM develop_stacks", [], |row| row.get(0))
            .expect("数");
        assert_eq!(stacks, 0, "栈本体也要删掉（不留空壳）");
    }

    #[test]
    fn curves_replace_and_clear_per_channel() {
        let (conn, asset_id) = catalog_with_asset();
        save(
            &conn,
            asset_id,
            &stack(
                &[],
                &[
                    ("rgb", vec![[0.0, 0.0], [1.0, 1.0]]),
                    ("b", vec![[0.0, 0.0], [0.4, 0.3], [1.0, 1.0]]),
                ],
            ),
            now_millis(),
        )
        .expect("写");
        let loaded = load(&conn, asset_id).expect("读");
        assert_eq!(loaded.curves.len(), 2);

        // 只留 rgb：b 要被删掉
        save(
            &conn,
            asset_id,
            &stack(&[], &[("rgb", vec![[0.0, 0.1], [1.0, 1.0]])]),
            now_millis(),
        )
        .expect("第二次");
        let loaded = load(&conn, asset_id).expect("读");
        assert_eq!(loaded.curves.len(), 1);
        assert_eq!(loaded.curves.get("rgb"), Some(&vec![[0.0, 0.1], [1.0, 1.0]]));
    }

    #[test]
    fn illegal_input_is_rejected_before_writing_anything() {
        let (conn, asset_id) = catalog_with_asset();
        // 未知参数
        assert!(save(&conn, asset_id, &stack(&[("nope", 1.0)], &[]), now_millis()).is_err());
        // 超范围
        assert!(save(&conn, asset_id, &stack(&[("exposure", 99.0)], &[]), now_millis()).is_err());
        // 未知通道
        assert!(
            save(
                &conn,
                asset_id,
                &stack(&[], &[("x", vec![[0.0, 0.0], [1.0, 1.0]])]),
                now_millis()
            )
            .is_err()
        );
        // 坏控制点（只有一个点）
        assert!(
            save(
                &conn,
                asset_id,
                &stack(&[], &[("rgb", vec![[0.0, 0.0]])]),
                now_millis()
            )
            .is_err()
        );
        // 被拒之后**库里什么都不该有**（校验在写之前）
        assert!(!has_edits(&conn, asset_id).expect("查"));
        let stacks: i64 = conn
            .query_row("SELECT count(*) FROM develop_stacks", [], |row| row.get(0))
            .expect("数");
        assert_eq!(stacks, 0, "校验失败不许留下半个栈");
    }

    #[test]
    fn clear_removes_everything_and_cascades() {
        let (conn, asset_id) = catalog_with_asset();
        save(
            &conn,
            asset_id,
            &stack(&[("exposure", 1.0)], &[("rgb", vec![[0.0, 0.0], [1.0, 1.0]])]),
            now_millis(),
        )
        .expect("写");
        clear(&conn, asset_id).expect("清");
        assert!(!has_edits(&conn, asset_id).expect("查"));
        let params: i64 = conn
            .query_row("SELECT count(*) FROM develop_params", [], |row| row.get(0))
            .expect("数");
        assert_eq!(params, 0, "参数行要跟着栈一起走（外键 CASCADE）");
    }

    #[test]
    fn two_assets_do_not_see_each_other() {
        let (conn, first) = catalog_with_asset();
        conn.execute(
            "INSERT INTO assets (rating, flag, imported_at, updated_at) VALUES (0, 'none', 0, 0)",
            [],
        )
        .expect("插第二张");
        let second = conn.last_insert_rowid();
        save(&conn, first, &stack(&[("exposure", 1.0)], &[]), now_millis()).expect("写一");
        save(&conn, second, &stack(&[("blacks", -30.0)], &[]), now_millis()).expect("写二");
        assert_eq!(
            load(&conn, first).expect("读一").params.get("exposure"),
            Some(&1.0)
        );
        assert!(!load(&conn, first).expect("读一").params.contains_key("blacks"));
        assert!(!load(&conn, second).expect("读二").params.contains_key("exposure"));
    }

    #[test]
    fn single_item_writes_and_deletes() {
        let (conn, asset_id) = catalog_with_asset();
        set_param(&conn, asset_id, "exposure", Some(0.75), now_millis()).expect("写一项");
        assert_eq!(load(&conn, asset_id).expect("读").params.len(), 1);
        // 写第二项：第一项**不许**被碰（这正是与 `save` 的区别 —— 撤销栈靠它）
        set_param(&conn, asset_id, "contrast", Some(20.0), now_millis()).expect("写第二项");
        let loaded = load(&conn, asset_id).expect("读");
        assert_eq!(loaded.params.len(), 2);
        // 删一项
        set_param(&conn, asset_id, "exposure", None, now_millis()).expect("删");
        let loaded = load(&conn, asset_id).expect("读");
        assert!(!loaded.params.contains_key("exposure"));
        assert!(loaded.params.contains_key("contrast"));
        // 删光 ⇒ 栈本体也走掉
        set_param(&conn, asset_id, "contrast", None, now_millis()).expect("删光");
        assert!(!has_edits(&conn, asset_id).expect("查"));
    }

    #[test]
    fn single_curve_writes_and_deletes() {
        let (conn, asset_id) = catalog_with_asset();
        let points = [[0.0f32, 0.0], [0.5, 0.6], [1.0, 1.0]];
        set_curve(&conn, asset_id, "rgb", Some(&points), now_millis()).expect("写曲线");
        let loaded = load(&conn, asset_id).expect("读");
        assert_eq!(loaded.curves.get("rgb"), Some(&points.to_vec()));
        set_curve(&conn, asset_id, "rgb", None, now_millis()).expect("删曲线");
        assert!(!has_edits(&conn, asset_id).expect("查"));
        // 非法输入要被拒
        assert!(set_curve(&conn, asset_id, "x", Some(&points), now_millis()).is_err());
        assert!(set_param(&conn, asset_id, "exposure", Some(99.0), now_millis()).is_err());
    }

    #[test]
    fn the_migration_upgrades_an_existing_v4_catalog_without_losing_data() {
        // 「从 v4 升上来数据不丢」——这条测试是 `AGENTS.md` §2.16 的要求
        let mut conn = Connection::open_in_memory().expect("内存库");
        let v4 = &crate::store::migration::CATALOG_MIGRATIONS[..4];
        crate::store::migration::apply_list(&mut conn, DbKind::Catalog, v4, Backups::none(), 0)
            .expect("升到 v4");
        conn.execute(
            "INSERT INTO assets (rating, flag, imported_at, updated_at) VALUES (3, 'pick', 7, 7)",
            [],
        )
        .expect("插一张 v4 时代的资产");
        let asset_id = conn.last_insert_rowid();

        apply(&mut conn, DbKind::Catalog, Backups::none(), 0).expect("升到 v5");

        let (rating, flag): (i64, String) = conn
            .query_row(
                "SELECT rating, flag FROM assets WHERE id = ?1",
                [asset_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("老数据还在");
        assert_eq!(rating, 3);
        assert_eq!(flag, "pick");
        // 新表可用
        save(&conn, asset_id, &stack(&[("exposure", 0.5)], &[]), now_millis()).expect("写");
        assert!(has_edits(&conn, asset_id).expect("查"));
    }
}
