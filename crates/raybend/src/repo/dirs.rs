//! 库根目录层面的目录操作：**新建子目录**与**删除空目录**。
//!
//! 这两个动作来自浏览模式左列目录树行尾的 `⋯` 菜单（`BROWSE.md` §4.3）：
//!
//! | 菜单项 | 规则 |
//! | --- | --- |
//! | 删除空目录 | 选项**一直在**，目录不为空时禁用；判定要**深度检索**（整个子树里没有文件才算空），删除时连同其下所有空子目录一起删 |
//! | 创建子目录 | 名字要校验（Windows 口径），**重名要挡住**（大小写不敏感） |
//!
//! # 为什么校验按 Windows 口径
//!
//! 主战场是 Windows（`AGENTS.md` §1）。一个在 Linux 上能建、到 Windows 上就建不出来的名字，
//! 等到用户换平台才炸是最糟的失败方式 —— 所以在**所有平台**上按 Windows 的规则拒绝：
//! 非法字符 `<>:"/\|?*`、保留名（`CON` / `NUL` / `COM1`…）、结尾的点与空格、控制字符。
//!
//! # 两条不可逾越的底线
//!
//! 1. **删除只删空目录**：实现只调用 `remove_dir`（目录非空时它自己会失败），
//!    绝不出现任何形式的递归删除 —— 那是一条能把用户照片删光的路径。
//! 2. **离线/权限问题一律当失败**：拿不准就不动，宁可让用户再点一次。

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

/// 深度检查的结果。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EmptyReport {
    /// 整个子树里一个文件都没有
    pub empty: bool,
    /// 子树里的文件数（用来解释「为什么不能删」）
    pub file_count: u64,
    /// 子树里的目录数（**不含**传进来的这个目录自己）
    pub dir_count: u64,
    /// 其中空目录的个数（删除时会被一起带走）
    pub empty_dir_count: u64,
    /// 遇到符号链接之类不好判断的东西（保守起见算「不空」，也不去删它）
    pub has_unresolved_link: bool,
}

/// 深度检索：`dir` 这棵子树里有没有文件。
///
/// 规则：
/// - 只看**文件**（目录不计数）—— 空目录不阻止删除，它们本来就是要一起删的；
/// - **不跟随符号链接**：遇到链接当作「不好判断」，记 `has_unresolved_link` 并把整棵树判成不空
///   （删别人指向的东西不是这个功能该干的事）；
/// - `dir` 自己不存在 → 报错（不是「空」）。
pub fn check_empty_tree(dir: &Path) -> std::io::Result<EmptyReport> {
    let meta = std::fs::symlink_metadata(dir)?;
    if !meta.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "不是一个目录",
        ));
    }
    let mut report = EmptyReport::default();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                report.has_unresolved_link = true;
                continue;
            }
            if file_type.is_dir() {
                report.dir_count += 1;
                // 先当空的，往下走；下面若发现文件会把它记成非空
                stack.push(entry.path());
            } else {
                report.file_count += 1;
            }
        }
    }
    // 再数一遍空目录：**自底向上**判定（判定规则见 `count_empty_dirs`）
    report.empty_dir_count = count_empty_dirs(dir)?;
    report.empty = report.file_count == 0 && !report.has_unresolved_link;
    Ok(report)
}

/// 数这棵子树里有多少个**空目录**：不包含该目录自己。
///
/// 「空」的判定是**递归的**：一个目录下面的所有子目录都空、自己又不含文件，它也算空
/// （它会随着删除一起消失）。所以 `a/b/c` 全空时是 **3 个**空目录，而不是 1 个叶子。
fn count_empty_dirs(dir: &Path) -> std::io::Result<u64> {
    let mut count = 0;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let child = entry.path();
        if !has_file_under(&child)? {
            count += 1;
        }
        count += count_empty_dirs(&child)?;
    }
    Ok(count)
}

/// 这棵子树（含自己这一层）里有没有文件。
///
/// 符号链接按「有东西」算 —— 它指向什么这里并不去看（也不跟随）。
fn has_file_under(dir: &Path) -> std::io::Result<bool> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            return Ok(true);
        }
        if file_type.is_dir() {
            if has_file_under(&entry.path())? {
                return Ok(true);
            }
        } else {
            return Ok(true);
        }
    }
    Ok(false)
}

/// 删除**空的**子树：`dir` 自己以及其下所有空目录。
///
/// **先查再删**：整棵子树里只要有文件就**一律不动**并报错。
/// 刻意不做「先把空子目录清掉、最后卡在非空的目标上」——那种半途而废最难解释，
/// 用户看到的是「报错了，但东西少了一些」。（界面上非空时菜单项本来就是禁用的，
/// 这里只是兜住「点了之后才有文件冒出来」那种竞态。）
///
/// 返回删掉的目录个数。**不会**删除任何文件：底层只调 `remove_dir`，
/// 而它对非空目录必然失败 —— 这条路径不可能删到用户的照片。
pub fn remove_empty_tree(dir: &Path) -> std::io::Result<usize> {
    let report = check_empty_tree(dir)?;
    if !report.empty {
        return Err(std::io::Error::new(
            std::io::ErrorKind::DirectoryNotEmpty,
            if report.has_unresolved_link {
                "目录里有符号链接，出于安全不自动删除".to_string()
            } else {
                format!("目录里还有 {} 个文件，不能删", report.file_count)
            },
        ));
    }
    let mut removed = remove_empty_children(dir)?;
    // 整个子树已经确认无文件，最后删自己
    std::fs::remove_dir(dir)?;
    removed += 1;
    Ok(removed)
}

fn remove_empty_children(dir: &Path) -> std::io::Result<usize> {
    let mut removed = 0usize;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        // 链接不跟随、也不删（那是别人家的东西）
        if file_type.is_symlink() || !file_type.is_dir() {
            continue;
        }
        let child = entry.path();
        removed += remove_empty_children(&child)?;
        // 只有真的空了才删；非空会失败 —— 那属于「这个目录不该删」，跳过而不是报错
        if std::fs::read_dir(&child)?.next().is_none() {
            std::fs::remove_dir(&child)?;
            removed += 1;
        }
    }
    Ok(removed)
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

    // ── 深度空判定 ────────────────────────────────────────────

    #[test]
    fn reports_empty_for_truly_empty_tree() {
        let root = temp_root("empty");
        std::fs::create_dir_all(root.join("a/b/c")).unwrap();
        let report = check_empty_tree(&root).unwrap();
        assert!(report.empty);
        assert_eq!(report.file_count, 0);
        assert_eq!(report.dir_count, 3);
        assert_eq!(report.empty_dir_count, 3);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn reports_not_empty_when_a_file_hides_deep_down() {
        let root = temp_root("deep-file");
        std::fs::create_dir_all(root.join("a/b/c")).unwrap();
        std::fs::write(root.join("a/b/c/photo.jpg"), b"x").unwrap();
        let report = check_empty_tree(&root).unwrap();
        assert!(!report.empty);
        assert_eq!(report.file_count, 1);
        // c 非空 → c 不算空目录；a、b 里有东西（c）也不算
        assert_eq!(report.empty_dir_count, 0);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn counts_empty_siblings_alongside_a_file() {
        let root = temp_root("mixed");
        std::fs::create_dir_all(root.join("keep")).unwrap();
        std::fs::create_dir_all(root.join("empty1/empty2")).unwrap();
        std::fs::write(root.join("keep/photo.jpg"), b"x").unwrap();
        let report = check_empty_tree(&root).unwrap();
        assert!(!report.empty);
        assert_eq!(report.file_count, 1);
        assert_eq!(report.empty_dir_count, 2, "empty1/empty2 都空");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_directory_is_an_error_not_empty() {
        let root = temp_root("missing");
        assert!(check_empty_tree(&root.join("nope")).is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn file_passed_as_directory_is_rejected() {
        let root = temp_root("file-as-dir");
        std::fs::write(root.join("f.txt"), b"x").unwrap();
        assert!(check_empty_tree(&root.join("f.txt")).is_err());
        assert!(remove_empty_tree(&root.join("f.txt")).is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    // ── 删除 ────────────────────────────────────────────────

    #[test]
    fn removes_whole_empty_tree_including_itself() {
        let root = temp_root("remove");
        std::fs::create_dir_all(root.join("gone/child/grand")).unwrap();
        let removed = remove_empty_tree(&root.join("gone")).unwrap();
        assert_eq!(removed, 3);
        assert!(!root.join("gone").exists());
        // 库根自己还在
        assert!(root.exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn refuses_to_touch_a_tree_that_has_files() {
        let root = temp_root("keep-files");
        std::fs::create_dir_all(root.join("keep/empty-child")).unwrap();
        std::fs::write(root.join("keep/photo.jpg"), b"photo").unwrap();
        // 非空 ⇒ **一点都不动**（连空子目录也不碰），并且要明确报错
        let err = remove_empty_tree(&root.join("keep")).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::DirectoryNotEmpty);
        assert!(err.to_string().contains("1 个文件"), "错误信息要能直接给用户看：{err}");
        assert!(root.join("keep/photo.jpg").exists(), "文件必须原样留着");
        assert!(root.join("keep/empty-child").exists(), "不空就一律不动");
        // 空的那一棵该删干净
        assert_eq!(remove_empty_tree(&root.join("keep/empty-child")).unwrap(), 1);
        assert!(root.join("keep/photo.jpg").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn refuses_to_follow_symlinks() {
        let root = temp_root("symlink");
        let outside = temp_root("symlink-outside");
        std::fs::write(outside.join("重要文件.jpg"), b"data").unwrap();
        std::fs::create_dir_all(root.join("tree")).unwrap();
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&outside, root.join("tree/link")).is_ok();
        #[cfg(not(unix))]
        let linked = false;

        if linked {
            let report = check_empty_tree(&root.join("tree")).unwrap();
            assert!(!report.empty, "有链接时保守判成不空");
            assert!(report.has_unresolved_link);
            // 删不掉（因为不空），而且外部文件必须毫发无伤
            assert!(remove_empty_tree(&root.join("tree")).is_err());
            assert!(outside.join("重要文件.jpg").exists());
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
