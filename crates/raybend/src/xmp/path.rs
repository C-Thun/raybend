//! Sidecar 的**路径规则**（`specs/xmp-sidecar.md` §3.1）—— 全项目唯一一处实现。
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
    let pick = |role: &str, online_only: bool| {
        files.iter().find(|file| {
            file.role == role
                && !file.ext.eq_ignore_ascii_case("xmp")
                && (!online_only || !file.missing)
        })
    };
    let file = pick("bitmap", true)
        .or_else(|| pick("raw", true))
        .or_else(|| pick("bitmap", false))
        .or_else(|| pick("raw", false))?;
    rel_sidecar_of(&file.rel_path)
}

/// 由一个**库内相对路径**推出 sidecar 的相对路径（`_RAW/` 折算 + 去扩展名 + `.xmp`）。
///
/// 单独公开是因为删除照片（`store::delete`）与导入采纳拿到的就是文件路径，而不是文件行。
#[must_use]
pub fn rel_sidecar_of(rel_path: &str) -> Option<String> {
    let (dir, name) = rel_path.rsplit_once('/').unwrap_or(("", rel_path));
    let stem =
        crate::media::kind::extension(name).map_or(name, |ext| &name[..name.len() - ext.len() - 1]);
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
    // 照片在 `_RAW/` 里时，sidecar 在它的上一层（`specs/xmp-sidecar.md` §3.1 的折算规则）
    let mut dir = file_abs.parent()?.to_path_buf();
    if dir
        .file_name()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("_RAW"))
        && dir.parent().is_some()
    {
        dir = dir.parent()?.to_path_buf();
    }
    let name = file_abs.file_name()?.to_string_lossy().into_owned();
    // 照片的折叠主体名（不带扩展名）——与位图/RAW 配对同一套口径
    let stem = crate::media::kind::extension(&name)
        .map_or(name.as_str(), |ext| &name[..name.len() - ext.len() - 1]);
    let direct = dir.join(format!("{stem}.xmp"));
    resolve_sidecar(&direct).ok().flatten()
}

/// 解析实际存在的 sidecar 路径。读回、发布、清空和回收共用大小写折叠规则。
/// 文件系统错误向上传递，避免读取失败后误当作「不存在」另建文件。
pub fn resolve_sidecar(path: &std::path::Path) -> crate::Result<Option<std::path::PathBuf>> {
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() => return Ok(Some(path.to_path_buf())),
        Ok(_) => return Err(crate::Error::Unsupported("sidecar 路径不是文件".into())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let dir = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let wanted_stem =
        crate::media::kind::stem_folded(&path.file_name().unwrap_or_default().to_string_lossy());
    let mut matches = Vec::new();
    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().into_owned();
        // 只认 .xmp，且主体名（折叠后）与照片一致
        if crate::media::kind::extension(&file_name).as_deref() == Some("xmp")
            && crate::media::kind::stem_folded(&file_name) == wanted_stem
        {
            let path = entry.path();
            if path.is_file() {
                matches.push(path);
            }
        }
    }
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.pop()),
        _ => Err(crate::Error::Unsupported(
            "多个大小写折叠同名 sidecar，保留文件等待消除歧义".into(),
        )),
    }
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
        // 无扩展名的文件：主体名就是全名（罕见，但行为要确定）
        assert_eq!(
            rel_sidecar_of("photos/nofile"),
            Some("photos/nofile.xmp".to_string())
        );
    }

    #[test]
    fn missing_bitmap_falls_back_to_raw() {
        let files = [
            file("bitmap", "photos/offline/a.JPG", "jpg", true),
            file("raw", "photos/online/_RAW/a.ORF", "orf", false),
        ];
        assert_eq!(
            sidecar_rel_path(&files),
            Some("photos/online/a.xmp".to_string())
        );
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

    #[test]
    fn bare_empty_and_long_unicode_relative_paths() {
        assert_eq!(rel_sidecar_of(""), None);
        assert_eq!(rel_sidecar_of("照片.JPG").as_deref(), Some("照片.xmp"));
        let dir = "旅行/".repeat(200);
        assert_eq!(
            rel_sidecar_of(&format!("{dir}照片.CR3")),
            Some(format!("{dir}照片.xmp"))
        );
        let files = [file("sidecar", "photos/a.xmp", "xmp", false)];
        assert_eq!(sidecar_rel_path(&files), None);
    }

    #[test]
    fn fully_offline_asset_keeps_a_sidecar_location() {
        let files = [
            file("bitmap", "photos/a.jpg", "jpg", true),
            file("raw", "photos/_RAW/a.orf", "orf", true),
        ];
        assert_eq!(sidecar_rel_path(&files).as_deref(), Some("photos/a.xmp"));
    }

    #[cfg(not(windows))]
    #[test]
    fn ambiguous_folded_names_are_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("PHOTO.XMP"), "a").unwrap();
        std::fs::write(dir.path().join("photo.XMP"), "b").unwrap();
        assert!(resolve_sidecar(&dir.path().join("Photo.xmp")).is_err());
    }
}
