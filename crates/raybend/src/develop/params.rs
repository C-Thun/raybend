//! **显影参数的数字契约** —— 与 `src/api/develop-params.json` 逐条对齐。
//!
//! # 为什么是「镜像 + 断言」而不是「运行时读 JSON」
//!
//! 参数的**范围 / 步长 / 默认值**是界面与管线共同依赖的口径，只允许有一处真相
//! （`AGENTS.md` §2.12）。真相放在 `src/api/develop-params.json`：
//!
//! * 前端 `src/features/editor/params.ts` 读它建界面；
//! * 本文件用 `include_str!` 把它嵌进二进制，单测**逐条比对**两张表。
//!
//! 于是改数会同时惊动两处，漏一处就红 —— 与 `dto-contract`（`src-tauri/src/contract.rs`）同一套做法。
//! 不在运行时解析 JSON 是因为：参数表是**常量**，运行时解析只会多一条可能失败的路径。
//!
//! # 参数的语义（管线只认这些 id）
//!
//! | id | 语义 | 范围 | 默认 |
//! | --- | --- | --- | --- |
//! | `exposure` | 曝光补偿（EV，线性域乘 `2^EV`） | −2..+2 | 0 |
//! | `contrast` | 反差（线性域绕中灰 0.18 的幂） | −100..+100 | 0 |
//! | `highlights` | 高光（线性域，作用在亮部） | −100..+100 | 0 |
//! | `blacks` | 黑区（线性域，作用在暗部） | −100..+100 | 0 |
//! | `temperature` | 色温（**绝对 K**，见 `super::color`） | 2500..10000 | 随照片 |
//! | `saturation` | 饱和度（线性域绕亮度缩放色度） | −100..+100 | 0 |
//! | `vibrance` | 自然饱和度（低饱和的加得多） | −100..+100 | 0 |
//!
//! `清晰度` 四条（`lumaNr` / `colorNr` / `sharpenAmount` / `sharpenRadius`）**已接进管线**（M3-W4）：
//! 亮度 / 色度降噪、锐化强度与半径。`镜头` 三条（`distortion` / `vignette` / `chromatic`）
//! 是**手动微调**，接进管线的那一部分与配置文件（lensfun）无关；
//! `wired = false` 的那几条由界面禁用并写明哪一波接。

use std::collections::BTreeMap;

/// 拉杆填充从哪长（`DESIGN.md` §14.10）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// 双极：填充从正中向把手长。
    Center,
    /// 单极：填充从左端长。
    Start,
}

/// 默认值从哪来。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Baseline {
    /// 固定值（表里的 `default`）。
    Static,
    /// **随照片变**（只有色温）：默认值是这张照片的 as-shot 色温，
    /// 读不到元数据时才退回 `default`（见 `super::color::as_shot_temperature`）。
    AsShot,
}

/// 一条参数的口径。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamSpec {
    pub id: &'static str,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub default: f64,
    pub origin: Origin,
    /// 本波（M3-W3）接进管线了吗；`false` = M3-W4 接。
    pub wired: bool,
    pub baseline: Baseline,
}

impl ParamSpec {
    /// 把值夹进合法范围（**非法值不静默通过** —— 调用方先看 [`Self::accepts`]）。
    #[must_use]
    pub fn clamp(&self, value: f64) -> f64 {
        value.clamp(self.min, self.max)
    }

    /// 这个值能接受吗（有限、在范围内）。
    ///
    /// 不接受时由调用方决定是拒绝还是夹取 —— 渲染线程那侧选择**拒绝并报错**，
    /// 免得一个 NaN 悄悄把整张图变成黑的。
    #[must_use]
    pub fn accepts(&self, value: f64) -> bool {
        value.is_finite() && value >= self.min && value <= self.max
    }
}

/// 参数表（顺序即界面顺序；与 `develop-params.json` 的 `params` 数组同序）。
pub const PARAMS: &[ParamSpec] = &[
    // ── 影调 ──
    ParamSpec { id: "exposure", min: -2.0, max: 2.0, step: 0.05, default: 0.0, origin: Origin::Center, wired: true, baseline: Baseline::Static },
    ParamSpec { id: "contrast", min: -100.0, max: 100.0, step: 1.0, default: 0.0, origin: Origin::Center, wired: true, baseline: Baseline::Static },
    ParamSpec { id: "highlights", min: -100.0, max: 100.0, step: 1.0, default: 0.0, origin: Origin::Center, wired: true, baseline: Baseline::Static },
    ParamSpec { id: "blacks", min: -100.0, max: 100.0, step: 1.0, default: 0.0, origin: Origin::Center, wired: true, baseline: Baseline::Static },
    // 动态反差（局部色调映射）：单极 0..100，0 = 完全不动画面（`local_tone` 的强度）
    ParamSpec { id: "dynamicContrast", min: 0.0, max: 100.0, step: 1.0, default: 0.0, origin: Origin::Start, wired: true, baseline: Baseline::Static },
    // ── 色彩 ──
    ParamSpec { id: "temperature", min: 2500.0, max: 10000.0, step: 50.0, default: 6250.0, origin: Origin::Center, wired: true, baseline: Baseline::AsShot },
    ParamSpec { id: "saturation", min: -100.0, max: 100.0, step: 1.0, default: 0.0, origin: Origin::Center, wired: true, baseline: Baseline::Static },
    ParamSpec { id: "vibrance", min: -100.0, max: 100.0, step: 1.0, default: 0.0, origin: Origin::Center, wired: true, baseline: Baseline::Static },
    // ── 清晰度（M3-W4 接入）──
    ParamSpec { id: "lumaNr", min: 0.0, max: 100.0, step: 1.0, default: 0.0, origin: Origin::Start, wired: true, baseline: Baseline::Static },
    ParamSpec { id: "colorNr", min: 0.0, max: 100.0, step: 1.0, default: 0.0, origin: Origin::Start, wired: true, baseline: Baseline::Static },
    ParamSpec { id: "sharpenAmount", min: 0.0, max: 100.0, step: 1.0, default: 0.0, origin: Origin::Start, wired: true, baseline: Baseline::Static },
    ParamSpec { id: "sharpenRadius", min: 0.0, max: 100.0, step: 1.0, default: 0.0, origin: Origin::Start, wired: true, baseline: Baseline::Static },
    // ── 镜头（W4）──
    ParamSpec { id: "distortion", min: -100.0, max: 100.0, step: 1.0, default: 0.0, origin: Origin::Center, wired: false, baseline: Baseline::Static },
    ParamSpec { id: "vignette", min: -100.0, max: 100.0, step: 1.0, default: 0.0, origin: Origin::Center, wired: false, baseline: Baseline::Static },
    ParamSpec { id: "chromatic", min: -100.0, max: 100.0, step: 1.0, default: 0.0, origin: Origin::Center, wired: false, baseline: Baseline::Static },
];

/// 前端那份契约文件（**唯一真相**，本文件与它逐条对齐）。
pub const CONTRACT_JSON: &str = include_str!("../../../../src/api/develop-params.json");

/// 按 id 取参数口径。
#[must_use]
pub fn spec(id: &str) -> Option<&'static ParamSpec> {
    PARAMS.iter().find(|param| param.id == id)
}

/// 参数 id 在表里吗（IPC 载荷校验用）。
#[must_use]
pub fn is_known(id: &str) -> bool {
    spec(id).is_some()
}

/// 这一条现在接进管线了吗（`wired = false` 的由管线忽略）。
#[must_use]
pub fn is_wired(id: &str) -> bool {
    spec(id).is_some_and(|param| param.wired)
}

/// 一组显影参数：**只装与基线不同的项**（`AGENTS.md` §11.5「只存非默认值」）。
///
/// 这么设计的好处：一条 `develop_params` 表里没有的行 = 这一项没动过，
/// 「重置这一项」「重置全部」「这项动没动」三个问题共用同一套判断。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DevelopParams {
    values: BTreeMap<String, f64>,
    /// 这张照片的 as-shot 色温（K）；`None` = 元数据里读不到，色温基线退回表里的 `default`。
    as_shot_temperature: Option<f32>,
}

impl DevelopParams {
    /// 空参数（全默认）+ 一个 as-shot 色温。
    #[must_use]
    pub fn new(as_shot_temperature: Option<f32>) -> Self {
        Self {
            values: BTreeMap::new(),
            as_shot_temperature,
        }
    }

    /// 从「id → 值」建（**逐条校验**：未知 id 或非法值直接报错，不静默丢）。
    ///
    /// # Errors
    /// 未知参数 id / 值非有限 / 值超范围。
    pub fn from_values(
        values: impl IntoIterator<Item = (String, f64)>,
        as_shot_temperature: Option<f32>,
    ) -> Result<Self, String> {
        let mut out = Self::new(as_shot_temperature);
        for (id, value) in values {
            out.set(&id, value)?;
        }
        Ok(out)
    }

    /// 设一个参数（未知 id / 非法值 → `Err`）。
    ///
    /// 值等于基线时**删掉这一项**（回到「没动过」）—— 这样「拖回原位」与
    /// 「重置这一项」在数据上是同一件事。
    ///
    /// # Errors
    /// 见上。
    pub fn set(&mut self, id: &str, value: f64) -> Result<(), String> {
        let Some(spec) = spec(id) else {
            return Err(format!("未知的显影参数：{id}"));
        };
        if !spec.accepts(value) {
            return Err(format!(
                "参数 {id} 的值非法：{value}（允许 {}..{}）",
                spec.min, spec.max
            ));
        }
        if (value - self.baseline(id)).abs() < f64::EPSILON {
            self.values.remove(id);
        } else {
            self.values.insert(id.to_string(), value);
        }
        Ok(())
    }

    /// 删掉一项（= 回到基线）。
    pub fn clear(&mut self, id: &str) {
        self.values.remove(id);
    }

    /// 清空全部（= 重置全部）。
    pub fn clear_all(&mut self) {
        self.values.clear();
    }

    /// 这一项的**基线值**（没动过时应当显示的值）。
    ///
    /// 色温特殊：基线是这张照片的 as-shot 色温（读不到才用表里的 6250）。
    #[must_use]
    pub fn baseline(&self, id: &str) -> f64 {
        let Some(spec) = spec(id) else {
            return 0.0;
        };
        match spec.baseline {
            Baseline::Static => spec.default,
            Baseline::AsShot => self
                .as_shot_temperature
                .map_or(spec.default, |k| f64::from(k).clamp(spec.min, spec.max)),
        }
    }

    /// 这一项的当前值（没动过 → 基线）。
    #[must_use]
    pub fn value(&self, id: &str) -> f64 {
        self.values
            .get(id)
            .copied()
            .unwrap_or_else(|| self.baseline(id))
    }

    /// 这一项动过吗（与基线不同）。
    #[must_use]
    pub fn is_dirty(&self, id: &str) -> bool {
        self.values.contains_key(id)
    }

    /// 动过的项（id → 值），**有序**（BTreeMap 的键序 = 字典序，测试与日志稳定）。
    #[must_use]
    pub fn dirty(&self) -> &BTreeMap<String, f64> {
        &self.values
    }

    /// 动过的项数。
    #[must_use]
    pub fn dirty_count(&self) -> usize {
        self.values.len()
    }

    /// 这张照片的 as-shot 色温。
    #[must_use]
    pub fn as_shot_temperature(&self) -> Option<f32> {
        self.as_shot_temperature
    }

    /// 换一张照片时更新基线（参数值本身**不动** —— 由调用方决定是重载还是保留草稿）。
    pub fn set_as_shot_temperature(&mut self, kelvin: Option<f32>) {
        self.as_shot_temperature = kelvin;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// JSON 里的一条（只取比对的字段）。
    #[derive(serde::Deserialize)]
    struct Contract {
        version: u32,
        params: Vec<ContractParam>,
    }

    #[derive(serde::Deserialize)]
    struct ContractParam {
        id: String,
        min: f64,
        max: f64,
        step: f64,
        default: f64,
        origin: String,
        wired: bool,
        baseline: String,
    }

    fn contract() -> Contract {
        serde_json::from_str(CONTRACT_JSON).expect("develop-params.json 必须能解析")
    }

    /// **这张表就是断言**：Rust 侧与前端那份 JSON 必须逐条一致（改一处不改另一处就红）。
    #[test]
    fn params_match_the_frontend_contract() {
        let contract = contract();
        assert_eq!(contract.version, 1, "契约版本变了要一起改这里的断言");
        assert_eq!(
            contract.params.len(),
            PARAMS.len(),
            "参数条数不一致（Rust {} 条 / JSON {} 条）",
            PARAMS.len(),
            contract.params.len()
        );
        for (spec, json) in PARAMS.iter().zip(&contract.params) {
            assert_eq!(spec.id, json.id, "第 {} 条的 id 不一致", json.id);
            assert!(
                (spec.min - json.min).abs() < f64::EPSILON,
                "{} 的 min：Rust {} / JSON {}",
                spec.id,
                spec.min,
                json.min
            );
            assert!(
                (spec.max - json.max).abs() < f64::EPSILON,
                "{} 的 max：Rust {} / JSON {}",
                spec.id,
                spec.max,
                json.max
            );
            assert!(
                (spec.step - json.step).abs() < f64::EPSILON,
                "{} 的 step：Rust {} / JSON {}",
                spec.id,
                spec.step,
                json.step
            );
            assert!(
                (spec.default - json.default).abs() < f64::EPSILON,
                "{} 的 default：Rust {} / JSON {}",
                spec.id,
                spec.default,
                json.default
            );
            let origin = match spec.origin {
                Origin::Center => "center",
                Origin::Start => "start",
            };
            assert_eq!(origin, json.origin, "{} 的 origin 不一致", spec.id);
            assert_eq!(spec.wired, json.wired, "{} 的 wired 不一致", spec.id);
            let baseline = match spec.baseline {
                Baseline::Static => "static",
                Baseline::AsShot => "as-shot",
            };
            assert_eq!(baseline, json.baseline, "{} 的 baseline 不一致", spec.id);
        }
    }

    #[test]
    fn spec_lookup_and_wired_flags() {
        assert!(spec("exposure").is_some());
        assert!(spec("no-such-param").is_none());
        assert!(is_wired("exposure"));
        assert!(is_wired("temperature"));
        assert!(is_wired("sharpenAmount"), "清晰度已在 M3-W4 接入");
        assert!(is_wired("lumaNr"));
        assert!(!is_wired("vignette"), "镜头手动微调接的是管线那一步（参数本身可用）");
        assert!(!is_known("curve"), "曲线不是参数（它有自己的模型）");
    }

    #[test]
    fn only_non_default_values_are_stored() {
        let mut params = DevelopParams::new(Some(5200.0));
        assert_eq!(params.dirty_count(), 0);
        assert_eq!(params.value("exposure"), 0.0);
        assert_eq!(params.value("temperature"), 5200.0, "色温基线随照片");

        params.set("exposure", 0.5).unwrap();
        assert_eq!(params.dirty_count(), 1);
        assert!(params.is_dirty("exposure"));
        assert_eq!(params.value("exposure"), 0.5);

        // 拖回基线 = 删掉这一项（不是留一个 0 在那里）
        params.set("exposure", 0.0).unwrap();
        assert_eq!(params.dirty_count(), 0);
        assert!(!params.is_dirty("exposure"));

        // 色温拖回 as-shot 也是删
        params.set("temperature", 5200.0).unwrap();
        assert_eq!(params.dirty_count(), 0);
        params.set("temperature", 7000.0).unwrap();
        assert_eq!(params.dirty_count(), 1);
    }

    #[test]
    fn temperature_baseline_falls_back_to_the_table_default() {
        let params = DevelopParams::new(None);
        assert_eq!(params.value("temperature"), 6250.0);
        // 元数据给的色温超范围时也要夹进拉杆范围（否则把手会画到轨道外面）
        let hot = DevelopParams::new(Some(20000.0));
        assert_eq!(hot.value("temperature"), 10000.0);
        let cold = DevelopParams::new(Some(1000.0));
        assert_eq!(cold.value("temperature"), 2500.0);
    }

    #[test]
    fn illegal_values_are_rejected_not_clamped() {
        let mut params = DevelopParams::new(None);
        assert!(params.set("exposure", 3.0).is_err(), "超出范围要报错");
        assert!(params.set("exposure", f64::NAN).is_err(), "NaN 要报错");
        assert!(params.set("exposure", f64::INFINITY).is_err());
        assert!(params.set("nope", 1.0).is_err(), "未知 id 要报错");
        assert_eq!(params.dirty_count(), 0, "被拒的值不许留下痕迹");
    }

    #[test]
    fn from_values_validates_every_entry() {
        let ok = DevelopParams::from_values(
            [("exposure".to_string(), 1.0), ("blacks".to_string(), -30.0)],
            None,
        )
        .expect("合法值");
        assert_eq!(ok.dirty_count(), 2);
        assert!(DevelopParams::from_values([("exposure".to_string(), 99.0)], None).is_err());
    }

    #[test]
    fn clear_and_clear_all_go_back_to_baseline() {
        let mut params = DevelopParams::new(Some(5000.0));
        params.set("exposure", 1.0).unwrap();
        params.set("blacks", -20.0).unwrap();
        params.set("temperature", 8000.0).unwrap();
        assert_eq!(params.dirty_count(), 3);

        params.clear("exposure");
        assert_eq!(params.value("exposure"), 0.0);
        assert_eq!(params.dirty_count(), 2);

        params.clear_all();
        assert_eq!(params.dirty_count(), 0);
        assert_eq!(params.value("temperature"), 5000.0, "重置全部后色温回到 as-shot");
    }
}
