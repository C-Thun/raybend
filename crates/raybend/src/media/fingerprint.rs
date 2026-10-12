//! 内容指纹：导入判重的第三级判据（`specs/import-dedup-fingerprint.md` §5）。
//!
//! 两级口径，**由媒体类型决定、不随大小变**（比较要求口径相同 —— 口径随大小变的话
//! 同一份内容的两个副本会算出两种口径，永远比不出相等）：
//!
//! * [`HashKind::Full`]（`sha256-full`）—— 整文件字节。位图走这条：文件不大，
//!   而且刚复制完那一份就在页缓存里；
//! * [`HashKind::Sample`]（`sha256-sample`）—— `size ‖ head 1 MiB ‖ mid 1 MiB ‖ tail 1 MiB`。
//!   RAW 及其它大文件走这条：单张 10–25 MB，全文哈希换不来判重收益，
//!   采样只读 3 MiB，正好躲开「导入要最快」这条红线（`AGENTS.md` §2.24）。
//!
//! 算法用仓库已有的 `sha2`（`fs_asset` 那两个是给 AI 包校验用的，口径不同，别混）。
//!
//! 指纹只用于**判等**，不是安全边界：采样口径理论上存在「采样一致而内容不同」的
//! 可能性（大小也相同、头/中/尾三段都一样）—— 判重把它当同一张，这是刻意的取舍。

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::Result;
use crate::media::kind::MediaKind;

/// 采样指纹每段读多少字节。
pub const SAMPLE_BYTES: u64 = 1024 * 1024;

/// 指纹口径。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HashKind {
    /// 整文件字节的 sha256。
    Full,
    /// `size ‖ head ‖ mid ‖ tail` 的 sha256（文件小于三段之和时就是整文件）。
    Sample,
}

impl HashKind {
    /// 存库用的字面值（`asset_files.content_hash_kind`）。
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "sha256-full",
            Self::Sample => "sha256-sample",
        }
    }

    /// 从库里读回来的字面值；不认识的一律当「没有指纹」。
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "sha256-full" => Some(Self::Full),
            "sha256-sample" => Some(Self::Sample),
            _ => None,
        }
    }

    /// 这种媒体用哪种口径（位图全文，其余采样）。
    #[must_use]
    pub fn for_media(kind: MediaKind) -> Self {
        match kind {
            MediaKind::Image => Self::Full,
            MediaKind::Raw | MediaKind::Other => Self::Sample,
        }
    }
}

/// 一条内容指纹。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentFingerprint {
    /// 口径。
    pub kind: HashKind,
    /// 小写 hex。
    pub hex: String,
}

impl ContentFingerprint {
    /// 拼一条（调用方保证 `hex` 是小写 hex）。
    #[must_use]
    pub fn new(kind: HashKind, hex: impl Into<String>) -> Self {
        Self {
            kind,
            hex: hex.into(),
        }
    }

    /// 两份指纹是不是同一份内容 —— **口径也必须相同**（不同口径的 hex 不可比）。
    #[must_use]
    pub fn same_content(&self, other: &Self) -> bool {
        self.kind == other.kind && self.hex == other.hex
    }
}

/// 算一个文件的指纹（按给定口径）。
///
/// # Errors
/// 文件打不开、读不动（含复制中途被删）。
pub fn fingerprint_path(path: &Path, kind: HashKind) -> Result<ContentFingerprint> {
    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    match kind {
        HashKind::Full => {
            let mut hasher = Sha256::new();
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            Ok(ContentFingerprint::new(kind, hex(hasher.finalize())))
        }
        HashKind::Sample => {
            let mut hasher = Sha256::new();
            hasher.update(size.to_le_bytes());
            for (offset, len) in sample_ranges(size) {
                file.seek(SeekFrom::Start(offset))?;
                let mut buffer = vec![0_u8; usize::try_from(len).unwrap_or(0)];
                file.read_exact(&mut buffer)?;
                hasher.update(&buffer);
            }
            Ok(ContentFingerprint::new(kind, hex(hasher.finalize())))
        }
    }
}

/// 算一段字节的指纹（内存实现与测试用；与 [`fingerprint_path`] 同一套口径）。
#[must_use]
pub fn fingerprint_bytes(bytes: &[u8], kind: HashKind) -> ContentFingerprint {
    let size = bytes.len() as u64;
    let mut hasher = Sha256::new();
    match kind {
        HashKind::Full => hasher.update(bytes),
        HashKind::Sample => {
            hasher.update(size.to_le_bytes());
            for (offset, len) in sample_ranges(size) {
                let start = usize::try_from(offset).unwrap_or(usize::MAX).min(bytes.len());
                let end = start.saturating_add(usize::try_from(len).unwrap_or(0)).min(bytes.len());
                hasher.update(&bytes[start..end]);
            }
        }
    }
    ContentFingerprint::new(kind, hex(hasher.finalize()))
}

/// 采样要读哪几段（`(offset, len)`）。
///
/// 文件不够三段之和时**整文件一把算**（避免三段互相重叠、同一段算两遍）。
fn sample_ranges(size: u64) -> Vec<(u64, u64)> {
    if size <= SAMPLE_BYTES * 3 {
        return vec![(0, size)];
    }
    let mid_start = size / 2 - SAMPLE_BYTES / 2;
    vec![
        (0, SAMPLE_BYTES),
        (mid_start, SAMPLE_BYTES),
        (size - SAMPLE_BYTES, SAMPLE_BYTES),
    ]
}

fn hex(digest: impl AsRef<[u8]>) -> String {
    digest
        .as_ref()
        .iter()
        .fold(String::with_capacity(64), |mut acc, byte| {
            use std::fmt::Write as _;
            let _ = write!(acc, "{byte:02x}");
            acc
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_tmp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rb-fp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn kind_is_decided_by_media_type_not_by_size() {
        assert_eq!(HashKind::for_media(MediaKind::Image), HashKind::Full);
        assert_eq!(HashKind::for_media(MediaKind::Raw), HashKind::Sample);
        assert_eq!(HashKind::parse("sha256-sample"), Some(HashKind::Sample));
        assert_eq!(HashKind::parse("别的"), None);
    }

    #[test]
    fn full_matches_a_known_digest() {
        // `printf 'abc' | sha256sum`
        let bytes = b"abc";
        let fp = fingerprint_bytes(bytes, HashKind::Full);
        assert_eq!(
            fp.hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sample_of_a_small_file_is_the_whole_file_with_the_size_prefix() {
        // 小于三段之和：整文件一把算 —— 口径仍然是 Sample（比较要靠它）。
        let bytes = b"hello";
        let fp = fingerprint_bytes(bytes, HashKind::Sample);
        assert_eq!(fp.kind, HashKind::Sample);
        let mut hasher = Sha256::new();
        hasher.update(5_u64.to_le_bytes());
        hasher.update(bytes);
        assert_eq!(fp.hex, hex(hasher.finalize()));
    }

    #[test]
    fn sample_reads_head_middle_and_tail() {
        // 4 MiB 的文件：采样区 = 头 1 MiB + 中间 1 MiB（1.5–2.5 MiB）+ 尾 1 MiB。
        // 改**采样区之间**（1.2 MiB 处）的字节 → 采样指纹不变；
        // 改中间那 1 MiB 里的字节 → 变。这就是「采样」这两个字的含义。
        let size = 4 * 1024 * 1024_usize;
        let mut bytes = vec![7_u8; size];
        let before = fingerprint_bytes(&bytes, HashKind::Sample);
        bytes[1_200_000] = 9;
        assert_eq!(
            fingerprint_bytes(&bytes, HashKind::Sample),
            before,
            "采样区外的字节不该影响采样指纹"
        );
        bytes[size / 2] = 9;
        assert_ne!(fingerprint_bytes(&bytes, HashKind::Sample), before);
    }

    #[test]
    fn size_prefix_keeps_equal_samples_apart() {
        // 同样的头部内容、不同长度 → 不能算同一份（大小写进摘要）。
        let mut a = vec![1_u8; 1024];
        let b = vec![1_u8; 2048];
        assert_ne!(
            fingerprint_bytes(&a, HashKind::Sample),
            fingerprint_bytes(&b, HashKind::Sample)
        );
        a.push(1);
        assert_ne!(
            fingerprint_bytes(&a, HashKind::Sample),
            fingerprint_bytes(&vec![1_u8; 1024], HashKind::Sample)
        );
    }

    #[test]
    fn path_and_bytes_agree() {
        let bytes = b"same thing".repeat(1000);
        let path = write_tmp("agree.bin", &bytes);
        for kind in [HashKind::Full, HashKind::Sample] {
            assert_eq!(
                fingerprint_path(&path, kind).unwrap(),
                fingerprint_bytes(&bytes, kind)
            );
        }
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn file_larger_than_three_samples_is_read_by_ranges() {
        // 5 MiB 走三段；与整文件不同（否则「采样」就退化成全文了）。
        let bytes: Vec<u8> = (0..5 * 1024 * 1024_u32).map(|i| (i % 251) as u8).collect();
        let path = write_tmp("ranges.bin", &bytes);
        let sampled = fingerprint_path(&path, HashKind::Sample).unwrap();
        let full = fingerprint_path(&path, HashKind::Full).unwrap();
        assert_ne!(sampled.hex, full.hex);
        assert_eq!(sampled, fingerprint_bytes(&bytes, HashKind::Sample));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn an_empty_file_has_a_fingerprint_too() {
        let fp = fingerprint_bytes(&[], HashKind::Sample);
        assert_eq!(fp.kind, HashKind::Sample);
        assert!(!fp.hex.is_empty());
    }

    #[test]
    fn same_content_requires_the_same_kind() {
        let full = ContentFingerprint::new(HashKind::Full, "ab");
        let sample = ContentFingerprint::new(HashKind::Sample, "ab");
        assert!(!full.same_content(&sample), "口径不同不可比");
        assert!(full.same_content(&ContentFingerprint::new(HashKind::Full, "ab")));
    }
}
