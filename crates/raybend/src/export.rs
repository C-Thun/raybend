//! 导出准备契约：轻量 issue 摘要、明确 variant 快照与预设校验；不执行输出、不持久化队列。
use crate::store::{
    develop::{self, DevelopStack, EditBase},
    issues,
};
use crate::{Error, Result};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

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
        });
        let latest = develop::load(conn, id)?;
        if !latest.is_empty() {
            variants.push(VariantSummary {
                rel_path: develop::edit_target(conn, id, latest.source_base)?.unwrap_or_default(),
                reference: VariantRef {
                    asset_id: id,
                    variant: "latest".into(),
                },
                name: "latest".into(),
                source_base: latest.source_base,
                profile_hash: None,
            });
        }
        result.insert(
            id,
            AssetVariants {
                asset_id: id,
                variants,
            },
        );
    }
    let mut stmt=conn.prepare("SELECT asset_id,id,name,source_base,profile_hash FROM issues WHERE asset_id IN (SELECT value FROM json_each(?1)) ORDER BY created_at DESC,id DESC")?;
    for row in stmt.query_map([&json], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })? {
        let (asset, id, name, base, hash) = row?;
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
            });
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
    let exists:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM asset_files WHERE asset_id=?1 AND rel_path=?2 AND missing_since IS NULL AND role=?3)",rusqlite::params![reference.asset_id,captured.rel_path,if captured.stack.source_base==EditBase::Raw{"raw"}else{"bitmap"}],|r|r.get(0))?;
    if !exists {
        return Err(Error::Unsupported("导出快照源文件已变更".into()));
    }
    let path = root.join(crate::store::path_semantics::to_platform_path(
        &captured.rel_path,
    ));
    if crate::media::source::source_signature(&path)? != captured.source_signature {
        return Err(Error::Unsupported(
            "导出快照源文件已变更，请重新入队".into(),
        ));
    }
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub format: String,
    pub quality: u8,
    pub max_edge: u32,
    pub directory: String,
    pub template: String,
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
        if !matches!(
            self.format.as_str(),
            "jpeg" | "tiff" | "png" | "webp" | "avif"
        ) {
            errors.insert("format".into(), "不支持此导出格式".into());
        }
        if !(1..=100).contains(&self.quality) {
            errors.insert("quality".into(), "质量应为 1–100".into());
        }
        if self.max_edge > 65535 {
            errors.insert("maxEdge".into(), "最大边长不能超过 65535".into());
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
        assert_eq!(a[0].variants.len(), 3);
        assert_eq!(a[0].variants[0].reference.variant, "sooc");
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
    fn preset_validates_unicode_quality_paths_and_templates() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = Preset {
            id: "stable".into(),
            name: "中文 📷".into(),
            format: "tiff".into(),
            quality: 90,
            max_edge: 0,
            directory: dir.path().to_string_lossy().into(),
            template: "中文/:FILENAME".into(),
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
        std::fs::write(dir.path().join("photos/中文.jpg"), b"different length").unwrap();
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
