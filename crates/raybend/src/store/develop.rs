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

use rusqlite::{Connection, OptionalExtension};

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
    /// **拍摄色温**（K）—— 色温拉杆的基线。
    ///
    /// 它是 issue 的一部分：色温参数是**绝对 K**，而「目标 K 相对谁」取决于这张照片
    /// 拍摄时的白平衡。不存它，缩略图那条路（读不到 RAW 元数据）就会用 6250 兜底，
    /// 于是同一份参数在编辑器与缩略图里渲染出**两种颜色**。
    pub as_shot_k: Option<f32>,
}

impl DevelopStack {
    /// 什么都没动过吗（没动过 = 与 SOOC 一样，不必渲染）。
    ///
    /// `as_shot_k` **不算「动过」**：它只是色温的解释基准，参数一个都没改就是没编辑过。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.params.is_empty() && self.curves.is_empty()
    }

    /// 动过的项数（参数 + 曲线通道）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.params.len() + self.curves.len()
    }

    /// **这份编辑栈的稳定指纹**（缓存键用）。
    ///
    /// 同一份参数在任何机器、任何时刻都算出同一个值（FNV-1a over 规范化文本）——
    /// 缩略图缓存靠它区分「编辑前 / 编辑后」，改一个参数就该重渲染。
    ///
    /// `as_shot_k` 也算进去：它变了，色温的解释就变了，画面跟着变。
    #[must_use]
    pub fn signature(&self) -> u64 {
        let mut text = String::new();
        for (id, value) in &self.params {
            text.push_str(id);
            text.push('=');
            text.push_str(&format!("{value:.6}"));
            text.push(';');
        }
        text.push('|');
        for (channel, points) in &self.curves {
            text.push_str(channel);
            text.push(':');
            for point in points {
                text.push_str(&format!("{:.4},{:.4};", point[0], point[1]));
            }
        }
        text.push('|');
        match self.as_shot_k {
            Some(kelvin) => text.push_str(&format!("k{kelvin:.0}")),
            None => text.push_str("k-"),
        }

        // FNV-1a（64 位）：短、稳定、够散 —— 这里不需要密码学强度
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in text.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }
}

/// 读一张照片的编辑栈（没有就是空栈 —— **不是错误**）。
///
/// # Errors
/// 数据库读失败（含坏数据：曲线 JSON 解不开）。
pub fn load(conn: &Connection, asset_id: i64) -> Result<DevelopStack> {
    let as_shot_k = conn
        .query_row(
            "SELECT as_shot_k FROM develop_stacks WHERE asset_id = ?1",
            [asset_id],
            |row| row.get::<_, Option<f64>>(0),
        )
        .optional()?
        .flatten()
        .map(|value| value as f32);
    let mut stack = DevelopStack {
        as_shot_k,
        ..DevelopStack::default()
    };

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

    // ② 栈本体（没有就建一个；as-shot 跟着一起写）
    ensure_stack(conn, asset_id, now_ms)?;
    conn.execute(
        "UPDATE develop_stacks SET as_shot_k = ?2 WHERE asset_id = ?1",
        rusqlite::params![asset_id, stack.as_shot_k.map(f64::from)],
    )?;

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

/// 这张照片该显示哪个**版本**（人类 2026-09-24 定的规则）。
///
/// 三个「issue」里只有 `latest` 是存下来的，另两个是虚拟的（见模块文档）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueChoice {
    /// 相机直出的 JPG（该资产有 JPG 且没编辑过）。
    Sooc,
    /// RAW 的基础解码（没有 JPG、也没编辑过）。
    Raw,
    /// 编辑结果（有编辑栈）。
    Latest,
}

/// 解析这张照片该显示哪个版本。
///
/// 规则（人类 2026-09-24）：
///
/// | 资产 | 没编辑过 | 编辑过 |
/// | --- | --- | --- |
/// | 只有 JPG | `SOOC` | `latest` |
/// | 只有 RAW | `RAW`（基础解码一次） | `latest` |
/// | JPG + RAW | `SOOC`（默认看相机直出） | `latest` |
///
/// # Errors
/// 数据库读失败。
pub fn choose_issue(conn: &Connection, asset_id: i64) -> Result<IssueChoice> {
    if has_edits(conn, asset_id)? {
        return Ok(IssueChoice::Latest);
    }
    let files = crate::store::assets::files_of_asset(conn, asset_id)?;
    let has_bitmap = files
        .iter()
        .any(|file| file.role == "bitmap" && !file.missing);
    Ok(if has_bitmap {
        IssueChoice::Sooc
    } else {
        IssueChoice::Raw
    })
}

/// **这张照片要不要生成 preview**（`IMAGING.md` §4，人类 2026-09-24 定）。
///
/// 一句话：**只有「编辑过的」才有 preview** —— 没编辑过时 `SOOC` / `RAW` 的内置位图
/// 就代替 preview，**不额外生成**。
///
/// 生成节点（进编辑 / 退出编辑）在 `src-tauri` 那条命令里，这里只回答「要不要」。
#[must_use]
pub fn needs_preview(choice: IssueChoice, stack: &DevelopStack) -> bool {
    choice == IssueChoice::Latest && !stack.is_empty()
}

/// 编辑**落在哪个文件上**（人类 2026-09-24 定：编辑器里可切，**默认 RAW**）。
///
/// 它不是「显示哪个 issue」（那是 [`IssueChoice`]），而是「这次编辑拿哪个当底」——
/// 将来每个 issue 会带上「基于 sooc 还是基于 raw 编辑」的标签（登记在 `FUTURE.md`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditBase {
    /// 相机直出的位图（JPG）—— 「基于 SOOC 编辑」
    Sooc,
    /// RAW（基础解码）—— **默认**
    #[default]
    Raw,
}

impl EditBase {
    /// 解析前端传来的字符串；**认不出给 `None`**（调用方报错，不静默回退到默认值）。
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "sooc" => Some(Self::Sooc),
            "raw" => Some(Self::Raw),
            _ => None,
        }
    }

    /// 反过来：写进载荷 / 日志用的字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sooc => "sooc",
            Self::Raw => "raw",
        }
    }

    /// **这个文件属于哪一侧**（从源文件推，不靠调用方传）。
    ///
    /// 大图缓存的命名要按基准分文件（`display::full_cache`）：
    /// 「渲染用的哪个文件，就是哪个基准」是唯一不会错的口径 ——
    /// 让调用方自己报一个基准，就一定会出现「报的和用的不是同一侧」。
    #[must_use]
    pub fn of_file(path: &std::path::Path) -> Self {
        let is_raw = path.file_name().is_some_and(|name| {
            crate::media::kind::kind_of_file(&name.to_string_lossy())
                == crate::media::kind::MediaKind::Raw
        });
        if is_raw { Self::Raw } else { Self::Sooc }
    }
}

/// 这张照片有没有可用的位图 / RAW（界面据此**禁用**切不过去的那一侧）。
///
/// 返回 `(has_bitmap, has_raw)`。
///
/// # Errors
/// 数据库读失败。
pub fn edit_base_available(conn: &Connection, asset_id: i64) -> Result<(bool, bool)> {
    let files = crate::store::assets::files_of_asset(conn, asset_id)?;
    let has = |role: &str| files.iter().any(|file| file.role == role && !file.missing);
    Ok((has("bitmap"), has("raw")))
}

/// 编辑器该**编辑哪个文件**（「编辑落在 RAW 上」，`REPOSITORY.md` §4.1）。
///
/// `base` 是用户在编辑器里选的编辑基准（人类 2026-09-24）：
///
/// * [`EditBase::Raw`]（默认）→ 有 RAW 就给 RAW，没有就退回位图；
/// * [`EditBase::Sooc`] → 有可用位图就给位图，没有就退回 RAW（只有 RAW 的照片没得选）。
///
/// 位图与 RAW 放在不同目录（`_RAW/`）这件事的规则**只在这里**实现一次 ——
/// 前端不许自己拼 `_RAW/` 路径。
///
/// # Errors
/// 数据库读失败。
pub fn edit_target(conn: &Connection, asset_id: i64, base: EditBase) -> Result<Option<String>> {
    let files = crate::store::assets::files_of_asset(conn, asset_id)?;
    let pick = |role: &str| {
        files
            .iter()
            .find(|file| file.role == role && !file.missing)
            .map(|file| file.rel_path.clone())
    };
    let raw = pick("raw");
    let bitmap = pick("bitmap");
    Ok(match base {
        EditBase::Raw => raw.or(bitmap),
        EditBase::Sooc => bitmap.or(raw),
    })
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
            as_shot_k: None,
        }
    }

    #[test]
    fn preview_only_for_edited_latest() {
        let edited = stack(&[("exposure", 0.5)], &[]);
        let untouched = stack(&[], &[]);
        // 编辑过的（latest）→ 要 preview
        assert!(needs_preview(IssueChoice::Latest, &edited));
        // 没编辑过 → sooc/raw 内置代替 preview，不生成
        assert!(!needs_preview(IssueChoice::Sooc, &untouched));
        assert!(!needs_preview(IssueChoice::Raw, &untouched));
        // 空栈的 latest 是自相矛盾的输入，但也不能生成（宁可少生成）
        assert!(!needs_preview(IssueChoice::Latest, &untouched));
        // 非 latest 却带着栈（理论上进不来）→ 同样不生成
        assert!(!needs_preview(IssueChoice::Sooc, &edited));
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

    /// 塞一个文件行（`role` = bitmap / raw）。
    fn add_file(conn: &Connection, asset_id: i64, role: &str, rel_path: &str) {
        conn.execute(
            "INSERT INTO asset_files (asset_id, role, rel_path, rel_path_folded, ext,
                                      created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3, 'x', 0, 0)",
            rusqlite::params![asset_id, role, rel_path],
        )
        .expect("插文件");
    }

    #[test]
    fn signature_is_stable_and_sensitive() {
        let base = stack(&[("exposure", 1.0), ("contrast", 20.0)], &[]);
        let same = stack(&[("contrast", 20.0), ("exposure", 1.0)], &[]);
        assert_eq!(base.signature(), same.signature(), "顺序不同、内容相同 ⇒ 同一个指纹");
        let changed = stack(&[("exposure", 1.5), ("contrast", 20.0)], &[]);
        assert_ne!(base.signature(), changed.signature(), "改一个参数就要变");
        // 曲线参与指纹
        let mut with_curve = base.clone();
        with_curve
            .curves
            .insert("rgb".to_string(), vec![[0.0, 0.0], [0.5, 0.6], [1.0, 1.0]]);
        assert_ne!(base.signature(), with_curve.signature());
        // as-shot 也参与（它变了画面的色温解释就变了）
        let mut with_baseline = base.clone();
        with_baseline.as_shot_k = Some(4300.0);
        assert_ne!(base.signature(), with_baseline.signature());
        // 空栈也有指纹（不 panic）
        assert_ne!(DevelopStack::default().signature(), 0);
    }

    #[test]
    fn issue_choice_follows_the_rules() {
        // 只有 JPG：没编辑 → SOOC
        let (conn, asset_id) = catalog_with_asset();
        add_file(&conn, asset_id, "bitmap", "photos/a.jpg");
        assert_eq!(choose_issue(&conn, asset_id).expect("解析"), IssueChoice::Sooc);

        // 编辑过 → latest（不管有没有 JPG）
        save(&conn, asset_id, &stack(&[("exposure", 1.0)], &[]), now_millis()).expect("写");
        assert_eq!(choose_issue(&conn, asset_id).expect("解析"), IssueChoice::Latest);

        // 只有 RAW：没编辑 → RAW
        let (conn, raw_asset) = catalog_with_asset();
        add_file(&conn, raw_asset, "raw", "photos/_RAW/a.RW2");
        assert_eq!(choose_issue(&conn, raw_asset).expect("解析"), IssueChoice::Raw);

        // JPG + RAW：没编辑 → SOOC（默认看相机直出）
        let (conn, both) = catalog_with_asset();
        add_file(&conn, both, "bitmap", "photos/a.jpg");
        add_file(&conn, both, "raw", "photos/_RAW/a.RW2");
        assert_eq!(choose_issue(&conn, both).expect("解析"), IssueChoice::Sooc);

        // 文件标记缺失（磁盘上没了）：不能当成「有」
        let (conn, missing) = catalog_with_asset();
        add_file(&conn, missing, "bitmap", "photos/gone.jpg");
        conn.execute("UPDATE asset_files SET missing_since = 1", [])
            .expect("标缺失");
        assert_eq!(choose_issue(&conn, missing).expect("解析"), IssueChoice::Raw);
    }

    #[test]
    fn edit_target_follows_the_base() {
        let (conn, asset_id) = catalog_with_asset();
        add_file(&conn, asset_id, "bitmap", "photos/2026/a.jpg");
        assert_eq!(
            edit_target(&conn, asset_id, EditBase::Raw).expect("解析").as_deref(),
            Some("photos/2026/a.jpg"),
            "只有 JPG 时编辑 JPG（RAW 基准也没 RAW 可给）"
        );
        assert_eq!(
            edit_target(&conn, asset_id, EditBase::Sooc).expect("解析").as_deref(),
            Some("photos/2026/a.jpg")
        );
        add_file(&conn, asset_id, "raw", "photos/2026/_RAW/a.RW2");
        assert_eq!(
            edit_target(&conn, asset_id, EditBase::Raw).expect("解析").as_deref(),
            Some("photos/2026/_RAW/a.RW2"),
            "RAW 基准：编辑落在 RAW 上"
        );
        assert_eq!(
            edit_target(&conn, asset_id, EditBase::Sooc).expect("解析").as_deref(),
            Some("photos/2026/a.jpg"),
            "SOOC 基准：编辑落在相机直出的位图上"
        );

        // 只有 RAW 的照片：SOOC 基准也退回 RAW（没得选）
        let (conn, raw_only) = catalog_with_asset();
        add_file(&conn, raw_only, "raw", "photos/_RAW/b.RW2");
        assert_eq!(
            edit_target(&conn, raw_only, EditBase::Sooc).expect("解析").as_deref(),
            Some("photos/_RAW/b.RW2")
        );

        // 位图标记缺失（磁盘上没了）：SOOC 基准也不许选中它
        let (conn, gone) = catalog_with_asset();
        add_file(&conn, gone, "bitmap", "photos/gone.jpg");
        add_file(&conn, gone, "raw", "photos/_RAW/c.RW2");
        conn.execute("UPDATE asset_files SET missing_since = 1 WHERE role = 'bitmap'", [])
            .expect("标缺失");
        assert_eq!(
            edit_target(&conn, gone, EditBase::Sooc).expect("解析").as_deref(),
            Some("photos/_RAW/c.RW2"),
            "位图没了就退回 RAW，而不是给一条死路径"
        );
    }

    #[test]
    fn edit_base_parses_only_the_two_known_words() {
        assert_eq!(EditBase::parse("sooc"), Some(EditBase::Sooc));
        assert_eq!(EditBase::parse("raw"), Some(EditBase::Raw));
        assert_eq!(EditBase::parse("RAW"), None, "大小写不许猜");
        assert_eq!(EditBase::parse(""), None);
        assert_eq!(EditBase::parse("jpeg"), None);
        assert_eq!(EditBase::default(), EditBase::Raw, "默认是 RAW");
        assert_eq!(EditBase::Sooc.as_str(), "sooc");
        assert_eq!(EditBase::Raw.as_str(), "raw");
    }

    #[test]
    fn as_shot_temperature_round_trips_with_the_stack() {
        let (conn, asset_id) = catalog_with_asset();
        let mut written = stack(&[("temperature", 4300.0)], &[]);
        written.as_shot_k = Some(4350.0);
        save(&conn, asset_id, &written, now_millis()).expect("写");
        let loaded = load(&conn, asset_id).expect("读");
        assert_eq!(loaded.as_shot_k, Some(4350.0), "基线要跟着 issue 一起存");
        assert_eq!(loaded, written);
        // 它不参与「编辑过没有」的判断
        let baseline_only = DevelopStack {
            as_shot_k: Some(5000.0),
            ..DevelopStack::default()
        };
        assert!(baseline_only.is_empty(), "只存基线不算编辑过");
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
