//! XMP sidecar（`specs/xmp-sidecar.md`）：路径规则、数据包构建/解析、标准层映射、文件发布。
//!
//! **定位**（崔总 2026-09-30）：XMP 不是第一公民——真相源是 `catalog.db`；
//! sidecar 的作用 = ①宣示开放（用户数据资产归用户）②照片带 sidecar 离开库时 best-effort 恢复。
//! 整库备份/迁移的正路是直接拷 repos 目录，不靠这里。
//!
//! 分层：
//!
//! * [`path`]：`<主体名>.xmp` 的位置规则（`_RAW/` 折算）—— 全项目唯一一处；
//! * [`packet`]：模型与 XML 构建/解析（转义与 dc: 构造在这里，供导出内嵌 XMP 复用）；
//! * [`mapping`]：`crs:` 调整映射（仅 raw 基 latest）与色标换算；
//! * 本文件：**发布**（归属检查、外来文件备份接管、原子写、清空删除）。
//!
//! 接入点（写出触发、导入采纳）在 `src-tauri` 侧，见规格 §6–§8。

mod adopt;
pub mod mapping;
pub mod packet;
pub mod path;

pub use adopt::adopt;

pub use packet::{
    ParsedSidecar, SidecarContent, SidecarMetadata, SidecarProfile, compose, compose_checked, escape, parse,
    rdf_alt, rdf_list,
};
pub use path::{probe_sidecar, rel_sidecar_of, resolve_sidecar, sidecar_rel_path};

use crate::store::issues::PROFILE_SCHEMA_VERSION;
use std::path::Path;

/// 一次发布的结果（诊断与测试用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    /// 写出了我们的文件（新建、覆盖或备份接管后写入）。
    Written,
    /// 内容与盘上一致，跳过重写。
    Unchanged,
    /// 内容为空，删除了我们的旧文件（「没有就没有」）。
    Deleted,
    /// 盘上是**更新版本/读不懂**的我们的文件 —— 保留不动（契约：未知版本必须保留）。
    Preserved,
}

/// 把一份 sidecar XML 发布到指定路径（`specs/xmp-sidecar.md` §3.3 / §6）。
///
/// * 目标不存在 → 原子写入（临时文件 + 替换，复用 [`crate::fs_atomic`]）。
/// * 目标存在且是**我们的**（含 `rb:` 域）且版本 ≤ 本版本 → 覆盖（内容相同则跳过）。
/// * 目标存在且是我们的但版本**更高**/读不懂 → 保留，返回 [`PublishOutcome::Preserved`]。
/// * 目标存在但是**别家的**（无 `rb:`）→ 先备份为 `<名>.xmp.bak`（已存在则覆盖该备份）再写入
///   —— 不静默毁掉别人的数据。
///
/// # Errors
/// 文件系统错误（权限、磁盘满等）。失败**不回滚**已落库的编辑，由下一次触发重试。
pub fn publish(path: &Path, xml: &str) -> crate::Result<PublishOutcome> {
    let actual = resolve_sidecar(path)?;
    let path = actual.as_deref().unwrap_or(path);
    let existing = std::fs::read(path);
    match existing {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            crate::fs_atomic::write(path, xml.as_bytes())?;
            Ok(PublishOutcome::Written)
        }
        Err(error) => Err(error.into()),
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            match ownership_bytes(&bytes) {
                Ownership::Ours(parsed) => {
                    if parsed.as_ref().is_none_or(|sidecar| !replaceable(sidecar)) {
                        // 读不懂或版本更高：保留不动（契约：未知版本必须保留）
                        return Ok(PublishOutcome::Preserved);
                    }
                    if text == xml {
                        return Ok(PublishOutcome::Unchanged);
                    }
                    crate::fs_atomic::write(path, xml.as_bytes())?;
                    Ok(PublishOutcome::Written)
                }
                Ownership::Foreign => {
                    backup(path)?;
                    crate::fs_atomic::write(path, xml.as_bytes())?;
                    Ok(PublishOutcome::Written)
                }
            }
        }
    }
}

/// 内容已全空时移除我们的 sidecar（「编辑了才有」的另一面：没有就没有）。
///
/// 只删本版本完整认识的**我们的**文件；未知/损坏或别家的不动。返回是否真的删了。
///
/// # Errors
/// 文件系统错误（`NotFound` 不算错，返回 `false`）。
pub fn remove_if_ours(path: &Path) -> crate::Result<bool> {
    let Some(actual) = resolve_sidecar(path)? else {
        return Ok(false);
    };
    let path = actual.as_path();
    let bytes = match std::fs::read(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
        Ok(bytes) => bytes,
    };
    match ownership_bytes(&bytes) {
        Ownership::Ours(Some(parsed)) if replaceable(&parsed) => {}
        Ownership::Ours(_) => {
            eprintln!(
                "[sidecar] {} 版本未知或内容损坏，保留未删除",
                path.display()
            );
            return Ok(false);
        }
        Ownership::Foreign => return Ok(false),
    }
    std::fs::remove_file(path)?;
    Ok(true)
}

/// 这个文件是**我们的** sidecar 吗（含 `rb:` 域即算；解析不了的也按命名空间认）。
///
/// `store::delete` 把 sidecar 与照片一起送回收站前用它做守卫。
#[must_use]
pub fn is_ours(path: &Path) -> bool {
    std::fs::read(path)
        .map(|bytes| !matches!(ownership_bytes(&bytes), Ownership::Foreign))
        .unwrap_or(false)
}

fn replaceable(parsed: &ParsedSidecar) -> bool {
    parsed.max_schema_version <= PROFILE_SCHEMA_VERSION && parsed.warnings.is_empty()
}

/// `Some` = 能解析的我们的文件；`None` = 坏掉但含 `rb:` 命名空间（可能是截断的我们的文件）。
enum Ownership {
    Ours(Option<Box<ParsedSidecar>>),
    Foreign,
}

fn ownership_bytes(bytes: &[u8]) -> Ownership {
    match std::str::from_utf8(bytes) {
        Ok(text) => ownership(text),
        Err(_) if String::from_utf8_lossy(bytes).contains(packet::RB_NS) => Ownership::Ours(None),
        Err(_) => Ownership::Foreign,
    }
}

fn ownership(text: &str) -> Ownership {
    match parse(text) {
        Ok(Some(parsed)) => Ownership::Ours(Some(Box::new(parsed))),
        Ok(None) => Ownership::Foreign,
        // XML 解析失败时退回子串探测：命名空间 URI 在正文里出现即认为是我们的
        // （损坏的我们的文件宁可保留也不当外来处理）。
        Err(_) => {
            if text.contains(packet::RB_NS) {
                Ownership::Ours(None)
            } else {
                Ownership::Foreign
            }
        }
    }
}

/// 外来文件备份接管：改名成 `<名>.xmp.bak`；备份已存在则覆盖它（保存的是「最后一个外来版本」）。
fn backup(path: &Path) -> crate::Result<()> {
    let mut bak = path.as_os_str().to_owned();
    bak.push(".bak");
    let bak = std::path::PathBuf::from(bak);
    if bak.exists() {
        std::fs::remove_file(&bak)?;
    }
    std::fs::rename(path, &bak)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::develop::{DevelopStack, EditBase};

    fn minimal_content() -> SidecarContent {
        let mut latest = DevelopStack {
            source_base: EditBase::Raw,
            ..DevelopStack::default()
        };
        latest.params.insert("exposure".to_string(), 0.5);
        SidecarContent {
            latest: Some(latest),
            ..SidecarContent::default()
        }
    }

    #[test]
    fn fresh_write_then_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.xmp");
        let xml = compose(&minimal_content(), 0, "t").unwrap();
        assert_eq!(publish(&path, &xml).unwrap(), PublishOutcome::Written);
        assert!(path.is_file());
        assert_eq!(publish(&path, &xml).unwrap(), PublishOutcome::Unchanged);
    }

    #[test]
    fn rewriting_ours_overwrites_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.xmp");
        let first = compose(&minimal_content(), 0, "t").unwrap();
        publish(&path, &first).unwrap();
        let mut content = minimal_content();
        content
            .latest
            .as_mut()
            .unwrap()
            .params
            .insert("contrast".to_string(), 10.0);
        let second = compose(&content, 1, "t").unwrap();
        assert_eq!(publish(&path, &second).unwrap(), PublishOutcome::Written);
        assert!(std::fs::read_to_string(&path).unwrap().contains("contrast"));
        // 没有残留的临时文件
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftovers, vec!["a.xmp".to_string()]);
    }

    #[test]
    fn foreign_file_is_backed_up_then_taken_over() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.xmp");
        std::fs::write(
            &path,
            "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\" xmlns:crs=\"http://ns.adobe.com/camera-raw-settings/1.0/\" crs:Exposure2012=\"1.0\"/></rdf:RDF></x:xmpmeta>",
        )
        .unwrap();
        let xml = compose(&minimal_content(), 0, "t").unwrap();
        assert_eq!(publish(&path, &xml).unwrap(), PublishOutcome::Written);
        let bak = dir.path().join("a.xmp.bak");
        assert!(bak.is_file(), "外来文件被备份");
        assert!(
            std::fs::read_to_string(&bak)
                .unwrap()
                .contains("crs:Exposure2012")
        );
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("rb:profiles")
        );
    }

    #[test]
    fn future_version_is_preserved_not_clobbered() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.xmp");
        let xml = compose(&minimal_content(), 0, "t").unwrap().replace(
            "<rb:latestSchemaVersion>1</rb:latestSchemaVersion>",
            "<rb:latestSchemaVersion>9</rb:latestSchemaVersion>",
        );
        std::fs::write(&path, &xml).unwrap();
        let newer = compose(&minimal_content(), 5, "t").unwrap();
        assert_eq!(publish(&path, &newer).unwrap(), PublishOutcome::Preserved);
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("latestSchemaVersion>9"),
            "未知版本的文件原样保留"
        );
    }

    #[test]
    fn remove_only_ours() {
        let dir = tempfile::tempdir().unwrap();
        let ours = dir.path().join("a.xmp");
        let xml = compose(&minimal_content(), 0, "t").unwrap();
        std::fs::write(&ours, &xml).unwrap();
        assert!(remove_if_ours(&ours).unwrap());
        assert!(!ours.exists());

        let foreign = dir.path().join("b.xmp");
        std::fs::write(&foreign, "<x/>").unwrap();
        assert!(!remove_if_ours(&foreign).unwrap(), "别家的不动");
        assert!(foreign.is_file());

        let absent = dir.path().join("c.xmp");
        assert!(!remove_if_ours(&absent).unwrap());
        assert!(!is_ours(&ours));
        assert!(!is_ours(&foreign));
    }

    #[test]
    fn broken_ours_file_is_preserved_broken_foreign_is_taken_over() {
        let dir = tempfile::tempdir().unwrap();
        // 坏掉但含 rb: 命名空间的（可能是截断的我们的文件）→ 保留
        let broken_ours = dir.path().join("a.xmp");
        std::fs::write(
            &broken_ours,
            "<x:xmpmeta rb=\"https://raybend.app/ns/issue/1.0/\" ",
        )
        .unwrap();
        let xml = compose(&minimal_content(), 0, "t").unwrap();
        assert_eq!(
            publish(&broken_ours, &xml).unwrap(),
            PublishOutcome::Preserved
        );
        // 坏掉且不是我们的 → 备份接管
        let broken_foreign = dir.path().join("b.xmp");
        std::fs::write(&broken_foreign, "not xml at all").unwrap();
        assert_eq!(
            publish(&broken_foreign, &xml).unwrap(),
            PublishOutcome::Written
        );
        assert!(dir.path().join("b.xmp.bak").is_file());
    }

    #[test]
    fn reset_preserves_unknown_or_damaged_own_sidecars() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.xmp");
        let valid = compose(&minimal_content(), 0, "t").unwrap();
        let cases = [
            valid.replace("latestSchemaVersion>1", "latestSchemaVersion>9"),
            valid.replace("latestSchemaVersion>1", "latestSchemaVersion>0"),
            valid.replace("latestSchemaVersion>1", "latestSchemaVersion>-1"),
            valid.replace("<rb:profileJson>", "<rb:profileJson>garbage"),
            format!("<broken xmlns:rb=\"{}\"", packet::RB_NS),
        ];
        for xml in cases {
            std::fs::write(&path, &xml).unwrap();
            assert!(is_ours(&path));
            assert_eq!(publish(&path, &valid).unwrap(), PublishOutcome::Preserved);
            assert!(!remove_if_ours(&path).unwrap());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), xml);
            assert!(!dir.path().join("a.xmp.bak").exists());
        }
    }

    #[test]
    fn case_folded_sidecar_is_reused_backed_up_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let requested = dir.path().join("Photo.xmp");
        let actual = dir.path().join("photo.XMP");
        std::fs::write(&actual, "<foreign/>").unwrap();
        let xml = compose(&minimal_content(), 0, "t").unwrap();
        assert_eq!(publish(&requested, &xml).unwrap(), PublishOutcome::Written);
        assert_eq!(std::fs::read_to_string(&actual).unwrap(), xml);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("photo.XMP.bak")).unwrap(),
            "<foreign/>"
        );
        assert_eq!(
            publish(&requested, &xml).unwrap(),
            PublishOutcome::Unchanged
        );
        assert!(remove_if_ours(&requested).unwrap());
        assert!(!actual.exists());
    }

    #[test]
    fn unknown_rb_properties_are_preserved_without_foreign_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.xmp");
        let xml = format!(
            r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:rb="{}"><rb:future>opaque</rb:future></rdf:Description></rdf:RDF>"#,
            packet::RB_NS
        );
        std::fs::write(&path, &xml).unwrap();
        assert_eq!(
            publish(&path, &compose(&minimal_content(), 0, "t").unwrap()).unwrap(),
            PublishOutcome::Preserved
        );
        assert!(!remove_if_ours(&path).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), xml);
    }
    #[test]
    fn invalid_utf8_own_sidecars_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.xmp");
        let xml = compose(&minimal_content(), 0, "t").unwrap();
        let mut bytes = xml.as_bytes().to_vec();
        let at = xml.find("CreatorTool=\"t").unwrap() + "CreatorTool=\"".len();
        bytes[at] = 0xff;
        std::fs::write(&path, &bytes).unwrap();
        assert!(is_ours(&path));
        assert_eq!(publish(&path, &xml).unwrap(), PublishOutcome::Preserved);
        assert!(!remove_if_ours(&path).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}
