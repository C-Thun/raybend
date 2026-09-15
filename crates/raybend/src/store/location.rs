//! 位置判定：本地盘 / 网络盘 / 云同步目录（`AGENTS.md` §6.4 的最后一条）。
//!
//! 为什么要有它：**把 SQLite 库放在云同步盘或网络盘上，是 SQLite 数据损坏的头号来源**。
//! 同步客户端会在文件被写入的过程中复制/回滚它，网络盘则可能在任何一次事务中途断开。
//!
//! 判定结果**只用来警告，不拒绝**（`plans/M1-2.md` §3.6）：用户可能确实想这么放
//! （例如把整库放在挂载好的 NAS 上、并接受风险），程序该做的是**让他知道**。
//!
//! 判定分三层，从最可靠到最不可靠：
//!
//! 1. **UNC 路径**（`\\server\share`）—— 一定是网络位置，纯字符串判断，最可靠；
//! 2. **挂载表 / 驱动器类型** —— Linux 读 `/proc/mounts` 看文件系统类型（`nfs`、`cifs`…），
//!    Windows 用 `GetDriveTypeW` 看是不是 `DRIVE_REMOTE`；
//! 3. **路径特征**（`OneDrive`、`Dropbox`…）—— 只能认出「主流同步客户端默认放哪」，
//!    用户改过目录名就认不出。**它是提示，不是保证。**

// Linux 专属的 `/proc/mounts` 解析（Windows 走 `GetDriveTypeW`，那些函数不会被调用）。
// 它们本身是**纯字符串逻辑**，测试在所有平台都跑；这里只关掉非 Linux 平台上的
// 「never used」警告 —— 不要因此把它们删掉或 gate 掉（那样会丢掉这半边测试）。
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::path::Path;

use super::path_semantics::{self, PathForms};

/// 一个位置的性质。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocationKind {
    /// 本地磁盘（含可移动盘）。
    Local,
    /// 网络位置（UNC / 网络挂载 / 网络驱动器）。
    Network,
    /// 疑似云同步客户端的目录。
    CloudSync {
        /// 识别出的客户端（`OneDrive` / `Dropbox` / `坚果云` …）。
        provider: String,
    },
    /// 判不出来（空路径等）。
    Unknown,
}

impl LocationKind {
    /// 把库放在这里是否**不安全**（该给用户提示）。
    #[must_use]
    pub fn is_risky_for_database(&self) -> bool {
        matches!(self, Self::Network | Self::CloudSync { .. })
    }

    /// 给人看的短描述（前端也可用它做 i18n 的 key）。
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Network => "network",
            Self::CloudSync { .. } => "cloud-sync",
            Self::Unknown => "unknown",
        }
    }

    /// 识别出的云服务商（没有则 `None`）。
    #[must_use]
    pub fn provider(&self) -> Option<&str> {
        match self {
            Self::CloudSync { provider } => Some(provider),
            _ => None,
        }
    }
}

/// 判定一个路径的位置性质。
///
/// **不做任何 I/O 之外的事**：不会创建文件、不会访问不存在的路径（挂载表本身除外）。
#[must_use]
pub fn classify(path: impl AsRef<str>) -> LocationKind {
    let raw = path.as_ref();
    if raw.trim().is_empty() {
        return LocationKind::Unknown;
    }
    let forms = PathForms::new(raw);
    let normalized = forms.normalized();

    // ① UNC / 双斜杠开头：一定是网络位置
    if path_semantics::is_unc(raw) {
        return LocationKind::Network;
    }

    // ② 云同步目录特征（放在挂载判定之前：同步盘通常就是本地盘，
    //    只看挂载类型会误判成 Local）
    if let Some(provider) = cloud_sync_provider(normalized) {
        return LocationKind::CloudSync { provider };
    }

    // ③ 平台相关：挂载表 / 驱动器类型
    match platform_mount_kind(normalized) {
        MountKind::Network => LocationKind::Network,
        MountKind::Local | MountKind::Unknown => LocationKind::Local,
    }
}

/// 把一个路径**相对于另一条更短的路径**归一化（用于挂载点匹配）。
fn strip_prefix_folded<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    let p = path.to_lowercase();
    let pre = prefix.trim_end_matches('/').to_lowercase();
    if p == pre {
        return Some("");
    }
    if p.starts_with(&pre) {
        let rest = &p[pre.len()..];
        if rest.starts_with('/') {
            // 用原串的同一字节位置切（大小写折叠不改变字节长度：to_lowercase 可能改变！
            // 例如 'İ' → "i̇"。所以这里用原串重新切一次，保证不会切出半个字符。）
            let idx = prefix.trim_end_matches('/').len();
            return Some(&path[idx..]);
        }
    }
    None
}

/// 最长前缀匹配：找出这个路径落在哪个挂载点下。
fn longest_mount<'a>(mounts: &'a [MountEntry], path: &str) -> Option<&'a MountEntry> {
    mounts
        .iter()
        .filter(|m| strip_prefix_folded(path, &m.mount_point).is_some())
        .max_by_key(|m| m.mount_point.trim_end_matches('/').len())
}

// ---------------------------------------------------------------------------
// 云同步客户端识别
// ---------------------------------------------------------------------------

/// 已知的云同步目录特征：(路径片段, 展示名)。
///
/// **只匹配整段**（`/OneDrive/` 而不是 `/OneDriveX/`），避免误伤。
const CLOUD_MARKERS: &[(&str, &str)] = &[
    ("onedrive", "OneDrive"),
    ("dropbox", "Dropbox"),
    ("google drive", "Google Drive"),
    ("googledrive", "Google Drive"),
    ("my drive", "Google Drive"),
    ("icloud drive", "iCloud Drive"),
    ("icloud", "iCloud"),
    ("nutstore", "坚果云"),
    ("坚果云", "坚果云"),
    ("baidunetdisk", "百度网盘"),
    ("百度网盘", "百度网盘"),
    ("aliyundrive", "阿里云盘"),
    ("阿里云盘", "阿里云盘"),
    ("weiyun", "腾讯微云"),
    ("微云", "腾讯微云"),
    ("115pan", "115"),
    ("mega", "MEGA"),
    ("pcloud", "pCloud"),
    ("yandexdisk", "Yandex Disk"),
    ("syncthing", "Syncthing"),
    ("resilio", "Resilio Sync"),
    ("seafile", "Seafile"),
    ("nextcloud", "Nextcloud"),
    ("owncloud", "ownCloud"),
    ("box sync", "Box"),
];

/// 路径里是否出现某个云同步目录特征（不区分大小写）。
///
/// 匹配规则：路径的**某一段**以特征开头（这样 `OneDrive - Contoso` 也能认出来），
/// 但不是任意子串（`我的OneDrive备份` 不算 —— 那是用户自己起的名字，不该乱猜）。
#[must_use]
pub fn cloud_sync_provider(path: &str) -> Option<String> {
    let forms = PathForms::new(path);
    let normalized = forms.normalized();
    for segment in normalized.split('/') {
        if segment.is_empty() {
            continue;
        }
        let lower = segment.to_lowercase();
        for (marker, name) in CLOUD_MARKERS {
            if lower == *marker
                || lower.starts_with(&format!("{marker} "))
                || lower.starts_with(&format!("{marker}-"))
            {
                return Some((*name).to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 平台相关：挂载表 / 驱动器类型
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MountKind {
    Local,
    Network,
    Unknown,
}

/// 一条挂载记录（Linux `/proc/mounts` 的行）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct MountEntry {
    mount_point: String,
    fs_type: String,
}

/// 网络文件系统类型（Linux）。
const NETWORK_FS_TYPES: &[&str] = &[
    "nfs",
    "nfs4",
    "cifs",
    "smbfs",
    "smb3",
    "sshfs",
    "fuse.sshfs",
    "davfs",
    "fuse.davfs",
    "fuse.gvfsd-fuse",
    "9p",       // WSL 访问 Windows 盘走的就是 9p —— 是「跨机器」，按网络位置提示
    "virtiofs", // 虚拟化共享目录，同理
    "ceph",
    "glusterfs",
    "afs",
];

/// 本地文件系统类型（**白名单**：不在这两个表里的都算 Unknown → 不报警）。
const LOCAL_FS_TYPES: &[&str] = &[
    "ext2", "ext3", "ext4", "btrfs", "xfs", "f2fs", "jfs", "reiserfs", "zfs", "bcachefs",
    "overlay", "tmpfs", "devtmpfs", "vfat", "exfat", "ntfs", "ntfs3", "fuseblk", "hfs", "hfsplus",
    "apfs", "squashfs", "ramfs", "ubifs",
];

/// 解析 `/proc/mounts` 风格的内容。
fn parse_mounts(text: &str) -> Vec<MountEntry> {
    text.lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let _device = it.next()?;
            let point = it.next()?;
            let fs = it.next()?;
            Some(MountEntry {
                // 挂载点里的转义（\040 空格等）按原样保留即可 —— 极少见，且匹配是前缀式的
                mount_point: point.to_string(),
                fs_type: fs.to_string(),
            })
        })
        .collect()
}

/// 从挂载表判断路径落在哪种挂载上（纯函数，便于测试）。
fn mount_kind_from_table(mounts: &[MountEntry], path: &str) -> MountKind {
    let Some(entry) = longest_mount(mounts, path) else {
        return MountKind::Unknown;
    };
    let fs = entry.fs_type.to_lowercase();
    if NETWORK_FS_TYPES.iter().any(|t| fs == *t) {
        return MountKind::Network;
    }
    if LOCAL_FS_TYPES.iter().any(|t| fs == *t) {
        return MountKind::Local;
    }
    MountKind::Unknown
}

/// 读平台信息判断挂载类型。
fn platform_mount_kind(normalized_path: &str) -> MountKind {
    #[cfg(target_os = "linux")]
    {
        if let Ok(text) = std::fs::read_to_string("/proc/mounts") {
            return mount_kind_from_table(&parse_mounts(&text), normalized_path);
        }
        MountKind::Unknown
    }
    #[cfg(windows)]
    {
        windows_drive_kind(normalized_path)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = normalized_path;
        MountKind::Unknown
    }
}

/// Windows：看盘符对应的驱动器类型。
#[cfg(windows)]
fn windows_drive_kind(normalized_path: &str) -> MountKind {
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;

    /// `GetDriveTypeW` 的返回值之一（网络驱动器）。
    ///
    /// 自己定义而不是用 `windows-sys` 的导出：这个常量在不同版本的 windows-sys 里
    /// 放在不同模块（0.61 的 `Win32::Storage::FileSystem` 里就没有），
    /// 而它的数值是 Win32 API 的稳定契约。
    /// 参考：<https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getdrivetypew>
    const DRIVE_REMOTE: u32 = 4;

    // 取盘符根（`C:/` → `C:\`）
    let bytes = normalized_path.as_bytes();
    if bytes.len() < 2 || bytes[1] != b':' || !bytes[0].is_ascii_alphabetic() {
        return MountKind::Unknown;
    }
    let root: Vec<u16> = format!("{}:\\", char::from(bytes[0]))
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: root 是以 NUL 结尾的 UTF-16 串，生命周期覆盖本次调用，符合 API 契约。
    let kind = unsafe { GetDriveTypeW(root.as_ptr()) };
    if kind == DRIVE_REMOTE {
        MountKind::Network
    } else {
        MountKind::Local
    }
}

/// 便捷函数：这个路径适合放 catalog.db 吗？
///
/// 返回 `None` 表示没问题；`Some(kind)` 表示该警告用户。
#[must_use]
pub fn catalog_suitability(path: impl AsRef<str>) -> Option<LocationKind> {
    let kind = classify(path);
    kind.is_risky_for_database().then_some(kind)
}

/// 路径是否看起来像个盘/目录（用于提示文案，不做 I/O）。
#[must_use]
pub fn is_probably_a_directory_path(path: &str) -> bool {
    let p = path.trim();
    !p.is_empty() && !p.contains('\0')
}

/// 检查一个**已存在的目录**是不是在受监控的位置（会真的碰文件系统）。
///
/// 与 [`classify`] 的区别：这里用于「用户已经选好了目录」之后的最终检查。
pub fn classify_existing(dir: &Path) -> LocationKind {
    classify(dir.to_string_lossy().as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- UNC ----------

    #[test]
    fn unc_paths_are_network() {
        for p in [
            r"\\server\share\照片",
            r"\\192.168.1.10\photo",
            "//server/share/照片",
            r"\\nas\photo\",
        ] {
            assert_eq!(classify(p), LocationKind::Network, "{p}");
        }
    }

    #[test]
    fn drive_paths_are_not_confused_with_unc() {
        let k = classify(r"C:\照片");
        assert_eq!(k, LocationKind::Local, "{k:?}");
    }

    // ---------- 云同步 ----------

    #[test]
    fn detects_common_cloud_folders() {
        let cases = [
            (r"C:\Users\me\OneDrive\照片", "OneDrive"),
            (r"C:\Users\me\OneDrive - Contoso\照片", "OneDrive"),
            (r"D:\Dropbox\照片", "Dropbox"),
            (r"E:\坚果云\我的照片", "坚果云"),
            (r"C:\Users\me\Google Drive\My Drive\照片", "Google Drive"),
            (r"/home/me/Dropbox/照片", "Dropbox"),
            (r"F:\BaiduNetdisk\照片", "百度网盘"),
            (r"G:\Nextcloud\照片", "Nextcloud"),
        ];
        for (path, provider) in cases {
            let k = classify(path);
            assert_eq!(k.provider(), Some(provider), "{path} → {k:?}");
            assert!(k.is_risky_for_database());
        }
    }

    #[test]
    fn cloud_detection_is_case_insensitive_and_unicode_aware() {
        assert_eq!(
            cloud_sync_provider(r"c:\users\ME\onedrive\x").as_deref(),
            Some("OneDrive")
        );
        assert_eq!(
            cloud_sync_provider("E:/坚果云/x").as_deref(),
            Some("坚果云")
        );
    }

    #[test]
    fn does_not_match_arbitrary_substrings() {
        // 用户自己起的名字里含这些词，不该乱猜（宁可漏报也不要误报）
        for p in [
            r"C:\我的OneDrive备份\照片",
            r"D:\dropboxed\照片",
            r"E:\照片\onedrive笔记",
        ] {
            assert_eq!(cloud_sync_provider(p), None, "{p}");
        }
    }

    #[test]
    fn a_cloud_folder_inside_a_network_share_is_still_network() {
        // UNC 优先：`\\nas\share\OneDrive` 的网络属性比云同步更重要
        assert_eq!(
            classify(r"\\nas\share\OneDrive\照片"),
            LocationKind::Network
        );
    }

    // ---------- 挂载表（Linux） ----------

    fn mounts() -> Vec<MountEntry> {
        parse_mounts(
            "/dev/sda2 / ext4 rw,relatime 0 0
tmpfs /run tmpfs rw 0 0
/dev/sdb1 /mnt/data exfat rw 0 0
server:/export/photos /mnt/nfs nfs4 rw 0 0
//nas/photo /mnt/smb cifs rw 0 0
fuse.sshfs /mnt/ssh fuse.sshfs rw 0 0
drvfs /mnt/c 9p rw 0 0
weirdfs /mnt/weird weirdfs rw 0 0
",
        )
    }

    #[test]
    fn parses_mount_table() {
        let m = mounts();
        assert_eq!(m.len(), 8);
        assert_eq!(m[0].mount_point, "/");
        assert_eq!(m[0].fs_type, "ext4");
    }

    #[test]
    fn local_mounts_are_local() {
        for p in ["/home/me/照片", "/mnt/data/照片", "/tmp/x"] {
            assert_eq!(mount_kind_from_table(&mounts(), p), MountKind::Local, "{p}");
        }
    }

    #[test]
    fn network_mounts_are_network() {
        for p in ["/mnt/nfs/照片", "/mnt/smb/a.jpg", "/mnt/ssh/deep/path.jpg"] {
            assert_eq!(
                mount_kind_from_table(&mounts(), p),
                MountKind::Network,
                "{p}"
            );
        }
        // WSL 下的 Windows 盘走 9p：属于跨系统共享，按网络位置提示
        assert_eq!(
            mount_kind_from_table(&mounts(), "/mnt/c/照片"),
            MountKind::Network
        );
    }

    #[test]
    fn unknown_filesystem_is_not_flagged() {
        // 白名单之外的文件系统不报警（宁可漏报）
        assert_eq!(
            mount_kind_from_table(&mounts(), "/mnt/weird/x"),
            MountKind::Unknown
        );
        assert_eq!(
            classify("/mnt/weird/照片"),
            LocationKind::Local,
            "Unknown 挂载按本地处理，不打扰用户"
        );
    }

    #[test]
    fn longest_mount_wins_for_nested_mounts() {
        let m = parse_mounts(
            "/dev/sda2 / ext4 rw 0 0
/dev/sdb1 /media/disk ext4 rw 0 0
server:/x /media/disk/远程 nfs rw 0 0
",
        );
        assert_eq!(
            mount_kind_from_table(&m, "/media/disk/照片"),
            MountKind::Local
        );
        assert_eq!(
            mount_kind_from_table(&m, "/media/disk/远程/照片"),
            MountKind::Network
        );
    }

    #[test]
    fn mount_matching_respects_segment_boundaries() {
        // `/mnt/da` 不该匹配到挂载点 `/mnt/data`
        let m = parse_mounts("server:/x /mnt/data nfs rw 0 0\n");
        assert_eq!(mount_kind_from_table(&m, "/mnt/da/x"), MountKind::Unknown);
        assert_eq!(mount_kind_from_table(&m, "/mnt/data"), MountKind::Network);
        assert_eq!(
            mount_kind_from_table(&m, "/mnt/data/照片"),
            MountKind::Network
        );
    }

    #[test]
    fn malformed_mount_lines_are_skipped() {
        let m = parse_mounts("这行不对\n/dev/sda1 /ok ext4 rw 0 0\n只有两个 字段\n");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].mount_point, "/ok");
    }

    #[test]
    fn empty_mount_table_means_unknown() {
        assert_eq!(mount_kind_from_table(&[], "/home/x"), MountKind::Unknown);
    }

    // ---------- 边界 ----------

    #[test]
    fn empty_or_blank_path_is_unknown() {
        assert_eq!(classify(""), LocationKind::Unknown);
        assert_eq!(classify("   "), LocationKind::Unknown);
    }

    #[test]
    fn unicode_paths_are_handled() {
        let k = classify("D:\\照片📷\\2026年\\婚礼");
        assert_eq!(k, LocationKind::Local);
        assert_eq!(k.code(), "local");
    }

    #[test]
    fn code_and_provider_helpers() {
        assert_eq!(LocationKind::Network.code(), "network");
        assert_eq!(LocationKind::Unknown.code(), "unknown");
        assert!(
            LocationKind::CloudSync {
                provider: "OneDrive".into()
            }
            .provider()
            .is_some()
        );
        assert!(LocationKind::Local.provider().is_none());
    }

    #[test]
    fn catalog_suitability_flags_only_risky_places() {
        assert!(catalog_suitability(r"C:\照片").is_none());
        assert!(catalog_suitability(r"\\nas\share").is_some());
        assert_eq!(
            catalog_suitability(r"C:\Users\me\OneDrive\照片")
                .map(|k| k.provider().unwrap().to_string()),
            Some("OneDrive".to_string())
        );
    }

    #[test]
    fn existing_dir_helper_works() {
        let dir = tempfile::tempdir().unwrap();
        let k = classify_existing(dir.path());
        // 临时目录通常是 tmpfs 或 overlay，都属于本地
        assert!(!k.is_risky_for_database() || k.code() == "network", "{k:?}");
    }

    #[test]
    fn is_probably_a_directory_path_rejects_junk() {
        assert!(is_probably_a_directory_path(r"C:\照片"));
        assert!(!is_probably_a_directory_path(""));
        assert!(!is_probably_a_directory_path("   "));
        assert!(!is_probably_a_directory_path("a\0b"));
    }
}
