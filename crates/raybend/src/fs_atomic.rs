//! 派生文件共用的原子写入：独立临时文件，发布失败时清理。

use crate::Result;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_WRITE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        NEXT_WRITE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        drop(file);
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

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
