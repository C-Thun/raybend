//! 派生文件共用的原子写入：独立临时文件，发布失败时清理。

use crate::Result;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_WRITE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    write_impl(path, bytes, false)
}

/// Atomic publication without overwriting any existing path, including racing writers.
pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    write_impl(path, bytes, true)
}

fn write_impl(path: &Path, bytes: &[u8], no_replace: bool) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        NEXT_WRITE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut created = false;
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        created = true;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if no_replace {
            // Both names live in one directory/filesystem. hard_link fails if destination exists.
            publish_new(&temp, path)?;
            Ok(())
        } else {
            std::fs::rename(&temp, path)
        }
    })();
    if result.is_err() && created {
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(Into::into)
}

/// Windows MoveFileEx without REPLACE_EXISTING is atomic and works on NTFS and
/// removable filesystems without hard links. Unix hard-link publication retains
/// the no-overwrite primitive used by the existing implementation.
#[cfg(windows)]
fn publish_new(temp: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let temp: Vec<_> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<_> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: both paths are owned, null-terminated UTF-16 buffers lasting through the call.
    if unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileExW(temp.as_ptr(), target.as_ptr(), 0)
    } == 0
    {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}
#[cfg(not(windows))]
fn publish_new(temp: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::hard_link(temp, target)?;
    let _ = std::fs::remove_file(temp);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_replace_has_one_winner_and_cleans_all_temporary_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("新图.tiff");
        let won = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for value in 0..8u8 {
                let path = &path;
                let won = &won;
                scope.spawn(move || {
                    if write_new(path, &[value; 4096]).is_ok() {
                        won.fetch_add(1, Ordering::SeqCst);
                    }
                });
            }
        });
        assert_eq!(won.load(Ordering::SeqCst), 1);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), 4096);
        assert!(bytes.iter().all(|v| *v == bytes[0]));
        assert!(write_new(&path, b"overwrite").is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn overwrite_and_concurrent_writes_publish_complete_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("中文/sub/cache.webp");
        write(&path, b"old").unwrap();
        std::thread::scope(|scope| {
            for value in 0..8_u8 {
                let path = &path;
                scope.spawn(move || write(path, &[value; 4096]).unwrap());
            }
        });
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), 4096);
        assert!(bytes.iter().all(|value| *value == bytes[0]));
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
        write(&path, b"").unwrap();
        assert!(std::fs::read(&path).unwrap().is_empty());
    }

    #[test]
    fn failed_publish_cleans_temp_without_destroying_destination() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("directory");
        std::fs::create_dir(&path).unwrap();
        assert!(write(&path, b"bytes").is_err());
        assert!(path.is_dir());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
