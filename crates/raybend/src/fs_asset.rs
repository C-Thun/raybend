//! 外部资源导入共用的受限字节快照、内容身份与原子发布。
//! 格式校验由 LUT / ICC 各自的解析器承担，不能在这里互相猜文件语义。

use crate::error::{Error, Result};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

pub(crate) fn read_limited(path: &Path, limit: u64, kind: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(Error::Unsupported(format!("{kind} 文件超过大小上限")));
    }
    Ok(bytes)
}

pub(crate) fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn hash_file(path: &Path, limit: u64, kind: &str) -> Result<String> {
    hash_reader(std::fs::File::open(path)?, limit, kind)
}

pub(crate) fn hash_reader(reader: impl Read, limit: u64, kind: &str) -> Result<String> {
    let mut reader = reader.take(limit.saturating_add(1));
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 32 * 1024];
    let mut length = 0_u64;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        length += count as u64;
        if length > limit {
            return Err(Error::Unsupported(format!("{kind} 文件超过大小上限")));
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(crate) fn copy_snapshot(
    source: &Path,
    target: &Path,
    limit: u64,
    kind: &str,
) -> Result<String> {
    let bytes = read_limited(source, limit, kind)?;
    let expected = hash_bytes(&bytes);
    crate::fs_atomic::write(target, &bytes)?;
    let actual = hash_file(target, limit, kind)?;
    if actual != expected {
        return Err(Error::Unsupported(format!(
            "{kind} 资源副本写入后身份不一致"
        )));
    }
    Ok(actual)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_hash_and_limits_are_shared_without_reinterpreting_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("中文.icc");
        let target = dir.path().join("库/配置.icc");
        std::fs::write(&source, b"\0abc\xff").unwrap();
        assert_eq!(read_limited(&source, 5, "ICC").unwrap(), b"\0abc\xff");
        let hash = copy_snapshot(&source, &target, 5, "ICC").unwrap();
        assert_eq!(hash, hash_bytes(b"\0abc\xff"));
        assert_eq!(hash_file(&target, 5, "ICC").unwrap(), hash);
        assert!(read_limited(&source, 4, "ICC").is_err());
        assert!(hash_file(&source, 4, "ICC").is_err());
        assert!(copy_snapshot(&source, &target, 4, "ICC").is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"\0abc\xff");
    }
}
