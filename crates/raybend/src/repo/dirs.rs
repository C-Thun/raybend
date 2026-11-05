//! 库内目录的操作：**新建目录**与**删除目录**。
//!
//! 两个动作的入口是浏览模式左列的**目录区工具栏**（崔总 2026-11-05 把行尾 `⋯` 菜单
//! 拆成了常驻按钮与 hover 标签按钮，见 `memory/FUNCTION-BROWSE.md` §4.3）：
//!
//! | 入口 | 规则 |
//! | --- | --- |
//! | 删除目录 | 判据**只看这一层**：没有用户可见的子目录、也没有照片（本层 + `_RAW/`）——
//!   也就是「浏览界面里既看不到照片、左边也看不到子目录」。**浅检查，不递归**；
//!   删的时候连同看不见的残留（`.xmp`、空的 `_RAW/`）一起走 |
//! | 新建目录 | 名字要校验（Windows 口径），**重名要挡住**（大小写不敏感） |
//!
//! # 为什么删除判据是「浏览里看得见的东西」
//!
//! （崔总 2026-11-05）浏览模式能看见的只有两样：**目录**与**照片**。库是我们自己维护的，
//! 侧车 `.xmp` 这类残留本来就不该存在；真残留了，用户看不见它，也不该因为它而删不掉目录。
//! 所以：`_RAW/`、`.xmp`、随手扔进去的 `.txt` 都**不阻止删除**，跟着一起删。
//!
//! 子目录不递归删除 —— 要删就一级一级删（删父之前先把子目录删掉）。
//! 唯一例外是 `_RAW/`：它在浏览里**不可见**，不是用户目录，所以它里面的照片算作**本目录的照片**
//! （那些照片在网格里是会显示成 tile 的，挡住删除是对的）。
//!
//! # 为什么校验按 Windows 口径
//!
//! 主战场是 Windows（`AGENTS.md` §1）。一个在 Linux 上能建、到 Windows 上就建不出来的名字，
//! 等到用户换平台才炸是最糟的失败方式 —— 所以在**所有平台**上按 Windows 的规则拒绝：
//! 非法字符 `<>:"/\|?*`、保留名（`CON` / `NUL` / `COM1`…）、结尾的点与空格、控制字符。
//!
//! # 两条不可逾越的底线
//!
//! 1. **删前必查、查和删用同一套判据**：`remove_dir_checked` 自己先跑一遍
//!    `check_visible_contents`，通过了才动盘 —— 不靠调用方记得先查。
//! 2. **符号链接一律算「有东西」**：不跟随、也不删（删别人指向的东西不是这个功能该干的事），
//!    拿不准就不动。

use std::path::{Component, Path, PathBuf};

/// 名字被拒的原因（给用户看的文案由界面层翻，这里只给结构）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirNameError {
    /// 空名字（或只有空白）
    Empty,
    /// 含路径分隔符 —— 这是「新建子目录」，不是「新建一串目录」
    Separator,
    /// `.` 与 `..`
    Relative,
    /// Windows 非法字符 `<>:"/\|?*` 或控制字符
    IllegalChar(char),
    /// Windows 保留名（`CON` / `PRN` / `AUX` / `NUL` / `COM1..9` / `LPT1..9`，大小写不敏感，带扩展名也算）
    Reserved,
    /// 结尾是点或空格 —— Windows 会把它悄悄吃掉，导致「建了却找不到」
    TrailingDotOrSpace,
    /// 单个名字段太长（超过 255 字节）
    TooLong,
}

impl std::fmt::Display for DirNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "目录名不能为空"),
            Self::Separator => write!(f, "目录名不能包含路径分隔符"),
            Self::Relative => write!(f, "目录名不能是 . 或 .."),
            Self::IllegalChar(c) => write!(f, "目录名不能包含字符 {c}"),
            Self::Reserved => write!(f, "这是 Windows 保留名，换一个吧"),
            Self::TrailingDotOrSpace => write!(f, "目录名不能以点或空格结尾"),
            Self::TooLong => write!(f, "目录名太长"),
        }
    }
}

impl std::error::Error for DirNameError {}

/// Windows 保留的设备名。
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// 单个名字段的最大字节数（各家文件系统的公共上限）。
pub const MAX_NAME_BYTES: usize = 255;

/// 校验一个**目录名**（不是路径）。
pub fn validate_dir_name(name: &str) -> Result<(), DirNameError> {
    if name.is_empty() || name.trim().is_empty() {
        return Err(DirNameError::Empty);
    }
    if name.chars().count() > 1 && name.contains(['/', '\\']) {
        return Err(DirNameError::Separator);
    }
    if name == "." || name == ".." {
        return Err(DirNameError::Relative);
    }
    for ch in name.chars() {
        if matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || ch.is_control() {
            return Err(DirNameError::IllegalChar(ch));
        }
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err(DirNameError::TrailingDotOrSpace);
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(DirNameError::TooLong);
    }
    // 保留名：`CON` 与 `CON.txt` 都算（Windows 只看到点之前那一段）
    let stem = name.split('.').next().unwrap_or(name).trim_end();
    if RESERVED.iter().any(|r| stem.eq_ignore_ascii_case(r)) {
        return Err(DirNameError::Reserved);
    }
    Ok(())
}

/// 把一个**库内相对路径**解析成绝对路径，并挡住越界。
///
/// 拒绝：绝对路径、`..`、以及任何能跳到库根外面的写法。
/// 允许空串（代表库根自己）与 `photos/2026-08-15` 这种多级相对路径。
pub fn resolve_inside(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel = rel.replace('\\', "/");
    // 绝对路径必须**在去掉首斜杠之前**拒掉 —— 先 trim 的话 `/etc` 会被削成 `etc` 溜过去
    if rel.starts_with('/') {
        return Err(format!("需要库内相对路径，不能是绝对路径（{rel}）"));
    }
    let trimmed = rel.trim_matches('/');
    if trimmed.is_empty() {
        return Ok(root.to_path_buf());
    }
    // 盘符形式（`C:/Windows`）：在 Linux 上它只是个普通名字，会一路溜到 `<root>/C:/Windows`
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return Err(format!("需要库内相对路径，不能是盘符路径（{rel}）"));
    }
    let candidate = Path::new(trimmed);
    for component in candidate.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {}
            Component::ParentDir => return Err(format!("相对路径不能包含 ..（{rel}）")),
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!("需要库内相对路径，不能是绝对路径（{rel}）"))
            }
        }
    }
    Ok(root.join(candidate))
}

/// 「这个目录能不能删」的报告（浅检查，只看**浏览界面里看得见的东西**）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VisibleReport {
    /// 用户可见的子目录个数（**不含** `_RAW`）
    pub visible_dir_count: u64,
    /// 照片个数：本层 + 本层 `_RAW/` 里的（RAW 与位图都算 —— 它们在网格里都是 tile）
    pub photo_count: u64,
    /// 有没有符号链接（保守：算「有东西」）
    pub has_link: bool,
}

impl VisibleReport {
    /// 能不能删：看不见任何照片、也看不见任何子目录、也没有链接。
    #[must_use]
    pub fn deletable(&self) -> bool {
        self.visible_dir_count == 0 && self.photo_count == 0 && !self.has_link
    }
}

/// `_RAW/` 的目录名（与 `import::plan::RAW_DIR` 同口径，比较大小写不敏感）。
const RAW_DIR: &str = "_RAW";

/// 浅检查：这一层在**浏览界面里**看得见东西吗（判据见文件头）。
///
/// 代价是**两次 `read_dir`**（本层 + `_RAW/`），不递归、不 stat 文件内容 ——
/// 所以选中目录时可以随时跑一次给按钮定 enable / disable。
pub fn check_visible_contents(dir: &Path) -> std::io::Result<VisibleReport> {
    let meta = std::fs::symlink_metadata(dir)?;
    if meta.is_symlink() {
        // 传进来的是个链接：绝对不动它
        return Ok(VisibleReport { has_link: true, ..VisibleReport::default() });
    }
    if !meta.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "不是一个目录",
        ));
    }
    let mut report = VisibleReport::default();
    count_layer(dir, &mut report, false)?;
    Ok(report)
}

/// 数一层。`in_raw` = 正在数 `_RAW/` 里面：那里只数照片，子目录与链接照旧保守处理。
fn count_layer(dir: &Path, report: &mut VisibleReport, in_raw: bool) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if file_type.is_symlink() {
            report.has_link = true;
            continue;
        }
        if file_type.is_dir() {
            // `_RAW/` 不是用户目录：它里面的照片算本目录的照片
            if in_raw || name.eq_ignore_ascii_case(RAW_DIR) {
                if in_raw {
                    // `_RAW/` 里面再套目录：浏览里一样看不见，照旧只数照片（不再往下走一层）
                    continue;
                }
                count_layer(&entry.path(), report, true)?;
            } else {
                report.visible_dir_count += 1;
            }
            continue;
        }
        if crate::media::kind::kind_of_file(&name).is_photo() {
            report.photo_count += 1;
        }
        // 余下的都是看不见的残留（`.xmp` / `.txt` / 缩略图 …）：不计数
    }
    Ok(())
}

/// 删除一个目录（连同看不见的残留）。**先查再删**，判据与按钮的 enable 是同一个函数。
///
/// 通过检查之后用 `remove_dir_all`：里面此刻只剩 `.xmp` / 空 `_RAW/` 这类看不见的东西。
/// 返回删掉的条目数（给了一句能看懂的报告用）。
pub fn remove_dir_checked(dir: &Path) -> std::io::Result<u64> {
    let report = check_visible_contents(dir)?;
    if !report.deletable() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::DirectoryNotEmpty,
            if report.has_link {
                "目录里有符号链接，出于安全没有删除".to_string()
            } else if report.photo_count > 0 {
                format!("目录里还有 {} 张照片，不能删", report.photo_count)
            } else {
                format!("目录里还有 {} 个子目录，先把它们删掉", report.visible_dir_count)
            },
        ));
    }
    // 约定几个文件系统都会有的错误（权限、被占用）要原样报出去
    std::fs::remove_dir_all(dir)?;
    Ok(1)
}

/// 在 `parent` 下建一个名为 `name` 的子目录。
///
/// 重名判定用**大小写不敏感**比较（Windows 口径）：`Photos` 与 `photos` 视为同名。
/// 返回新建的绝对路径。
pub fn create_subdir(parent: &Path, name: &str) -> Result<PathBuf, String> {
    validate_dir_name(name).map_err(|e| e.to_string())?;
    let target = parent.join(name);
    if let Some(existing) = find_child_ignoring_case(parent, name)? {
        return Err(format!("{} 已经存在", existing.display()));
    }
    // `create_dir`（不是 `create_dir_all`）：父目录不在就该报错，而不是替用户造一串
    std::fs::create_dir(&target).map_err(|e| format!("创建目录失败：{e}"))?;
    Ok(target)
}

/// 在 `parent` 里找一个名字**忽略大小写**等于 `name` 的条目。
fn find_child_ignoring_case(parent: &Path, name: &str) -> Result<Option<PathBuf>, String> {
    let entries = match std::fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("读目录失败：{e}")),
    };
    for entry in entries {
        let entry = entry.map_err(|e| format!("读目录失败：{e}"))?;
        if entry.file_name().to_string_lossy().eq_ignore_ascii_case(name) {
            return Ok(Some(entry.path()));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "raybend-dirs-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建临时目录");
        dir
    }

    // ── 名字校验 ──────────────────────────────────────────────

    #[test]
    fn accepts_normal_names() {
        for name in ["2026-08-15", "旅行", "raw_2026", "a", "带空格 的名字", "emoji-📷"] {
            assert!(validate_dir_name(name).is_ok(), "{name} 应当被接受");
        }
    }

    #[test]
    fn rejects_empty_and_blank() {
        assert_eq!(validate_dir_name(""), Err(DirNameError::Empty));
        assert_eq!(validate_dir_name("   "), Err(DirNameError::Empty));
        assert_eq!(validate_dir_name("\t"), Err(DirNameError::Empty));
    }

    #[test]
    fn rejects_separators_and_relative() {
        assert_eq!(validate_dir_name("a/b"), Err(DirNameError::Separator));
        assert_eq!(validate_dir_name("a\\b"), Err(DirNameError::Separator));
        assert_eq!(validate_dir_name(".."), Err(DirNameError::Relative));
        assert_eq!(validate_dir_name("."), Err(DirNameError::Relative));
    }

    #[test]
    fn rejects_windows_illegal_chars_and_controls() {
        for bad in ["a<b", "a>b", "a:b", "a\"b", "a|b", "a?b", "a*b", "a\u{0}b", "a\nb"] {
            assert!(
                matches!(validate_dir_name(bad), Err(DirNameError::IllegalChar(_))),
                "{bad} 应当被拒"
            );
        }
    }

    #[test]
    fn rejects_reserved_names_any_case_and_with_extension() {
        for bad in ["CON", "con", "Con", "NUL", "com1", "LPT9", "con.txt", "AUX.raw"] {
            assert_eq!(validate_dir_name(bad), Err(DirNameError::Reserved), "{bad}");
        }
        // 只是以保留名**开头**不算：`console` / `com10` 是合法名字
        assert!(validate_dir_name("console").is_ok());
        assert!(validate_dir_name("com10").is_ok());
    }

    #[test]
    fn rejects_trailing_dot_or_space() {
        assert_eq!(validate_dir_name("name."), Err(DirNameError::TrailingDotOrSpace));
        assert_eq!(validate_dir_name("name "), Err(DirNameError::TrailingDotOrSpace));
        // 中间的点/空格没问题
        assert!(validate_dir_name("a.b c").is_ok());
    }

    #[test]
    fn rejects_too_long() {
        assert!(validate_dir_name(&"a".repeat(MAX_NAME_BYTES)).is_ok());
        assert_eq!(validate_dir_name(&"a".repeat(MAX_NAME_BYTES + 1)), Err(DirNameError::TooLong));
        // 中文按**字节**算长度（文件系统的上限是字节）
        let cjk = "库".repeat(100); // 300 字节
        assert_eq!(validate_dir_name(&cjk), Err(DirNameError::TooLong));
    }

    // ── 相对路径解析 ──────────────────────────────────────────

    #[test]
    fn resolves_relative_and_blocks_escape() {
        let root = Path::new("/lib");
        assert_eq!(resolve_inside(root, "").unwrap(), PathBuf::from("/lib"));
        assert_eq!(resolve_inside(root, "photos/2026").unwrap(), PathBuf::from("/lib/photos/2026"));
        // 反斜杠也当分隔符（Windows 侧存进来的值）
        assert_eq!(resolve_inside(root, "photos\\2026").unwrap(), PathBuf::from("/lib/photos/2026"));
        for bad in ["../etc", "photos/../../etc", "/etc", "C:/Windows"] {
            assert!(resolve_inside(root, bad).is_err(), "{bad} 应当被拒");
        }
    }

    // ── 浅检查：「浏览里看得见东西吗」 ────────────────────────────

    #[test]
    fn empty_directory_is_deletable() {
        let root = temp_root("shallow-empty");
        std::fs::create_dir_all(root.join("a")).unwrap();
        let report = check_visible_contents(&root.join("a")).unwrap();
        assert_eq!(report, VisibleReport::default());
        assert!(report.deletable());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn residue_dot_xmp_and_empty_raw_do_not_block_deletion() {
        let root = temp_root("shallow-residue");
        std::fs::create_dir_all(root.join("a/_RAW")).unwrap();
        std::fs::write(root.join("a/IMG_0001.xmp"), b"<x/>").unwrap();
        std::fs::write(root.join("a/notes.txt"), b"hello").unwrap();
        let report = check_visible_contents(&root.join("a")).unwrap();
        assert!(report.deletable(), "看不见的残留不该阻止删除：{report:?}");
        // 删的时候一起走 —— 这是「库是我们自己维护的」那条口径
        assert_eq!(remove_dir_checked(&root.join("a")).unwrap(), 1);
        assert!(!root.join("a").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn photos_inside_raw_block_deletion() {
        let root = temp_root("shallow-raw-photo");
        std::fs::create_dir_all(root.join("a/_RAW")).unwrap();
        std::fs::write(root.join("a/_RAW/IMG_0001.ORF"), b"raw").unwrap();
        let report = check_visible_contents(&root.join("a")).unwrap();
        assert_eq!(report.photo_count, 1, "_RAW 里的照片是会显示成 tile 的");
        assert!(!report.deletable());
        let err = remove_dir_checked(&root.join("a")).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::DirectoryNotEmpty);
        assert!(root.join("a/_RAW/IMG_0001.ORF").exists(), "照片必须原样留着");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn any_subdirectory_blocks_deletion_but_deep_files_do_not_matter() {
        let root = temp_root("shallow-subdir");
        // 子目录里有照片：子目录本身已经足以挡住删除
        std::fs::create_dir_all(root.join("a/child")).unwrap();
        std::fs::write(root.join("a/child/photo.jpg"), b"x").unwrap();
        let report = check_visible_contents(&root.join("a")).unwrap();
        assert_eq!(report.visible_dir_count, 1);
        assert_eq!(report.photo_count, 0, "不递归数照片 —— 只看这一层");
        assert!(!report.deletable());
        let err = remove_dir_checked(&root.join("a")).unwrap_err();
        assert!(err.to_string().contains("子目录"), "{err}");
        assert!(root.join("a/child/photo.jpg").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn top_level_photo_blocks_deletion() {
        let root = temp_root("shallow-photo");
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::write(root.join("a/photo.JPG"), b"x").unwrap();
        assert!(!check_visible_contents(&root.join("a")).unwrap().deletable());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_or_file_target_is_an_error() {
        let root = temp_root("shallow-missing");
        assert!(check_visible_contents(&root.join("nope")).is_err());
        std::fs::write(root.join("f.txt"), b"x").unwrap();
        assert!(check_visible_contents(&root.join("f.txt")).is_err());
        assert!(remove_dir_checked(&root.join("f.txt")).is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn symlink_counts_as_something_and_is_never_deleted_through() {
        let root = temp_root("shallow-symlink");
        let outside = temp_root("shallow-symlink-outside");
        std::fs::write(outside.join("重要文件.jpg"), b"data").unwrap();
        std::fs::create_dir_all(root.join("tree")).unwrap();
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&outside, root.join("tree/link")).is_ok();
        #[cfg(not(unix))]
        let linked = false;

        if linked {
            let report = check_visible_contents(&root.join("tree")).unwrap();
            assert!(report.has_link);
            assert!(!report.deletable(), "有链接时保守判成有东西");
            assert!(remove_dir_checked(&root.join("tree")).is_err());
            assert!(outside.join("重要文件.jpg").exists(), "外部文件毫发无伤");
            // 链接自己的目标目录也不能被当成「目录里的东西」删掉
            assert!(std::fs::symlink_metadata(root.join("tree/link")).is_ok());
        }
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&outside).ok();
    }

    // ── 新建 ────────────────────────────────────────────────

    #[test]
    fn creates_subdir_and_returns_path() {
        let root = temp_root("create");
        let made = create_subdir(&root, "2026-09-17").unwrap();
        assert_eq!(made, root.join("2026-09-17"));
        assert!(made.is_dir());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn create_rejects_duplicate_ignoring_case() {
        let root = temp_root("dup");
        create_subdir(&root, "Photos").unwrap();
        let err = create_subdir(&root, "photos").unwrap_err();
        assert!(err.contains("已经存在"), "错误信息要能直接给用户看：{err}");
        // 目录里应当还是只有一个
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn create_supports_chinese_names() {
        let root = temp_root("cjk");
        let made = create_subdir(&root, "婚礼跟拍").unwrap();
        assert!(made.is_dir());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn create_does_not_make_parents() {
        let root = temp_root("no-parents");
        // 父目录不存在 ⇒ 报错，而不是替用户造一串
        assert!(create_subdir(&root.join("nope"), "child").is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn create_rejects_bad_names_without_touching_disk() {
        let root = temp_root("bad-names");
        for bad in ["", "..", "a/b", "CON", "name.", "a:b"] {
            assert!(create_subdir(&root, bad).is_err(), "{bad} 应当被拒");
        }
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        std::fs::remove_dir_all(&root).ok();
    }
}
