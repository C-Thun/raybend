//! Fixed App Paths and bounded known-directory patterns. No installed-program/full-disk enumeration.
use crate::{Error, Result};
use std::{
    ffi::OsStr,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};
pub fn discover() -> Vec<super::Application> {
    let mut apps = Vec::new();
    for candidate in super::CANDIDATES {
        for path in candidate
            .exes
            .iter()
            .flat_map(|exe| registry(exe))
            .chain(candidate.paths.iter().flat_map(|pattern| known(pattern)))
        {
            apps.push(super::Application {
                name: candidate.name.into(),
                path: path.to_string_lossy().into_owned(),
            });
        }
    }
    apps
}
fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(Some(0)).collect()
}
pub fn registry(exe: &str) -> Vec<PathBuf> {
    use windows_sys::Win32::System::Registry::*;
    let key = wide(format!(
        "Software\\Microsoft\\Windows\\CurrentVersion\\App Paths\\{exe}"
    ));
    let mut found = Vec::new();
    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for view in [RRF_SUBKEY_WOW6464KEY, RRF_SUBKEY_WOW6432KEY] {
            let mut buffer = vec![0u16; 32768];
            let mut size = (buffer.len() * 2) as u32;
            // SAFETY: NUL-terminated key; output buffer and byte count point to live, sized storage.
            let result = unsafe {
                RegGetValueW(
                    hive,
                    key.as_ptr(),
                    std::ptr::null(),
                    RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | view,
                    std::ptr::null_mut(),
                    buffer.as_mut_ptr().cast(),
                    &mut size,
                )
            };
            if result == 0 {
                let length = buffer.iter().position(|x| *x == 0).unwrap_or(buffer.len());
                let value = String::from_utf16_lossy(&buffer[..length]);
                found.push(PathBuf::from(value.trim().trim_matches('"')));
            }
        }
    }
    found
}
fn glob_match(pattern: &str, name: &str) -> bool {
    let pattern = pattern.to_lowercase();
    let name = name.to_lowercase();
    match pattern.split_once('*') {
        None => pattern == name,
        Some((prefix, suffix)) => {
            name.len() >= prefix.len() + suffix.len()
                && name.starts_with(prefix)
                && name.ends_with(suffix)
        }
    }
}
fn expand(base: &Path, pattern: &str) -> Vec<PathBuf> {
    let mut paths = vec![base.to_path_buf()];
    for part in pattern.split('/') {
        if part.contains('*') {
            paths = paths
                .into_iter()
                .flat_map(|base| {
                    std::fs::read_dir(base)
                        .into_iter()
                        .flatten()
                        .take(2048)
                        .filter_map(std::result::Result::ok)
                        .filter(|entry| glob_match(part, &entry.file_name().to_string_lossy()))
                        .map(|entry| entry.path())
                        .collect::<Vec<_>>()
                })
                .take(256)
                .collect();
        } else {
            paths = paths.into_iter().map(|base| base.join(part)).collect();
        }
    }
    paths
}
pub fn known(pattern: &str) -> Vec<PathBuf> {
    if let Some(alias) = pattern.strip_prefix("WindowsApps/") {
        return std::env::var_os("LOCALAPPDATA")
            .map(|root| {
                vec![
                    PathBuf::from(root)
                        .join("Microsoft/WindowsApps")
                        .join(alias),
                ]
            })
            .unwrap_or_default();
    }
    ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
        .into_iter()
        .filter_map(std::env::var_os)
        .flat_map(|root| expand(Path::new(&root), pattern))
        .collect()
}
/// Windows argv quotation (one TIFF argument); no cmd.exe/shell script interpretation.
pub fn quote_arg(arg: &OsStr) -> Vec<u16> {
    let mut result = vec![b'"' as u16];
    let mut slashes = 0;
    for unit in arg.encode_wide() {
        if unit == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        if unit == b'"' as u16 {
            result.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2 + 1));
        } else {
            result.extend(std::iter::repeat_n(b'\\' as u16, slashes));
        }
        slashes = 0;
        result.push(unit);
    }
    result.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    result.extend([b'"' as u16, 0]);
    result
}
pub fn launch(application: &Path, file: &Path) -> Result<()> {
    let verb = wide("open");
    let executable = wide(application);
    let argument = quote_arg(file.as_os_str());
    // SAFETY: all strings stay alive and are NUL-terminated. Shell owns no supplied storage.
    let result = unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            executable.as_ptr(),
            argument.as_ptr(),
            std::ptr::null(),
            windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        )
    } as isize;
    if result <= 32 {
        Err(Error::Unsupported(format!(
            "外部应用启动失败（Windows {result}）"
        )))
    } else {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_alias_and_argv_with_unicode_quotes() {
        assert!(glob_match("Adobe Photoshop*", "Adobe Photoshop 2026"));
        assert!(!glob_match("Adobe Photoshop*", "Adobe Premiere"));
        let units = quote_arg(OsStr::new("C:\\照片\\a b.tiff"));
        assert_eq!(
            String::from_utf16_lossy(&units[..units.len() - 1]),
            "\"C:\\照片\\a b.tiff\""
        );
        let units = quote_arg(OsStr::new("x\\\"y\\"));
        assert_eq!(
            String::from_utf16_lossy(&units[..units.len() - 1]),
            "\"x\\\\\\\"y\\\\\""
        );
    }
}
