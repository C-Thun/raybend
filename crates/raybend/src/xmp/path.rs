//! Sidecar 的**路径规则**（`specs/xmp-w1.md` §3.1）—— 全项目唯一一处实现。
//!
//! ```text
//! sidecar_path(asset) = <照片目录>/<主体名>.xmp
//! ```
//!
//! * 「照片目录」= 位图所在目录；没有位图时用 RAW 所在目录；
//!   若该目录最后一段是 `_RAW/`，折算回父目录（复用 `store::assets::normalize_raw_dir`）。
//! * 主体名 = 文件名去扩展名（保留原始大小写）。
//! * **绝不写进 `_RAW/`**。
//!
//! 前端不许自己拼 sidecar 路径 —— 与 `develop::edit_target` 的 `_RAW/` 规则同一条纪律。

use crate::store::assets::{FileRow, normalize_raw_dir};

/// `photos/2026/MYP0001.JPG` + `photos/2026/_RAW/MYP0001.ORF` → `photos/2026/MYP0001.xmp`。
///
/// 挑「代表文件」：位图优先（跳过已标缺失的），没有位图才用 RAW；两者都没在线时
/// 退回任一记录（路径还在库里，写出去总能对上）。
#[must_use]
pub fn sidecar_rel_path(files: &[FileRow]) -> Option<String> {
    let pick = |role: &str| {
        files
            .iter()
            .filter(|file| file.role == role && file.ext != "xmp")
            .find(|file| !file.missing)
            .or_else(|| {
                files
                    .iter()
                    .find(|file| file.role == role && file.ext != "xmp")
            })
            .map(|file| file.rel_path.clone())
    };
    let rel = pick("bitmap").or_else(|| pick("raw"))?;
    rel_sidecar_of(&rel)
}

/// 由一个**库内相对路径**推出 sidecar 的相对路径（`_RAW/` 折算 + 去扩展名 + `.xmp`）。
///
/// 单独公开是因为删除照片（`store::delete`）与导入采纳拿到的就是文件路径，而不是文件行。
#[must_use]
pub fn rel_sidecar_of(rel_path: &str) -> Option<String> {
    let (dir, name) = rel_path.rsplit_once('/')?;
    let stem = crate::media::kind::extension(name)
        .map_or(name, |ext| &name[..name.len() - ext.len() - 1]);
    if stem.is_empty() {
        return None;
    }
    let dir = normalize_raw_dir(dir);
    Some(if dir.is_empty() {
        format!("{stem}.xmp")
    } else {
        format!("{dir}/{stem}.xmp")
    })
}

/// 在磁盘上按「同目录 + 主体名（大小写折叠）」探测 sidecar 的**绝对路径**。
///
/// 导入采纳用：源目录可能是任意文件系统，Windows/macOS 天然不敏感，Linux 要扫一遍目录项。
/// 找不到返回 `None`（大多数照片旁边本来就没有 sidecar，这是常态而不是错误）。
#[must_use]
pub fn probe_sidecar(file_abs: &std::path::Path) -> Option<std::path::PathBuf> {
    let dir = file_abs.parent()?;
    let name = file_abs.file_name()?.to_string_lossy().into_owned();
    let stem = crate::media::kind::extension(&name)
        .map_or(name.as_str(), |ext| &name[..name.len() - ext.len() - 1]);
    let wanted = format!("{}.xmp", crate::media::kind::stem_folded(&format!("{stem}.xmp")));
    let direct = dir.join(format!("{stem}.xmp"));
    if direct.is_file() {
        return Some(direct);
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if crate::media::kind::stem_folded(&file_name) == wanted {
            let path = entry.path();
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(role: &str, rel: &str, ext: &str, missing: bool) -> FileRow {
        FileRow {
            row_id: 0,
            asset_id: 1,
            rel_path: rel.to_string(),
            rel_path_folded: rel.to_lowercase(),
            role: role.to_string(),
            ext: ext.to_string(),
            size_bytes: None,
            mtime_ms: None,
            file_created_ms: None,
            identity: None,
            missing,
        }
    }

    #[test]
    fn bitmap_wins_and_raw_dir_is_folded() {
        let files = [
            file("bitmap", "photos/2026/MYP0001.JPG", "jpg", false),
            file("raw", "photos/2026/_RAW/MYP0001.ORF", "orf", false),
        ];
        assert_eq!(
            sidecar_rel_path(&files),
            Some("photos/2026/MYP0001.xmp".to_string())
        );
    }

    #[test]
    fn raw_only_sits_in_the_normal_dir() {
        let files = [file("raw", "photos/2026/MYP0001.ORF", "orf", false)];
        assert_eq!(
            sidecar_rel_path(&files),
            Some("photos/2026/MYP0001.xmp".to_string())
        );
    }

    #[test]
    fn lone_raw_inside_raw_dir_folds_to_parent() {
        let files = [file("raw", "photos/2026/_RAW/a.RW2", "rw2", false)];
        assert_eq!(
            sidecar_rel_path(&files),
            Some("photos/2026/a.xmp".to_string())
        );
    }

    #[test]
    fn raw_dir_match_is_case_insensitive_and_only_last_segment() {
        // 大小写不同的 _raw 也要折
        assert_eq!(
            rel_sidecar_of("photos/2026/_raw/b.NEF"),
            Some("photos/2026/b.xmp".to_string())
        );
        // 中间一段叫 _RAW 的是用户自己的目录，不折
        assert_eq!(
            rel_sidecar_of("photos/_RAW/2026/c.JPG"),
            Some("photos/_RAW/2026/c.xmp".to_string())
        );
    }

    #[test]
    fn unicode_and_extensionless_names() {
        assert_eq!(
            rel_sidecar_of("photos/旅行/照片 甲.CR3"),
            Some("photos/旅行/照片 甲.xmp".to_string())
        );
        assert_eq!(rel_sidecar_of("photos/nofile"), None);
    }

    #[test]
    fn missing_bitmap_falls_back_to_raw() {
        let files = [
            file("bitmap", "photos/a.JPG", "jpg", true),
            file("raw", "photos/_RAW/a.ORF", "orf", false),
        ];
        assert_eq!(sidecar_rel_path(&files), Some("photos/a.xmp".to_string()));
    }

    #[test]
    fn no_files_means_no_sidecar() {
        assert_eq!(sidecar_rel_path(&[]), None);
    }

    #[test]
    fn probe_finds_exact_and_case_folded_matches() {
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("IMG_0001.CR3");
        std::fs::write(&photo, b"x").unwrap();
        // 没有时 → None
        assert!(probe_sidecar(&photo).is_none());
        // 精确名
        let sc = dir.path().join("IMG_0001.xmp");
        std::fs::write(&sc, b"<x/>").unwrap();
        assert_eq!(probe_sidecar(&photo), Some(sc.clone()));
        std::fs::remove_file(&sc).unwrap();
        // 折叠名（Linux 上大小写敏感的场景）
        let folded = dir.path().join("img_0001.xmp");
        std::fs::write(&folded, b"<x/>").unwrap();
        assert_eq!(probe_sidecar(&photo), Some(folded));
    }

    #[test]
    fn probe_folds_raw_dir_when_looking() {
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("_RAW").join("IMG_0001.CR3");
        std::fs::create_dir_all(raw.parent().unwrap()).unwrap();
        std::fs::write(&raw, b"x").unwrap();
        let sc = dir.path().join("IMG_0001.xmp");
        std::fs::write(&sc, b"<x/>").unwrap();
        assert_eq!(probe_sidecar(&raw), Some(sc));
    }
}
