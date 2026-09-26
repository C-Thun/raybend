//! 可选「来源」的枚列：Windows 驱动器 / Linux 挂载点（`design/main.md` §3.1.2）。
//!
//! 「来源树」的第一层是驱动器。设计要求：**不能只标盘符** —— 前方必须有来源类型图标，
//! 因为将来要扩展到 NAS、云盘等不同来源。所以这一层给出的不只是路径，还有
//! [`VolumeKind`]（本地 / 可移动 / 光盘 / 网络 / 云 / 未知）。
//!
//! 与 [`crate::store::location`] 的分工：
//!
//! * `location` 回答「**这个**路径适合放数据库吗」（用于警告）；
//! * `volumes` 回答「**有哪些**可以挑的来源」（用于列表）。
//!
//! 两者共用同一份 `/proc/mounts` 解析（`location::parse_mounts`）—— 解析规则只有一份。
//!
//! # 已知取舍
//!
//! * **不取卷标**：Windows 的 `GetVolumeInformationW` 在空光驱、断线的网络盘上**会阻塞**，
//!   而设计稿只要求「盘符 + 来源类型图标」。要卷标的话得放到后台线程里做（登记 `FUTURE.md`）。
//! * **Windows 上的「云」暂不产生**：OneDrive 之类是用户目录里的普通文件夹，不是盘符。
//!   等做「自定义来源」时再补（`design/main.md` 已把它标为预留）。

// 挂载表那条路是 **Linux 专属**（Windows 走盘符枚列），但它的纯函数与测试在所有平台都跑
// —— 不要因为非 Linux 上报「never used」就把它们删掉或 gate 掉，那会丢掉安全网的测试。
// 与 `store::location` 同一处理（见那里的注释）。
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use super::location::{self, MountEntry};

/// 一个可选来源的种类（决定树里的图标）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeKind {
    /// 本地固定盘。
    Local,
    /// 可移动盘（U 盘、移动硬盘、存储卡）。
    Removable,
    /// 光盘。
    Optical,
    /// 网络位置（UNC / NFS / SMB / 9p…）。
    Network,
    /// 云同步客户端的挂载（rclone / gvfs 之类）。
    Cloud,
    /// 认不出来（或不适合作为来源）。
    Unknown,
}

impl VolumeKind {
    /// 给前端的稳定标识（**不要改**：前端按它选图标）。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Removable => "removable",
            Self::Optical => "optical",
            Self::Network => "network",
            Self::Cloud => "cloud",
            Self::Unknown => "unknown",
        }
    }

    /// 中文名（提示与日志用）。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Local => "本地磁盘",
            Self::Removable => "可移动磁盘",
            Self::Optical => "光盘",
            Self::Network => "网络位置",
            Self::Cloud => "云盘",
            Self::Unknown => "未知来源",
        }
    }

    /// Windows `GetDriveTypeW` 的返回值 → 种类（纯函数，任何平台都能测）。
    ///
    /// 常量按 Win32 的稳定契约自己定义：`windows-sys` 0.61 并没在
    /// `Win32::Storage::FileSystem` 里导出这些名字（与 `location.rs` 同一处理）。
    /// 参考：<https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getdrivetypew>
    #[must_use]
    pub const fn from_drive_type(value: u32) -> Self {
        const DRIVE_REMOVABLE: u32 = 2;
        const DRIVE_FIXED: u32 = 3;
        const DRIVE_REMOTE: u32 = 4;
        const DRIVE_CDROM: u32 = 5;
        const DRIVE_RAMDISK: u32 = 6;
        match value {
            DRIVE_REMOVABLE => Self::Removable,
            DRIVE_FIXED | DRIVE_RAMDISK => Self::Local,
            DRIVE_REMOTE => Self::Network,
            DRIVE_CDROM => Self::Optical,
            // 0 = DRIVE_UNKNOWN，1 = DRIVE_NO_ROOT_DIR（没介质 / 盘符失效）…
            _ => Self::Unknown,
        }
    }
}

/// 一个可选来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    /// 打开时用的路径：Windows 是 `D:\`，Linux 是挂载点。
    pub path: String,
    /// 来源类型。
    pub kind: VolumeKind,
}

impl Volume {
    fn new(path: impl Into<String>, kind: VolumeKind) -> Self {
        Self {
            path: path.into(),
            kind,
        }
    }
}

/// 可以作为「来源」列出来的文件系统类型（Linux 侧白名单）。
///
/// **白名单而不是黑名单**：`/proc/mounts` 里绝大多数条目是内核的伪文件系统
/// （`proc` / `sysfs` / `cgroup2` / `tmpfs` / `overlay`…），列出来只会干扰用户；
/// 黑名单永远漏得比白名单多。
const VOLUME_FS_TYPES: &[&str] = &[
    // ── 本地盘 ──
    "ext2", "ext3", "ext4", "btrfs", "xfs", "f2fs", "jfs", "reiserfs", "zfs", "bcachefs", "vfat",
    "exfat", "ntfs", "ntfs3", "fuseblk", "hfs", "hfsplus", "apfs", // ── 光盘 / 镜像 ──
    "iso9660", "udf", // ── 跨系统共享（WSL 的 Windows 盘走 drvfs / 9p）──
    "drvfs", "9p", "virtiofs", "vmhgfs-fuse", "fuse.vmhgfs-fuse", // ── 网络 ──
    "nfs", "nfs4", "cifs", "smbfs", "smb3", "sshfs", "fuse.sshfs", "davfs", "fuse.davfs", "ceph",
    "glusterfs", // ── 云盘客户端 ──
    "fuse.rclone", "fuse.gvfsd-fuse",
];

/// 枚列本机可选的来源。
///
/// 读不到平台信息时返回**空列表**（而不是报错）：来源树显示空态，
/// 用户仍可以在「最近」里挑；这一层不值得让整个界面失败。
#[must_use]
pub fn list() -> Vec<Volume> {
    #[cfg(target_os = "linux")]
    {
        match std::fs::read_to_string("/proc/mounts") {
            Ok(text) => volumes_from_mounts(&location::parse_mounts(&text)),
            Err(_) => Vec::new(),
        }
    }
    #[cfg(windows)]
    {
        windows_drives()
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        Vec::new()
    }
}

/// 系统自留的挂载点：列出来只是噪声。
///
/// **为什么需要这条规则**（`specs/M1-8.md` §F，2026-09-16 的真机反馈）：
/// Linux 侧的卷来自 `/proc/mounts`，其中**必然**有根分区 `/`。在 `/` 上展开一次，
/// 看到的就是 `mnt`、`home`、`usr`、`etc` 这一堆 —— 用户会以为「来源树在读一个莫名其妙的合集」。
/// 真正有用的来源只有两类：**用户主目录**与**外挂/可移动盘**（含 WSL 里的 `/mnt/c`）。
const SYSTEM_MOUNTS: &[&str] = &[
    "/bin",
    "/boot",
    "/dev",
    "/etc",
    "/lib",
    "/lib32",
    "/lib64",
    "/libx32",
    "/lost+found",
    "/opt",
    "/proc",
    "/root",
    "/run",
    "/sbin",
    "/snap",
    "/srv",
    "/sys",
    "/tmp",
    "/usr",
    "/var",
];

/// 落在[系统目录](SYSTEM_MOUNTS)之下、但**确实是来源**的例外。
///
/// * `/run/media/<用户>/<卷>` —— udisks 挂可移动介质的地方（U 盘、SD 卡）；
/// * `/run/user/<uid>/gvfs` —— GNOME 的 GVfs：手机、云盘、网络位置都挂在这里。
///
/// 没有这两条例外，`/run` 那条规则会把「刚插上的 U 盘」也一起挡掉 ——
/// 那正是用户最想导入的来源。
const ALLOWED_UNDER_SYSTEM: &[&str] = &["/run/media", "/run/user"];

/// WSL 自己的内部挂载（`/mnt/wsl` 家族）：对用户没有意义。
const WSL_INTERNAL_MOUNTS: &[&str] = &["/mnt/host", "/mnt/wsl", "/mnt/wslg"];

/// 这个挂载点值得出现在「来源」里吗？
///
/// * 根分区 `/` 不算「一个来源」—— 展开它只会看到系统目录；
/// * 各发行版的系统目录（[`SYSTEM_MOUNTS`]）不算；
/// * **其余一律保留**：`/home/...`、`/mnt/*`、`/media/*`、`/run/media/*`，
///   以及用户自己挂的 `/data`、`/storage` 这类非常规位置 —— 宁可多留，
///   也不要在用户真把照片放在那里时把它藏起来。
#[must_use]
pub fn is_useful_source_mount(mount_point: &str) -> bool {
    let path = mount_point.trim();
    if path.is_empty() {
        return false;
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return false; // 只有斜杠 = 根
    }
    let under = |skip: &str| -> bool {
        trimmed
            .strip_prefix(skip)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    };
    if WSL_INTERNAL_MOUNTS.iter().any(|skip| under(skip)) {
        return false;
    }
    // 先看例外：`/run/media/...` 与 `/run/user/...` 是**真的有来源**的地方
    if ALLOWED_UNDER_SYSTEM.iter().any(|allow| under(allow)) {
        return true;
    }
    !SYSTEM_MOUNTS.iter().any(|skip| under(skip))
}

/// 从挂载表挑出可作为来源的条目（纯函数，任何平台都能测）。
///
/// 规则：
/// * 只认 [`VOLUME_FS_TYPES`] 里的类型（伪文件系统一律跳过）；
/// * 只要[值得作为来源](is_useful_source_mount)的挂载点（`/` 与系统目录不算，见那里的说明）；
/// * 同一个挂载点只留一条（`/proc/mounts` 里同一处可能被挂多次）；
/// * 空挂载点跳过；
/// * 结果按路径排序（稳定、可断言）。
#[must_use]
pub(crate) fn volumes_from_mounts(mounts: &[MountEntry]) -> Vec<Volume> {
    let mut out: Vec<Volume> = Vec::new();
    for entry in mounts {
        if entry.mount_point.is_empty() || !is_volume_fs(&entry.fs_type) {
            continue;
        }
        // 系统挂载点与根分区不进来源列表（否则用户会看到 `/mnt`、`/home` 这种「奇怪的合集」）
        if !is_useful_source_mount(&entry.mount_point) {
            continue;
        }
        if out.iter().any(|v| v.path == entry.mount_point) {
            continue;
        }
        let kind = mount_kind(&entry.mount_point, &entry.fs_type);
        out.push(Volume::new(entry.mount_point.clone(), kind));
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out.dedup_by(|a, b| a.path == b.path);
    out
}

fn is_volume_fs(fs_type: &str) -> bool {
    let fs = fs_type.to_lowercase();
    VOLUME_FS_TYPES.iter().any(|known| fs == *known)
}

/// 一个挂载点属于哪种来源。
fn mount_kind(mount_point: &str, fs_type: &str) -> VolumeKind {
    let fs = fs_type.to_lowercase();
    if location::NETWORK_FS_TYPES.iter().any(|known| fs == *known) {
        return VolumeKind::Network;
    }
    // 云盘客户端：路径里出现 OneDrive / Dropbox… 这样的段（与位置警告同一套判据）
    if location::cloud_sync_provider(mount_point).is_some() {
        return VolumeKind::Cloud;
    }
    if fs == "iso9660" || fs == "udf" {
        return VolumeKind::Optical;
    }
    // udisks 的惯例：可移动介质挂到 /media/<用户>/<卷> 或 /run/media/<用户>/<卷>
    if mount_point.starts_with("/media/") || mount_point.starts_with("/run/media/") {
        return VolumeKind::Removable;
    }
    VolumeKind::Local
}

/// Windows：按盘符枚列（`GetLogicalDrives` 的位掩码 + `GetDriveTypeW` 的类型）。
#[cfg(windows)]
fn windows_drives() -> Vec<Volume> {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};

    // SAFETY: 无参数、无出参，返回值是 26 位位掩码；失败时返回 0。
    let mask = unsafe { GetLogicalDrives() };
    if mask == 0 {
        return Vec::new();
    }

    let mut out = Vec::new();
    for index in 0..26u32 {
        if mask & (1 << index) == 0 {
            continue;
        }
        // 位 0 = A: … 位 25 = Z:
        let Ok(offset) = u8::try_from(index) else {
            continue;
        };
        let letter = char::from(b'A' + offset);
        let root = format!("{letter}:\\");
        let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();

        // SAFETY: wide 是以 NUL 结尾的 UTF-16 串，生命周期覆盖本次调用。
        let kind = VolumeKind::from_drive_type(unsafe { GetDriveTypeW(wide.as_ptr()) });
        // DRIVE_UNKNOWN / DRIVE_NO_ROOT_DIR（空读卡器、已断开的盘符）不进列表
        if kind == VolumeKind::Unknown {
            continue;
        }
        out.push(Volume::new(root, kind));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(device: &str, point: &str, fs: &str) -> MountEntry {
        MountEntry {
            device: device.to_string(),
            mount_point: point.to_string(),
            fs_type: fs.to_string(),
        }
    }

    /// 按路径取种类（断言用）——**不要按下标断言**：结果是排序过的，
    /// 下标会随路径的字典序变，测试会莫名其妙地碎。
    fn kind_at(out: &[Volume], path: &str) -> VolumeKind {
        out.iter()
            .find(|v| v.path == path)
            .unwrap_or_else(|| panic!("没有这个来源：{path}（实际：{out:?}）"))
            .kind
    }

    // ---------- Windows 驱动器类型映射（纯函数，各平台都跑） ----------

    #[test]
    fn windows_drive_types_map_to_kinds() {
        assert_eq!(VolumeKind::from_drive_type(2), VolumeKind::Removable);
        assert_eq!(VolumeKind::from_drive_type(3), VolumeKind::Local);
        assert_eq!(VolumeKind::from_drive_type(4), VolumeKind::Network);
        assert_eq!(VolumeKind::from_drive_type(5), VolumeKind::Optical);
        assert_eq!(VolumeKind::from_drive_type(6), VolumeKind::Local, "内存盘");
        assert_eq!(VolumeKind::from_drive_type(0), VolumeKind::Unknown);
        assert_eq!(
            VolumeKind::from_drive_type(1),
            VolumeKind::Unknown,
            "DRIVE_NO_ROOT_DIR：没介质 / 盘符失效"
        );
        assert_eq!(VolumeKind::from_drive_type(999), VolumeKind::Unknown);
    }

    #[test]
    fn kind_codes_are_stable_for_the_ui() {
        // 前端按 code 选图标，改了就是破坏性变更
        assert_eq!(VolumeKind::Local.code(), "local");
        assert_eq!(VolumeKind::Removable.code(), "removable");
        assert_eq!(VolumeKind::Optical.code(), "optical");
        assert_eq!(VolumeKind::Network.code(), "network");
        assert_eq!(VolumeKind::Cloud.code(), "cloud");
        assert_eq!(VolumeKind::Unknown.code(), "unknown");
        assert!(VolumeKind::Network.label().contains("网络"));
    }

    // ---------- 挂载表 → 来源列表 ----------

    #[test]
    fn pseudo_filesystems_are_not_volumes() {
        let mounts = [
            entry("proc", "/proc", "proc"),
            entry("sysfs", "/sys", "sysfs"),
            entry("cgroup2", "/sys/fs/cgroup", "cgroup2"),
            entry("tmpfs", "/tmp", "tmpfs"),
            entry("overlay", "/", "overlay"),
            entry("tmpfs", "/dev/shm", "tmpfs"),
        ];
        assert!(
            volumes_from_mounts(&mounts).is_empty(),
            "伪文件系统不能出现在来源列表里"
        );
    }

    #[test]
    fn real_filesystems_become_volumes_and_are_sorted() {
        let mounts = [
            entry("/dev/nvme0n1p2", "/", "ext4"),
            entry("/dev/sdb1", "/mnt/data", "xfs"),
            entry("/dev/nvme0n1p1", "/boot/efi", "vfat"),
            entry("none", "/tmp", "tmpfs"),
        ];
        let out = volumes_from_mounts(&mounts);
        let paths: Vec<&str> = out.iter().map(|v| v.path.as_str()).collect();
        // 根分区与 `/boot/efi` 被 `is_useful_source_mount` 挡掉：它们是系统盘的一部分，
        // 不是「某个来源」（展开只会看到 mnt/home/usr 那一堆）
        assert_eq!(paths, vec!["/mnt/data"]);
        assert!(out.iter().all(|v| v.kind == VolumeKind::Local));
    }

    #[test]
    fn root_and_system_mounts_are_not_sources() {
        // 根分区：用户在 `specs/M1-8.md` §F 里报的那个「奇怪的合集」就是它
        assert!(!is_useful_source_mount("/"));
        assert!(!is_useful_source_mount(""));
        assert!(!is_useful_source_mount("   "));
        // 系统目录（含它们的子挂载）
        for path in [
            "/boot",
            "/boot/efi",
            "/usr",
            "/usr/local",
            "/var",
            "/var/lib/docker",
            "/etc",
            "/proc",
            "/sys",
            "/dev",
            "/run",
            "/snap/foo",
            "/tmp",
            "/opt/app",
            "/srv",
            "/root",
            "/lost+found",
        ] {
            assert!(!is_useful_source_mount(path), "{path} 不该作为来源");
        }
        // `/run` 本身排除，但它下面的两类例外要放行（U 盘与 GVfs）
        assert!(!is_useful_source_mount("/run"));
        assert!(!is_useful_source_mount("/run/lock"));
        assert!(is_useful_source_mount("/run/media/andares/CARD"));
        assert!(is_useful_source_mount("/run/user/1000/gvfs"));
        assert!(is_useful_source_mount("/run/user/1000/gvfs/mtp:host=phone"));
        // WSL 的内部挂载
        assert!(!is_useful_source_mount("/mnt/wslg"));
        assert!(!is_useful_source_mount("/mnt/wsl/distro"));
        assert!(!is_useful_source_mount("/mnt/host"));
    }

    #[test]
    fn user_homes_and_external_mounts_stay() {
        for path in [
            "/home/andares",
            "/home/andares/Pictures/",  // 尾斜杠不影响
            "/mnt/c",                   // WSL 里的 Windows 盘 —— 最常用的那个
            "/mnt/data",
            "/media/andares/USB",
            "/run/media/andares/CARD",
            "/data/photos", // 非常规位置：宁可多留，也不要把用户的照片藏起来
            "/storage",
        ] {
            assert!(is_useful_source_mount(path), "{path} 应当保留");
        }
    }

    #[test]
    fn network_filesystems_are_marked_as_network() {
        let mounts = [
            entry("//nas/photo", "/mnt/nas", "cifs"),
            entry("10.0.0.2:/export", "/mnt/nfs", "nfs4"),
            entry("//server/share", "/mnt/smb", "smb3"),
            entry("tmpfs", "/mnt/ram", "tmpfs"),
        ];
        let out = volumes_from_mounts(&mounts);
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|v| v.kind == VolumeKind::Network), "{out:?}");
    }

    #[test]
    fn removable_media_under_media_dirs_is_removable() {
        let mounts = [
            entry("/dev/sdc1", "/media/andares/USB32", "exfat"),
            entry("/dev/sdd1", "/run/media/andares/CARD", "vfat"),
            entry("/dev/sdb1", "/mnt/bigdisk", "ext4"),
        ];
        let out = volumes_from_mounts(&mounts);
        assert_eq!(kind_at(&out, "/media/andares/USB32"), VolumeKind::Removable);
        assert_eq!(
            kind_at(&out, "/run/media/andares/CARD"),
            VolumeKind::Removable
        );
        assert_eq!(
            kind_at(&out, "/mnt/bigdisk"),
            VolumeKind::Local,
            "/mnt 下的固定盘不算可移动介质"
        );
    }

    #[test]
    fn optical_media_is_marked_as_optical() {
        let mounts = [
            entry("/dev/sr0", "/media/cdrom", "iso9660"),
            entry("/dev/sr1", "/mnt/dvd", "udf"),
        ];
        let out = volumes_from_mounts(&mounts);
        // /media/cdrom 既在 /media 下、又是 iso9660 —— 光盘优先（更具体）
        assert_eq!(kind_at(&out, "/media/cdrom"), VolumeKind::Optical);
        assert_eq!(kind_at(&out, "/mnt/dvd"), VolumeKind::Optical);
    }

    #[test]
    fn cloud_mounts_are_marked_as_cloud() {
        let mounts = [
            entry("onedrive:", "/home/andares/OneDrive", "fuse.rclone"),
            // gvfs 在 `location` 的网络表里（它确实是「别处的文件系统」），
            // 所以这里按网络位置处理 —— 而不是伪装成本地盘
            entry("gvfsd", "/run/user/1000/gvfs", "fuse.gvfsd-fuse"),
        ];
        let out = volumes_from_mounts(&mounts);
        assert_eq!(
            kind_at(&out, "/home/andares/OneDrive"),
            VolumeKind::Cloud
        );
        assert_eq!(
            kind_at(&out, "/run/user/1000/gvfs"),
            VolumeKind::Network,
            "gvfs 是远程位置（手机 MTP、SMB 共享都挂在它下面）"
        );
    }

    #[test]
    fn network_beats_cloud_when_both_match() {
        let mounts = [entry(
            "//nas/OneDrive",
            "/mnt/OneDrive",
            "cifs",
        )];
        let out = volumes_from_mounts(&mounts);
        assert_eq!(out[0].kind, VolumeKind::Network, "网络是更硬的事实");
    }

    #[test]
    fn duplicate_mount_points_collapse_into_one() {
        let mounts = [
            entry("/dev/sdb1", "/mnt/data", "ext4"),
            entry("/dev/sdb1", "/mnt/data", "ext4"),
            entry("/dev/sdb2", "/mnt/data2", "ext4"),
        ];
        let out = volumes_from_mounts(&mounts);
        let paths: Vec<&str> = out.iter().map(|v| v.path.as_str()).collect();
        assert_eq!(paths, vec!["/mnt/data", "/mnt/data2"]);
    }

    #[test]
    fn empty_mount_table_and_blank_points_are_safe() {
        assert!(volumes_from_mounts(&[]).is_empty());
        let mounts = [entry("/dev/sdb1", "", "ext4")];
        assert!(volumes_from_mounts(&mounts).is_empty());
    }

    #[test]
    fn mount_point_case_does_not_change_the_kind() {
        let mounts = [
            entry("/dev/sdb1", "/Media/andares/USB", "exfat"),
            entry("/dev/sdb2", "/MNT/data", "ext4"),
        ];
        let out = volumes_from_mounts(&mounts);
        // 挂载点匹配是**大小写敏感**的（Linux 就是字节串）：/Media ≠ /media
        assert_eq!(kind_at(&out, "/Media/andares/USB"), VolumeKind::Local);
        assert_eq!(kind_at(&out, "/MNT/data"), VolumeKind::Local);
    }

    #[test]
    fn unicode_mount_points_survive() {
        let mounts = [
            entry("/dev/sdb1", "/媒体/照片", "ext4"),
            entry("/dev/sdc1", "/media/用户/我的 U 盘", "exfat"),
        ];
        let out = volumes_from_mounts(&mounts);
        assert_eq!(out.len(), 2);
        assert_eq!(kind_at(&out, "/媒体/照片"), VolumeKind::Local);
        assert_eq!(
            kind_at(&out, "/media/用户/我的 U 盘"),
            VolumeKind::Removable,
            "中文卷名也是可移动盘"
        );
    }

    #[test]
    fn octal_escaped_mount_points_are_kept_verbatim() {
        // /proc/mounts 用 \040 表示空格。我们不做反转义（极少见，且匹配是前缀式的）
        // —— 这里只是把「原样保留」这个决定钉住，别哪天以为漏处理了
        let mounts = [entry("/dev/sdb1", "/mnt/My\\040Disk", "ext4")];
        let out = volumes_from_mounts(&mounts);
        assert_eq!(out[0].path, "/mnt/My\\040Disk");
    }

    #[test]
    fn fs_type_match_is_case_insensitive() {
        let mounts = [entry("/dev/sdb1", "/mnt/data", "EXT4")];
        let out = volumes_from_mounts(&mounts);
        assert_eq!(out.len(), 1, "大小写不该让一个盘消失");
    }

    #[test]
    fn wsl_style_drives_are_listed() {
        // WSL 下 Windows 盘走 9p / drvfs —— 开发期要能挑到它们
        let mounts = [
            entry("C:\\", "/mnt/c", "9p"),
            entry("D:\\", "/mnt/d", "drvfs"),
            entry("none", "/mnt/wslg", "tmpfs"),
        ];
        let out = volumes_from_mounts(&mounts);
        let paths: Vec<&str> = out.iter().map(|v| v.path.as_str()).collect();
        assert_eq!(paths, vec!["/mnt/c", "/mnt/d"]);
    }
}
