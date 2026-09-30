//! 标准层映射（`specs/xmp-w1.md` §5）：**只写能直映的项**，每个值都是近似。
//!
//! 口径（崔总 2026-09-30）：兼容层只针对**基于 raw 的 latest**；sooc 基 ⇒ 不写任何 `crs:`。
//! 明确不映射：`temperature`、`dynamicContrast`、镜头项、LUT、基准曲线、`nrMethod`、几何 ——
//! 理由见规格 §5.1（无对应 / 尺度不同 / 身份无法平移）。

use crate::store::develop::DevelopStack;

/// `crs:` 块：标量走属性，曲线走元素（Lightroom 的真实形状）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CrsBlock {
    /// `(属性名, 值)`，如 `("Exposure2012", "0.35")`。
    pub attrs: Vec<(&'static str, String)>,
    /// `(元素名, 控制点)`，如 `("ToneCurvePV2012", [(0,0), (255,255)])`。
    pub curves: Vec<(&'static str, Vec<(u32, u32)>)>,
    /// 有没有任何可映射的调整（没有就不写 `crs:` 块）。
    pub has_any: bool,
}

/// 我们的参数键 → `crs:` 属性（一一对应，见规格 §5.1 的表）。
const SCALARS: &[(&str, &str)] = &[
    ("exposure", "Exposure2012"),
    ("contrast", "Contrast2012"),
    ("highlights", "Highlights2012"),
    ("blacks", "Blacks2012"),
    ("saturation", "Saturation"),
    ("vibrance", "Vibrance"),
    ("lumaNr", "LuminanceSmoothing"),
    ("colorNr", "ColorNoiseReduction"),
    ("sharpenAmount", "Sharpness"),
];

/// 从 raw 基的 latest 推出 `crs:` 块。
///
/// `raw_file_name`：RAW 文件的文件名（`crs:RawFileName`；`None` = 没有独立 RAW 文件）。
#[must_use]
pub fn crs_block(latest: &DevelopStack, raw_file_name: Option<&str>) -> CrsBlock {
    let mut block = CrsBlock::default();
    for (key, name) in SCALARS {
        if let Some(value) = latest.params.get(*key) {
            block.attrs.push((name, format_f64(*value)));
            block.has_any = true;
        }
    }

    for (channel, element) in [
        ("rgb", "ToneCurvePV2012"),
        ("r", "ToneCurvePV2012Red"),
        ("g", "ToneCurvePV2012Green"),
        ("b", "ToneCurvePV2012Blue"),
    ] {
        if let Some(points) = latest.curves.get(channel)
            && let Some(normalized) = normalize_curve(points)
        {
            block.curves.push((element, normalized));
            block.has_any = true;
        }
    }

    if block.has_any {
        if let Some(name) = raw_file_name {
            block.attrs.push(("RawFileName", name.to_string()));
        }
        block.attrs.push(("HasSettings", "True".to_string()));
        block.attrs.push(("ProcessVersion", "11.0".to_string()));
    }
    block
}

/// 曲线 → Lightroom 可接受的形状：0–255 整数、x 严格递增、首末补端点、最多 32 点。
///
/// `None` = 恒等曲线（≤2 点且落在对角线上）—— 不写。
#[must_use]
pub fn normalize_curve(points: &[[f32; 2]]) -> Option<Vec<(u32, u32)>> {
    let clamp255 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    let mut normalized: Vec<(u32, u32)> = points
        .iter()
        .map(|[x, y]| (clamp255(*x), clamp255(*y)))
        .collect();
    normalized.sort_by_key(|(x, _)| *x);
    normalized.dedup_by_key(|(x, _)| *x);
    if normalized.len() <= 2 && normalized.iter().all(|(x, y)| x == y) {
        return None;
    }
    if normalized.first().map_or(true, |(x, _)| *x > 0) {
        normalized.insert(0, (0, 0));
    }
    if normalized.last().map_or(true, |(x, _)| *x < 255) {
        normalized.push((255, 255));
    }
    if normalized.len() > 32 {
        let mut sampled: Vec<(u32, u32)> = Vec::with_capacity(32);
        let last = normalized.len() - 1;
        for index in 0..32 {
            let at = index * last / 31;
            sampled.push(normalized[at]);
        }
        sampled.dedup_by_key(|(x, _)| *x);
        normalized = sampled;
    }
    Some(normalized)
}

/// 数值文本：整数不带小数点，小数最多四位、去尾零（避免科学计数法进 XML）。
#[must_use]
pub fn format_f64(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    if (value - value.trunc()).abs() < f64::EPSILON && value.abs() < 1.0e15 {
        format!("{}", value as i64)
    } else {
        let mut text = format!("{value:.4}");
        if text.contains('.') {
            while text.ends_with('0') {
                text.pop();
            }
            if text.ends_with('.') {
                text.pop();
            }
        }
        text
    }
}

/// 我们的色标（red/yellow/green/cyan/blue/purple）↔ `xmp:Label`。
///
/// 写出：首字母大写（Adobe 口径）；`Cyan` 没有 Adobe 对应值——照写，别家可能忽略（已知）。
#[must_use]
pub fn label_to_xmp(label: &str) -> String {
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// 读回：`"Red"` → `"red"`；认不出的值（Bridge 的 Select 之类）→ `None`（跳过）。
#[must_use]
pub fn label_from_xmp(text: &str) -> Option<String> {
    let folded = text.trim().to_ascii_lowercase();
    match folded.as_str() {
        "red" | "yellow" | "green" | "cyan" | "blue" | "purple" => Some(folded),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn stack(params: &[(&str, f64)], curves: &[(&str, Vec<[f32; 2]>)]) -> DevelopStack {
        let mut stack = DevelopStack::default();
        for (key, value) in params {
            stack.params.insert((*key).to_string(), *value);
        }
        for (channel, points) in curves {
            stack
                .curves
                .insert((*channel).to_string(), points.clone());
        }
        stack
    }

    #[test]
    fn scalars_map_one_to_one() {
        let block = crs_block(
            &stack(
                &[
                    ("exposure", 0.35),
                    ("contrast", 12.0),
                    ("highlights", -30.0),
                    ("blacks", 8.0),
                    ("saturation", -5.0),
                    ("vibrance", 18.0),
                    ("lumaNr", 25.0),
                    ("colorNr", 30.0),
                    ("sharpenAmount", 40.0),
                ],
                &[],
            ),
            Some("MYP0001.ORF"),
        );
        let get = |name: &str| {
            block
                .attrs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.clone())
                .unwrap_or_default()
        };
        assert_eq!(get("Exposure2012"), "0.35");
        assert_eq!(get("Contrast2012"), "12");
        assert_eq!(get("Highlights2012"), "-30");
        assert_eq!(get("Blacks2012"), "8");
        assert_eq!(get("Saturation"), "-5");
        assert_eq!(get("Vibrance"), "18");
        assert_eq!(get("LuminanceSmoothing"), "25");
        assert_eq!(get("ColorNoiseReduction"), "30");
        assert_eq!(get("Sharpness"), "40");
        assert_eq!(get("RawFileName"), "MYP0001.ORF");
        assert_eq!(get("HasSettings"), "True");
        assert_eq!(get("ProcessVersion"), "11.0");
        assert!(block.has_any);
    }

    #[test]
    fn unmapped_params_do_not_leak() {
        let block = crs_block(
            &stack(
                &[("temperature", 5500.0), ("dynamicContrast", 40.0), ("vignette", -20.0)],
                &[],
            ),
            None,
        );
        assert!(!block.has_any, "只有不映射的项 ⇒ 整块不写");
        assert!(block.attrs.is_empty());
    }

    #[test]
    fn curves_map_to_255_space_with_endpoints() {
        let block = crs_block(
            &stack(
                &[],
                &[("rgb", vec![[0.0, 0.0], [0.5, 0.6], [1.0, 1.0]])],
            ),
            None,
        );
        assert_eq!(
            block.curves,
            vec![("ToneCurvePV2012", vec![(0, 0), (128, 153), (255, 255)])]
        );
    }

    #[test]
    fn identity_curve_is_skipped() {
        assert_eq!(normalize_curve(&[[0.0, 0.0], [1.0, 1.0]]), None);
        assert_eq!(normalize_curve(&[[0.2, 0.2]]), None);
        let mut pts = Vec::new();
        for index in 0..40 {
            let t = f32::from(index) / 39.0;
            pts.push([t, t.powi(2)]);
        }
        let normalized = normalize_curve(&pts).unwrap();
        assert!(normalized.len() <= 32, "超 32 点要降采样");
        assert_eq!(normalized.first(), Some(&(0, 0)));
        assert_eq!(normalized.last(), Some(&(255, 255)));
    }

    #[test]
    fn curve_points_are_sorted_deduped_and_clamped() {
        let normalized = normalize_curve(&[[0.9, 1.2], [0.1, -0.2], [0.1, 0.05]]).unwrap();
        assert_eq!(normalized.first(), Some(&(0, 0)));
        assert!(normalized.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert!(normalized.iter().all(|(_, y)| *y <= 255));
    }

    #[test]
    fn numbers_format_without_scientific_notation() {
        assert_eq!(format_f64(0.0), "0");
        assert_eq!(format_f64(12.0), "12");
        assert_eq!(format_f64(-30.0), "-30");
        assert_eq!(format_f64(0.35), "0.35");
        assert_eq!(format_f64(1.0 / 3.0), "0.3333");
        assert_eq!(format_f64(f64::NAN), "0");
    }

    #[test]
    fn labels_round_trip_with_adobe_casing() {
        for ours in ["red", "yellow", "green", "cyan", "blue", "purple"] {
            let xmp = label_to_xmp(ours);
            assert_eq!(label_from_xmp(&xmp).as_deref(), Some(ours));
        }
        assert_eq!(label_to_xmp("red"), "Red");
        assert_eq!(label_from_xmp("Select"), None, "Bridge 的值不认");
    }
}
