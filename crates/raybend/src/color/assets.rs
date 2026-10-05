//! RGB ICC 文件资源的受限导入与重读。系统 ICC 是只读事实，用户导入的资产在此复制保全。

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

use super::ProfileId;
use super::icc::{
    IccError, IccRole, MAX_ICC_BYTES, RgbIcc, input_rgb16_to_working, working_to_output_rgb16,
};

fn profile_error(error: IccError) -> Error {
    Error::Unsupported(format!("ICC 配置文件无效：{error}"))
}

fn validate_extension(path: &Path) -> Result<()> {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "icc" | "icm"))
    {
        return Err(Error::Unsupported("仅支持 .icc/.icm 配置文件".into()));
    }
    Ok(())
}

fn parse_asset(bytes: &[u8]) -> Result<RgbIcc> {
    let profile = RgbIcc::parse(bytes, IccRole::PhotoInput)
        .or_else(|error| {
            if error == IccError::WrongRole {
                RgbIcc::parse(bytes, IccRole::RgbOutput)
            } else {
                Err(error)
            }
        })
        .map_err(profile_error)?;
    use lcms2::ProfileClassSignature as Class;
    if matches!(
        profile.class(),
        Class::InputClass | Class::DisplayClass | Class::ColorSpaceClass
    ) {
        input_rgb16_to_working(&profile, &[[12345, 32768, 60000]]).map_err(profile_error)?;
    }
    if matches!(
        profile.class(),
        Class::OutputClass | Class::DisplayClass | Class::ColorSpaceClass
    ) {
        working_to_output_rgb16(&profile, &[[0.18, 0.5, 0.95]]).map_err(profile_error)?;
    }
    Ok(profile)
}

#[must_use]
pub fn asset_path(root: &Path, id: &ProfileId) -> PathBuf {
    root.join(format!("{}.icc", id.as_str()))
}

/// 读取用户明确选择的单份文件快照，校验后以内容名无覆盖发布。
/// 已存在时检查原件字节身份；不能用新内容覆盖旧 issue 引用。
pub fn import_file(source: &Path, root: &Path) -> Result<RgbIcc> {
    validate_extension(source)?;
    let bytes = crate::fs_asset::read_limited(source, MAX_ICC_BYTES as u64, "ICC")?;
    let profile = parse_asset(&bytes)?;
    let target = asset_path(root, profile.id());
    match crate::fs_atomic::write_new(&target, &bytes) {
        Ok(()) => Ok(profile),
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = crate::fs_asset::hash_file(&target, MAX_ICC_BYTES as u64, "ICC")?;
            if existing != profile.id().as_str() {
                return Err(Error::Unsupported(
                    "应用内 ICC 原件与内容身份不符，需要修复后重试".into(),
                ));
            }
            Ok(profile)
        }
        Err(error) => Err(error),
    }
}

/// 每次用于精确成片时重读并校验；目录列表只做轻量存在性检查。
pub fn resolve_file(root: &Path, id: &ProfileId, role: IccRole) -> Result<RgbIcc> {
    let profile = read_profile(&asset_path(root, id), role)?;
    if profile.id() != id {
        return Err(Error::Unsupported("ICC 原件内容身份已变化".into()));
    }
    Ok(profile)
}

/// The same bounded reader serves imported assets and read-only system profiles.
pub fn read_profile(path: &Path, role: IccRole) -> Result<RgbIcc> {
    let bytes = crate::fs_asset::read_limited(path, MAX_ICC_BYTES as u64, "ICC")?;
    RgbIcc::parse(&bytes, role).map_err(profile_error)
}

/// Built-in resources share the same portable SHA identity as imported files.
/// Hidden imported resources remain resolvable for already-frozen references.
pub fn resolve_profile(root: &Path, id: &ProfileId, role: IccRole) -> Result<RgbIcc> {
    for profile in [super::icc::srgb_icc(), super::icc::display_p3_icc(), super::icc::adobe_rgb_icc()] {
        let profile = profile.map_err(profile_error)?;
        if profile.id() == id {
            if !profile.accepts_role(role) { return Err(profile_error(IccError::WrongRole)); }
            return Ok(profile);
        }
    }
    resolve_file(root, id, role)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::icc::{display_p3_icc, srgb_icc};

    #[test]
    fn import_keeps_original_bytes_after_source_moves_and_checks_identity() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("摄影棚.ICM");
        let root = dir.path().join("profile library");
        let icc = srgb_icc().unwrap();
        std::fs::write(&source, icc.bytes()).unwrap();
        let stored = import_file(&source, &root).unwrap();
        assert_eq!(stored.id(), icc.id());
        assert_eq!(import_file(&source, &root).unwrap().id(), icc.id());
        std::fs::remove_file(source).unwrap();
        assert_eq!(
            resolve_file(&root, icc.id(), IccRole::PhotoInput)
                .unwrap()
                .id(),
            icc.id()
        );
        std::fs::write(
            asset_path(&root, icc.id()),
            display_p3_icc().unwrap().bytes(),
        )
        .unwrap();
        assert!(resolve_file(&root, icc.id(), IccRole::PhotoInput).is_err());
    }

    #[test]
    fn invalid_extension_and_over_limit_leave_library_empty() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("profiles");
        let wrong = dir.path().join("profile.txt");
        std::fs::write(&wrong, srgb_icc().unwrap().bytes()).unwrap();
        assert!(import_file(&wrong, &root).is_err());
        let oversized = dir.path().join("big.icc");
        std::fs::write(&oversized, vec![0; MAX_ICC_BYTES + 1]).unwrap();
        assert!(import_file(&oversized, &root).is_err());
        assert!(!root.exists());
    }
}
