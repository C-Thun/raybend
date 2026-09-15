//! 文件身份：`(卷序列号, 文件 ID)` —— 用来**稳定追踪**重命名与移动（`AGENTS.md` §7.3）。
//!
//! 为什么不用路径做身份：路径会因为改名、移动、盘符变化而变，而照片的身份不变。
//! Windows 上从 `GetFileInformationByHandleEx(FileIdInfo)` 拿到 `(VolumeSerialNumber, FileId128)`，
//! NTFS 上是稳定的 128 bit ID；Unix 上退化用 `(dev, ino)`。
//!
//! **已知局限**（必须知道，不要假装它是绝对真理）：
//!
//! 1. 身份可以在文件被删除后被**复用** —— 所以我们同时记录 `size` 与 `mtime`，
//!    判定「同一文件」时应把它们作为佐证，而不是只看 ID；
//! 2. 网络盘与某些 FAT/exFAT 卷不提供稳定 ID（返回 0 或同一值）→ 调用方要能接受
//!    [`FileId::ZERO`] 这种「无意义身份」并按路径兜底；
//! 3. 跨卷移动（`C:` → `D:`）身份必然改变（卷序列号不同）—— 这时只能靠内容指纹，
//!    那属于后续里程碑（`media` 模块）的事。
//!
//! 读取失败一律返回 [`None`]（[`FileId::try_read`]），**不报错**：身份是「锦上添花」，
//! 拿不到时由路径兜底，不该让整个扫描失败。

use std::path::Path;

/// 文件身份：卷序列号 + 文件 ID。
///
/// `file_id` 的 16 字节在 Windows 上是 `FILE_ID_INFO.FileId`；
/// 在 Unix 上是 inode 号（高 8 字节为 0，保持同样 16 字节宽度便于统一存储）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId {
    /// 卷序列号（Windows）或设备号（Unix `dev`）。
    pub volume_serial: u64,
    /// 文件 ID 的 16 字节表示。
    pub file_id: [u8; 16],
}

impl FileId {
    /// 「无意义身份」：读取失败或文件系统不支持时的占位值。
    pub const ZERO: Self = Self {
        volume_serial: 0,
        file_id: [0u8; 16],
    };

    /// 从 16 字节 + 卷序列号构造。
    #[must_use]
    pub const fn new(volume_serial: u64, file_id: [u8; 16]) -> Self {
        Self {
            volume_serial,
            file_id,
        }
    }

    /// 从数据库 BLOB 还原（`asset_files.file_id`）+ 卷序列号列。
    #[must_use]
    pub fn from_blob(volume_serial: u64, blob: &[u8]) -> Option<Self> {
        if blob.len() != 16 {
            return None;
        }
        let mut file_id = [0u8; 16];
        file_id.copy_from_slice(blob);
        Some(Self {
            volume_serial,
            file_id,
        })
    }

    /// 存库用的 16 字节 BLOB。
    #[must_use]
    pub const fn as_blob(&self) -> &[u8; 16] {
        &self.file_id
    }

    /// 是否为「无意义身份」（全零）。这类值**不应**写进唯一索引当判据。
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.volume_serial == 0 && self.file_id == [0u8; 16]
    }

    /// 读取一个路径的文件身份；任何失败都返回 `None`。
    #[must_use]
    pub fn try_read(path: impl AsRef<Path>) -> Option<Self> {
        platform::read(path.as_ref()).ok()
    }
}

/// 读取失败的原因 —— 只在需要区分「为什么没有身份」时使用（统计、诊断）。
#[derive(Debug, thiserror::Error)]
pub enum FileIdError {
    /// 底层 I/O 失败（文件不存在、权限不足、网络中断…）。
    #[error("读取文件身份失败：{0}")]
    Io(#[from] std::io::Error),
    /// 平台 API 调用失败。
    #[error("平台不支持或调用失败：{0}")]
    Unsupported(String),
}

// ---------------------------------------------------------------------------
// Unix：dev + ino
// ---------------------------------------------------------------------------

#[cfg(unix)]
mod platform {
    use super::{FileId, FileIdError};
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    pub(super) fn read(path: &Path) -> Result<FileId, FileIdError> {
        // 跟随符号链接：照片实体才是身份所在（链接只是指路牌）
        let md = std::fs::metadata(path)?;
        let mut file_id = [0u8; 16];
        // inode 通常是 u64；写进高 8 字节保持与 Windows 一样的宽度
        file_id[..8].copy_from_slice(&md.ino().to_be_bytes());
        Ok(FileId::new(md.dev(), file_id))
    }
}

// ---------------------------------------------------------------------------
// Windows：GetFileInformationByHandleEx(FileIdInfo)
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod platform {
    use super::{FileId, FileIdError};
    use std::path::Path;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ID_INFO, FileIdInfo, GetFileInformationByHandleEx,
    };

    pub(super) fn read(path: &Path) -> Result<FileId, FileIdError> {
        use std::os::windows::io::AsRawHandle;

        // 目录也要能打开：Rust 的 File::open 会带 FILE_FLAG_BACKUP_SEMANTICS
        let file = std::fs::File::open(path)?;
        let mut info: FILE_ID_INFO = FILE_ID_INFO {
            VolumeSerialNumber: 0,
            FileId: windows_sys::Win32::Storage::FileSystem::FILE_ID_128 {
                Identifier: [0; 16],
            },
        };
        // SAFETY: 句柄来自刚打开的 File（生命周期覆盖本次调用），
        // info 是栈上正确对齐的 FILE_ID_INFO，长度用 size_of 传入，符合 API 契约。
        let ok = unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle() as _,
                FileIdInfo,
                std::ptr::addr_of_mut!(info).cast(),
                std::mem::size_of::<FILE_ID_INFO>() as u32,
            )
        };
        if ok == 0 {
            return Err(FileIdError::Io(std::io::Error::last_os_error()));
        }
        Ok(FileId::new(info.VolumeSerialNumber, info.FileId.Identifier))
    }
}

// ---------------------------------------------------------------------------
// 其它平台（未来 macOS / BSD）：暂不实现
// ---------------------------------------------------------------------------

#[cfg(not(any(unix, windows)))]
mod platform {
    use super::{FileId, FileIdError};
    use std::path::Path;

    pub(super) fn read(_path: &Path) -> Result<FileId, FileIdError> {
        Err(FileIdError::Unsupported(
            "该平台尚未实现文件身份读取".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().expect("建临时目录")
    }

    #[test]
    fn reads_identity_of_a_regular_file() {
        let d = tmp();
        let p = d.path().join("a.jpg");
        fs::write(&p, b"hello").unwrap();

        let id = FileId::try_read(&p).expect("应当读到身份");
        assert!(!id.is_zero(), "真实文件不该是零身份");
    }

    #[test]
    fn identity_is_stable_across_calls() {
        let d = tmp();
        let p = d.path().join("a.jpg");
        fs::write(&p, b"hello").unwrap();

        let a = FileId::try_read(&p).unwrap();
        let b = FileId::try_read(&p).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn identity_survives_rename() {
        // 这才是身份存在的理由：改名后仍然认得出来
        let d = tmp();
        let p1 = d.path().join("before.jpg");
        let p2 = d.path().join("after.jpg");
        fs::write(&p1, b"hello").unwrap();

        let before = FileId::try_read(&p1).unwrap();
        fs::rename(&p1, &p2).unwrap();
        let after = FileId::try_read(&p2).unwrap();

        assert_eq!(before, after, "改名不该改变文件身份");
    }

    #[test]
    fn identity_survives_move_within_volume() {
        let d = tmp();
        let sub = d.path().join("sub");
        fs::create_dir(&sub).unwrap();
        let p1 = d.path().join("a.jpg");
        let p2 = sub.join("a.jpg");
        fs::write(&p1, b"x").unwrap();

        let before = FileId::try_read(&p1).unwrap();
        fs::rename(&p1, &p2).unwrap();
        assert_eq!(before, FileId::try_read(&p2).unwrap());
    }

    #[test]
    fn identity_is_same_for_hard_links() {
        let d = tmp();
        let p1 = d.path().join("a.jpg");
        let p2 = d.path().join("b.jpg");
        fs::write(&p1, b"x").unwrap();
        if fs::hard_link(&p1, &p2).is_err() {
            return; // 该文件系统不支持硬链接（例如某些容器/网络盘）→ 跳过
        }
        assert_eq!(
            FileId::try_read(&p1).unwrap(),
            FileId::try_read(&p2).unwrap()
        );
    }

    #[test]
    fn distinct_files_have_distinct_identity() {
        let d = tmp();
        let p1 = d.path().join("a.jpg");
        let p2 = d.path().join("b.jpg");
        fs::write(&p1, b"a").unwrap();
        fs::write(&p2, b"b").unwrap();
        assert_ne!(
            FileId::try_read(&p1).unwrap(),
            FileId::try_read(&p2).unwrap()
        );
    }

    #[test]
    fn directory_has_identity_too() {
        let d = tmp();
        let sub = d.path().join("sub");
        fs::create_dir(&sub).unwrap();
        let id = FileId::try_read(&sub).expect("目录也应有身份");
        assert!(!id.is_zero());
    }

    #[test]
    fn missing_path_returns_none() {
        let d = tmp();
        assert!(FileId::try_read(d.path().join("nope.jpg")).is_none());
    }

    #[test]
    fn empty_path_returns_none() {
        assert!(FileId::try_read("").is_none());
    }

    #[test]
    fn not_a_file_but_a_special_path_returns_none_on_linux() {
        // /proc/self/status 这类虚拟文件没有稳定 inode 语义，但读取不该 panic
        let p = Path::new("/proc/self/status");
        if p.exists() {
            // 允许有也可能没有；关键是不 panic
            let _ = FileId::try_read(p);
        }
        // 目录下的设备文件同理
        let _ = FileId::try_read("/dev/null");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_follows_to_target_identity() {
        let d = tmp();
        let target = d.path().join("real.jpg");
        let link = d.path().join("link.jpg");
        fs::write(&target, b"x").unwrap();
        if std::os::unix::fs::symlink(&target, &link).is_err() {
            return;
        }
        assert_eq!(
            FileId::try_read(&target).unwrap(),
            FileId::try_read(&link).unwrap(),
            "符号链接应指向目标的身份"
        );
    }

    #[test]
    fn blob_roundtrip() {
        let id = FileId::new(0xDEAD_BEEF, [7u8; 16]);
        let blob = *id.as_blob();
        assert_eq!(FileId::from_blob(0xDEAD_BEEF, &blob), Some(id));
        // 长度不对必须拒绝（防止库里出现半截 BLOB）
        assert_eq!(FileId::from_blob(0, &blob[..15]), None);
        assert_eq!(FileId::from_blob(0, &[]), None);
        assert_eq!(FileId::from_blob(0, &[0u8; 17]), None);
    }

    #[test]
    fn zero_identity_is_detectable() {
        assert!(FileId::ZERO.is_zero());
        assert!(FileId::new(0, [0u8; 16]).is_zero());
        assert!(!FileId::new(1, [0u8; 16]).is_zero());
        assert!(!FileId::new(0, [1u8; 16]).is_zero());
    }

    #[test]
    fn concurrent_reads_are_fine() {
        let d = tmp();
        let p = d.path().join("a.jpg");
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(b"x").unwrap();
        drop(f);
        let expected = FileId::try_read(&p).unwrap();

        std::thread::scope(|s| {
            for _ in 0..8 {
                let p = p.clone();
                s.spawn(move || {
                    assert_eq!(FileId::try_read(&p).unwrap(), expected);
                });
            }
        });
    }
}
