//! **编辑栈**（M3-W3）：一张照片的调整参数与曲线 —— 无损编辑的真相源。
//!
//! # latest 工作副本与不可变定稿
//!
//! 界面上能看到三个特殊版本；此外还有 `store::issues` 中的不可变命名定稿：
//!
//! | 版本 | 是什么 | 存哪 |
//! | --- | --- | --- |
//! | `SOOC` | 相机直出的 JPG（该资产有 JPG 时才有） | **不存**：它就是那个文件 |
//! | `RAW` | RAW 完整解码（该资产有 RAW 时才有） | **不存**：解码即得 |
//! | `latest` | 当前编辑结果 | **本模块**：`develop_stacks` 三张表 |
//!
//! `latest` 是自动保存的工作副本；切换命名定稿会把其完整 profile 写入 latest。
//! 画面是否有调整由 `DevelopStack::is_empty` 判断；只保留自动来源元数据的栈也可能没有像素调整。
//! 两层重置通过同一覆盖提交写回目标 profile；自动来源元数据随撤销恢复。
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
use crate::develop::denoise::NrMethod;
use crate::develop::geometry::EditGeometry;
use crate::develop::params::spec;
use crate::error::{Error, Result};

/// 自动调整的结果基线；只含允许的参数、镜头和降噪方式，不包含用户曲线 / LUT / 几何。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoAdjustBaseline {
    pub values: BTreeMap<String, f64>,
    pub lens_profile: Option<String>,
    pub lens_enabled: Option<bool>,
    pub nr_method: Option<NrMethod>,
}

impl AutoAdjustBaseline {
    pub fn validate(&self) -> Result<()> {
        for (id, value) in &self.values {
            if !spec(id).is_some_and(|spec| spec.accepts(*value)) {
                return Err(Error::Unsupported(format!(
                    "自动调整基线参数非法：{id}={value}"
                )));
            }
        }
        if self.lens_profile.as_ref().is_some_and(|key| {
            key.is_empty() || key.len() > 1024 || key.chars().any(char::is_control)
        }) {
            return Err(Error::Unsupported("自动调整基线镜头配置无效".into()));
        }
        Ok(())
    }
}

/// 一张照片的编辑栈（`latest`）。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DevelopStack {
    /// latest 的唯一源；SOOC/RAW 切换改变它，不复制其它调整参数。
    pub source_base: EditBase,
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
    /// **镜头配置文件**（lensfun 的 `maker|model` 键；M3-W4）。
    ///
    /// * `None`   = 没动过 ⇒ 未选择配置文件
    /// * `"none"` = 用户**显式清除了配置**
    pub lens_profile: Option<String>,
    /// NULL = 新照片未选择；"none" = 明确不用；其它 = 全局机型档案 ID。
    pub base_curve_profile: Option<String>,
    /// 被选中档案的曲线快照：旧照片不会被之后的机型统计悄悄改画面。
    pub base_curve_points: Option<Vec<[f32; 2]>>,
    /// 当前选择的应用级 LUT ID；禁用时仍保留选择。
    pub lut_id: Option<String>,
    /// LUT 开关；None 与 false 都表示未启用。
    pub lut_enabled: Option<bool>,
    /// **配置文件那一半**的开关（`None` = 默认开）。
    ///
    /// ❗ 它只管配置文件：三根手动拉杆不受它影响（人类 2026-09-25 拍板）。
    pub lens_enabled: Option<bool>,
    /// **降噪方式**（`None` = 快速档）。
    pub nr_method: Option<NrMethod>,
    /// 成片旋转和裁切（`None` = 原图）。
    pub geometry: Option<EditGeometry>,
    /// 重置时恢复的自动结果；不改变像素，不进入像素缓存 / 定稿匹配指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_adjust: Option<AutoAdjustBaseline>,
}

impl DevelopStack {
    /// 什么都没动过吗（没动过 = 与 SOOC 一样，不必渲染）。
    ///
    /// `as_shot_k` **不算「动过」**：它只是色温的解释基准，参数一个都没改就是没编辑过。
    /// 关闭已选择的镜头校正本身会改变像素，因此显式 `false` 也算编辑。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.params.is_empty()
            && self.curves.is_empty()
            && self.lens_profile.is_none()
            && self.base_curve_profile.is_none()
            && self.lut_id.is_none()
            && self.lut_enabled != Some(true)
            && self.lens_enabled != Some(false)
            && self.nr_method.is_none_or(NrMethod::is_default)
            && self.geometry.is_none_or(EditGeometry::is_identity)
    }

    /// 动过的项数（参数 + 曲线 + 镜头配置 + 非默认的降噪方式）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.params.len()
            + self.curves.len()
            + usize::from(self.lens_profile.is_some())
            + usize::from(self.base_curve_profile.is_some())
            + usize::from(self.lut_id.is_some())
            + usize::from(self.lens_enabled == Some(false))
            + usize::from(self.nr_method.is_some_and(|method| !method.is_default()))
            + usize::from(
                self.geometry
                    .is_some_and(|geometry| !geometry.is_identity()),
            )
    }

    /// **这份编辑栈的稳定指纹**（缓存键用）。
    ///
    /// 同一份参数在任何机器、任何时刻都算出同一个值（FNV-1a over 规范化文本）——
    /// 缩略图缓存靠它区分「编辑前 / 编辑后」，改一个参数就该重渲染。
    ///
    /// `as_shot_k` 也算进去：它变了，色温的解释就变了，画面跟着变。
    #[must_use]
    pub fn signature(&self) -> u64 {
        let mut text = String::from(self.source_base.as_str());
        text.push('|');
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
        // 镜头配置与降噪方式**同样改变像素** ⇒ 必须进指纹，
        // 否则「换了个配置文件、缩略图还是旧的」（毒缓存，`AGENTS.md` §2.16 同一条纪律）
        text.push('|');
        text.push_str(self.lens_profile.as_deref().unwrap_or("-"));
        text.push('|');
        text.push_str(self.base_curve_profile.as_deref().unwrap_or("-"));
        if let Some(points) = &self.base_curve_points {
            for point in points {
                text.push_str(&format!("{:.5},{:.5};", point[0], point[1]));
            }
        }
        text.push('|');
        text.push_str(self.lut_id.as_deref().unwrap_or("-"));
        text.push(if self.lut_enabled == Some(true) {
            '1'
        } else {
            '0'
        });
        text.push('|');
        text.push_str(match self.lens_enabled {
            Some(true) => "on",
            Some(false) => "off",
            None => "-",
        });
        text.push('|');
        text.push_str(self.nr_method.map_or("-", NrMethod::as_str));
        text.push('|');
        if let Some(geometry) = self.geometry.filter(|geometry| !geometry.is_identity()) {
            text.push_str(&format!("r{:.5}", geometry.rotation));
            if let Some(crop) = geometry.crop {
                text.push_str(&format!(
                    "c{:.7},{:.7},{:.7},{:.7}",
                    crop.x, crop.y, crop.width, crop.height
                ));
            }
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
    let row = conn
        .query_row(
            "SELECT as_shot_k, lens_profile, lens_enabled, nr_method, source_base, edit_geometry, \
                    base_curve_profile, base_curve_points, lut_id, lut_enabled, auto_adjust FROM develop_stacks WHERE asset_id = ?1",
            [asset_id],
            |row| {
                Ok((
                    row.get::<_, Option<f64>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<i64>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                ))
            },
        )
        .optional()?;
    let mut stack = DevelopStack::default();
    if let Some((
        as_shot_k,
        lens_profile,
        lens_enabled,
        nr_method,
        source_base,
        geometry,
        base_curve_profile,
        base_curve_json,
        lut_id,
        lut_enabled,
        auto_adjust,
    )) = row
    {
        stack.source_base = EditBase::parse(&source_base)
            .ok_or_else(|| Error::Unsupported(format!("未知的 issue 源：{source_base}")))?;
        #[allow(clippy::cast_possible_truncation)]
        {
            stack.as_shot_k = as_shot_k.map(|value| value as f32);
        }
        stack.lens_profile = lens_profile;
        stack.base_curve_profile = base_curve_profile;
        stack.lut_id = lut_id;
        stack.lut_enabled = lut_enabled.map(|value| value != 0);
        stack.base_curve_points = base_curve_json
            .map(|json| {
                serde_json::from_str::<Vec<[f32; 2]>>(&json)
                    .map_err(|error| Error::Unsupported(format!("基础曲线数据坏了：{error}")))
            })
            .transpose()?;
        stack.auto_adjust = auto_adjust
            .map(|json| {
                serde_json::from_str::<AutoAdjustBaseline>(&json)
                    .map_err(|error| Error::Unsupported(format!("自动调整基线数据坏了：{error}")))
            })
            .transpose()?;
        if let Some(ref automatic) = stack.auto_adjust {
            automatic.validate()?;
        }
        stack.lens_enabled = lens_enabled.map(|value| value != 0);
        // 认不出的方式**当成默认**（不是错误）：库里存着一个以后版本才有的值，
        // 老版本应当照旧能打开照片（向前兼容的最低要求）
        stack.nr_method = nr_method.as_deref().and_then(NrMethod::parse);
        stack.geometry = geometry
            .map(|json| {
                serde_json::from_str::<EditGeometry>(&json)
                    .map_err(|error| Error::Unsupported(format!("成片几何数据坏了：{error}")))
            })
            .transpose()?;
    }

    let mut statement =
        conn.prepare("SELECT param_id, value FROM develop_params WHERE asset_id = ?1")?;
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
    if let Some(ref automatic) = stack.auto_adjust {
        automatic.validate()?;
    }
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

    if stack
        .lut_id
        .as_ref()
        .is_some_and(|id| id.is_empty() || id.len() > 128 || id.contains(['/', '\\']))
    {
        return Err(Error::Unsupported("LUT ID 不合法".into()));
    }
    if stack.lut_enabled == Some(true) && stack.lut_id.is_none() {
        return Err(Error::Unsupported("启用 LUT 时必须选择 LUT".into()));
    }
    match (
        stack.base_curve_profile.as_deref(),
        stack.base_curve_points.as_ref(),
    ) {
        (None | Some("none"), None) => {}
        (Some(id), Some(points)) if id.parse::<i64>().is_ok_and(|id| id > 0) => {
            Curve::from_points(points.clone()).map_err(Error::Unsupported)?;
        }
        _ => return Err(Error::Unsupported("基础曲线档案与曲线快照不一致".into())),
    }

    if let Some(geometry) = stack.geometry
        && (!geometry.rotation.is_finite()
            || geometry.rotation.abs() > 360.0
            || geometry.crop.is_some_and(|crop| !crop.valid())
            || geometry.crop_ratio.is_some_and(|setting| !setting.valid()))
    {
        return Err(Error::Unsupported("成片几何不合法".into()));
    }

    // ② 栈本体（没有就建一个；as-shot 与镜头 / 降噪那几项跟着一起写）
    ensure_stack(conn, asset_id, now_ms)?;
    conn.execute(
        "UPDATE develop_stacks SET as_shot_k = ?2, lens_profile = ?3, lens_enabled = ?4, \
         nr_method = ?5, source_base = ?6, edit_geometry = ?7, base_curve_profile = ?8, \
         base_curve_points = ?9, lut_id = ?10, lut_enabled = ?11, auto_adjust = ?12 WHERE asset_id = ?1",
        rusqlite::params![
            asset_id,
            stack.as_shot_k.map(f64::from),
            stack.lens_profile,
            stack.lens_enabled.map(i64::from),
            stack
                .nr_method
                .filter(|method| !method.is_default())
                .map(NrMethod::as_str),
            stack.source_base.as_str(),
            stack
                .geometry
                .filter(|geometry| !geometry.is_identity())
                .map(|geometry| serde_json::to_string(&geometry))
                .transpose()
                .map_err(|error| Error::Unsupported(format!("成片几何序列化失败：{error}")))?,
            stack.base_curve_profile,
            stack
                .base_curve_points
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|error| Error::Unsupported(format!("基础曲线序列化失败：{error}")))?,
            stack.lut_id,
            stack.lut_enabled.map(i64::from),
            stack.auto_adjust.as_ref().map(serde_json::to_string).transpose()
                .map_err(|error| Error::Unsupported(format!("自动调整基线序列化失败：{error}")))?,
        ],
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
    if stack.is_empty() && stack.auto_adjust.is_none() && stack.source_base == EditBase::Raw {
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

/// 编辑栈里**不是参数也不是曲线**的那几项（M3-W4）。
///
/// 它们共用一种撤销操作（[`crate::store::marking::Op::DevelopSetting`]）：
/// 三项都是「一个可空的字符串/布尔」，各自的语义与校验写在 [`set_setting`] 里。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    /// 自动调整基线 JSON，作为整体参与撤销。
    AutoAdjust,
    /// 机型基础曲线档案选择（撤销时同时还原快照）。
    BaseCurveProfile,
    /// 被选档案的曲线快照，与选择一同撤销。
    BaseCurvePoints,
    /// 应用级 LUT ID。
    LutId,
    /// LUT 开关。
    LutEnabled,
    /// 镜头配置文件（`None` = 没动过；`"none"` = 显式清除配置；否则是 `maker|model`）
    LensProfile,
    /// 配置文件那一半的开关（`"1"` / `"0"`）
    LensEnabled,
    /// 降噪方式（`"fast"` / `"high"`）
    NrMethod,
    /// latest 的唯一源基准（raw / sooc）。
    SourceBase,
    /// 成片几何 JSON。
    Geometry,
}

impl Setting {
    /// 撤销操作里用的键（也是前端载荷里的字段名）。
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::AutoAdjust => "autoAdjust",
            Self::BaseCurveProfile => "baseCurveProfile",
            Self::BaseCurvePoints => "baseCurvePoints",
            Self::LutId => "lutId",
            Self::LutEnabled => "lutEnabled",
            Self::LensProfile => "lensProfile",
            Self::LensEnabled => "lensEnabled",
            Self::NrMethod => "nrMethod",
            Self::SourceBase => "sourceBase",
            Self::Geometry => "geometry",
        }
    }

    /// 反查（认不出给 `None`）。
    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "autoAdjust" => Some(Self::AutoAdjust),
            "baseCurveProfile" => Some(Self::BaseCurveProfile),
            "baseCurvePoints" => Some(Self::BaseCurvePoints),
            "lutId" => Some(Self::LutId),
            "lutEnabled" => Some(Self::LutEnabled),
            "lensProfile" => Some(Self::LensProfile),
            "lensEnabled" => Some(Self::LensEnabled),
            "nrMethod" => Some(Self::NrMethod),
            "sourceBase" => Some(Self::SourceBase),
            "geometry" => Some(Self::Geometry),
            _ => None,
        }
    }

    /// 它在 `develop_stacks` 里对应的列名（**枚举自带的值，不是用户输入**）。
    const fn column(self) -> &'static str {
        match self {
            Self::AutoAdjust => "auto_adjust",
            Self::BaseCurveProfile => "base_curve_profile",
            Self::BaseCurvePoints => "base_curve_points",
            Self::LutId => "lut_id",
            Self::LutEnabled => "lut_enabled",
            Self::LensProfile => "lens_profile",
            Self::LensEnabled => "lens_enabled",
            Self::NrMethod => "nr_method",
            Self::SourceBase => "source_base",
            Self::Geometry => "edit_geometry",
        }
    }

    /// 值合法吗（`None` = 清掉这一项，永远合法）。
    fn accepts(self, value: Option<&str>) -> bool {
        match (self, value) {
            (_, None) => true,
            (Self::AutoAdjust, Some(text)) => serde_json::from_str::<AutoAdjustBaseline>(text)
                .is_ok_and(|automatic| automatic.validate().is_ok()),
            (Self::BaseCurveProfile, Some(text)) => {
                text == "none" || text.parse::<i64>().is_ok_and(|id| id > 0)
            }
            (Self::BaseCurvePoints, Some(text)) => serde_json::from_str::<Vec<[f32; 2]>>(text)
                .is_ok_and(|points| Curve::from_points(points).is_ok()),
            (Self::LutId, Some(text)) => {
                !text.is_empty() && text.len() <= 128 && !text.contains(['/', '\\'])
            }
            (Self::LutEnabled, Some("0" | "1")) => true,
            (Self::LutEnabled, Some(_)) => false,
            (Self::LensProfile, Some(text)) => !text.is_empty(),
            (Self::LensEnabled, Some("0" | "1")) => true,
            (Self::LensEnabled, Some(_)) => false,
            (Self::NrMethod, Some(text)) => NrMethod::parse(text).is_some(),
            (Self::SourceBase, Some(text)) => EditBase::parse(text).is_some(),
            (Self::Geometry, Some(text)) => {
                serde_json::from_str::<EditGeometry>(text).is_ok_and(|geometry| {
                    geometry.rotation.is_finite()
                        && geometry.rotation.abs() <= 360.0
                        && geometry.crop.is_none_or(|crop| crop.valid())
                        && geometry.crop_ratio.is_none_or(|setting| setting.valid())
                })
            }
        }
    }
}

impl DevelopStack {
    /// 某一项设置现在的**字符串形式**（`None` = 没动过）。
    ///
    /// 撤销操作（`Op::DevelopSetting`）与前端载荷共用这一份口径 ——
    /// 两边各写一遍 `match` 的话，以后加一项就会漏一处。
    #[must_use]
    pub fn setting_value(&self, setting: Setting) -> Option<String> {
        match setting {
            Setting::AutoAdjust => self
                .auto_adjust
                .as_ref()
                .and_then(|automatic| serde_json::to_string(automatic).ok()),
            Setting::BaseCurveProfile => self.base_curve_profile.clone(),
            Setting::BaseCurvePoints => self
                .base_curve_points
                .as_ref()
                .and_then(|points| serde_json::to_string(points).ok()),
            Setting::LutId => self.lut_id.clone(),
            Setting::LutEnabled => self
                .lut_enabled
                .map(|enabled| if enabled { "1" } else { "0" }.to_string()),
            Setting::LensProfile => self.lens_profile.clone(),
            Setting::LensEnabled => self
                .lens_enabled
                .map(|enabled| if enabled { "1" } else { "0" }.to_string()),
            Setting::NrMethod => self
                .nr_method
                .filter(|method| !method.is_default())
                .map(|method| method.as_str().to_string()),
            Setting::SourceBase => Some(self.source_base.as_str().to_string()),
            Setting::Geometry => self
                .geometry
                .filter(|geometry| !geometry.is_identity())
                .and_then(|geometry| serde_json::to_string(&geometry).ok()),
        }
    }
}

/// 写**一项**设置（`value = None` ⇒ 清掉 = 回到默认）。
///
/// 撤销栈走的就是这一条（与 [`set_param`] 对称）。
///
/// # Errors
/// 值非法（例如把降噪方式写成 `"medium"`）/ 未知键 / 数据库写失败。
///
/// # Panics
/// `setting.column()` 是枚举自带的白名单字面量，拼 SQL 不会引入注入面。
pub fn set_setting(
    conn: &Connection,
    asset_id: i64,
    setting: Setting,
    value: Option<&str>,
    now_ms: i64,
) -> Result<()> {
    if !setting.accepts(value) {
        return Err(Error::Unsupported(format!(
            "{} 的值非法：{value:?}",
            setting.key()
        )));
    }
    ensure_stack(conn, asset_id, now_ms)?;
    let sql = format!(
        "UPDATE develop_stacks SET {} = ?2 WHERE asset_id = ?1",
        setting.column()
    );
    let stored = if setting == Setting::SourceBase {
        Some(value.unwrap_or("raw"))
    } else {
        value
    };
    conn.execute(&sql, rusqlite::params![asset_id, stored])?;
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

/// 像素设置与自动来源都为空才删除栈；删除最后一个手动参数不丢自动基线。
///
/// 镜头 / 降噪那几列也要看：只设了配置文件、没动过参数的照片**仍然算编辑过**
/// （配置文件会改变画面）。
fn prune_empty_stack(conn: &Connection, asset_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM develop_stacks
          WHERE asset_id = ?1
            AND source_base = 'raw'
            AND auto_adjust IS NULL
            AND lens_profile IS NULL
            AND base_curve_profile IS NULL
            AND lut_id IS NULL
            AND (lut_enabled IS NULL OR lut_enabled = 0)
            AND nr_method IS NULL
            AND edit_geometry IS NULL
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
/// 这里是当前工作副本的显示源；命名定稿保存在 `store::issues`，由 profile 哈希识别。
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
/// 命名定稿记录了编辑基准，切换定稿时随完整 profile 一起恢复。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
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
    // 可用 latest 的判据只有 DevelopStack::is_empty；显式关闭镜头也算编辑，
    // 快速降噪/恒等几何与自动基线本身不算。浏览和导出不得各自维护第二套 SQL 判据。
    Ok(!load(conn, asset_id)?.is_empty())
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
            ..DevelopStack::default()
        }
    }

    #[test]
    fn automatic_baseline_roundtrips_but_does_not_change_pixel_signature() {
        let (conn, asset_id) = catalog_with_asset();
        let original = stack(&[("exposure", 1.0)], &[]);
        let mut edited = original.clone();
        edited.auto_adjust = Some(AutoAdjustBaseline {
            values: [("exposure".to_owned(), 0.5), ("lumaNr".to_owned(), 9.0)].into(),
            lens_profile: Some("Maker|镜头".into()),
            lens_enabled: Some(true),
            nr_method: Some(NrMethod::High),
        });
        save(&conn, asset_id, &edited, 1).unwrap();
        assert_eq!(load(&conn, asset_id).unwrap(), edited);
        assert_eq!(edited.signature(), original.signature());
        let encoded = serde_json::to_string(&original).unwrap();
        assert!(!encoded.contains("auto_adjust"));
        assert_eq!(
            serde_json::from_str::<DevelopStack>(&encoded)
                .unwrap()
                .auto_adjust,
            None
        );
    }

    #[test]
    fn baseline_only_stack_survives_pruning_and_is_reversible_as_a_setting() {
        let (conn, asset_id) = catalog_with_asset();
        let edited = DevelopStack {
            auto_adjust: Some(AutoAdjustBaseline {
                values: [("exposure".to_owned(), 0.5)].into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(edited.is_empty());
        save(&conn, asset_id, &edited, 1).unwrap();
        assert_eq!(load(&conn, asset_id).unwrap(), edited);
        set_param(&conn, asset_id, "exposure", Some(0.7), 2).unwrap();
        set_param(&conn, asset_id, "exposure", None, 3).unwrap();
        assert_eq!(
            load(&conn, asset_id).unwrap(),
            edited,
            "删除最后一个参数不丢自动来源"
        );
        let json = edited.setting_value(Setting::AutoAdjust).unwrap();
        assert_eq!(Setting::parse("autoAdjust"), Some(Setting::AutoAdjust));
        set_setting(&conn, asset_id, Setting::AutoAdjust, None, 4).unwrap();
        assert_eq!(load(&conn, asset_id).unwrap(), DevelopStack::default());
        set_setting(&conn, asset_id, Setting::AutoAdjust, Some(&json), 5).unwrap();
        assert_eq!(load(&conn, asset_id).unwrap(), edited);
    }

    #[test]
    fn automatic_baseline_rejects_invalid_inputs_before_writing() {
        let (conn, asset_id) = catalog_with_asset();
        for (id, value) in [
            ("bogus", 1.0),
            ("exposure", f64::NAN),
            ("exposure", f64::INFINITY),
            ("exposure", 100.0),
        ] {
            let automatic = AutoAdjustBaseline {
                values: [(id.to_owned(), value)].into(),
                ..Default::default()
            };
            assert!(automatic.validate().is_err());
            let edited = DevelopStack {
                auto_adjust: Some(automatic),
                ..Default::default()
            };
            assert!(save(&conn, asset_id, &edited, 1).is_err());
        }
        for key in ["", "bad\nkey", &"x".repeat(1025)] {
            assert!(
                AutoAdjustBaseline {
                    lens_profile: Some(key.into()),
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        for value in ["{}", "{\"values\":{\"bogus\":1}}", "invalid JSON"] {
            assert!(set_setting(&conn, asset_id, Setting::AutoAdjust, Some(value), 1).is_err());
        }
        assert_eq!(load(&conn, asset_id).unwrap(), DevelopStack::default());
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
    fn geometry_round_trips_and_changes_preview_signature() {
        use crate::develop::geometry::{CropRect, EditGeometry};
        let (conn, asset_id) = catalog_with_asset();
        let baseline = DevelopStack::default();
        let mut edited = DevelopStack {
            geometry: Some(EditGeometry {
                rotation: 8.0,
                crop: Some(CropRect {
                    x: 0.25,
                    y: 0.25,
                    width: 0.5,
                    height: 0.5,
                }),
                crop_ratio: None,
            }),
            ..baseline.clone()
        };
        assert_ne!(baseline.signature(), edited.signature());
        assert!(needs_preview(IssueChoice::Latest, &edited));
        save(&conn, asset_id, &edited, now_millis()).expect("存几何");
        assert_eq!(
            load(&conn, asset_id).expect("读几何").geometry,
            edited.geometry
        );
        let signature = edited.signature();
        edited.geometry.as_mut().expect("几何").crop_ratio =
            Some(crate::develop::geometry::CropRatioSetting {
                id: crate::develop::geometry::CropRatioId::Custom,
                width: 5.0,
                height: 4.0,
            });
        assert_eq!(
            edited.signature(),
            signature,
            "比例面板元数据不能改变像素缓存键"
        );
        save(&conn, asset_id, &edited, now_millis()).expect("存比例配置");
        assert_eq!(
            load(&conn, asset_id).expect("读比例配置").geometry,
            edited.geometry
        );
        assert!(has_edits(&conn, asset_id).expect("编辑标记"));
        edited.geometry = None;
        save(&conn, asset_id, &edited, now_millis()).expect("清几何");
        assert!(!has_edits(&conn, asset_id).expect("清编辑标记"));
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
        save(
            &conn,
            asset_id,
            &stack(&[("exposure", 0.25)], &[]),
            now_millis(),
        )
        .expect("第二次");
        let loaded = load(&conn, asset_id).expect("读");
        assert_eq!(loaded.params.len(), 1);
        assert_eq!(loaded.params.get("exposure"), Some(&0.25));
        assert!(!loaded.params.contains_key("contrast"));
    }

    #[test]
    fn saving_an_empty_stack_cleans_the_row_up() {
        let (conn, asset_id) = catalog_with_asset();
        save(
            &conn,
            asset_id,
            &stack(&[("exposure", 1.0)], &[]),
            now_millis(),
        )
        .expect("写");
        assert!(has_edits(&conn, asset_id).expect("查"));
        save(&conn, asset_id, &DevelopStack::default(), now_millis()).expect("清空");
        assert!(
            !has_edits(&conn, asset_id).expect("查"),
            "全空就该回到没编辑过"
        );
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
        assert_eq!(
            loaded.curves.get("rgb"),
            Some(&vec![[0.0, 0.1], [1.0, 1.0]])
        );
    }

    #[test]
    fn illegal_input_is_rejected_before_writing_anything() {
        let (conn, asset_id) = catalog_with_asset();
        // 未知参数
        assert!(save(&conn, asset_id, &stack(&[("nope", 1.0)], &[]), now_millis()).is_err());
        // 超范围
        assert!(
            save(
                &conn,
                asset_id,
                &stack(&[("exposure", 99.0)], &[]),
                now_millis()
            )
            .is_err()
        );
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
            &stack(
                &[("exposure", 1.0)],
                &[("rgb", vec![[0.0, 0.0], [1.0, 1.0]])],
            ),
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
        assert_eq!(
            base.signature(),
            same.signature(),
            "顺序不同、内容相同 ⇒ 同一个指纹"
        );
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
        assert_eq!(
            choose_issue(&conn, asset_id).expect("解析"),
            IssueChoice::Sooc
        );

        // 编辑过 → latest（不管有没有 JPG）
        save(
            &conn,
            asset_id,
            &stack(&[("exposure", 1.0)], &[]),
            now_millis(),
        )
        .expect("写");
        assert_eq!(
            choose_issue(&conn, asset_id).expect("解析"),
            IssueChoice::Latest
        );

        // 只有 RAW：没编辑 → RAW
        let (conn, raw_asset) = catalog_with_asset();
        add_file(&conn, raw_asset, "raw", "photos/_RAW/a.RW2");
        assert_eq!(
            choose_issue(&conn, raw_asset).expect("解析"),
            IssueChoice::Raw
        );

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
        assert_eq!(
            choose_issue(&conn, missing).expect("解析"),
            IssueChoice::Raw
        );
    }

    #[test]
    fn edit_target_follows_the_base() {
        let (conn, asset_id) = catalog_with_asset();
        add_file(&conn, asset_id, "bitmap", "photos/2026/a.jpg");
        assert_eq!(
            edit_target(&conn, asset_id, EditBase::Raw)
                .expect("解析")
                .as_deref(),
            Some("photos/2026/a.jpg"),
            "只有 JPG 时编辑 JPG（RAW 基准也没 RAW 可给）"
        );
        assert_eq!(
            edit_target(&conn, asset_id, EditBase::Sooc)
                .expect("解析")
                .as_deref(),
            Some("photos/2026/a.jpg")
        );
        add_file(&conn, asset_id, "raw", "photos/2026/_RAW/a.RW2");
        assert_eq!(
            edit_target(&conn, asset_id, EditBase::Raw)
                .expect("解析")
                .as_deref(),
            Some("photos/2026/_RAW/a.RW2"),
            "RAW 基准：编辑落在 RAW 上"
        );
        assert_eq!(
            edit_target(&conn, asset_id, EditBase::Sooc)
                .expect("解析")
                .as_deref(),
            Some("photos/2026/a.jpg"),
            "SOOC 基准：编辑落在相机直出的位图上"
        );

        // 只有 RAW 的照片：SOOC 基准也退回 RAW（没得选）
        let (conn, raw_only) = catalog_with_asset();
        add_file(&conn, raw_only, "raw", "photos/_RAW/b.RW2");
        assert_eq!(
            edit_target(&conn, raw_only, EditBase::Sooc)
                .expect("解析")
                .as_deref(),
            Some("photos/_RAW/b.RW2")
        );

        // 位图标记缺失（磁盘上没了）：SOOC 基准也不许选中它
        let (conn, gone) = catalog_with_asset();
        add_file(&conn, gone, "bitmap", "photos/gone.jpg");
        add_file(&conn, gone, "raw", "photos/_RAW/c.RW2");
        conn.execute(
            "UPDATE asset_files SET missing_since = 1 WHERE role = 'bitmap'",
            [],
        )
        .expect("标缺失");
        assert_eq!(
            edit_target(&conn, gone, EditBase::Sooc)
                .expect("解析")
                .as_deref(),
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
        save(
            &conn,
            first,
            &stack(&[("exposure", 1.0)], &[]),
            now_millis(),
        )
        .expect("写一");
        save(
            &conn,
            second,
            &stack(&[("blacks", -30.0)], &[]),
            now_millis(),
        )
        .expect("写二");
        assert_eq!(
            load(&conn, first).expect("读一").params.get("exposure"),
            Some(&1.0)
        );
        assert!(
            !load(&conn, first)
                .expect("读一")
                .params
                .contains_key("blacks")
        );
        assert!(
            !load(&conn, second)
                .expect("读二")
                .params
                .contains_key("exposure")
        );
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
    fn the_migration_upgrades_an_existing_v5_stack_without_losing_data() {
        // v5 → v6：老编辑栈（参数 + 曲线）必须原样还在（`AGENTS.md` §2.16）
        let mut conn = Connection::open_in_memory().expect("内存库");
        let v5 = &crate::store::migration::CATALOG_MIGRATIONS[..5];
        crate::store::migration::apply_list(&mut conn, DbKind::Catalog, v5, Backups::none(), 0)
            .expect("升到 v5");
        conn.execute(
            "INSERT INTO assets (rating, flag, imported_at, updated_at) VALUES (0, 'none', 0, 0)",
            [],
        )
        .expect("插资产");
        let asset_id = conn.last_insert_rowid();
        // ⚠️ v5 时代的库还没有那三列 —— 不能调 `save`（它会写新列）
        conn.execute(
            "INSERT INTO develop_stacks (asset_id, created_at, updated_at, as_shot_k) \
             VALUES (?1, 0, 0, 5200.0)",
            [asset_id],
        )
        .expect("v5 时代的栈本体");
        conn.execute(
            "INSERT INTO develop_params (asset_id, param_id, value) VALUES (?1, 'exposure', 0.75)",
            [asset_id],
        )
        .expect("v5 时代的参数");
        conn.execute(
            "INSERT INTO develop_curves (asset_id, channel, points) \
             VALUES (?1, 'rgb', '[[0,0],[0.5,0.6],[1,1]]')",
            [asset_id],
        )
        .expect("v5 时代的曲线");

        apply(&mut conn, DbKind::Catalog, Backups::none(), 0).expect("升到 v6");

        let loaded = load(&conn, asset_id).expect("读");
        assert_eq!(loaded.params.get("exposure"), Some(&0.75), "老参数不丢");
        assert!(loaded.curves.contains_key("rgb"), "老曲线不丢");
        assert_eq!(loaded.as_shot_k, Some(5200.0), "拍摄色温不丢");
        assert_eq!(loaded.lens_profile, None, "新列默认是「没动过」");
        assert_eq!(loaded.nr_method, None);
    }

    #[test]
    fn latest_source_round_trips_changes_signature_and_is_undoable() {
        let (conn, asset_id) = catalog_with_asset();
        let mut edited = stack(&[("exposure", 0.5)], &[]);
        let raw_signature = edited.signature();
        edited.source_base = EditBase::Sooc;
        assert_ne!(edited.signature(), raw_signature);
        save(&conn, asset_id, &edited, now_millis()).expect("存 SOOC issue");
        assert_eq!(
            load(&conn, asset_id).expect("读").source_base,
            EditBase::Sooc
        );
        set_setting(
            &conn,
            asset_id,
            Setting::SourceBase,
            Some("raw"),
            now_millis(),
        )
        .expect("撤销源切换");
        assert_eq!(
            load(&conn, asset_id).expect("读").source_base,
            EditBase::Raw
        );
        assert!(
            set_setting(
                &conn,
                asset_id,
                Setting::SourceBase,
                Some("jpeg"),
                now_millis()
            )
            .is_err()
        );
    }

    #[test]
    fn v7_migration_infers_existing_issue_source_from_available_files() {
        let mut conn = Connection::open_in_memory().expect("内存库");
        let v6 = &crate::store::migration::CATALOG_MIGRATIONS[..6];
        crate::store::migration::apply_list(&mut conn, DbKind::Catalog, v6, Backups::none(), 0)
            .expect("升到 v6");
        let mut assets = Vec::new();
        for role in ["raw", "bitmap"] {
            conn.execute(
                "INSERT INTO assets (rating, flag, imported_at, updated_at) VALUES (0, 'none', 0, 0)",
                [],
            ).expect("插资产");
            let id = conn.last_insert_rowid();
            add_file(&conn, id, role, &format!("{id}.photo"));
            conn.execute(
                "INSERT INTO develop_stacks (asset_id, created_at, updated_at) VALUES (?1, 0, 0)",
                [id],
            )
            .expect("旧栈");
            conn.execute(
                "INSERT INTO develop_params (asset_id, param_id, value) VALUES (?1, 'exposure', 0.5)",
                [id],
            ).expect("旧参数");
            assets.push(id);
        }
        apply(&mut conn, DbKind::Catalog, Backups::none(), 0).expect("升到 v7");
        assert_eq!(
            load(&conn, assets[0]).expect("RAW 旧栈").source_base,
            EditBase::Raw
        );
        assert_eq!(
            load(&conn, assets[1]).expect("位图旧栈").source_base,
            EditBase::Sooc
        );
        assert_eq!(
            load(&conn, assets[0])
                .expect("旧参数")
                .params
                .get("exposure"),
            Some(&0.5)
        );
    }

    #[test]
    fn lens_and_nr_fields_round_trip_and_feed_the_signature() {
        let (conn, asset_id) = catalog_with_asset();
        let mut base = stack(&[("exposure", 0.5)], &[]);
        let plain = base.signature();
        save(&conn, asset_id, &base, now_millis()).expect("写");
        assert_eq!(load(&conn, asset_id).expect("读"), base);

        // 换配置文件 ⇒ 指纹必须变（否则缩略图不重渲染 —— 毒缓存）
        base.lens_profile = Some("Panasonic|LUMIX G 12-35mm".to_string());
        assert_ne!(base.signature(), plain, "配置文件要进指纹");
        save(&conn, asset_id, &base, now_millis()).expect("写");
        assert_eq!(load(&conn, asset_id).expect("读"), base);

        // 降噪方式同样改画面 ⇒ 也要进指纹，而且要能存下来
        base.nr_method = Some(NrMethod::High);
        save(&conn, asset_id, &base, now_millis()).expect("写");
        assert_eq!(
            load(&conn, asset_id).expect("读").nr_method,
            Some(NrMethod::High)
        );

        // 开关也要存住（它只影响配置文件那一半）
        base.lens_enabled = Some(false);
        save(&conn, asset_id, &base, now_millis()).expect("写");
        assert_eq!(load(&conn, asset_id).expect("读").lens_enabled, Some(false));
    }

    #[test]
    fn a_profile_alone_counts_as_an_edit() {
        // 只挑了配置文件、没动参数：也是编辑（画面变了），不能当成「没编辑过」
        let (conn, asset_id) = catalog_with_asset();
        let mut only_profile = DevelopStack::default();
        assert!(only_profile.is_empty());
        only_profile.lens_profile = Some("A|B".to_string());
        assert!(!only_profile.is_empty());
        assert_eq!(only_profile.len(), 1);
        save(&conn, asset_id, &only_profile, now_millis()).expect("写");
        assert!(has_edits(&conn, asset_id).expect("查"));
        assert_eq!(load(&conn, asset_id).expect("读"), only_profile);

        // 只关闭自动校正也算编辑：已有自动匹配时它会改变画面
        let only_switch = DevelopStack {
            lens_enabled: Some(false),
            ..DevelopStack::default()
        };
        assert!(!only_switch.is_empty());
        assert_eq!(only_switch.len(), 1);

        // 清掉配置文件之后，空栈要回到干净状态（本体行被删掉）
        let cleared = DevelopStack::default();
        save(&conn, asset_id, &cleared, now_millis()).expect("写");
        assert!(!has_edits(&conn, asset_id).expect("查"));
    }

    #[test]
    fn set_setting_writes_one_field_and_prunes() {
        let (conn, asset_id) = catalog_with_asset();
        // 写一个配置文件：要建栈、要能读回来
        set_setting(
            &conn,
            asset_id,
            Setting::LensProfile,
            Some("A|B"),
            now_millis(),
        )
        .expect("写");
        assert_eq!(
            load(&conn, asset_id).expect("读").lens_profile,
            Some("A|B".to_string())
        );
        // 合法性：降噪方式只认两个值、开关只认 0/1
        assert!(set_setting(&conn, asset_id, Setting::NrMethod, Some("medium"), 0).is_err());
        assert!(set_setting(&conn, asset_id, Setting::LensEnabled, Some("true"), 0).is_err());
        set_setting(&conn, asset_id, Setting::NrMethod, Some("high"), 0).expect("写");
        set_setting(&conn, asset_id, Setting::LensEnabled, Some("0"), 0).expect("写");
        let loaded = load(&conn, asset_id).expect("读");
        assert_eq!(loaded.nr_method, Some(NrMethod::High));
        assert_eq!(loaded.lens_enabled, Some(false));
        // 键 ↔ 设置一一对应（撤销操作靠它）
        for setting in [
            Setting::LensProfile,
            Setting::LensEnabled,
            Setting::NrMethod,
        ] {
            assert_eq!(Setting::parse(setting.key()), Some(setting));
        }
        assert_eq!(Setting::parse("不存在"), None);
        // 清掉全部 ⇒ 栈本体要消失
        set_setting(&conn, asset_id, Setting::LensProfile, None, 0).expect("清");
        set_setting(&conn, asset_id, Setting::NrMethod, None, 0).expect("清");
        set_setting(&conn, asset_id, Setting::LensEnabled, None, 0).expect("清");
        assert!(!has_edits(&conn, asset_id).expect("查"));
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
        save(
            &conn,
            asset_id,
            &stack(&[("exposure", 0.5)], &[]),
            now_millis(),
        )
        .expect("写");
        assert!(has_edits(&conn, asset_id).expect("查"));
    }
}
