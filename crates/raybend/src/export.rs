//! 导出准备契约：轻量 issue 摘要、明确 variant 快照与预设校验；高精度单稿输出；不持久化队列。
use crate::store::{
    develop::{self, DevelopStack, EditBase},
    issues,
};
use crate::{Error, Result};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[cfg(test)]
mod batch_smoke;
pub mod jobs;
pub mod metadata;
pub mod output;
pub mod presets;

pub const MAX_BATCH: usize = 128;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VariantRef {
    pub asset_id: i64,
    pub variant: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantSummary {
    pub rel_path: String,
    pub reference: VariantRef,
    pub name: String,
    pub source_base: EditBase,
    pub profile_hash: Option<String>,
    pub main: bool,
    pub edited: bool,
    pub created_at: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetVariants {
    pub asset_id: i64,
    pub variants: Vec<VariantSummary>,
}

pub fn validate_batch(ids: &[i64]) -> Result<()> {
    if ids.len() > MAX_BATCH || ids.iter().any(|id| *id <= 0) {
        return Err(Error::Unsupported("导出摘要最多 128 个合法资产".into()));
    }
    Ok(())
}
/// 一次 IPC 一批；命名稿只读摘要，不搬运 profile_json。
pub fn summaries(conn: &Connection, ids: &[i64]) -> Result<Vec<AssetVariants>> {
    validate_batch(ids)?;
    let ids: BTreeSet<_> = ids.iter().copied().collect();
    let json = serde_json::to_string(&ids).map_err(|e| Error::Unsupported(e.to_string()))?;
    let mut roles: BTreeMap<i64, BTreeMap<String, String>> = BTreeMap::new();
    let mut stmt = conn.prepare("SELECT asset_id,role,rel_path FROM asset_files WHERE asset_id IN (SELECT value FROM json_each(?1)) AND missing_since IS NULL AND role IN ('bitmap','raw')")?;
    for row in stmt.query_map([&json], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (id, role, path) = row?;
        roles.entry(id).or_default().insert(role, path);
    }
    let mut result: BTreeMap<i64, AssetVariants> = BTreeMap::new();
    for id in ids {
        let Some(roles) = roles.get(&id) else {
            continue;
        };
        let mut variants = Vec::new();
        let base = if roles.contains_key("bitmap") {
            EditBase::Sooc
        } else {
            EditBase::Raw
        };
        variants.push(VariantSummary {
            rel_path: roles
                .get(if base == EditBase::Sooc {
                    "bitmap"
                } else {
                    "raw"
                })
                .unwrap()
                .clone(),
            reference: VariantRef {
                asset_id: id,
                variant: base.as_str().into(),
            },
            name: if base == EditBase::Sooc {
                "SOOC".into()
            } else {
                "RAW".into()
            },
            source_base: base,
            profile_hash: None,
            main: false,
            edited: false,
            created_at: None,
        });
        let mut latest = develop::load(conn, id)?;
        if latest.is_empty()
            && !roles.contains_key(if latest.source_base == EditBase::Raw {
                "raw"
            } else {
                "bitmap"
            })
        {
            latest.source_base = base;
        }
        let edited = !latest.is_empty();
        let variant = if edited {
            "latest"
        } else {
            latest.source_base.as_str()
        };
        variants.retain(|v| v.reference.variant != variant);
        variants.insert(
            0,
            VariantSummary {
                rel_path: develop::edit_target(conn, id, latest.source_base)?.unwrap_or_default(),
                reference: VariantRef {
                    asset_id: id,
                    variant: variant.into(),
                },
                name: if edited {
                    "latest".into()
                } else {
                    variant.to_uppercase()
                },
                source_base: latest.source_base,
                profile_hash: Some(issues::profile_hash(&latest)?),
                main: true,
                edited,
                created_at: None,
            },
        );
        result.insert(
            id,
            AssetVariants {
                asset_id: id,
                variants,
            },
        );
    }
    let mut stmt=conn.prepare("SELECT asset_id,id,name,source_base,profile_hash,created_at FROM issues WHERE asset_id IN (SELECT value FROM json_each(?1)) ORDER BY created_at DESC,id DESC")?;
    for row in stmt.query_map([&json], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, i64>(5)?,
        ))
    })? {
        let (asset, id, name, base, hash, created) = row?;
        let base =
            EditBase::parse(&base).ok_or_else(|| Error::Unsupported("定稿源类型无效".into()))?;
        if let Some(entry) = result.get_mut(&asset) {
            entry.variants.push(VariantSummary {
                rel_path: develop::edit_target(conn, asset, base)?.unwrap_or_default(),
                reference: VariantRef {
                    asset_id: asset,
                    variant: format!("issue:{id}"),
                },
                name,
                source_base: base,
                profile_hash: Some(hash),
                main: false,
                edited: false,
                created_at: Some(created),
            });
        }
    }
    for entry in result.values_mut() {
        let matched = entry
            .variants
            .iter()
            .skip(1)
            .find(|v| {
                v.reference.variant.starts_with("issue:")
                    && v.profile_hash == entry.variants[0].profile_hash
                    && v.source_base == entry.variants[0].source_base
            })
            .cloned();
        if let Some(mut named) = matched {
            named.main = true;
            named.edited = entry.variants[0].edited;
            entry.variants.retain(|v| v.reference != named.reference);
            entry.variants[0] = named;
        }
    }
    Ok(result.into_values().collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantSnapshot {
    pub reference: VariantRef,
    pub name: String,
    pub rel_path: String,
    pub profile_hash: String,
    pub stack: DevelopStack,
    pub source_signature: String,
}
/// 明确取所需稿，不改变最新工作副本。基准缺失时不自动换另一源。
pub fn snapshot(conn: &Connection, root: &Path, reference: &VariantRef) -> Result<VariantSnapshot> {
    validate_batch(&[reference.asset_id])?;
    let (name, stack) = match reference.variant.as_str() {
        "sooc" => (
            "SOOC".into(),
            DevelopStack {
                source_base: EditBase::Sooc,
                ..Default::default()
            },
        ),
        "raw" => ("RAW".into(), DevelopStack::default()),
        "latest" => {
            let stack = develop::load(conn, reference.asset_id)?;
            if stack.is_empty() {
                return Err(Error::Unsupported("latest 已不存在".into()));
            }
            ("latest".into(), stack)
        }
        key => {
            let id = key
                .strip_prefix("issue:")
                .and_then(|id| id.parse::<i64>().ok())
                .filter(|id| *id > 0)
                .ok_or_else(|| Error::Unsupported("无效的导出 issue".into()))?;
            let issue = issues::get(conn, reference.asset_id, id)?
                .ok_or_else(|| Error::Unsupported("定稿已不存在".into()))?;
            (issue.name, issue.stack)
        }
    };
    let role = if stack.source_base == EditBase::Raw {
        "raw"
    } else {
        "bitmap"
    };
    // RAW 基准的 latest/命名稿允许历史 JPG 单文件编辑；中性 RAW 项必须确实有 RAW。
    let rel:Option<String>=conn.query_row("SELECT rel_path FROM asset_files WHERE asset_id=?1 AND missing_since IS NULL AND (role=?2 OR (?2='raw' AND ?3!='raw' AND role='bitmap')) ORDER BY CASE WHEN role=?2 THEN 0 ELSE 1 END,id LIMIT 1",rusqlite::params![reference.asset_id,role,reference.variant],|r|r.get(0)).optional()?;
    let rel = rel.ok_or_else(|| Error::Unsupported("导出稿的源文件不存在".into()))?;
    let source = crate::store::path_semantics::to_platform_path(&rel);
    let sig = crate::media::source::source_signature(&root.join(source))?;
    let hash = issues::profile_hash(&stack)?;
    Ok(VariantSnapshot {
        reference: reference.clone(),
        name,
        rel_path: rel,
        profile_hash: hash,
        stack,
        source_signature: sig,
    })
}
/// 队列预览验证入队快照，既不重取 latest，也不要求命名稿仍存在。
pub fn validate_captured(
    conn: &Connection,
    root: &Path,
    reference: &VariantRef,
    captured: &VariantSnapshot,
) -> Result<()> {
    if reference != &captured.reference
        || issues::profile_hash(&captured.stack)? != captured.profile_hash
    {
        return Err(Error::Unsupported("导出快照不一致".into()));
    }
    resolve_captured_source(conn, root, captured).map(|_| ())
}

/// Locate the same asset role at its current path. A catalog move keeps the queued snapshot valid.
pub fn resolve_captured_source(
    conn: &Connection,
    root: &Path,
    captured: &VariantSnapshot,
) -> Result<std::path::PathBuf> {
    validate_batch(&[captured.reference.asset_id])?;
    if issues::profile_hash(&captured.stack)? != captured.profile_hash {
        return Err(Error::Unsupported("导出快照不一致".into()));
    }
    let original = crate::store::path_semantics::to_platform_path(&captured.rel_path);
    if original.is_empty()
        || Path::new(&original)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(Error::Unsupported("导出快照路径无效".into()));
    }
    let rel: Option<String> = conn.query_row(
        "SELECT rel_path FROM asset_files WHERE asset_id=?1 AND missing_since IS NULL AND role=?2",
        rusqlite::params![captured.reference.asset_id, if captured.stack.source_base == EditBase::Raw { "raw" } else { "bitmap" }],
        |r| r.get(0)).optional()?;
    let rel = rel.ok_or_else(|| Error::Unsupported("导出源已移除或缺失".into()))?;
    let path = root
        .join(crate::store::path_semantics::to_platform_path(&rel))
        .canonicalize()?;
    if !path.starts_with(root.canonicalize()?) {
        return Err(Error::Unsupported("导出源位于照片库之外".into()));
    }
    check_source(&path, captured)?;
    Ok(path)
}

/// Execution-time validity of the enqueued issue hash. Removed/changed issues and
/// missing photos share one terminal skip path; database/I/O faults still propagate.
pub fn still_current(conn: &Connection, root: &Path, captured: &VariantSnapshot) -> Result<bool> {
    match snapshot(conn, root, &captured.reference) {
        Ok(current) => Ok(current.profile_hash == captured.profile_hash),
        Err(Error::Unsupported(_)) => Ok(false),
        Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

pub(crate) fn check_source(path: &Path, captured: &VariantSnapshot) -> Result<()> {
    if crate::media::source::source_signature(path)? != captured.source_signature {
        return Err(Error::Unsupported("导出源已变更，请重新入队".into()));
    }
    Ok(())
}

/// Full precision rendering of an immutable profile. No previews, placeholders or source writes.
pub fn render_captured(
    path: &Path,
    captured: &VariantSnapshot,
    lens: Option<&crate::develop::lens::LensCorrection>,
    lut: Option<&crate::develop::lut::Lut>,
    max_edge: u32,
) -> Result<crate::display::output::Rgb16Image> {
    if max_edge > 65535 || issues::profile_hash(&captured.stack)? != captured.profile_hash {
        return Err(Error::Unsupported("导出快照或输出尺寸无效".into()));
    }
    if captured.stack.lut_id.is_some() && captured.stack.lut_enabled == Some(true) && lut.is_none()
    {
        return Err(Error::Unsupported("导出定稿引用的 LUT 不可用".into()));
    }
    let name = path
        .file_name()
        .ok_or_else(|| Error::Unsupported("导出源路径无效".into()))?;
    let raw = crate::media::kind::kind_of_file(&name.to_string_lossy())
        == crate::media::kind::MediaKind::Raw;
    if raw != (captured.stack.source_base == EditBase::Raw) {
        return Err(Error::Unsupported("导出源类型与定稿不符".into()));
    }
    check_source(path, captured)?;
    let (linear, shot) = crate::display::output::decode_linear_source(path)?;
    let image =
        crate::display::output::render_linear(&linear, &captured.stack, lens, shot, lut, max_edge)?;
    check_source(path, captured)?;
    Ok(image)
}

/// Normal export and its preview share the same post-geometry sizing policy.
pub fn render_for_preset(
    path: &Path,
    captured: &VariantSnapshot,
    lens: Option<&crate::develop::lens::LensCorrection>,
    lut: Option<&crate::develop::lut::Lut>,
    preset: &Preset,
) -> Result<crate::display::output::Rgb16Image> {
    validate_size(preset)?;
    let image = render_captured(path, captured, lens, lut, 0)?;
    resize_for_preset(image, preset)
}
fn validate_size(preset: &Preset) -> Result<()> {
    if !(1..=100).contains(&preset.percent)
        || preset.max_edge > 65535
        || (preset.size_mode == SizeMode::MaxEdge && preset.max_edge == 0)
    {
        return Err(Error::Unsupported("导出尺寸无效".into()));
    }
    Ok(())
}
pub fn resize_for_preset(
    image: crate::display::output::Rgb16Image,
    preset: &Preset,
) -> Result<crate::display::output::Rgb16Image> {
    validate_size(preset)?;
    let edge = match preset.size_mode {
        SizeMode::Original => 0,
        SizeMode::MaxEdge => preset.max_edge,
        SizeMode::Percent => {
            ((u64::from(image.width().max(image.height())) * u64::from(preset.percent) + 50) / 100)
                .max(1) as u32
        }
    };
    Ok(crate::display::output::resize_output(image, edge))
}

pub fn encode_tiff16(image: &crate::display::output::Rgb16Image) -> Result<Vec<u8>> {
    output::encode(image, "tiff", 90, &metadata::Metadata::default())
}

/// W3 single-file output. W4/W5 reuse rendering, encoding and atomic publication separately.
pub fn write_tiff16(
    path: &Path,
    target: &Path,
    captured: &VariantSnapshot,
    lens: Option<&crate::develop::lens::LensCorrection>,
    lut: Option<&crate::develop::lut::Lut>,
    max_edge: u32,
) -> Result<(u32, u32)> {
    if !target.is_absolute()
        || !target
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("tif") || e.eq_ignore_ascii_case("tiff"))
    {
        return Err(Error::Unsupported(
            "TIFF 输出需要绝对路径与 tif/tiff 扩展名".into(),
        ));
    }
    if target.exists() {
        return Err(Error::Unsupported("输出目标已存在".into()));
    }
    let image = render_captured(path, captured, lens, lut, max_edge)?;
    let bytes = encode_tiff16(&image)?;
    check_source(path, captured)?;
    crate::fs_atomic::write_new(target, &bytes)?;
    Ok(image.dimensions())
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SizeMode {
    #[default]
    Original,
    Percent,
    MaxEdge,
}
/// Disk name collisions are independent of session queue deduplication.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExistingFile {
    Overwrite,
    Skip,
    #[default]
    Append,
}

fn default_percent() -> u32 {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub format: String,
    pub quality: u8,
    pub max_edge: u32,
    #[serde(default)]
    pub size_mode: SizeMode,
    #[serde(default = "default_percent")]
    pub percent: u32,
    pub directory: String,
    pub template: String,
    #[serde(default)]
    pub existing_file: ExistingFile,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetValidation {
    pub errors: BTreeMap<String, String>,
    pub warnings: Vec<String>,
}
impl Preset {
    pub fn validate(&self, check_disk: bool) -> PresetValidation {
        let mut errors = BTreeMap::new();
        if self.id.is_empty() || self.id.len() > 128 || self.id.contains(['\0', '/', '\\']) {
            errors.insert("id".into(), "预设标识无效".into());
        }
        if self.name.trim().is_empty()
            || self.name.chars().count() > 128
            || self.name.chars().any(char::is_control)
        {
            errors.insert(
                "name".into(),
                "名称不能为空、包含控制字符或超过 128 字".into(),
            );
        }
        if !matches!(self.format.as_str(), "jpeg" | "png" | "webp" | "avif") {
            errors.insert("format".into(), "不支持此导出格式".into());
        }
        if self.format != "png" && !(1..=100).contains(&self.quality) {
            errors.insert("quality".into(), "质量应为 1–100".into());
        }
        if self.max_edge > 65535 || (self.size_mode == SizeMode::MaxEdge && self.max_edge == 0) {
            errors.insert("maxEdge".into(), "最大边长应为 1–65535".into());
        }
        if !(1..=100).contains(&self.percent) {
            errors.insert("percent".into(), "缩小百分比应为 1–100".into());
        }
        let mut warnings = Vec::new();
        match crate::import::template::parse(&self.template) {
            Ok(parsed) => {
                warnings.extend(parsed.warnings().iter().map(ToString::to_string));
                let seqs: Vec<_> = parsed
                    .seq_widths()
                    .into_iter()
                    .map(|width| (width, 1))
                    .collect();
                let rendered = parsed.render(&crate::import::template::RenderCtx {
                    taken_at: None,
                    stem: "example",
                    brand: None,
                    model: None,
                    seqs: crate::import::template::SeqValues::new(&seqs),
                });
                if let Err(e) = crate::import::template::check_output(&rendered.text) {
                    errors.insert("template".into(), e.to_string());
                }
            }
            Err(e) => {
                errors.insert("template".into(), e.to_string());
            }
        }
        if self.directory.is_empty() || self.directory.contains('\0') {
            errors.insert("directory".into(), "请选择导出目录".into());
        } else if check_disk {
            let dir = Path::new(&self.directory);
            if !dir.is_absolute() || !dir.is_dir() {
                errors.insert("directory".into(), "导出目录不存在或不是绝对路径".into());
            } else if let Err(e) = check_writable(dir) {
                errors.insert("directory".into(), format!("导出目录不可写：{e}"));
            }
        }
        PresetValidation { errors, warnings }
    }
}
fn check_writable(dir: &Path) -> std::io::Result<()> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(std::io::Error::other)?;
    let suffix = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let path = dir.join(format!(".raybend-export-probe-{suffix}.tmp"));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    drop(file);
    std::fs::remove_file(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn catalog() -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        crate::store::migration::apply(
            &mut c,
            crate::store::migration::DbKind::Catalog,
            crate::store::migration::Backups::none(),
            0,
        )
        .unwrap();
        c
    }
    #[test]
    fn tiff_export_retains_16bit_full_size_and_never_changes_source() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("原片.png");
        let image = crate::display::output::Rgb16Image::from_fn(64, 32, |x, y| {
            image::Rgb([((x + y * 64) * 23 + 500) as u16; 3])
        });
        image.save(&source).unwrap();
        let before = std::fs::read(&source).unwrap();
        let stack = DevelopStack {
            source_base: EditBase::Sooc,
            ..Default::default()
        };
        let captured = VariantSnapshot {
            reference: VariantRef {
                asset_id: 1,
                variant: "sooc".into(),
            },
            name: "SOOC".into(),
            rel_path: "原片.png".into(),
            profile_hash: issues::profile_hash(&stack).unwrap(),
            stack,
            source_signature: crate::media::source::source_signature(&source).unwrap(),
        };
        let target = dir.path().join("输出/原尺寸.TIFF");
        assert_eq!(
            write_tiff16(&source, &target, &captured, None, None, 0).unwrap(),
            (64, 32)
        );
        let output = image::open(&target).unwrap();
        assert_eq!(output.color(), image::ColorType::Rgb16);
        let levels: BTreeSet<_> = output.to_rgb16().into_raw().into_iter().collect();
        assert!(levels.len() > 256);
        let bytes = std::fs::read(&target).unwrap();
        assert!(write_tiff16(&source, &target, &captured, None, None, 0).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), bytes);
        assert_eq!(std::fs::read(&source).unwrap(), before);
        let resized = dir.path().join("小图.tif");
        assert_eq!(
            write_tiff16(&source, &resized, &captured, None, None, 16).unwrap(),
            (16, 8)
        );
        for target in [
            source.clone(),
            dir.path().join("wrong.jpg"),
            std::path::PathBuf::from("relative.tif"),
        ] {
            assert!(write_tiff16(&source, &target, &captured, None, None, 0).is_err());
        }
        let mut invalid = captured.clone();
        invalid.stack.lut_id = Some("missing".into());
        invalid.stack.lut_enabled = Some(true);
        invalid.profile_hash = issues::profile_hash(&invalid.stack).unwrap();
        assert!(render_captured(&source, &invalid, None, None, 0).is_err());
        assert!(render_captured(&source, &captured, None, None, u32::MAX).is_err());
        std::fs::write(&source, b"changed").unwrap();
        assert!(render_captured(&source, &captured, None, None, 0).is_err());
        assert!(
            write_tiff16(
                &source,
                &dir.path().join("失败.tif"),
                &captured,
                None,
                None,
                0
            )
            .is_err()
        );
        assert!(!dir.path().join("失败.tif").exists());
        assert!(encode_tiff16(&crate::display::output::Rgb16Image::new(0, 0)).is_err());
    }
    #[test]
    fn tiff_codec_is_lossless_including_endpoints_and_sub_byte_steps() {
        let values = vec![0, 1, 128, 129, 256, 257, 65534, 65535, 12345];
        let image = crate::display::output::Rgb16Image::from_raw(3, 1, values.clone()).unwrap();
        let bytes = encode_tiff16(&image).unwrap();
        assert_eq!(
            image::load_from_memory_with_format(&bytes, image::ImageFormat::Tiff)
                .unwrap()
                .to_rgb16()
                .into_raw(),
            values
        );
    }
    #[test]
    fn summary_pairs_named_issues_and_never_changes_latest() {
        let c = catalog();
        c.execute(
            "INSERT INTO assets(id,imported_at,updated_at)VALUES(1,0,0)",
            [],
        )
        .unwrap();
        c.execute("INSERT INTO asset_files(asset_id,role,rel_path,rel_path_folded,ext,created_at,updated_at)VALUES(1,'bitmap','photos/中文.jpg','photos/中文.jpg','jpg',0,0)",[]).unwrap();
        let s = DevelopStack {
            source_base: EditBase::Sooc,
            params: BTreeMap::from([("exposure".into(), 1.0)]),
            ..Default::default()
        };
        develop::save(&c, 1, &s, 0).unwrap();
        issues::create(&c, 1, "中文定稿", &s, 0).unwrap();
        let a = summaries(&c, &[1, 1]).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].variants.len(), 2);
        assert!(a[0].variants[0].main && a[0].variants[0].edited);
        assert!(a[0].variants[0].reference.variant.starts_with("issue:"));
        assert!(summaries(&c, &[]).unwrap().is_empty());
        assert_eq!(develop::load(&c, 1).unwrap(), s);
        assert!(summaries(&c, &[0]).is_err());
        assert!(summaries(&c, &vec![1; 129]).is_err());
    }
    #[test]
    fn raw_only_is_not_sooc_and_missing_is_not_selectable() {
        let c = catalog();
        c.execute(
            "INSERT INTO assets(id,imported_at,updated_at)VALUES(1,0,0)",
            [],
        )
        .unwrap();
        c.execute("INSERT INTO asset_files(asset_id,role,rel_path,rel_path_folded,ext,created_at,updated_at)VALUES(1,'raw','photos/_RAW/a.nef','photos/_raw/a.nef','nef',0,0)",[]).unwrap();
        assert_eq!(
            summaries(&c, &[1]).unwrap()[0].variants[0]
                .reference
                .variant,
            "raw"
        );
        c.execute("UPDATE asset_files SET missing_since=1", [])
            .unwrap();
        assert!(summaries(&c, &[1]).unwrap().is_empty());
    }
    #[test]
    fn execution_hash_skips_changed_deleted_issues_and_photos() {
        let c = catalog();
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("photos")).unwrap();
        std::fs::write(dir.path().join("photos/a.jpg"), b"stub").unwrap();
        c.execute(
            "INSERT INTO assets(id,imported_at,updated_at)VALUES(1,0,0)",
            [],
        )
        .unwrap();
        c.execute("INSERT INTO asset_files(asset_id,role,rel_path,rel_path_folded,ext,created_at,updated_at)VALUES(1,'bitmap','photos/a.jpg','photos/a.jpg','jpg',0,0)",[]).unwrap();
        let stack = DevelopStack {
            source_base: EditBase::Sooc,
            params: BTreeMap::from([("exposure".into(), 1.0)]),
            ..Default::default()
        };
        develop::save(&c, 1, &stack, 0).unwrap();
        let id = issues::create(&c, 1, "定稿", &stack, 0).unwrap().id;
        let named = snapshot(
            &c,
            dir.path(),
            &VariantRef {
                asset_id: 1,
                variant: format!("issue:{id}"),
            },
        )
        .unwrap();
        let latest = snapshot(
            &c,
            dir.path(),
            &VariantRef {
                asset_id: 1,
                variant: "latest".into(),
            },
        )
        .unwrap();
        assert!(still_current(&c, dir.path(), &named).unwrap());
        develop::save(
            &c,
            1,
            &DevelopStack {
                params: BTreeMap::from([("exposure".into(), 2.0)]),
                ..stack
            },
            1,
        )
        .unwrap();
        assert!(!still_current(&c, dir.path(), &latest).unwrap());
        assert!(still_current(&c, dir.path(), &named).unwrap());
        issues::delete(&c, 1, id).unwrap();
        assert!(!still_current(&c, dir.path(), &named).unwrap());
        let base = snapshot(
            &c,
            dir.path(),
            &VariantRef {
                asset_id: 1,
                variant: "sooc".into(),
            },
        )
        .unwrap();
        std::fs::remove_file(dir.path().join("photos/a.jpg")).unwrap();
        assert!(!still_current(&c, dir.path(), &base).unwrap());
    }
    #[test]
    fn snapshot_stays_immutable_and_rejects_missing_or_wrong_source() {
        let c = catalog();
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("photos")).unwrap();
        std::fs::write(dir.path().join("photos/中文.jpg"), b"stub").unwrap();
        c.execute(
            "INSERT INTO assets(id,imported_at,updated_at)VALUES(1,0,0)",
            [],
        )
        .unwrap();
        c.execute("INSERT INTO asset_files(asset_id,role,rel_path,rel_path_folded,ext,created_at,updated_at)VALUES(1,'bitmap','photos/中文.jpg','photos/中文.jpg','jpg',0,0)",[]).unwrap();
        let r = VariantRef {
            asset_id: 1,
            variant: "sooc".into(),
        };
        let snap = snapshot(&c, dir.path(), &r).unwrap();
        assert_eq!(snap.rel_path, "photos/中文.jpg");
        assert_eq!(snap.stack.source_base, EditBase::Sooc);
        assert!(
            snapshot(
                &c,
                dir.path(),
                &VariantRef {
                    variant: "raw".into(),
                    ..r.clone()
                }
            )
            .is_err()
        );
        assert!(
            snapshot(
                &c,
                dir.path(),
                &VariantRef {
                    variant: "latest".into(),
                    ..r.clone()
                }
            )
            .is_err()
        );
        for variant in ["issue:-1", "issue:0", "bad"] {
            assert!(
                snapshot(
                    &c,
                    dir.path(),
                    &VariantRef {
                        variant: variant.into(),
                        ..r.clone()
                    }
                )
                .is_err()
            );
        }
        std::fs::remove_file(dir.path().join("photos/中文.jpg")).unwrap();
        assert!(snapshot(&c, dir.path(), &r).is_err());
    }
    #[test]
    fn output_size_modes_share_geometry_and_preserve_precision() {
        let mut preset = Preset {
            id: "p".into(),
            name: "尺寸".into(),
            format: "png".into(),
            quality: 90,
            max_edge: 40,
            size_mode: SizeMode::Original,
            percent: 50,
            directory: "C:/output".into(),
            template: ":FILENAME".into(),
            existing_file: crate::export::ExistingFile::Append,
        };
        let image = crate::display::output::Rgb16Image::from_fn(100, 60, |x, y| {
            image::Rgb([(x * 200 + y) as u16; 3])
        });
        assert_eq!(resize_for_preset(image.clone(), &preset).unwrap(), image);
        preset.size_mode = SizeMode::Percent;
        assert_eq!(
            resize_for_preset(image.clone(), &preset)
                .unwrap()
                .dimensions(),
            (50, 30)
        );
        preset.percent = 100;
        assert_eq!(resize_for_preset(image.clone(), &preset).unwrap(), image);
        preset.percent = 1;
        assert_eq!(
            resize_for_preset(image.clone(), &preset)
                .unwrap()
                .dimensions(),
            (1, 1)
        );
        preset.size_mode = SizeMode::MaxEdge;
        assert_eq!(
            resize_for_preset(image.clone(), &preset)
                .unwrap()
                .dimensions(),
            (40, 24)
        );
        preset.max_edge = 1000;
        assert_eq!(resize_for_preset(image.clone(), &preset).unwrap(), image);
        for bad in [0, 101, u32::MAX] {
            preset.percent = bad;
            assert!(resize_for_preset(image.clone(), &preset).is_err());
        }
        preset.percent = 100;
        preset.max_edge = 0;
        assert!(resize_for_preset(image, &preset).is_err());
        preset.format = "tiff".into();
        assert!(preset.validate(false).errors.contains_key("format"));
    }
    #[test]
    fn preset_validates_unicode_quality_paths_and_templates() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = Preset {
            id: "stable".into(),
            name: "中文 📷".into(),
            format: "png".into(),
            quality: 90,
            max_edge: 0,
            size_mode: crate::export::SizeMode::Original,
            percent: 100,
            directory: dir.path().to_string_lossy().into(),
            template: "中文/:FILENAME".into(),
            existing_file: ExistingFile::Append,
        };
        assert!(p.validate(true).errors.is_empty());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        p.quality = 0;
        p.name = "\0".into();
        p.max_edge = u32::MAX;
        p.template = "../:FILENAME".into();
        p.directory = "relative".into();
        p.format = "raw".into();
        assert_eq!(p.validate(true).errors.len(), 6);
        for format in ["png"] {
            p.format = format.into();
            assert!(!p.validate(false).errors.contains_key("quality"));
        }
    }
    #[test]
    fn captured_validation_preserves_latest_and_checks_source_hash_and_reference() {
        let c = catalog();
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("photos")).unwrap();
        std::fs::write(dir.path().join("photos/中文.jpg"), b"stub").unwrap();
        c.execute(
            "INSERT INTO assets(id,imported_at,updated_at)VALUES(1,0,0)",
            [],
        )
        .unwrap();
        c.execute("INSERT INTO asset_files(asset_id,role,rel_path,rel_path_folded,ext,created_at,updated_at)VALUES(1,'bitmap','photos/中文.jpg','photos/中文.jpg','jpg',0,0)",[]).unwrap();
        let stack = DevelopStack {
            source_base: EditBase::Sooc,
            params: BTreeMap::from([("exposure".into(), 1.0)]),
            ..Default::default()
        };
        develop::save(&c, 1, &stack, 0).unwrap();
        let reference = VariantRef {
            asset_id: 1,
            variant: "latest".into(),
        };
        let captured = snapshot(&c, dir.path(), &reference).unwrap();
        let updated = DevelopStack {
            params: BTreeMap::from([("exposure".into(), 2.0)]),
            ..stack
        };
        develop::save(&c, 1, &updated, 0).unwrap();
        validate_captured(&c, dir.path(), &reference, &captured).unwrap();
        assert_ne!(
            captured.profile_hash,
            snapshot(&c, dir.path(), &reference).unwrap().profile_hash
        );
        std::fs::rename(
            dir.path().join("photos/中文.jpg"),
            dir.path().join("photos/改名.jpg"),
        )
        .unwrap();
        c.execute("UPDATE asset_files SET rel_path='photos/改名.jpg',rel_path_folded='photos/改名.jpg' WHERE asset_id=1",[]).unwrap();
        validate_captured(&c, dir.path(), &reference, &captured).unwrap();
        assert_eq!(
            resolve_captured_source(&c, dir.path(), &captured).unwrap(),
            dir.path().join("photos/改名.jpg").canonicalize().unwrap()
        );
        let mut altered = captured.clone();
        altered.profile_hash = "wrong".into();
        assert!(validate_captured(&c, dir.path(), &reference, &altered).is_err());
        altered = captured.clone();
        altered.rel_path = "../outside".into();
        assert!(validate_captured(&c, dir.path(), &reference, &altered).is_err());
        assert!(
            validate_captured(
                &c,
                dir.path(),
                &VariantRef {
                    asset_id: 2,
                    ..reference.clone()
                },
                &captured
            )
            .is_err()
        );
        std::fs::write(dir.path().join("photos/改名.jpg"), b"different length").unwrap();
        assert!(validate_captured(&c, dir.path(), &reference, &captured).is_err());
    }
    #[test]
    fn summary_latest_agrees_with_canonical_empty_stack() {
        let c = catalog();
        c.execute(
            "INSERT INTO assets(id,imported_at,updated_at)VALUES(1,0,0)",
            [],
        )
        .unwrap();
        c.execute("INSERT INTO asset_files(asset_id,role,rel_path,rel_path_folded,ext,created_at,updated_at)VALUES(1,'bitmap','photos/a.jpg','photos/a.jpg','jpg',0,0)",[]).unwrap();
        let stack = DevelopStack {
            source_base: EditBase::Sooc,
            lens_enabled: Some(false),
            ..Default::default()
        };
        develop::save(&c, 1, &stack, 0).unwrap();
        assert_eq!(summaries(&c, &[1]).unwrap()[0].variants.len(), 2);
        let stack = DevelopStack {
            source_base: EditBase::Sooc,
            nr_method: Some(crate::develop::denoise::NrMethod::Fast),
            ..Default::default()
        };
        develop::save(&c, 1, &stack, 0).unwrap();
        assert_eq!(summaries(&c, &[1]).unwrap()[0].variants.len(), 1);
    }
}
