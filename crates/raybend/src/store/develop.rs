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
//! `crates/raybend/assets/develop-params.json`，**不进库**）—— 这样改默认值不需要数据迁移，
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
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
#[serde(try_from = "DevelopStackWire")]
pub struct DevelopStack {
    /// Missing color preserves the historical pipeline and canonical issue hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<crate::color::PhotoColorState>,
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
    /// **应用层的链**（有序；空 = 没有 LUT）。自上而下 = 逐层处理。
    pub lut_chain: Vec<LutLayer>,
    /// **总开关**：None 与 false 都表示整条链不参与渲染（链保留）。
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
    /// **默认层之外**的层（`specs/editor-area.md`）。顺序 = 计算顺序，新层追加在末尾。
    ///
    /// **默认层不在这里**：它就是本结构自己的 `params` / `curves` 加那几个全局字段
    /// （`color` / `lut_chain` / `lens_*` / `nr_method` / `base_curve_*` / `geometry`）。
    /// 这么切是为了让「退化 = 只有默认层」时**序列化字节与改动前逐字节相同**：
    /// 空 `areas` 直接不写，旧定稿的 `profile_hash` 与旧缓存签名全部不受影响。
    pub areas: Vec<Area>,
    /// 默认层在层列表里的位置（0-based）。**只有存在额外层时才写**。
    pub default_index: Option<u8>,
}

/// 链长上限（GPU 同一趟 pass 内逐层串联；`specs/editor-lut-chain.md` §1）。
pub const MAX_LUT_LAYERS: usize = 8;

/// 编辑区层数上限（**不含默认层**；`memory/LIMITS.md`）。
pub const MAX_AREAS: usize = 32;

/// 层名长度上限（字符，不是字节）。
pub const MAX_AREA_NAME_CHARS: usize = 30;

/// 控制点颜色的**色板长度**（崔总 2026-10-31 定 8 个）。
///
/// 层里存的是**下标**不是十六进制：以后调色板只改渲染侧，不重写已有数据。
pub const POINT_COLORS: usize = 8;

/// 编辑区的一层（**默认层不在其中**，见 [`DevelopStack::areas`]）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Area {
    /// 稳定 ID：撤销 / 引用 / 面板排序都靠它，单调递增、永不复用。
    pub id: u32,
    #[serde(flatten)]
    pub kind: AreaKind,
    /// 用户起的名字（≤30 字符）。空 = 用类型默认名。
    #[serde(default)]
    pub name: String,
    /// 启停（`false` = 整层旁路，不删）。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 不透明度 0..=1。近期无 UI（`specs/editor-area.md` §10 D3）。
    #[serde(default = "default_strength")]
    pub opacity: f32,
    /// 控制点的颜色槽位（`0..POINT_COLORS`）。只有 `Point` 用得上；全图层不画标记。
    ///
    /// 存**下标**：色板在渲染侧定义，换一套颜色不需要重写任何已有数据。
    #[serde(default)]
    pub color: u8,
    /// 逐像素参数（只装非默认项）。
    ///
    /// ❗**不许省字段**（2026-11-01）：空层序列化时也必须带回 `"params": {}` 与
    /// `"curves": {}` —— 前端 `Object.entries(area.curves)` 拿到 `undefined` 会抛
    /// `TypeError`，整次编辑栈载入作废（现象：建过 area 再进来只剩默认层）。
    /// 退化形状（根本没有额外层）仍由 `DevelopStack.areas` 的 `skip_serializing_if` 保住。
    #[serde(default)]
    pub params: BTreeMap<String, f64>,
    /// 通道 → 控制点。
    #[serde(default)]
    pub curves: BTreeMap<String, Vec<[f32; 2]>>,
    /// 控制点种子的定义版本（`SEED_VERSION`；`0` = 旧定义，需要重新取样）。
    ///
    /// 只有控制点用得到；全图层恒为 0，不写也合法。
    #[serde(default)]
    pub seed_version: u8,
}

/// 层的类型。默认层不在枚举里（它是栈本身）。
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AreaKind {
    /// 用户建的全幅调整层：权重恒 1。
    Full,
    /// 控制点：种子向量 + 容差 + 平滑权重（`specs/editor-area.md` §4.2）。
    ///
    /// **不是蒙版、也不是圆形蒙版**：它在**全图**范围内找"和种子向量相近"的像素，
    /// 与采样位的空间距离无关（`x` / `y` 只是采样位，用来取种子、画标记）。
    Point {
        /// 采样位：源图归一化坐标（方向已归正、裁切前）。
        x: f32,
        y: f32,
        /// 创建时冻结的特征向量 `[Y′, Cb, Cr]`（感知域；`develop::areas::seed_version()`）。
        seed: [f32; 3],
        /// **容差**：界面上那条白线的长度就是这个数（`100..=1000`，连续无台阶）。
        ///
        /// 它经 `develop::areas::PointWeightConfig` 映射成「亮度轴 / 色相轴」各自的
        /// （完全容差, 过渡终点）—— 存储层不解释它，只保证范围。
        tolerance: f32,
    },
}

/// 控制点种子的**定义版本**。
///
/// * `1` = `[Y′, Cb, Cr]`（感知域，2026-11-01 起）；
/// * `0` / 缺省 = 老定义（线性域 `[luma, c1, c2]`）—— **必须重新取样**才能用。
///
/// 前端在载入 latest 时看到旧版本会逐点重取一次（位置不变），见
/// `EditorWorkspace` 的 `migrateLegacySeeds`。
pub const SEED_VERSION: u8 = 1;

/// `develop_stacks.areas` 这一列的形状（**只在有额外层时写**）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AreasBlob {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_index: Option<u8>,
    #[serde(default)]
    areas: Vec<Area>,
}

impl Area {
    /// 转成**渲染侧形态**（`develop::areas::AreaSpec`）。
    ///
    /// 层的数据形状住在 store（持久化 / 校验），求值住在 develop（纯数学）——
    /// 这一条转换是两边**唯一**的桥（`develop` 不认识 store 的类型）。
    ///
    /// 两个旧数据防线：
    /// * 容差曾是 `0..1`，在这里统一折算到 `100..1000`（旧定稿 / 侧车不经过 DB `load`）；
    /// * 旧定义的 `seed_version` **不能按新定义解释**，先把层关掉，等重新取样，
    ///   不能悄悄把旧线性种子当感知域种子用。
    #[must_use]
    pub fn to_spec(&self) -> crate::develop::AreaSpec {
        crate::develop::AreaSpec {
            kind: match self.kind {
                AreaKind::Full => crate::develop::AreaKind::Full,
                AreaKind::Point {
                    x, y, seed, tolerance
                } => crate::develop::AreaKind::Point {
                    x,
                    y,
                    seed,
                    tolerance: if tolerance.is_finite() && tolerance <= 1.0 {
                        crate::develop::areas::TOLERANCE_MIN
                            + tolerance.max(0.0)
                                * (crate::develop::areas::TOLERANCE_MAX
                                    - crate::develop::areas::TOLERANCE_MIN)
                    } else {
                        tolerance
                    },
                },
            },
            enabled: self.enabled
                && (!matches!(self.kind, AreaKind::Point { .. })
                    || self.seed_version == SEED_VERSION),
            opacity: self.opacity,
            params: self.params.clone(),
            curves: self.curves.clone(),
        }
    }

    fn valid(&self) -> Result<()> {
        if self.id == 0 {
            return Err(Error::Unsupported("编辑区 ID 不能为 0".into()));
        }
        if self.name.chars().count() > MAX_AREA_NAME_CHARS
            || self.name.chars().any(char::is_control)
        {
            return Err(Error::Unsupported(format!(
                "层名最长 {MAX_AREA_NAME_CHARS} 个字符且不含控制字符"
            )));
        }
        if !self.opacity.is_finite() || !(0.0..=1.0).contains(&self.opacity) {
            return Err(Error::Unsupported("层不透明度必须在 0..=1".into()));
        }
        if usize::from(self.color) >= POINT_COLORS {
            return Err(Error::Unsupported(format!(
                "控制点颜色槽位越界：{}（只有 {POINT_COLORS} 个）",
                self.color
            )));
        }
        for (id, value) in &self.params {
            let Some(spec) = spec(id) else {
                return Err(Error::Unsupported(format!("未知的显影参数：{id}")));
            };
            if spec.target == crate::develop::params::ParamTarget::Global {
                return Err(Error::Unsupported(format!(
                    "{id} 是全局阶段参数，只能挂在默认层"
                )));
            }
            if spec.target == crate::develop::params::ParamTarget::Mask {
                return Err(Error::Unsupported(format!("{id} 由掩膜自己管，不能写进层参数")));
            }
            if !spec.accepts(*value) {
                return Err(Error::Unsupported(format!(
                    "参数 {id} 的值非法：{value}（允许 {}..{}）",
                    spec.min, spec.max
                )));
            }
        }
        for (channel, points) in &self.curves {
            if CurveChannel::parse(channel).is_none() {
                return Err(Error::Unsupported(format!("未知的曲线通道：{channel}")));
            }
            Curve::from_points(points.clone())
                .map_err(|e| Error::Unsupported(format!("曲线 {channel} 不合法：{e}")))?;
        }
        if let AreaKind::Point { x, y, seed, tolerance } = self.kind {
            let unit = |value: f32| value.is_finite() && (0.0..=1.0).contains(&value);
            let chroma = |value: f32| value.is_finite() && (-0.5..=0.5).contains(&value);
            let in_range = tolerance.is_finite()
                && ((crate::develop::areas::TOLERANCE_MIN
                    ..=crate::develop::areas::TOLERANCE_MAX)
                    .contains(&tolerance)
                    // 旧版本存的是 0..1 的分数：读旧定稿 / 侧车时仍要放行，
                    // 渲染入口 `to_spec` 会折算成新数值域。
                    || (0.0..=1.0).contains(&tolerance));
            /*
             * **旧种子（`seed_version < SEED_VERSION`）只要求是有限数** —— 它不是
             * 新定义的 `[Y′, Cb, Cr]`，量纲与范围都不同（旧的是线性域），拿新范围去卡
             * 它必然误杀。
             *
             * ❗这里曾经要求旧种子也满足新范围，结果是一条**死锁**（2026-11-01 崔总报的
             * 「调节容差时报错」）：
             * ① 取样命令 `develop_area_seed` 自己先 `stack.validate()`；
             * ② 栈里只要有一个范围外的旧种子，取样就失败；
             * ③ 而旧种子只能靠取样来迁移 —— 于是永远迁不动，任何一次落库都报
             *    「不支持：控制点坐标必须在 0..=1、容差必须在 100..=1000…」。
             * 旧种子本来**不参与渲染**（`to_spec` 会把该层旁路），只校验有限数就够。
             */
            let legacy_seed = self.seed_version != SEED_VERSION;
            let seed_ok = if legacy_seed {
                seed.iter().all(|value| value.is_finite())
            } else {
                unit(seed[0]) && chroma(seed[1]) && chroma(seed[2])
            };
            if !unit(x) || !unit(y) || !in_range || !seed_ok {
                let (min, max) = (
                    crate::develop::areas::TOLERANCE_MIN,
                    crate::develop::areas::TOLERANCE_MAX,
                );
                return Err(Error::Unsupported(format!(
                    "控制点不合法：坐标 ({x}, {y})、容差 {tolerance}（允许 {min}..={max}）、\
                     种子 [{}, {}, {}]（版本 {}/{SEED_VERSION}）",
                    seed[0], seed[1], seed[2], self.seed_version
                )));
            }
        }
        Ok(())
    }
}

/// **应用层的一项**：哪一支 LUT、开没开、强度多少。
///
/// 语义（唯一口径）：
/// `像素 = layer_n(...(layer_1(原像素)))`，
/// 单层作用 = `mix(输入, Lut::eval_extended(输入), strength)`。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LutLayer {
    /// 应用级 LUT 的 ID（与 `app.db` 的 `luts.id` 对应）。
    pub id: String,
    /// 单层开关：`false` = 该层旁路（不删）。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 强度 0..=1（1 = 100%）。
    #[serde(default = "default_strength")]
    pub strength: f32,
}

fn default_true() -> bool {
    true
}

fn default_strength() -> f32 {
    1.0
}

impl LutLayer {
    /// 新建一层（默认开、100%）。
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            enabled: true,
            strength: 1.0,
        }
    }

    fn valid(&self) -> bool {
        !(self.id.is_empty()
            || self.id.len() > 128
            || self.id.contains(['/', '\\'])
            || !self.strength.is_finite()
            || !(0.0..=1.0).contains(&self.strength))
    }
}

/// **反序列化的唯一入口**：旧数据（`lut_id` 单支）在这里并入链。
///
/// 为什么用 wire 而不是在各个装载点各调一次归一化：DB、XMP `rb:profileJson`、
/// issue 快照、preset 全都会反序列化 `DevelopStack`，只有离成一处才不会漏掉某个入口。
#[derive(serde::Deserialize)]
struct DevelopStackWire {
    #[serde(default)]
    color: Option<crate::color::PhotoColorState>,
    #[serde(default)]
    source_base: Option<String>,
    #[serde(default)]
    params: BTreeMap<String, f64>,
    #[serde(default)]
    curves: BTreeMap<String, Vec<[f32; 2]>>,
    #[serde(default)]
    as_shot_k: Option<f32>,
    #[serde(default)]
    lens_profile: Option<String>,
    #[serde(default)]
    base_curve_profile: Option<String>,
    #[serde(default)]
    base_curve_points: Option<Vec<[f32; 2]>>,
    #[serde(default)]
    lut_chain: Option<Vec<LutLayer>>,
    /// 旧格式的单支选择（只读兼容）。
    #[serde(default)]
    lut_id: Option<String>,
    #[serde(default)]
    lut_enabled: Option<bool>,
    #[serde(default)]
    lens_enabled: Option<bool>,
    #[serde(default)]
    nr_method: Option<NrMethod>,
    #[serde(default)]
    geometry: Option<EditGeometry>,
    #[serde(default)]
    auto_adjust: Option<AutoAdjustBaseline>,
    /// 默认层之外的层；空 = 只有默认层（旧数据全是这种）。
    #[serde(default)]
    areas: Vec<Area>,
    #[serde(default)]
    default_index: Option<u8>,
}

/// **序列化的唯一入口**：字段顺序与旧版逐字一致，且「单层 @100% 且开着」会被写成
/// 旧的 `lut_id` 形状 —— 于是**已有数据的 profile 哈希逐字节不变**（定稿的 `profile_hash`
/// 就是对这些字节做的 FNV，改了字段就会让所有旧定稿「哈希对不上」）。
#[derive(serde::Serialize)]
struct DevelopStackOut<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<&'a crate::color::PhotoColorState>,
    source_base: EditBase,
    params: &'a BTreeMap<String, f64>,
    curves: &'a BTreeMap<String, Vec<[f32; 2]>>,
    as_shot_k: Option<f32>,
    lens_profile: Option<&'a str>,
    base_curve_profile: Option<&'a str>,
    base_curve_points: Option<&'a Vec<[f32; 2]>>,
    /// 旧形状：**只有**「整条链等价于单支」时才有值（见 [`legacy_lut_id`]）。
    lut_id: Option<&'a str>,
    /// 新形状：多层的链。旧形状能表达时**不写**（保证旧字节不变）。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    lut_chain: Vec<LutLayer>,
    lut_enabled: Option<bool>,
    lens_enabled: Option<bool>,
    nr_method: Option<NrMethod>,
    geometry: Option<EditGeometry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_adjust: Option<&'a AutoAdjustBaseline>,
    /// 退化（只有默认层）时**不写**，旧字节因此逐字不变。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    areas: &'a Vec<Area>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_index: Option<u8>,
}

/// 这条链能不能用旧的「单支 `lut_id`」形状表达（单层、开、强度 100%）。
fn legacy_lut_id(chain: &[LutLayer]) -> Option<&str> {
    match chain {
        [layer] if layer.enabled && layer.strength == 1.0 => {
            Some(layer.id.as_str())
        }
        [] => None,
        _ => None,
    }
}

impl serde::Serialize for DevelopStack {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let legacy = legacy_lut_id(&self.lut_chain);
        DevelopStackOut {
            color: self.color.as_ref(),
            source_base: self.source_base,
            params: &self.params,
            curves: &self.curves,
            as_shot_k: self.as_shot_k,
            lens_profile: self.lens_profile.as_deref(),
            base_curve_profile: self.base_curve_profile.as_deref(),
            base_curve_points: self.base_curve_points.as_ref(),
            lut_id: legacy,
            lut_chain: if legacy.is_some() {
                Vec::new()
            } else {
                self.lut_chain.clone()
            },
            lut_enabled: self.lut_enabled,
            lens_enabled: self.lens_enabled,
            nr_method: self.nr_method,
            geometry: self.geometry,
            auto_adjust: self.auto_adjust.as_ref(),
            areas: &self.areas,
            default_index: self.default_index,
        }
        .serialize(serializer)
    }
}

impl TryFrom<DevelopStackWire> for DevelopStack {
    type Error = String;

    fn try_from(wire: DevelopStackWire) -> Result<Self, Self::Error> {
        // 与 `DevelopStackDto::into_stack` 同一口径：未知 source_base 是坏数据，
        // 不能静默变成 RAW（那会把一张 SOOC 编辑栈悄悄换成另一条管线）。
        let source_base = wire.source_base.as_deref().map_or(Ok(EditBase::Raw), |text| {
            EditBase::parse(text).ok_or_else(|| format!("未知的 issue 源：{text}"))
        })?;
        let has_lut_chain = wire.lut_chain.is_some();
        let mut stack = Self {
            color: wire.color,
            source_base,
            params: wire.params,
            curves: wire.curves,
            as_shot_k: wire.as_shot_k,
            lens_profile: wire.lens_profile,
            base_curve_profile: wire.base_curve_profile,
            base_curve_points: wire.base_curve_points,
            lut_chain: wire.lut_chain.unwrap_or_default(),
            lut_enabled: wire.lut_enabled,
            lens_enabled: wire.lens_enabled,
            nr_method: wire.nr_method,
            geometry: wire.geometry,
            auto_adjust: wire.auto_adjust,
            areas: wire.areas,
            default_index: wire.default_index,
        };
        // 只有「字段根本不存在」才按旧格式并入；显式写了空链 = 新数据已经决定不用 LUT。
        if !has_lut_chain {
            stack.adopt_legacy_lut(wire.lut_id);
        }
        Ok(stack)
    }
}

impl DevelopStack {
    /// 旧数据（`lut_id` 单支 + `lut_enabled`）并入链。**只在本模块的装载口调用**：
    /// [`DevelopStackWire`]（所有反序列化）与 [`load`]（DB）。
    fn adopt_legacy_lut(&mut self, legacy_id: Option<String>) {
        if self.lut_chain.is_empty()
            && let Some(id) = legacy_id.filter(|id| !id.is_empty())
        {
            self.lut_chain.push(LutLayer::new(id));
        }
    }

    /// 下一个控制点该用哪个颜色槽位：**取已用得最少的那个**，并列时取下标最小的。
    ///
    /// 只数 `Point` 层（全图层不画标记、不占颜色）。算法刻意做成"最简单的一种"：
    /// 不需要记住上次用了谁，删掉几个点之后再建也自然均衡。
    #[must_use]
    pub fn next_point_color(&self) -> u8 {
        let mut used = [0usize; POINT_COLORS];
        for area in &self.areas {
            if matches!(area.kind, AreaKind::Point { .. }) {
                used[usize::from(area.color) % POINT_COLORS] += 1;
            }
        }
        let mut best = 0usize;
        for slot in 1..POINT_COLORS {
            if used[slot] < used[best] {
                best = slot;
            }
        }
        #[allow(clippy::cast_possible_truncation)]
        {
            best as u8
        }
    }

    /// 这一份栈在**当前编辑基准**下要计算的额外层。
    ///
    /// SOOC（相机直出位图）没有 RAW 取样前提，控制点**整层旁路**；
    /// 全图层不受影响。层数据一个字不改，切回 RAW 即恢复。
    /// 编辑器帧、缩略图、定稿与导出都必须走这里，不能只在前端载荷里拦一次。
    #[must_use]
    pub fn area_specs(&self) -> Vec<crate::develop::AreaSpec> {
        self.areas
            .iter()
            .map(|area| {
                let mut spec = area.to_spec();
                if self.source_base == EditBase::Sooc
                    && matches!(area.kind, AreaKind::Point { .. })
                {
                    spec.enabled = false;
                }
                spec
            })
            .collect()
    }

    /// 这一份栈现在真的要套 LUT 吗（总开关开 **且** 至少一层开着）。
    #[must_use]
    pub fn lut_chain_active(&self) -> bool {
        self.lut_enabled == Some(true)
            && self
                .lut_chain
                .iter()
                .any(|layer| layer.enabled && layer.strength.is_finite() && layer.strength > 0.0)
    }

    /// 完整编辑栈校验；交互保存、定稿读回和 XMP 共用，写入前调用。
    pub fn validate(&self) -> Result<()> {
        if let Some(color) = &self.color { color.validate_frozen().map_err(|e| Error::Unsupported(e.into()))?; }
        if let Some(ref automatic) = self.auto_adjust {
            automatic.validate()?;
        }
        // ① 先校验（**写之前**，别写一半才发现有错）
        for (id, value) in &self.params {
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
        for (channel, points) in &self.curves {
            if CurveChannel::parse(channel).is_none() {
                return Err(Error::Unsupported(format!("未知的曲线通道：{channel}")));
            }
            Curve::from_points(points.clone())
                .map_err(|e| Error::Unsupported(format!("曲线 {channel} 不合法：{e}")))?;
        }

        if self.lut_chain.len() > MAX_LUT_LAYERS {
            return Err(Error::Unsupported(format!(
                "LUT 链最多 {MAX_LUT_LAYERS} 层"
            )));
        }
        for layer in &self.lut_chain {
            if !layer.valid() {
                return Err(Error::Unsupported(
                    "LUT 层不合法：ID 为空/过长/含路径分隔符，或强度不在 0..=1".into(),
                ));
            }
        }
        for (index, layer) in self.lut_chain.iter().enumerate() {
            if self.lut_chain[..index]
                .iter()
                .any(|other| other.id == layer.id)
            {
                return Err(Error::Unsupported("同一支 LUT 不能在链里出现两次".into()));
            }
        }
        if self.lut_enabled == Some(true) && self.lut_chain.is_empty() {
            return Err(Error::Unsupported("启用 LUT 时链不能为空".into()));
        }
        match (
            self.base_curve_profile.as_deref(),
            self.base_curve_points.as_ref(),
        ) {
            (None | Some("none"), None) => {}
            (Some(id), Some(points)) if id.parse::<i64>().is_ok_and(|id| id > 0) => {
                Curve::from_points(points.clone()).map_err(Error::Unsupported)?;
            }
            _ => return Err(Error::Unsupported("基础曲线档案与曲线快照不一致".into())),
        }

        if let Some(geometry) = self.geometry
            && (!geometry.rotation.is_finite()
                || geometry.rotation.abs() > 360.0
                || geometry.crop.is_some_and(|crop| !crop.valid())
                || geometry.crop_ratio.is_some_and(|setting| !setting.valid()))
        {
            return Err(Error::Unsupported("成片几何不合法".into()));
        }

        // ② 编辑区：上限、ID 唯一、每层自洽、默认层位置落得进列表
        if self.areas.len() > MAX_AREAS {
            return Err(Error::Unsupported(format!("编辑区最多 {MAX_AREAS} 层")));
        }
        for area in &self.areas {
            area.valid()?;
        }
        for (index, area) in self.areas.iter().enumerate() {
            if self.areas[..index].iter().any(|other| other.id == area.id) {
                return Err(Error::Unsupported("同一张照片里层 ID 不能重复".into()));
            }
        }
        if let Some(index) = self.default_index {
            let layers = self.areas.len() + 1;
            if usize::from(index) >= layers {
                return Err(Error::Unsupported("默认层的位置超出了层列表".into()));
            }
        }
        Ok(())
    }

    /// 什么都没动过吗（没动过 = 与 SOOC 一样，不必渲染）。
    ///
    /// `as_shot_k` **不算「动过」**：它只是色温的解释基准，参数一个都没改就是没编辑过。
    /// 关闭已选择的镜头校正本身会改变像素，因此显式 `false` 也算编辑。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.color.is_none() && self.params.is_empty()
            && self.curves.is_empty()
            && self.areas.is_empty()
            && self.lens_profile.is_none()
            && self.base_curve_profile.is_none()
            && self.lut_chain.is_empty()
            && self.lut_enabled != Some(true)
            && self.lens_enabled != Some(false)
            && self.nr_method.is_none_or(NrMethod::is_default)
            && self.geometry.is_none_or(EditGeometry::is_identity)
    }

    /// 动过的项数（参数 + 曲线 + 镜头配置 + 非默认的降噪方式）。
    #[must_use]
    pub fn len(&self) -> usize {
        usize::from(self.color.is_some()) + self.params.len()
            + self.curves.len()
            + usize::from(self.lens_profile.is_some())
            + usize::from(self.base_curve_profile.is_some())
            + usize::from(!self.lut_chain.is_empty())
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
        // 旧形状（单支 @100% / 空）**逐字照旧**，否则已有缩略图缓存与定稿签名全部失效。
        if let Some(id) = legacy_lut_id(&self.lut_chain) {
            text.push_str(id);
        } else if self.lut_chain.is_empty() {
            text.push('-');
        } else {
            for layer in &self.lut_chain {
                text.push_str(&format!(
                    "#{}{}{:.5};",
                    layer.id,
                    if layer.enabled { "+" } else { "-" },
                    layer.strength
                ));
            }
        }
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

        if let Some(color) = &self.color {
            text.push_str("|color:");
            text.push_str(&serde_json::to_string(color).expect("finite color identity"));
        }
        // 层：**只有出现额外层时才续写**，否则旧签名逐字不变（缓存不失效）。
        if !self.areas.is_empty() {
            text.push_str("|areas:");
            text.push_str(&format!("d{}:", self.default_index.unwrap_or(0)));
            for area in &self.areas {
                text.push_str(&format!(
                    "#{}/{}{}{:.4};",
                    area.id,
                    match area.kind {
                        AreaKind::Full => "full".to_string(),
                        AreaKind::Point { x, y, tolerance, .. } => {
                            format!("pt{x:.4},{y:.4},{tolerance:.4}")
                        }
                    },
                    if area.enabled { "+" } else { "-" },
                    area.opacity
                ));
                for (id, value) in &area.params {
                    text.push_str(&format!("{id}={value:.6};"));
                }
                for (channel, points) in &area.curves {
                    text.push_str(channel);
                    text.push(':');
                    for point in points {
                        text.push_str(&format!("{:.4},{:.4};", point[0], point[1]));
                    }
                }
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
                    base_curve_profile, base_curve_points, lut_id, lut_enabled, lut_chain, auto_adjust, color_state, areas FROM develop_stacks WHERE asset_id = ?1",
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
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, Option<String>>(12)?,
                    row.get::<_, Option<String>>(13)?,
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
        lut_chain_json,
        auto_adjust,
        color_state,
        areas_json,
    )) = row
    {
        stack.color = color_state.map(|json| serde_json::from_str::<crate::color::PhotoColorState>(&json)
            .map_err(|e| Error::Unsupported(format!("色彩状态损坏：{e}")))).transpose()?;
        if let Some(color) = &stack.color { color.validate_frozen().map_err(|e| Error::Unsupported(e.into()))?; }
        stack.source_base = EditBase::parse(&source_base)
            .ok_or_else(|| Error::Unsupported(format!("未知的 issue 源：{source_base}")))?;
        #[allow(clippy::cast_possible_truncation)]
        {
            stack.as_shot_k = as_shot_k.map(|value| value as f32);
        }
        stack.lens_profile = lens_profile;
        stack.base_curve_profile = base_curve_profile;
        stack.lut_enabled = lut_enabled.map(|value| value != 0);
        let has_lut_chain = lut_chain_json.is_some();
        stack.lut_chain = lut_chain_json
            .map(|json| {
                serde_json::from_str::<Vec<LutLayer>>(&json)
                    .map_err(|error| Error::Unsupported(format!("LUT 链数据坏了：{error}")))
            })
            .transpose()?
            .unwrap_or_default();
        if !has_lut_chain {
            stack.adopt_legacy_lut(lut_id);
        }
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
        if let Some(json) = areas_json {
            let mut blob: AreasBlob = serde_json::from_str(&json)
                .map_err(|error| Error::Unsupported(format!("编辑区数据坏了：{error}")))?;
            /*
             * **旧容差的数值域换算**（2026-11-01 前存的是 `0..1` 的分数）：
             * 读进来的那一刻换成 `100..1000`，下次落库就写成新口径 ——
             * 只在内存里换算，不动历史记录（旧库不会被无谓改写）。
             *
             * 种子的定义也换过一次（线性域 → 感知域），但**没法换算**：
             * 旧 `seed_version = 0` 留给前端逐个重新取样（位置不变）。
             */
            for area in &mut blob.areas {
                if let AreaKind::Point { tolerance, .. } = &mut area.kind
                    && tolerance.is_finite()
                    && *tolerance <= 1.0
                {
                    *tolerance = crate::develop::areas::TOLERANCE_MIN
                        + tolerance.max(0.0)
                            * (crate::develop::areas::TOLERANCE_MAX
                                - crate::develop::areas::TOLERANCE_MIN);
                }
            }
            stack.areas = blob.areas;
            stack.default_index = blob.default_index;
        }
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

/// **存量修复候选**：温度是显式值、但 `as_shot_k` 为空的编辑栈
/// （`specs/editor-session-pipeline.md` §6.8）。
///
/// 白平衡的基线缺失时，`temperature` 这个绝对值没有「相对谁」的解释（§6.3）——
/// 这类行要用该照片的**解码真值**回填 `as_shot_k` 才能重新解释。
/// 返回 `(asset_id, temperature)`，按 asset 排序；是否执行由调用方（人类）决定。
///
/// # Errors
/// 数据库读失败。
pub fn stacks_missing_as_shot_baseline(conn: &Connection) -> Result<Vec<(i64, f64)>> {
    let mut statement = conn.prepare(
        "SELECT p.asset_id, p.value FROM develop_params p \
         JOIN develop_stacks d ON d.asset_id = p.asset_id \
         WHERE p.param_id = 'temperature' AND d.as_shot_k IS NULL \
         ORDER BY p.asset_id",
    )?;
    Ok(statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

/// **覆盖式写入**一张照片的编辑栈（前端那份载荷的语义）。
///
/// 语义：栈里没有的项**删掉**、有的项写入 —— 于是「重置这一项」与「拖回默认」
/// 在数据上是同一件事（前端把该发的都发过来，落库就是这一份）。
///
/// # Errors
/// 校验不过（未知 id / 值非法 / 坏曲线）或数据库写失败。
/// 懒回填：把这张照片 **latest 输出尺寸为空**的编辑栈补上（能算才算，算不出保持 NULL）。
///
/// 与 `issues::backfill_output_sizes` 对称、同口径（`store::sizes::stack_output_size`，
/// 零 I/O、零渲染）；幂等、只写缺值。**老库 / 源尺寸当时未知**留下的 NULL
/// 在这一步补成现成值（`specs/display-size-lifecycle.md` §2），读侧下次就纯读。
///
/// 返回这次真写了多少行。
///
/// # Errors
/// 数据库读写失败。
pub fn backfill_output_sizes(conn: &Connection, asset_id: i64) -> Result<usize> {
    let pending: Option<i64> = conn
        .query_row(
            "SELECT asset_id FROM develop_stacks
              WHERE asset_id = ?1 AND (output_width IS NULL OR output_height IS NULL)",
            [asset_id],
            |row| row.get(0),
        )
        .optional()?;
    if pending.is_none() {
        return Ok(0);
    }
    // 栈本体走读侧同一个装载口（参数在 `develop_params`，不是一个 JSON 列）
    let stack = load(conn, asset_id)?;
    let Some((width, height)) = crate::store::sizes::stack_output_size(conn, asset_id, &stack)?
    else {
        return Ok(0); // 源尺寸还未知：保持 NULL，下次再算
    };
    Ok(conn.execute(
        "UPDATE develop_stacks SET output_width = ?2, output_height = ?3
          WHERE asset_id = ?1 AND (output_width IS NULL OR output_height IS NULL)",
        rusqlite::params![asset_id, i64::from(width), i64::from(height)],
    )?)
}

pub fn save(conn: &Connection, asset_id: i64, stack: &DevelopStack, now_ms: i64) -> Result<usize> {
    stack.validate()?;

    // ② 栈本体（没有就建一个；as-shot 与镜头 / 降噪那几项跟着一起写）
    ensure_stack(conn, asset_id, now_ms)?;
    /*
     * latest 的**输出尺寸**在同一条 UPDATE / 同一笔事务里写（`specs/display-size-source.md` §3.2）：
     * 尺寸与它描述的栈同生共死 —— 栈在则尺寸在，本次编辑改了几何就跟着更新。
     * 只读库（源文件的展示尺寸是导入时写下的），**零渲染、零读盘**；
     * 源尺寸未知（老库未回填）就写 NULL，消费方按 `store::sizes` 的回退链拿。
     */
    let output = crate::store::sizes::stack_output_size(conn, asset_id, stack)?;
    let areas_json = if stack.areas.is_empty() {
        None
    } else {
        Some(
            serde_json::to_string(&AreasBlob {
                default_index: stack.default_index,
                areas: stack.areas.clone(),
            })
            .map_err(|error| Error::Unsupported(format!("编辑区序列化失败：{error}")))?,
        )
    };
    let lut_chain_json = if stack.lut_chain.is_empty() {
        None
    } else {
        Some(
            serde_json::to_string(&stack.lut_chain)
                .map_err(|error| Error::Unsupported(format!("LUT 链序列化失败：{error}")))?,
        )
    };
    conn.execute(
        "UPDATE develop_stacks SET as_shot_k = ?2, lens_profile = ?3, lens_enabled = ?4, \
         nr_method = ?5, source_base = ?6, edit_geometry = ?7, base_curve_profile = ?8, \
         base_curve_points = ?9, lut_chain = ?10, lut_id = NULL, lut_enabled = ?11, auto_adjust = ?12, color_state = ?13, \
         output_width = ?14, output_height = ?15, areas = ?16 WHERE asset_id = ?1",
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
            lut_chain_json,
            stack.lut_enabled.map(i64::from),
            stack.auto_adjust.as_ref().map(serde_json::to_string).transpose()
                .map_err(|error| Error::Unsupported(format!("自动调整基线序列化失败：{error}")))?,
            stack.color.as_ref().map(serde_json::to_string).transpose()
                .map_err(|error| Error::Unsupported(format!("色彩状态序列化失败：{error}")))?,
            output.map(|(width, _)| i64::from(width)),
            output.map(|(_, height)| i64::from(height)),
            areas_json,
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

/* ══════════════════════════════════════════════════════════════
 * 编辑区（area）的写入口（W3）
 * ══════════════════════════════════════════════════════════════ */

/// 一次「读栈 → 改 → 校验 → 落库」的公共外壳。
///
/// 所有层写入口都走它：截上限、结构校验、`default_index` 的合法性只在
/// [`DevelopStack::validate`] 里判一次（不在每个入口各抄一遍）。
fn mutate_stack(
    conn: &Connection,
    asset_id: i64,
    now_ms: i64,
    change: impl FnOnce(&mut DevelopStack) -> Result<()>,
) -> Result<()> {
    let mut stack = load(conn, asset_id)?;
    change(&mut stack)?;
    stack.validate()?;
    save(conn, asset_id, &stack, now_ms)?;
    Ok(())
}

/// 层列表 → **行序**（`None` = 默认层，`Some(id)` = 附加层）。
///
/// 界面上看到的就是这个顺序（列表自上而下 = 计算顺序），结构类撤销操作的
/// `from` / `to` / `index` 一律是它的下标。
#[must_use]
pub fn area_rows(stack: &DevelopStack) -> Vec<Option<u32>> {
    let default_index = stack.default_index.map_or(0, usize::from);
    let mut rows = Vec::with_capacity(stack.areas.len() + 1);
    let mut areas = stack.areas.iter();
    for position in 0..stack.areas.len() + 1 {
        if position == default_index {
            rows.push(None);
        } else if let Some(area) = areas.next() {
            rows.push(Some(area.id));
        }
    }
    rows
}

/// 行序 → `(areas, default_index)`（[`area_rows`] 的逆）。
fn rows_to_stack(rows: &[Option<u32>], stack: &mut DevelopStack) -> Result<()> {
    let mut areas = Vec::with_capacity(stack.areas.len());
    let mut default_index = None;
    for (position, row) in rows.iter().enumerate() {
        match row {
            None => default_index = Some(position),
            Some(id) => {
                let Some(area) = stack.areas.iter().find(|area| area.id == *id) else {
                    return Err(Error::Unsupported(format!("撤销栈里的层 {id} 不在编辑栈里")));
                };
                areas.push(area.clone());
            }
        }
    }
    if areas.len() != stack.areas.len() {
        return Err(Error::Unsupported("编辑区行序与层列表对不上".into()));
    }
    #[allow(clippy::cast_possible_truncation)]
    {
        stack.default_index = default_index.map(|index| index as u8);
    }
    stack.areas = areas;
    Ok(())
}

fn area_index(stack: &DevelopStack, id: u32) -> Result<usize> {
    stack
        .areas
        .iter()
        .position(|area| area.id == id)
        .ok_or_else(|| Error::Unsupported(format!("编辑栈里没有层 {id}")))
}

/// 写**一层**里的一个参数（`area_id = None` ⇒ 默认层，走原来的入口）。
///
/// # Errors
/// 层不存在 / 参数非法 / 全局参数写进非默认层 / 数据库写失败。
pub fn set_area_param(
    conn: &Connection,
    asset_id: i64,
    area_id: Option<u32>,
    param_id: &str,
    value: Option<f64>,
    now_ms: i64,
) -> Result<()> {
    let Some(area_id) = area_id else {
        return set_param(conn, asset_id, param_id, value, now_ms);
    };
    let Some(spec) = spec(param_id) else {
        return Err(Error::Unsupported(format!("未知的显影参数：{param_id}")));
    };
    if spec.target == crate::develop::params::ParamTarget::Global {
        return Err(Error::Unsupported(format!(
            "{param_id} 是全局阶段参数，只能挂在默认层"
        )));
    }
    if spec.target == crate::develop::params::ParamTarget::Mask {
        return Err(Error::Unsupported(format!("{param_id} 由掩膜自己管，不能写进层参数")));
    }
    if let Some(value) = value
        && !spec.accepts(value)
    {
        return Err(Error::Unsupported(format!(
            "参数 {param_id} 的值非法：{value}（允许 {}..{}）",
            spec.min, spec.max
        )));
    }
    mutate_stack(conn, asset_id, now_ms, |stack| {
        let index = area_index(stack, area_id)?;
        match value {
            Some(value) => {
                stack.areas[index].params.insert(param_id.to_string(), value);
            }
            None => {
                stack.areas[index].params.remove(param_id);
            }
        }
        Ok(())
    })
}

/// 写**一层**里的一条曲线（`area_id = None` ⇒ 默认层）。
///
/// # Errors
/// 层不存在 / 通道未知 / 控制点不合法 / 数据库写失败。
pub fn set_area_curve(
    conn: &Connection,
    asset_id: i64,
    area_id: Option<u32>,
    channel: &str,
    points: Option<&[[f32; 2]]>,
    now_ms: i64,
) -> Result<()> {
    let Some(area_id) = area_id else {
        return set_curve(conn, asset_id, channel, points, now_ms);
    };
    if CurveChannel::parse(channel).is_none() {
        return Err(Error::Unsupported(format!("未知的曲线通道：{channel}")));
    }
    if let Some(points) = points {
        Curve::from_points(points.to_vec())
            .map_err(|e| Error::Unsupported(format!("曲线 {channel} 不合法：{e}")))?;
    }
    mutate_stack(conn, asset_id, now_ms, |stack| {
        let index = area_index(stack, area_id)?;
        match points {
            Some(points) => {
                stack.areas[index]
                    .curves
                    .insert(channel.to_string(), points.to_vec());
            }
            None => {
                stack.areas[index].curves.remove(channel);
            }
        }
        Ok(())
    })
}

/// 在第 `index` 行插入一层（`None` = 追加到末尾）。
///
/// # Errors
/// 层 JSON 坏 / 层数超限（`validate` 会拒）/ 数据库写失败。
pub fn add_area(
    conn: &Connection,
    asset_id: i64,
    index: Option<usize>,
    area_json: &str,
    now_ms: i64,
) -> Result<()> {
    let area: Area = serde_json::from_str(area_json)
        .map_err(|e| Error::Unsupported(format!("撤销栈里的层数据坏了：{e}")))?;
    mutate_stack(conn, asset_id, now_ms, |stack| {
        let mut rows = area_rows(stack);
        let position = index.unwrap_or(rows.len()).min(rows.len());
        rows.insert(position, Some(area.id));
        // 层本体先放进列表（`rows_to_stack` 按 id 回填顺序）
        stack.areas.push(area);
        rows_to_stack(&rows, stack)?;
        if stack.default_index.is_none() {
            stack.default_index = Some(0);
        }
        Ok(())
    })
}

/// 删掉第 `index` 行的那一层。
///
/// # Errors
/// 行下标越界 / 那里是默认层（默认层不可删）/ 数据库写失败。
pub fn remove_area(conn: &Connection, asset_id: i64, index: usize, now_ms: i64) -> Result<()> {
    mutate_stack(conn, asset_id, now_ms, |stack| {
        let rows = area_rows(stack);
        let Some(row) = rows.get(index).copied() else {
            return Err(Error::Unsupported("要删的层不在列表里".into()));
        };
        let Some(id) = row else {
            return Err(Error::Unsupported("默认层不能删除".into()));
        };
        let at = area_index(stack, id)?;
        stack.areas.remove(at);
        Ok(())
    })
}

/// 把第 `from` 行移到第 `to` 行（拖动排序）。
///
/// 行序含默认层 —— 把默认层拖到别的位置就是「LUT 在这个全图层之后」。
///
/// # Errors
/// 行下标越界 / 数据库写失败。
pub fn move_area(conn: &Connection, asset_id: i64, from: usize, to: usize, now_ms: i64) -> Result<()> {
    mutate_stack(conn, asset_id, now_ms, |stack| {
        let mut rows = area_rows(stack);
        if from >= rows.len() || to >= rows.len() {
            return Err(Error::Unsupported("层排序的下标越界".into()));
        }
        if from == to {
            return Ok(());
        }
        let row = rows.remove(from);
        rows.insert(to, row);
        rows_to_stack(&rows, stack)
    })
}

/// 改层名（空串 = 回到类型默认名）。
///
/// # Errors
/// 层不存在 / 名字超长（`validate` 会拒）/ 数据库写失败。
pub fn rename_area(conn: &Connection, asset_id: i64, id: u32, name: &str, now_ms: i64) -> Result<()> {
    mutate_stack(conn, asset_id, now_ms, |stack| {
        let index = area_index(stack, id)?;
        stack.areas[index].name = name.to_string();
        Ok(())
    })
}

/// 层的启停。
///
/// # Errors
/// 层不存在 / 数据库写失败。
pub fn toggle_area(
    conn: &Connection,
    asset_id: i64,
    id: u32,
    enabled: bool,
    now_ms: i64,
) -> Result<()> {
    mutate_stack(conn, asset_id, now_ms, |stack| {
        let index = area_index(stack, id)?;
        stack.areas[index].enabled = enabled;
        Ok(())
    })
}

/// 控制点的颜色槽位。
///
/// # Errors
/// 层不存在 / 槽位越界 / 数据库写失败。
pub fn set_area_color(conn: &Connection, asset_id: i64, id: u32, color: u8, now_ms: i64) -> Result<()> {
    mutate_stack(conn, asset_id, now_ms, |stack| {
        let index = area_index(stack, id)?;
        stack.areas[index].color = color;
        Ok(())
    })
}

/// 控制点的采样位 / 种子 / 容差（`[x, y, seed0, seed1, seed2, tolerance]`）。
///
/// 六个数一起写：落点（位置 + 种子）与拖 handle（容差）都是同一组事实的一部分，
/// 分成两个入口只会让撤销栈里多出「一半旧、一半新」的中间态。
///
/// # Errors
/// 层不存在 / 不是控制点 / 数值非法 / 数据库写失败。
pub fn set_area_point(
    conn: &Connection,
    asset_id: i64,
    id: u32,
    values: [f32; 6],
    now_ms: i64,
) -> Result<()> {
    mutate_stack(conn, asset_id, now_ms, |stack| {
        let index = area_index(stack, id)?;
        let AreaKind::Point { x, y, seed, tolerance } = &mut stack.areas[index].kind else {
            return Err(Error::Unsupported("只有控制点有采样位与容差".into()));
        };
        *x = values[0];
        *y = values[1];
        seed[0] = values[2];
        seed[1] = values[3];
        seed[2] = values[4];
        *tolerance = values[5];
        Ok(())
    })
}

/// 控制点的六个数 → `[u32; 6]` 位模式（撤销栈要 `Eq`，f32 没有）。
#[must_use]
pub fn point_bits(values: [f32; 6]) -> [u32; 6] {
    values.map(f32::to_bits)
}

/// 位模式 → 六个 f32（[`point_bits`] 的逆）。
#[must_use]
pub fn point_from_bits(bits: [u32; 6]) -> [f32; 6] {
    bits.map(f32::from_bits)
}

/// 控制点的六个数（层不是控制点 ⇒ `None`）。
#[must_use]
pub fn area_point(area: &Area) -> Option<[f32; 6]> {
    match area.kind {
        AreaKind::Point { x, y, seed, tolerance } => Some([
            x,
            y,
            seed[0],
            seed[1],
            seed[2],
            tolerance,
        ]),
        AreaKind::Full => None,
    }
}

/// 编辑栈里**不是参数也不是曲线**的那几项（M3-W4）。
///
/// 它们共用一种撤销操作（[`crate::store::marking::Op::DevelopSetting`]）：
/// 三项都是「一个可空的字符串/布尔」，各自的语义与校验写在 [`set_setting`] 里。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Color,
    /// 自动调整基线 JSON，作为整体参与撤销。
    AutoAdjust,
    /// 机型基础曲线档案选择（撤销时同时还原快照）。
    BaseCurveProfile,
    /// 被选档案的曲线快照，与选择一同撤销。
    BaseCurvePoints,
    /// 应用级 LUT **链**（JSON 数组：`[{id, enabled, strength}]`）。
    LutChain,
    /// LUT 总开关。
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
            Self::Color => "color",
            Self::AutoAdjust => "autoAdjust",
            Self::BaseCurveProfile => "baseCurveProfile",
            Self::BaseCurvePoints => "baseCurvePoints",
            Self::LutChain => "lutChain",
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
            "color" => Some(Self::Color),
            "autoAdjust" => Some(Self::AutoAdjust),
            "baseCurveProfile" => Some(Self::BaseCurveProfile),
            "baseCurvePoints" => Some(Self::BaseCurvePoints),
            "lutChain" | "lutId" => Some(Self::LutChain),
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
            Self::Color => "color_state",
            Self::AutoAdjust => "auto_adjust",
            Self::BaseCurveProfile => "base_curve_profile",
            Self::BaseCurvePoints => "base_curve_points",
            Self::LutChain => "lut_chain",
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
            (Self::Color, Some(text)) => serde_json::from_str::<crate::color::PhotoColorState>(text)
                .is_ok_and(|color| color.validate_frozen().is_ok()),
            (Self::AutoAdjust, Some(text)) => serde_json::from_str::<AutoAdjustBaseline>(text)
                .is_ok_and(|automatic| automatic.validate().is_ok()),
            (Self::BaseCurveProfile, Some(text)) => {
                text == "none" || text.parse::<i64>().is_ok_and(|id| id > 0)
            }
            (Self::BaseCurvePoints, Some(text)) => serde_json::from_str::<Vec<[f32; 2]>>(text)
                .is_ok_and(|points| Curve::from_points(points).is_ok()),
            (Self::LutChain, Some(text)) => parse_lut_chain_setting(text).is_some(),
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
            Setting::Color => self.color.as_ref().and_then(|color| serde_json::to_string(color).ok()),
            Setting::AutoAdjust => self
                .auto_adjust
                .as_ref()
                .and_then(|automatic| serde_json::to_string(automatic).ok()),
            Setting::BaseCurveProfile => self.base_curve_profile.clone(),
            Setting::BaseCurvePoints => self
                .base_curve_points
                .as_ref()
                .and_then(|points| serde_json::to_string(points).ok()),
            Setting::LutChain => {
                if self.lut_chain.is_empty() {
                    None
                } else {
                    serde_json::to_string(&self.lut_chain).ok()
                }
            }
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

/// 撤销值 → 链。**同时接受**新 JSON 数组与旧版 `lutId` 的纯 id。
///
/// 旧撤销记录（`Op::DevelopSetting { key: "lutId", after: "film" }`）必须还能还原；
/// 但写库永远只写新链 JSON，不会再生成第二套单支真相。
fn parse_lut_chain_setting(text: &str) -> Option<Vec<LutLayer>> {
    if let Ok(chain) = serde_json::from_str::<Vec<LutLayer>>(text) {
        let unique = chain.iter().enumerate().all(|(index, layer)| {
            !chain[..index].iter().any(|other| other.id == layer.id)
        });
        return (chain.len() <= MAX_LUT_LAYERS
            && chain.iter().all(LutLayer::valid)
            && unique)
            .then_some(chain);
    }
    let layer = LutLayer::new(text);
    layer.valid().then(|| vec![layer])
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
    let sql = if setting == Setting::LutChain {
        // 新链落地时同时清掉旧单支列，否则下一次 load 会把它重新并入链。
        "UPDATE develop_stacks SET lut_chain = ?2, lut_id = NULL WHERE asset_id = ?1".to_string()
    } else {
        format!(
            "UPDATE develop_stacks SET {} = ?2 WHERE asset_id = ?1",
            setting.column()
        )
    };
    let normalized = if setting == Setting::LutChain {
        value
            .map(|text| {
                let chain = parse_lut_chain_setting(text)
                    .ok_or_else(|| Error::Unsupported("LUT 链撤销值非法".into()))?;
                serde_json::to_string(&chain)
                    .map_err(|error| Error::Unsupported(format!("LUT 链序列化失败：{error}")))
            })
            .transpose()?
    } else {
        value.map(str::to_string)
    };
    let stored = if setting == Setting::SourceBase {
        Some(normalized.as_deref().unwrap_or("raw"))
    } else {
        normalized.as_deref()
    };
    conn.execute(&sql, rusqlite::params![asset_id, stored])?;
    /*
     * 这一项改变**源或几何**时，latest 的输出尺寸跟着变（撤销拖一次几何、
     * 切换编辑基准都走这条路；§3.3 的「栈变→尺寸跟着变」不只是 `save` 的约定）。
     */
    if matches!(setting, Setting::SourceBase | Setting::Geometry) {
        refresh_output_size(conn, asset_id)?;
    }
    prune_empty_stack(conn, asset_id)?;
    Ok(())
}

/// 用**库里当前那一行**重算 latest 的输出尺寸（`save` 之外的写入口共用；只读库）。
///
/// 幂等：算不出来就写 NULL（消费方回退）—— 与 [`save`] 同一套口径。
fn refresh_output_size(conn: &Connection, asset_id: i64) -> Result<()> {
    let stack = load(conn, asset_id)?;
    let output = crate::store::sizes::stack_output_size(conn, asset_id, &stack)?;
    conn.execute(
        "UPDATE develop_stacks SET output_width = ?2, output_height = ?3 WHERE asset_id = ?1",
        rusqlite::params![
            asset_id,
            output.map(|(width, _)| i64::from(width)),
            output.map(|(_, height)| i64::from(height)),
        ],
    )?;
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
            AND color_state IS NULL
            AND auto_adjust IS NULL
            AND lens_profile IS NULL
            AND base_curve_profile IS NULL
            -- 基础曲线的**快照**也是栈的一部分：撤销是逐项重放的，
            -- 「先写 points、再写 profile」这两个中间态之间不能把整行删掉
            -- （2026-10-31：`ChangeSet::invert` 改逆序之后暴露出来的）。
            AND base_curve_points IS NULL
            AND lut_chain IS NULL
            AND areas IS NULL
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

/// **这张照片要不要生成 preview**（`memory/FUNCTION-IMAGING.md` §4，人类 2026-09-24 定）。
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

/// 编辑器该**编辑哪个文件**（「编辑落在 RAW 上」，`memory/FUNCTION-REPOSITORY.md` §4.1）。
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

/// 一批照片里**编辑过的那些**（浏览分页每行要带一个 `edited` 用，2026-09-29）。
///
/// 判据与 [`has_edits`] 完全同一份（`DevelopStack::is_empty`），不另立 SQL 判据 ——
/// 这里只是把「哪些照片有栈」合成一趟 `IN` 查询，再对真正有栈的少数几张 `load`。
/// 分页一次几十上百行，逐张 `load` 是纯浪费。
///
/// # Errors
/// 数据库读失败。
pub fn has_edits_batch(conn: &Connection, asset_ids: &[i64]) -> Result<Vec<i64>> {
    let mut edited = Vec::new();
    // SQLite 默认变量上限 999，按 500 一段切（分页远小于它，这里只是保险）
    for chunk in asset_ids.chunks(500) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!("SELECT asset_id FROM develop_stacks WHERE asset_id IN ({placeholders})");
        let candidates: Vec<i64> = {
            let mut statement = conn.prepare(&sql)?;
            let mut rows = statement.query(rusqlite::params_from_iter(chunk.iter()))?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                out.push(row.get::<_, i64>(0)?);
            }
            out
        };
        for id in candidates {
            if !load(conn, id)?.is_empty() {
                edited.push(id);
            }
        }
    }
    Ok(edited)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::develop::geometry::CropRect;
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

    #[test]
    fn frozen_color_roundtrips_and_uses_existing_undo_without_changing_legacy() {
        use crate::store::marking::{self, ChangeSet, Op, UndoStack};
        let (conn, asset_id) = catalog_with_asset();
        let legacy = DevelopStack::default();
        let json = serde_json::to_string(&legacy).unwrap();
        assert!(!json.contains("color"));
        let original_signature = legacy.signature();
        let color = crate::color::PhotoColorState::new_pipeline(crate::color::SourceColor::AssumedSrgb);
        let encoded = serde_json::to_string(&color).unwrap();
        let change = ChangeSet::new("指定输入色彩", vec![Op::DevelopSetting {
            asset_id, key: "color".into(), before: None, after: Some(encoded.clone()),
        }]);
        marking::apply(&conn, &change).unwrap();
        let loaded = load(&conn, asset_id).unwrap();
        assert_eq!(loaded.color, Some(color));
        assert!(!loaded.is_empty());
        assert_ne!(loaded.signature(), original_signature);
        assert!(has_edits(&conn, asset_id).unwrap());
        let mut undo = UndoStack::default(); undo.push(change);
        undo.undo(&conn).unwrap();
        assert_eq!(load(&conn, asset_id).unwrap(), legacy);
        assert_eq!(load(&conn, asset_id).unwrap().signature(), original_signature);
        undo.redo(&conn).unwrap();
        assert_eq!(load(&conn, asset_id).unwrap(), loaded);
        for invalid in ["{}", r#"{"process_version":"linear_rec2020_v2","source":{"kind":"auto"}}"#,
            r#"{"process_version":"linear_rec2020_v2","source":{"kind":"assumed_srgb"},"monitor":"x"}"#] {
            assert!(set_setting(&conn, asset_id, Setting::Color, Some(invalid), 3).is_err());
        }
        assert_eq!(load(&conn, asset_id).unwrap(), loaded);
        save(&conn, asset_id, &loaded, 4).unwrap();
        assert_eq!(load(&conn, asset_id).unwrap().setting_value(Setting::Color), Some(encoded));
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

    fn point_area(id: u32, color: u8, params: &[(&str, f64)]) -> Area {
        Area {
            id,
            kind: AreaKind::Point {
                x: 0.5,
                y: 0.5,
                seed: [0.4, -0.1, 0.05],
                tolerance: crate::develop::areas::TOLERANCE_DEFAULT,
            },
            name: format!("控制点 {id}"),
            enabled: true,
            opacity: 1.0,
            color,
            params: params
                .iter()
                .map(|(id, value)| ((*id).to_string(), *value))
                .collect(),
            curves: BTreeMap::new(),
            seed_version: SEED_VERSION,
        }
    }

    #[test]
    fn area_specs_disable_points_for_sooc_and_legacy_seed_versions() {
        let mut stack = stack(&[], &[]);
        stack.areas.push(point_area(1, 0, &[]));
        stack.areas.push(Area {
            id: 2,
            kind: AreaKind::Full,
            name: "全图".into(),
            ..point_area(1, 0, &[])
        });
        stack.source_base = EditBase::Raw;
        let raw = stack.area_specs();
        assert!(raw[0].enabled, "RAW 下当前版本的控制点应参与计算");
        assert!(raw[1].enabled, "全图层不受基准限制");

        stack.source_base = EditBase::Sooc;
        let sooc = stack.area_specs();
        assert!(!sooc[0].enabled, "SOOC 下控制点必须整层旁路");
        assert!(sooc[1].enabled, "SOOC 下全图层仍然计算");

        // 旧种子不能按新定义解释：宁可暂时不生效，也不能改错区域
        stack.source_base = EditBase::Raw;
        stack.areas[0].seed_version = 0;
        assert!(!stack.area_specs()[0].enabled);
    }

    /// **旧种子不拿新范围卡**（2026-11-01 真机：调容差就报「控制点…不合法」）。
    ///
    /// 旧种子是线性域的 `[luma, c1, c2]`，量纲/范围都跟新的 `[Y′, Cb, Cr]` 不同。
    /// 只要还能用新范围去校验它，就会死锁：
    /// `develop_area_seed` 先校验整栈 → 抛错 → 旧种子永远没机会重新取样 →
    /// 每次落库都报错。这里钉住「旧种子只需要是有限数」。
    #[test]
    fn legacy_seed_outside_the_new_ranges_still_validates_and_bypasses() {
        let mut area = point_area(1, 0, &[]);
        area.seed_version = 0;
        area.kind = AreaKind::Point {
            x: 0.3,
            y: 0.7,
            // 线性域的旧值：Cb/Cr 轻松越过 ±0.5
            seed: [1.6, 0.93, -2.1],
            tolerance: 0.4,
        };
        area.valid().expect("旧种子只要求有限数，不能被新范围误杀");
        // 渲染侧仍然整层旁路（旧种子不能按新定义解释）
        assert!(!area.to_spec().enabled);

        // 真的坏数据（NaN）照样拦
        area.kind = AreaKind::Point {
            x: 0.3,
            y: 0.7,
            seed: [f32::NAN, 0.0, 0.0],
            tolerance: 0.4,
        };
        area.seed_version = 0;
        assert!(area.valid().is_err(), "非有限的种子不能被放行");
    }

    #[test]
    fn legacy_tolerance_is_accepted_and_mapped_to_the_new_domain() {
        let mut area = point_area(1, 0, &[]);
        area.kind = AreaKind::Point {
            x: 0.4,
            y: 0.6,
            seed: [0.3, 0.0, 0.0],
            tolerance: 0.2,
        };
        area.valid().expect("旧 0..1 容差仍应能读回");
        let spec = area.to_spec();
        match spec.kind {
            crate::develop::AreaKind::Point { tolerance, .. } => {
                assert!(
                    (tolerance - 280.0).abs() < 1e-3,
                    "0.2 应映射为 100 + 0.2×900 = 280（实际 {tolerance}）"
                );
            }
            crate::develop::AreaKind::Full => panic!("层类型不能走样"),
        }
    }

    /// **层要能存进去、读回来**（2026-11-01 真机：「建过 area 退出再进来只剩默认层」）。
    ///
    /// 这条同时钉住两件事：
    /// * `AreasBlob` 的 `default_index` 与 `areas` 一起落库、一起读回；
    /// * 空 `params` / `curves` 的层序列化后带 `{}`（不再是**缺字段**）——
    ///   前端 `Object.entries(area.curves)` 才不会拿到 `undefined`。
    #[test]
    fn areas_roundtrip_through_the_catalog() {
        let (conn, asset_id) = catalog_with_asset();
        let mut stack = stack(&[], &[]);
        stack.areas.push(point_area(3, 2, &[("exposure", 0.75)]));
        stack.areas.push(Area {
            id: 4,
            kind: AreaKind::Full,
            name: "全图 1".into(),
            ..point_area(3, 2, &[])
        });
        stack.default_index = Some(1);
        save(&conn, asset_id, &stack, 1).expect("落库");

        let loaded = load(&conn, asset_id).expect("读回");
        assert_eq!(loaded.areas.len(), 2, "层必须真的读回来");
        assert_eq!(loaded.default_index, Some(1), "默认层位置跟着回来");
        assert_eq!(loaded.areas[0].id, 3);
        assert_eq!(loaded.areas[0].name, "控制点 3");
        assert_eq!(loaded.areas[0].color, 2);
        assert!((loaded.areas[0].params["exposure"] - 0.75).abs() < 1e-9);
        assert!(loaded.areas[0].curves.is_empty());
        assert!(matches!(
            loaded.areas[0].kind,
            AreaKind::Point { tolerance, .. }
                if (tolerance - crate::develop::areas::TOLERANCE_DEFAULT).abs() < 1e-3
        ));
        assert!(matches!(loaded.areas[1].kind, AreaKind::Full), "层类型不许走样");

        // 空层也要把两个映射写成 `{}`（不是省掉字段）
        let blob: serde_json::Value = serde_json::from_str(
            &conn
                .query_row(
                    "SELECT areas FROM develop_stacks WHERE asset_id = ?1",
                    [asset_id],
                    |row| row.get::<_, String>(0),
                )
                .expect("读 blob"),
        )
        .expect("blob 是 JSON");
        let empty = &blob["areas"][1];
        assert_eq!(empty["params"], serde_json::json!({}));
        assert_eq!(empty["curves"], serde_json::json!({}));
    }

    #[test]
    fn areas_stay_out_of_the_legacy_serialized_form() {
        // 不变量 1：没用到层时，序列化里**不许**出现 areas / default_index
        let plain = stack(&[("exposure", 1.0)], &[]);
        let json = serde_json::to_value(&plain).unwrap();
        assert!(json.get("areas").is_none(), "退化形状不许写 areas");
        assert!(json.get("default_index").is_none(), "退化形状不许写 default_index");

        // 有额外层 ⇒ 必须写，而且能原样读回
        let with_area = DevelopStack {
            areas: vec![point_area(1, 0, &[("contrast", 10.0)])],
            default_index: Some(0),
            ..plain
        };
        let value = serde_json::to_value(&with_area).unwrap();
        assert!(value.get("areas").is_some(), "有额外层必须写 areas");
        let back: DevelopStack = serde_json::from_value(value).unwrap();
        assert_eq!(back, with_area);
    }

    #[test]
    fn global_params_are_rejected_inside_an_area() {
        // 不变量 2：阶段边界 —— 全局阶段参数只能挂在默认层
        let mut global = stack(&[], &[]);
        global.areas.push(point_area(1, 0, &[("sharpenAmount", 10.0)]));
        assert!(global.validate().is_err(), "清晰度是全局阶段，不能进层");

        let mut lens = stack(&[], &[]);
        lens.areas.push(point_area(1, 0, &[("vignette", 10.0)]));
        assert!(lens.validate().is_err(), "镜头是全局阶段，不能进层");

        let mut layered = stack(&[], &[]);
        layered.areas.push(point_area(1, 0, &[("contrast", 10.0)]));
        assert!(layered.validate().is_ok(), "反差是逐像素项，可以进层");
    }

    #[test]
    fn areas_change_the_cache_signature() {
        // 不变量 1 的另一半：没有额外层时签名不变（这里对同一个栈前后比），有层就变
        let plain = stack(&[("exposure", 1.0)], &[]);
        assert_eq!(plain.signature(), plain.clone().signature());
        let with_area = DevelopStack {
            areas: vec![point_area(1, 0, &[])],
            ..plain.clone()
        };
        assert_ne!(plain.signature(), with_area.signature(), "多一层必须换签名");
    }

    #[test]
    fn invalid_area_shapes_are_rejected() {
        let mut too_many = stack(&[], &[]);
        for id in 1..=(MAX_AREAS as u32 + 1) {
            too_many.areas.push(point_area(id, 0, &[]));
        }
        assert!(too_many.validate().is_err(), "超过 32 层要拦");

        let mut duplicate = stack(&[], &[]);
        duplicate.areas.push(point_area(1, 0, &[]));
        duplicate.areas.push(point_area(1, 1, &[]));
        assert!(duplicate.validate().is_err(), "层 ID 重复要拦");

        let mut bad_color = stack(&[], &[]);
        bad_color.areas.push(point_area(1, 8, &[]));
        assert!(bad_color.validate().is_err(), "颜色槽位越界要拦");

        let mut bad_index = stack(&[], &[]);
        bad_index.default_index = Some(1);
        assert!(bad_index.validate().is_err(), "默认层位置越界要拦");
    }

    #[test]
    fn point_colors_go_to_the_least_used_slot() {
        let mut s = stack(&[], &[]);
        assert_eq!(s.next_point_color(), 0, "空栈从 0 开始");

        s.areas.push(point_area(1, 0, &[]));
        assert_eq!(s.next_point_color(), 1, "0 用过一次，下一次取 1");

        s.areas.push(point_area(2, 1, &[]));
        assert_eq!(s.next_point_color(), 2);

        // 全图不占颜色
        s.areas.push(Area {
            kind: AreaKind::Full,
            name: "全图 1".into(),
            ..point_area(3, 5, &[])
        });
        assert_eq!(s.next_point_color(), 2, "全图层不占颜色槽位");

        // 0..3 各用过一次 ⇒ 下一个是最小的**没用过**的 4
        s.areas.push(point_area(4, 2, &[]));
        s.areas.push(point_area(5, 3, &[]));
        assert_eq!(s.next_point_color(), 4, "0..3 各一次，取最小空闲 4");

        // 八个槽位都用满之后，并列时取下标最小的
        for (index, color) in [4u8, 5, 6, 7].into_iter().enumerate() {
            s.areas.push(point_area(10 + index as u32, color, &[]));
        }
        assert_eq!(s.next_point_color(), 0, "全满后并列在 0，取下标最小的 0");
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
    fn batch_flags_match_the_single_photo_judge() {
        // 三张：没编辑过 / 编辑过（动了一个参数）/ 有栈但只剩自动基线（不算编辑）
        let (conn, first) = catalog_with_asset();
        let mut add_asset = || {
            conn.execute(
                "INSERT INTO assets (rating, flag, imported_at, updated_at) VALUES (0, 'none', 0, 0)",
                [],
            )
            .expect("插资产");
            conn.last_insert_rowid()
        };
        let edited = add_asset();
        let baseline_only = add_asset();

        save(&conn, edited, &stack(&[("exposure", 0.5)], &[]), 1).expect("存编辑");
        let mut baseline = stack(&[], &[]);
        baseline.auto_adjust = Some(AutoAdjustBaseline {
            values: [("exposure".to_owned(), 0.5)].into(),
            lens_profile: None,
            lens_enabled: None,
            nr_method: None,
        });
        save(&conn, baseline_only, &baseline, 1).expect("存基线");

        // 空切片与不存在的 id 都安全
        assert!(has_edits_batch(&conn, &[]).expect("空批").is_empty());
        assert!(has_edits_batch(&conn, &[999_999]).expect("无栈").is_empty());
        // 判据与逐张 has_edits 完全一致：动过参数的才算，只剩自动基线的不算
        let flagged = has_edits_batch(&conn, &[first, edited, baseline_only, edited])
            .expect("批量查")
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(flagged, [edited].into_iter().collect());
        assert_eq!(flagged.len(), 1);
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

    /// 存量修复候选的选取（`specs/editor-session-pipeline.md` §6.8）：
    /// 只要「显式温度 + 基线为空」那一类；基线已在、没有显式温度的一律不动。
    #[test]
    fn missing_baseline_candidates_are_exactly_the_dirty_temperature_rows() {
        let (conn, _) = catalog_with_asset();
        conn.execute(
            "INSERT INTO assets (rating, flag, imported_at, updated_at) VALUES (0, 'none', 0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO assets (rating, flag, imported_at, updated_at) VALUES (0, 'none', 0, 0)",
            [],
        )
        .unwrap();
        // ① 污染行：显式温度 + 基线为空 → 候选
        save(
            &conn,
            1,
            &stack(&[("temperature", 5008.4043)], &[]),
            now_millis(),
        )
        .unwrap();
        // ② 显式温度 + 基线已在 → 不动
        let mut fine = stack(&[("temperature", 4300.0)], &[]);
        fine.as_shot_k = Some(4350.0);
        save(&conn, 2, &fine, now_millis()).unwrap();
        // ③ 基线为空但没有显式温度（未动过白平衡）→ 不动
        let mut neutral = stack(&[("exposure", 0.5)], &[]);
        neutral.as_shot_k = None;
        save(&conn, 3, &neutral, now_millis()).unwrap();

        let candidates = stacks_missing_as_shot_baseline(&conn).unwrap();
        assert_eq!(candidates, vec![(1, 5008.4043)]);
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
    fn legacy_single_lut_is_normalized_into_the_chain() {
        // 旧 JSON（单支 `lut_id`）→ 一层 @100%。反序列化是唯一入口，所以 XMP / issue / preset 全都吃到。
        let legacy: DevelopStack = serde_json::from_str(
            r#"{"source_base":"raw","params":{},"curves":{},"lut_id":"film","lut_enabled":true}"#,
        )
        .unwrap();
        assert_eq!(legacy.lut_chain, vec![LutLayer::new("film")]);
        assert!(legacy.lut_chain_active());
        // 已经有多层链时，旧字段不覆盖它（不会被再插一层）。
        let both: DevelopStack = serde_json::from_str(
            r#"{"lut_id":"old","lut_chain":[{"id":"a","enabled":true,"strength":1.0}]}"#,
        )
        .unwrap();
        assert_eq!(both.lut_chain.len(), 1);
        assert_eq!(both.lut_chain[0].id, "a");
        // 显式空链已经是新语义，不能再被旧的 lut_id 复活。
        let cleared: DevelopStack = serde_json::from_str(r#"{"lut_id":"old","lut_chain":[]}"#).unwrap();
        assert!(cleared.lut_chain.is_empty());
    }

    #[test]
    fn single_layer_at_full_strength_keeps_the_legacy_json_and_signature() {
        // 定稿哈希就是对这份 JSON 做的 FNV：老数据（单支 @100%）必须逐字不变。
        let mut stack = DevelopStack::default();
        stack.lut_chain = vec![LutLayer::new("film")];
        let text = serde_json::to_string(&stack).unwrap();
        assert!(text.contains(r#""lut_id":"film""#), "{text}");
        assert!(!text.contains("lut_chain"), "{text}");
        let empty = serde_json::to_string(&DevelopStack::default()).unwrap();
        assert!(empty.contains(r#""lut_id":null"#), "{empty}");
        // 多层 / 改强度才换形状。
        let mut multi = stack.clone();
        multi.lut_chain.push(LutLayer::new("warm"));
        let text = serde_json::to_string(&multi).unwrap();
        assert!(text.contains("lut_chain"), "{text}");
        assert!(!text.contains(r#""lut_id":"film""#), "{text}");
    }

    #[test]
    fn wire_rejects_unknown_source_base_and_tolerates_null_chain() {
        let error = serde_json::from_str::<DevelopStack>(
            r#"{"source_base":"not-a-base","params":{},"curves":{}}"#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("未知的 issue 源"), "{error}");
        // IPC / preset 可能直接带 null（Option 的形状），不能因此让整份 profile 读不出来。
        let stack: DevelopStack = serde_json::from_str(
            r#"{"source_base":"raw","lut_chain":null,"params":{},"curves":{}}"#,
        )
        .unwrap();
        assert!(stack.lut_chain.is_empty());
    }

    #[test]
    fn zero_strength_chain_is_not_active() {
        let mut stack = DevelopStack::default();
        stack.lut_enabled = Some(true);
        stack.lut_chain = vec![LutLayer {
            id: "film".into(),
            enabled: true,
            strength: 0.0,
        }];
        assert!(!stack.lut_chain_active(), "0% 强度等于没有 LUT，不应让导出报缺文件");
        stack.lut_chain[0].strength = 0.5;
        assert!(stack.lut_chain_active());
    }

    #[test]
    fn chain_validation_rejects_empty_duplicate_and_too_many_layers() {
        let mut stack = DevelopStack::default();
        stack.lut_chain = vec![LutLayer::new("a")];
        assert!(stack.validate().is_ok());
        // 重复
        stack.lut_chain = vec![LutLayer::new("a"), LutLayer::new("a")];
        assert!(stack.validate().is_err(), "同一支不能出现两次");
        // 超上限
        stack.lut_chain = (0..=MAX_LUT_LAYERS).map(|i| LutLayer::new(format!("l{i}"))).collect();
        assert!(stack.validate().is_err(), "最多 {MAX_LUT_LAYERS} 层");
        // 强度越界 / 非法 id
        let mut bad = DevelopStack::default();
        bad.lut_chain = vec![LutLayer { id: "a".into(), enabled: true, strength: 1.5 }];
        assert!(bad.validate().is_err());
        bad.lut_chain = vec![LutLayer { id: "a/b".into(), enabled: true, strength: 1.0 }];
        assert!(bad.validate().is_err());
        // 总开关开着但链是空的
        let mut empty = DevelopStack::default();
        empty.lut_enabled = Some(true);
        assert!(empty.validate().is_err());
    }

    #[test]
    fn signature_tracks_chain_order_and_strength() {
        let mut a = DevelopStack::default();
        a.lut_chain = vec![LutLayer::new("x"), LutLayer::new("y")];
        let mut b = a.clone();
        b.lut_chain.swap(0, 1);
        assert_ne!(a.signature(), b.signature(), "顺序必须改变指纹");
        let mut c = a.clone();
        c.lut_chain[1].strength = 0.5;
        assert_ne!(a.signature(), c.signature(), "强度必须改变指纹");
        let mut d = a.clone();
        d.lut_chain[1].enabled = false;
        assert_ne!(a.signature(), d.signature(), "单层开关必须改变指纹");
        // 单层 @100% 与旧单支的指纹一致（缓存不无谓失效）
        let mut single = DevelopStack::default();
        single.lut_chain = vec![LutLayer::new("film")];
        let mut legacy = DevelopStack::default();
        legacy.lut_chain = vec![LutLayer::new("film")];
        assert_eq!(single.signature(), legacy.signature());
    }

    #[test]
    fn chain_round_trips_through_the_catalog() {
        let (conn, asset_id) = catalog_with_asset();
        let mut stack = DevelopStack::default();
        stack.lut_chain = vec![
            LutLayer::new("film"),
            LutLayer { id: "warm".into(), enabled: false, strength: 0.4 },
        ];
        stack.lut_enabled = Some(true);
        save(&conn, asset_id, &stack, 1).unwrap();
        let loaded = load(&conn, asset_id).unwrap();
        assert_eq!(loaded.lut_chain, stack.lut_chain);
        assert_eq!(loaded.lut_enabled, Some(true));
    }

    #[test]
    fn clearing_the_chain_does_not_resurrect_the_legacy_lut_id() {
        let (conn, asset_id) = catalog_with_asset();
        // 模拟 v22 之前的库：只有旧的单支列，没有新链列。
        conn.execute(
            "INSERT INTO develop_stacks (asset_id, lut_id, lut_enabled, created_at, updated_at)
             VALUES (?1, 'legacy', 1, 1, 1)",
            [asset_id],
        )
        .unwrap();
        set_param(&conn, asset_id, "exposure", Some(0.5), 1).unwrap();
        let legacy = load(&conn, asset_id).unwrap();
        assert_eq!(legacy.lut_chain, vec![LutLayer::new("legacy")]);

        // 撤销/清空链：旧 id 必须一并清掉，否则下一次读库会复活它。
        set_setting(&conn, asset_id, Setting::LutChain, None, 2).unwrap();
        let cleared = load(&conn, asset_id).unwrap();
        assert!(cleared.lut_chain.is_empty());
        assert_eq!(cleared.params.get("exposure"), Some(&0.5));
        assert_eq!(
            conn.query_row(
                "SELECT lut_id FROM develop_stacks WHERE asset_id = ?1",
                [asset_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .unwrap(),
            None
        );

        // 覆盖式保存同样不能让旧列成为第二真相源。
        let mut again = load(&conn, asset_id).unwrap();
        again.lut_chain = vec![LutLayer::new("legacy")];
        save(&conn, asset_id, &again, 3).unwrap();
        let mut empty = load(&conn, asset_id).unwrap();
        empty.lut_chain.clear();
        empty.lut_enabled = Some(false);
        save(&conn, asset_id, &empty, 4).unwrap();
        assert!(load(&conn, asset_id).unwrap().lut_chain.is_empty());
    }

    #[test]
    fn legacy_lut_undo_values_are_normalized_into_the_chain() {
        let (conn, asset_id) = catalog_with_asset();
        // 旧撤销记录的 key = lutId、值 = 纯 id，不是 JSON。
        set_setting(&conn, asset_id, Setting::LutChain, Some("legacy"), 1).unwrap();
        assert_eq!(load(&conn, asset_id).unwrap().lut_chain, vec![LutLayer::new("legacy")]);
        assert!(Setting::LutChain.accepts(Some("legacy")));
        assert!(!Setting::LutChain.accepts(Some("a/b")));
        assert!(!Setting::LutChain.accepts(Some(r#"[{"id":"a/b"}]"#)));
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

    /// `develop_stacks.output_*` 读回（latest 的成片输出尺寸）。
    fn latest_output(conn: &Connection, asset_id: i64) -> (Option<i64>, Option<i64>) {
        conn.query_row(
            "SELECT output_width, output_height FROM develop_stacks WHERE asset_id = ?1",
            [asset_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("栈行在")
    }

    /// 给资产挂一个带展示尺寸的文件行。
    fn file_row(conn: &Connection, asset_id: i64, role: &str, width: i64, height: i64) {
        conn.execute(
            "INSERT INTO asset_files
                (asset_id, role, rel_path, rel_path_folded, ext, size_bytes, mtime_ms,
                 display_width, display_height, orientation, created_at, updated_at)
             VALUES (?1, ?2, ?2 || '.bin', ?2 || '.bin', 'bin', 1, 1, ?3, ?4, 1, 0, 0)",
            rusqlite::params![asset_id, role, width, height],
        )
        .expect("插文件行");
    }

    #[test]
    fn stack_changes_rewrite_the_latest_output_size_in_the_same_transaction() {
        // `specs/display-size-source.md` §3.3：栈变 → 尺寸跟着变；
        // 撤销走路（`set_setting`）也要跟着重算（它也是栈的写入口）。
        let (conn, asset_id) = catalog_with_asset();
        file_row(&conn, asset_id, "bitmap", 4000, 3000);
        file_row(&conn, asset_id, "raw", 5000, 4000);

        // 没几何：输出尺寸 = 源展示尺寸（默认基准 raw）
        let mut editing = stack(&[("exposure", 0.5)], &[]);
        save(&conn, asset_id, &editing, 1).expect("写");
        assert_eq!(latest_output(&conn, asset_id), (Some(5000), Some(4000)));

        // 裁一半：输出尺寸跟着栈一起变（同一条 UPDATE）
        editing.geometry = Some(EditGeometry {
            rotation: 0.0,
            crop: Some(CropRect {
                x: 0.0,
                y: 0.0,
                width: 0.5,
                height: 0.5,
            }),
            crop_ratio: None,
        });
        save(&conn, asset_id, &editing, 2).expect("写");
        assert_eq!(latest_output(&conn, asset_id), (Some(2500), Some(2000)));

        // 撤销那一半走路：清掉几何 → 尺寸回到源展示尺寸
        set_setting(&conn, asset_id, Setting::Geometry, None, 3).expect("清几何");
        assert_eq!(latest_output(&conn, asset_id), (Some(5000), Some(4000)));

        // 切换编辑基准：源换了一侧，尺寸跟着换
        set_setting(&conn, asset_id, Setting::SourceBase, Some("sooc"), 4).expect("切基准");
        assert_eq!(latest_output(&conn, asset_id), (Some(4000), Some(3000)));

        // 老库：源尺寸也缺 → 写 NULL（消费方按回退链现算，不报错）
        let (bare, bare_id) = catalog_with_asset();
        let mut bare_stack = stack(&[("exposure", 0.5)], &[]);
        bare_stack.geometry = Some(EditGeometry {
            rotation: 0.0,
            crop: Some(CropRect {
                x: 0.0,
                y: 0.0,
                width: 0.5,
                height: 0.5,
            }),
            crop_ratio: None,
        });
        save(&bare, bare_id, &bare_stack, 5).expect("写");
        assert_eq!(latest_output(&bare, bare_id), (None, None));
    }
}
