//! XMP 数据包的**构建与解析**（`specs/xmp-w1.md` §4–§5）。
//!
//! * 写：手写序列化（结构固定、转义集中一处），带 `xpacket` 包装。
//! * 读：`roxmltree`（既有依赖，不新引 XML 库）。
//! * 转义与 dc: 列表构造是**共享**的 —— 导出内嵌 XMP（`export::metadata`）用同一套。
//!
//! `rb:profileJson` = `DevelopStack` 的规范 JSON（**剔除 `auto_adjust`**），与
//! `store::issues::profile_hash` 同一口径 —— 任何读者都能直接重算哈希校验。

use crate::store::develop::{DevelopStack, EditBase};
use crate::store::issues::{PROFILE_SCHEMA_VERSION, profile_hash};

/// 自有命名空间（`specs/issue-xmp-contract.md`）。
pub const RB_NS: &str = "https://raybend.app/ns/issue/1.0/";

/// —— 模型（写出与读回共用）——

/// 一份不可变定稿（sidecar 形态；不含库内自增 ID）。
#[derive(Debug, Clone, PartialEq)]
pub struct SidecarProfile {
    pub name: String,
    pub source_base: EditBase,
    pub created_at_ms: i64,
    /// 导出尾号（0–99）；提示性，读回时优先沿用。
    pub ordinal: Option<i64>,
    pub stack: DevelopStack,
}

/// 标准层承载的基础元数据（DB 里有值才填）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SidecarMetadata {
    pub rating: u8,
    pub color_label: Option<String>,
    pub keywords: Vec<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub country: Option<String>,
    pub province_state: Option<String>,
    pub city: Option<String>,
    pub sublocation: Option<String>,
}

impl SidecarMetadata {
    /// 一个字段都没有吗（rating 0 = 没评）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rating == 0
            && self.color_label.is_none()
            && self.keywords.is_empty()
            && self.author.is_none()
            && self.description.is_none()
            && self.country.is_none()
            && self.province_state.is_none()
            && self.city.is_none()
            && self.sublocation.is_none()
    }
}

/// 一张照片的 sidecar 全部内容。
#[derive(Debug, Clone, Default)]
pub struct SidecarContent {
    /// latest 工作副本；`None` 或空栈 = 没有编辑。
    pub latest: Option<DevelopStack>,
    pub profiles: Vec<SidecarProfile>,
    pub metadata: SidecarMetadata,
    /// RAW 文件的文件名（`crs:RawFileName` 用；`None` = 无独立 RAW）。
    pub raw_file_name: Option<String>,
}

impl SidecarContent {
    /// 「编辑了才有」的判据：有可表达内容才写文件（规格 §6.1）。
    #[must_use]
    pub fn has_content(&self) -> bool {
        self.latest.as_ref().is_some_and(|stack| !stack.is_empty())
            || !self.profiles.is_empty()
            || !self.metadata.is_empty()
    }
}

/// 读回的结果（宽容解析：坏条目跳过并记 warning，不拖垮整份文件）。
#[derive(Debug, Clone, Default)]
pub struct ParsedSidecar {
    pub latest: Option<DevelopStack>,
    pub profiles: Vec<SidecarProfile>,
    pub metadata: SidecarMetadata,
    /// 文件里出现过的最高 `schemaVersion`（含读不懂的条目）—— 版本闸门用。
    pub max_schema_version: i64,
    pub warnings: Vec<String>,
}

/// —— 共享的 XML 基础件（导出内嵌 XMP 与 sidecar 同一套）——

/// 文本与属性值共用的转义；控制字符（XML 1.0 不允许）直接丢弃。
#[must_use]
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ if ch.is_control() => {}
            _ => out.push(ch),
        }
    }
    out
}

/// `rdf:Bag` / `rdf:Seq` 列表（`dc:subject` / `dc:creator` 用）。
#[must_use]
pub fn rdf_list(values: &[String], kind: &str) -> String {
    format!(
        "<rdf:{kind}>{}</rdf:{kind}>",
        values
            .iter()
            .map(|value| format!("<rdf:li>{}</rdf:li>", escape(value)))
            .collect::<String>()
    )
}

/// `rdf:Alt` + `x-default`（`dc:description` 用）。
#[must_use]
pub fn rdf_alt(value: &str) -> String {
    format!(
        "<rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt>",
        escape(value)
    )
}

/// —— 构建 ——

/// 组装整份 sidecar XML。`None` = 没有可表达内容（调用方据此删文件）。
///
/// `tool`：`xmp:CreatorTool`（如 `RayBend 0.1.1`）；`now_ms`：`xmp:MetadataDate`（UTC）。
#[must_use]
pub fn compose(content: &SidecarContent, now_ms: i64, tool: &str) -> Option<String> {
    if !content.has_content() {
        return None;
    }

    let mut attrs = String::new();
    let mut elements = String::new();

    attrs.push_str(&format!(
        " xmp:CreatorTool=\"{}\" xmp:MetadataDate=\"{}\"",
        escape(tool),
        iso8601_utc(now_ms)
    ));

    let meta = &content.metadata;
    if meta.rating > 0 {
        attrs.push_str(&format!(" xmp:Rating=\"{}\"", meta.rating));
    }
    if let Some(label) = &meta.color_label {
        attrs.push_str(&format!(
            " xmp:Label=\"{}\"",
            escape(&crate::xmp::mapping::label_to_xmp(label))
        ));
    }
    if let Some(country) = &meta.country {
        attrs.push_str(&format!(" photoshop:Country=\"{}\"", escape(country)));
    }
    if let Some(state) = &meta.province_state {
        attrs.push_str(&format!(" photoshop:State=\"{}\"", escape(state)));
    }
    if let Some(city) = &meta.city {
        attrs.push_str(&format!(" photoshop:City=\"{}\"", escape(city)));
    }
    if let Some(location) = &meta.sublocation {
        attrs.push_str(&format!(
            " Iptc4xmpCore:Location=\"{}\"",
            escape(location)
        ));
    }
    if !meta.keywords.is_empty() {
        elements.push_str(&format!(
            "<dc:subject>{}</dc:subject>",
            rdf_list(&meta.keywords, "Bag")
        ));
    }
    if let Some(author) = &meta.author {
        elements.push_str(&format!(
            "<dc:creator>{}</dc:creator>",
            rdf_list(&[author.clone()], "Seq")
        ));
    }
    if let Some(description) = &meta.description {
        elements.push_str(&format!("<dc:description>{}</dc:description>", rdf_alt(description)));
    }

    // 兼容层：仅 raw 基的 latest（崔总 2026-09-30）
    if let Some(latest) = content
        .latest
        .as_ref()
        .filter(|stack| stack.source_base == EditBase::Raw && !stack.is_empty())
    {
        let block = crate::xmp::mapping::crs_block(latest, content.raw_file_name.as_deref());
        if block.has_any {
            for (name, value) in &block.attrs {
                attrs.push_str(&format!(" crs:{name}=\"{}\"", escape(value)));
            }
            for (name, points) in &block.curves {
                let items = points
                    .iter()
                    .map(|(x, y)| format!("<rdf:li>{x}, {y}</rdf:li>"))
                    .collect::<String>();
                elements.push_str(&format!(
                    "<crs:{name}><rdf:Seq>{items}</rdf:Seq></crs:{name}>"
                ));
            }
        }
    }

    if let Some(latest) = content
        .latest
        .as_ref()
        .filter(|stack| !stack.is_empty())
        .and_then(canonical_payload)
    {
        elements.push_str(&format!(
            "<rb:latest rdf:parseType=\"Resource\">\
                 <rb:latestSchemaVersion>{PROFILE_SCHEMA_VERSION}</rb:latestSchemaVersion>\
                 <rb:sourceBase>{}</rb:sourceBase>\
                 <rb:profileHash>{}</rb:profileHash>\
                 <rb:profileJson>{}</rb:profileJson>\
             </rb:latest>",
            latest.source_base.as_str(),
            latest.hash,
            escape(&latest.json)
        ));
    }

    let mut items = String::new();
    for profile in &content.profiles {
        let Some(stack) = canonical_payload(&profile.stack) else {
            continue;
        };
        items.push_str(&format!(
            "<rdf:li rdf:parseType=\"Resource\">\
                     <rb:name>{}</rb:name>\
                     <rb:sourceBase>{}</rb:sourceBase>\
                     <rb:createdAtMs>{}</rb:createdAtMs>\
                     <rb:ordinal>{}</rb:ordinal>\
                     <rb:schemaVersion>{PROFILE_SCHEMA_VERSION}</rb:schemaVersion>\
                     <rb:profileHash>{}</rb:profileHash>\
                     <rb:profileJson>{}</rb:profileJson>\
                 </rdf:li>",
            escape(&profile.name),
            profile.source_base.as_str(),
            profile.created_at_ms,
            profile.ordinal.unwrap_or(-1),
            stack.hash,
            escape(&stack.json)
        ));
    }
    // 空的也要写：`rb:profiles` 元素本身就是「这是 raybend 的文件」的标记，
    // 否则只有元数据的 sidecar 会被误判成外来文件（写出侧会反复备份接管）。
    elements.push_str(&format!("<rb:profiles><rdf:Seq>{items}</rdf:Seq></rb:profiles>"));

    let mut xml = String::with_capacity(1024 + elements.len());
    xml.push_str("<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n");
    xml.push_str(&format!(
        "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"{}\">\n",
        escape(tool)
    ));
    xml.push_str("  <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n");
    xml.push_str(&format!(
        "    <rdf:Description rdf:about=\"\"\
             xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"\
             xmlns:crs=\"http://ns.adobe.com/camera-raw-settings/1.0/\"\
             xmlns:dc=\"http://purl.org/dc/elements/1.1/\"\
             xmlns:photoshop=\"http://ns.adobe.com/photoshop/1.0/\"\
             xmlns:Iptc4xmpCore=\"http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/\"\
             xmlns:rb=\"{RB_NS}\"{attrs}>\n"
    ));
    xml.push_str(&elements);
    xml.push_str("    </rdf:Description>\n  </rdf:RDF>\n</x:xmpmeta>\n");
    xml.push_str("<?xpacket end=\"w\"?>");
    Some(xml)
}

/// `DevelopStack` 的规范载荷（剔除 `auto_adjust` 后的 JSON 与指纹）；序列化失败给 `None`。
#[must_use]
fn canonical_payload(stack: &DevelopStack) -> Option<CanonicalStack> {
    let mut canonical = stack.clone();
    canonical.auto_adjust = None;
    let json = serde_json::to_string(&canonical).ok()?;
    let hash = profile_hash(&canonical).ok()?;
    Some(CanonicalStack {
        source_base: canonical.source_base,
        json,
        hash,
    })
}

struct CanonicalStack {
    source_base: crate::store::develop::EditBase,
    json: String,
    hash: String,
}

/// Unix 毫秒 → `2026-09-30T12:34:56Z`（UTC；项目口径：时间戳一律 UTC）。
#[must_use]
pub fn iso8601_utc(millis: i64) -> String {
    let (y, mo, d, h, mi, s) = crate::store::time::civil(millis);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// —— 解析 ——

/// 解析一份 sidecar 文本。
///
/// * `Err`：XML 语法坏（调用方决定按「外来/损坏」处理）。
/// * `Ok(None)`：格式合法但**没有 `rb:` 域**（别家软件的文件）。
/// * `Ok(Some)`：我们的文件（条目已校验；问题进 `warnings`）。
pub fn parse(text: &str) -> Result<Option<ParsedSidecar>, String> {
    let trimmed = text.trim_end_matches('\0');
    let doc = roxmltree::Document::parse(trimmed).map_err(|error| error.to_string())?;
    let Some(description) = doc
        .descendants()
        .find(|node| node.tag_name().namespace() == Some(RDF_NS) && node.tag_name().name() == "Description")
    else {
        return Ok(None);
    };
    let mut parsed = ParsedSidecar::default();
    let mut saw_rb = false;

    for node in description.children().filter(|node| node.is_element()) {
        let namespace = node.tag_name().namespace();
        let name = node.tag_name().name();
        match (namespace, name) {
            (Some(RB_NS), "latest") => {
                saw_rb = true;
                parse_rb_resource(&node, true, &mut parsed);
            }
            (Some(RB_NS), "profiles") => {
                saw_rb = true;
                for item in node.children().filter(|node| node.is_element()) {
                    parse_rb_resource(&item, false, &mut parsed);
                }
            }
            _ => parse_standard_element(&node, &mut parsed.metadata),
        }
    }
    // 属性形态的元数据（我们写出的就是属性；也认元素形态，见 parse_standard_element）
    for (namespace, name, value) in description.attributes().filter_map(|attr| {
        Some((attr.namespace()?, attr.name(), attr.value()))
    }) {
        apply_standard_attr(namespace, name, value, &mut parsed.metadata);
    }

    if !saw_rb {
        return Ok(None);
    }
    Ok(Some(parsed))
}

const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

/// 解析一个 `rb:latest` 资源或 `rb:profiles` 的一个 `rdf:li`。
fn parse_rb_resource(node: &roxmltree::Node<'_, '_>, is_latest: bool, parsed: &mut ParsedSidecar) {
    let text = |tag: &str| {
        node.children()
            .find(|child| child.is_element() && child.tag_name().namespace() == Some(RB_NS) && child.tag_name().name() == tag)
            .and_then(|child| child.text())
            .map(str::trim)
    };
    let version = text(if is_latest { "latestSchemaVersion" } else { "schemaVersion" })
        .and_then(|value| value.parse::<i64>().ok());
    let version = match version {
        Some(version) if (1..=PROFILE_SCHEMA_VERSION).contains(&version) => version,
        other => {
            parsed.max_schema_version = parsed
                .max_schema_version
                .max(other.unwrap_or(i64::MAX / 2));
            parsed.warnings.push(format!(
                "跳过一个 schemaVersion={} 的{}（本版本最高认 {PROFILE_SCHEMA_VERSION}）",
                other.map_or("?".to_string(), |v| v.to_string()),
                if is_latest { "latest" } else { "定稿" }
            ));
            return;
        }
    };
    parsed.max_schema_version = parsed.max_schema_version.max(version);

    let Some(json) = text("profileJson") else {
        parsed.warnings.push("跳过一个缺 profileJson 的条目".to_string());
        return;
    };
    let mut stack: DevelopStack = match serde_json::from_str(json) {
        Ok(stack) => stack,
        Err(error) => {
            parsed
                .warnings
                .push(format!("跳过一个 profileJson 解不开的条目：{error}"));
            return;
        }
    };
    stack.auto_adjust = None;
    let recomputed = profile_hash(&stack).ok().unwrap_or_default();
    match text("profileHash") {
        Some(declared) if declared.eq_ignore_ascii_case(&recomputed) => {}
        _ => {
            parsed
                .warnings
                .push("跳过一个哈希不符的条目（文件内容与指纹对不上）".to_string());
            return;
        }
    }
    let source_base = match text("sourceBase").and_then(EditBase::parse) {
        Some(base) => base,
        None => {
            parsed.warnings.push("跳过一个编辑基准不明的条目".to_string());
            return;
        }
    };

    if is_latest {
        parsed.latest = Some(stack);
        return;
    }
    let name = text("name").unwrap_or_default().to_string();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        parsed.warnings.push("跳过一个名称无效的定稿".to_string());
        return;
    }
    parsed.profiles.push(SidecarProfile {
        name,
        source_base,
        created_at_ms: text("createdAtMs").and_then(|v| v.parse().ok()).unwrap_or(0),
        ordinal: text("ordinal").and_then(|v| v.parse::<i64>().ok()).filter(|v| (0..100).contains(v)),
        stack,
    });
}

/// 元数据：元素形态（`<dc:subject>`、`<dc:creator>`、`<dc:description>`）。
fn parse_standard_element(node: &roxmltree::Node<'_, '_>, meta: &mut SidecarMetadata) {
    let namespace = node.tag_name().namespace();
    let name = node.tag_name().name();
    let list_values = |node: &roxmltree::Node<'_, '_>| {
        node.descendants()
            .filter(|desc| desc.is_element() && desc.tag_name().namespace() == Some(RDF_NS) && desc.tag_name().name() == "li")
            .filter_map(|li| li.text())
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    match (namespace, name) {
        (Some("http://purl.org/dc/elements/1.1/"), "subject") => {
            meta.keywords.extend(list_values(node));
        }
        (Some("http://purl.org/dc/elements/1.1/"), "creator") => {
            if let Some(first) = list_values(node).into_iter().next() {
                meta.author.get_or_insert(first);
            }
        }
        (Some("http://purl.org/dc/elements/1.1/"), "description") => {
            let preferred = node
                .descendants()
                .filter(|desc| desc.is_element() && desc.tag_name().namespace() == Some(RDF_NS) && desc.tag_name().name() == "li")
                .find(|li| li.attribute("http://www.w3.org/XML/1998/namespace", "lang") == Some("x-default"))
                .or_else(|| {
                    node.descendants().find(|desc| {
                        desc.is_element()
                            && desc.tag_name().namespace() == Some(RDF_NS)
                            && desc.tag_name().name() == "li"
                    })
                })
                .and_then(|li| li.text());
            if let Some(text) = preferred {
                meta.description.get_or_insert(text.to_string());
            }
        }
        (Some("http://ns.adobe.com/photoshop/1.0/"), "Country") => {
            if let Some(text) = node.text() {
                meta.country.get_or_insert(text.trim().to_string());
            }
        }
        (Some("http://ns.adobe.com/photoshop/1.0/"), "State") => {
            if let Some(text) = node.text() {
                meta.province_state.get_or_insert(text.trim().to_string());
            }
        }
        (Some("http://ns.adobe.com/photoshop/1.0/"), "City") => {
            if let Some(text) = node.text() {
                meta.city.get_or_insert(text.trim().to_string());
            }
        }
        (Some("http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/"), "Location") => {
            if let Some(text) = node.text() {
                meta.sublocation.get_or_insert(text.trim().to_string());
            }
        }
        (Some("http://ns.adobe.com/xap/1.0/"), "Rating") => {
            if let Some(value) = node.text().and_then(|t| t.trim().parse::<u8>().ok()) {
                meta.rating = meta.rating.max(value.min(5));
            }
        }
        (Some("http://ns.adobe.com/xap/1.0/"), "Label") => {
            if let Some(label) = node.text().and_then(crate::xmp::mapping::label_from_xmp) {
                meta.color_label.get_or_insert(label);
            }
        }
        _ => {}
    }
}

/// 元数据：属性形态（我们自己的写法）。
fn apply_standard_attr(namespace: &str, name: &str, value: &str, meta: &mut SidecarMetadata) {
    match (namespace, name) {
        ("http://ns.adobe.com/xap/1.0/", "Rating") => {
            if let Ok(rating) = value.trim().parse::<u8>() {
                meta.rating = meta.rating.max(rating.min(5));
            }
        }
        ("http://ns.adobe.com/xap/1.0/", "Label") => {
            if let Some(label) = crate::xmp::mapping::label_from_xmp(value) {
                meta.color_label.get_or_insert(label);
            }
        }
        ("http://ns.adobe.com/photoshop/1.0/", "Country") => {
            meta.country.get_or_insert(value.to_string());
        }
        ("http://ns.adobe.com/photoshop/1.0/", "State") => {
            meta.province_state.get_or_insert(value.to_string());
        }
        ("http://ns.adobe.com/photoshop/1.0/", "City") => {
            meta.city.get_or_insert(value.to_string());
        }
        ("http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/", "Location") => {
            meta.sublocation.get_or_insert(value.to_string());
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(base: EditBase, params: &[(&str, f64)]) -> DevelopStack {
        let mut stack = DevelopStack::default();
        stack.source_base = base;
        for (key, value) in params {
            stack.params.insert((*key).to_string(), *value);
        }
        stack
    }

    fn sample() -> SidecarContent {
        SidecarContent {
            latest: Some(stack(EditBase::Raw, &[("exposure", 0.35), ("contrast", 12.0)])),
            profiles: vec![SidecarProfile {
                name: "暖调".to_string(),
                source_base: EditBase::Sooc,
                created_at_ms: 1_759_000_000_000,
                ordinal: Some(3),
                stack: stack(EditBase::Sooc, &[("vibrance", 18.0)]),
            }],
            metadata: SidecarMetadata {
                rating: 4,
                color_label: Some("green".to_string()),
                keywords: vec!["旅行".to_string(), "家庭".to_string()],
                author: Some("崔总".to_string()),
                description: Some("雨后的湖 & <西湖>".to_string()),
                country: Some("中国".to_string()),
                province_state: Some("浙江".to_string()),
                city: Some("杭州".to_string()),
                sublocation: Some("西湖".to_string()),
            },
            raw_file_name: Some("MYP0001.ORF".to_string()),
        }
    }

    #[test]
    fn compose_then_parse_round_trips() {
        let xml = compose(&sample(), 1_759_000_123_456, "RayBend 0.1.1").unwrap();
        let parsed = parse(&xml).unwrap().expect("含 rb: 域");
        assert_eq!(parsed.warnings, Vec::<String>::new());
        let latest = parsed.latest.expect("有 latest");
        assert_eq!(latest.source_base, EditBase::Raw);
        assert_eq!(latest.params.get("exposure"), Some(&0.35));
        assert_eq!(parsed.profiles.len(), 1);
        assert_eq!(parsed.profiles[0].name, "暖调");
        assert_eq!(parsed.profiles[0].ordinal, Some(3));
        assert_eq!(parsed.profiles[0].source_base, EditBase::Sooc);
        assert_eq!(parsed.metadata.rating, 4);
        assert_eq!(parsed.metadata.color_label.as_deref(), Some("green"));
        assert_eq!(parsed.metadata.keywords, vec!["旅行", "家庭"]);
        assert_eq!(parsed.metadata.author.as_deref(), Some("崔总"));
        assert_eq!(parsed.metadata.description.as_deref(), Some("雨后的湖 & <西湖>"));
        assert_eq!(parsed.metadata.city.as_deref(), Some("杭州"));
        assert_eq!(parsed.metadata.sublocation.as_deref(), Some("西湖"));
        assert_eq!(parsed.metadata.country.as_deref(), Some("中国"));
        assert_eq!(parsed.metadata.province_state.as_deref(), Some("浙江"));
    }

    #[test]
    fn crs_layer_follows_raw_base_only() {
        let mut content = sample();
        let xml = compose(&content, 0, "t").unwrap();
        assert!(xml.contains("crs:Exposure2012=\"0.35\""), "raw 基 latest 有兼容层");
        assert!(xml.contains("crs:ProcessVersion=\"11.0\""));

        content.latest = Some(stack(EditBase::Sooc, &[("exposure", 0.35)]));
        let xml = compose(&content, 0, "t").unwrap();
        assert!(!xml.contains("crs:Exposure2012"), "sooc 基 ⇒ 无兼容层");
        assert!(xml.contains("rb:sourceBase>sooc"), "rb: 仍记录");

        // 元数据与基准无关
        assert!(xml.contains("xmp:Rating=\"4\""));
        assert!(xml.contains("dc:subject"));
    }

    #[test]
    fn empty_content_composes_to_none() {
        assert_eq!(compose(&SidecarContent::default(), 0, "t"), None);
        let only_empty_latest = SidecarContent {
            latest: Some(DevelopStack::default()),
            ..SidecarContent::default()
        };
        assert_eq!(compose(&only_empty_latest, 0, "t"), None);
        let only_meta = SidecarContent {
            metadata: SidecarMetadata {
                rating: 5,
                ..SidecarMetadata::default()
            },
            ..SidecarContent::default()
        };
        assert!(compose(&only_meta, 0, "t").is_some(), "只有评级也算有内容");
    }

    #[test]
    fn rating_zero_is_omitted_but_metadata_present() {
        let mut content = sample();
        content.metadata.rating = 0;
        let xml = compose(&content, 0, "t").unwrap();
        assert!(!xml.contains("xmp:Rating"));
        assert!(xml.contains("xmp:Label=\"Green\""));
    }

    #[test]
    fn foreign_file_parses_to_none() {
        let foreign = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\
            <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
            <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
            <dc:subject><rdf:Bag><rdf:li>x</rdf:li></rdf:Bag></dc:subject>\
            </rdf:Description></rdf:RDF></x:xmpmeta>";
        assert!(parse(foreign).unwrap().is_none());
    }

    #[test]
    fn broken_xml_is_an_error() {
        assert!(parse("<not-xml").is_err());
    }

    #[test]
    fn tampered_hash_is_skipped_with_warning() {
        let xml = compose(&sample(), 0, "t").unwrap();
        let tampered = xml.replace(
            &format!("crs:Exposure2012=\"0.35\""),
            "crs:Exposure2012=\"0.9\"",
        );
        // 动的是 crs 层（不在哈希里），latest 哈希仍应一致 —— 换个真正动 JSON 的方式：
        let _ = tampered;
        let json_marker = "\"exposure\":0.35";
        assert!(xml.contains(json_marker), "规范 JSON 里应有 exposure");
        let broken = xml.replace(json_marker, "\"exposure\":0.36");
        let parsed = parse(&broken).unwrap().expect("仍是我们的文件");
        assert!(parsed.latest.is_none(), "哈希不符 ⇒ latest 被跳过");
        assert!(parsed
            .warnings
            .iter()
            .any(|warning| warning.contains("哈希不符")));
    }

    #[test]
    fn future_schema_version_is_preserved_as_warning() {
        let xml = compose(&sample(), 0, "t")
            .unwrap()
            .replace("<rb:latestSchemaVersion>1</rb:latestSchemaVersion>", "<rb:latestSchemaVersion>7</rb:latestSchemaVersion>");
        let parsed = parse(&xml).unwrap().expect("仍是我们的文件");
        assert!(parsed.latest.is_none());
        assert_eq!(parsed.max_schema_version, 7);
        assert!(!parsed.warnings.is_empty());
    }

    #[test]
    fn curves_serialize_as_pairs_and_round_trip_element_form() {
        let mut latest = stack(EditBase::Raw, &[]);
        latest
            .curves
            .insert("rgb".to_string(), vec![[0.0, 0.0], [0.5, 0.6], [1.0, 1.0]]);
        let content = SidecarContent {
            latest: Some(latest),
            ..SidecarContent::default()
        };
        let xml = compose(&content, 0, "t").unwrap();
        assert!(xml.contains("<crs:ToneCurvePV2012><rdf:Seq><rdf:li>0, 0</rdf:li><rdf:li>128, 153</rdf:li><rdf:li>255, 255</rdf:li></rdf:Seq></crs:ToneCurvePV2012>"));
    }

    #[test]
    fn unicode_and_emoji_survive_escaping() {
        let content = SidecarContent {
            metadata: SidecarMetadata {
                keywords: vec!["🎉 旅行".to_string(), "a&b<c>\"'".to_string()],
                author: Some(" PHOTO 📷 ".to_string()),
                ..SidecarMetadata::default()
            },
            ..SidecarContent::default()
        };
        let xml = compose(&content, 0, "t").unwrap();
        let parsed = parse(&xml).unwrap().unwrap();
        assert_eq!(parsed.metadata.keywords[0], "🎉 旅行");
        assert_eq!(parsed.metadata.keywords[1], "a&b<c>\"'");
        assert_eq!(parsed.metadata.author.as_deref(), Some(" PHOTO 📷 "));
    }

    #[test]
    fn metadata_parses_from_element_form_too() {
        // 属性可能以元素形态出现（手写或别的工具）—— 我们认两种形态。
        // 注意 `rb:profiles`（哪怕为空）是「我们的文件」的标记。
        let xml = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\
            <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
            <rdf:Description rdf:about=\"\" xmlns:rb=\"https://raybend.app/ns/issue/1.0/\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\">\
            <xmp:Rating>5</xmp:Rating>\
            <rb:profiles><rdf:Seq></rdf:Seq></rb:profiles>\
            </rdf:Description></rdf:RDF></x:xmpmeta>";
        let parsed = parse(xml).unwrap().expect("含 rb: 域");
        assert_eq!(parsed.metadata.rating, 5);
        assert!(parsed.profiles.is_empty());
    }

    #[test]
    fn metadata_only_sidecar_is_ours() {
        // 只有评级、没有编辑 —— 也是我们的文件（rb:profiles 空标记）
        let content = SidecarContent {
            metadata: SidecarMetadata {
                rating: 3,
                ..SidecarMetadata::default()
            },
            ..SidecarContent::default()
        };
        let xml = compose(&content, 0, "t").unwrap();
        let parsed = parse(&xml).unwrap().expect("是我们的文件");
        assert_eq!(parsed.metadata.rating, 3);
        assert!(parsed.latest.is_none());
        assert!(parsed.profiles.is_empty());
    }

    #[test]
    fn iso_format_is_utc_and_sortable() {
        assert_eq!(iso8601_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601_utc(1_759_000_123_456), "2025-09-27T19:08:43Z");
    }

    #[test]
    fn auto_adjust_is_excluded_from_canonical_json() {
        let mut stack = stack(EditBase::Raw, &[("exposure", 0.1)]);
        stack.auto_adjust = Some(crate::store::develop::AutoAdjustBaseline::default());
        let content = SidecarContent {
            latest: Some(stack),
            ..SidecarContent::default()
        };
        let xml = compose(&content, 0, "t").unwrap();
        assert!(!xml.contains("autoAdjust"), "规范 JSON 不含 auto_adjust");
    }
}
