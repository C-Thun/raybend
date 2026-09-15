//! 路径语义：**NFC 规范化 + 大小写折叠 + 分隔符统一**。
//!
//! 为什么需要这一层（`AGENTS.md` §7.3）：同一张照片的路径在不同平台上的「相等」含义不同 ——
//!
//! | 平台 | 大小写 | 规范化 |
//! | --- | --- | --- |
//! | Windows | 不敏感 | 无（但保留用户输入的字节） |
//! | macOS | 不敏感（默认 HFS+/APFS） | **NFD**（分解形式） |
//! | Linux | 敏感 | 字节串，无规范化 |
//!
//! 如果把「相等」交给各处的字符串比较，这个差异就会散落到整个代码库；
//! 所以统一在这里压成**三种表示**，其余代码只认这三种：
//!
//! - [`PathForms::raw`]：**原始**输入（用户看得见的那个样子，含原始大小写与分隔符），
//!   用于显示与「原样还原」；
//! - [`PathForms::normalized`]：**NFC** 规范化 + 分隔符统一为 `/`（UNC 前缀保留 `\\`），
//!   用于跨平台稳定的展示与哈希；
//! - [`PathForms::folded`]：在 `normalized` 之上再做**小写折叠**，
//!   **唯一索引与相等判断一律用它**（`asset_files.path_folded`）。
//!
//! 注意：`folded` 是**我们自己定义**的折叠（Unicode `to_lowercase`），
//! 不是 Windows 的 `CompareStringOrdinal` 语义 —— 只要「写入」与「查询」都走这里，
//! 结果就是自洽的。真正的边界情况（极少见的 IPP/特殊折叠）留到将来按需处理。
//!
//! 这一层是**纯函数**：不碰磁盘、不解析 `..`（只有在文件系统上才能可靠解析）。

use unicode_normalization::UnicodeNormalization;

/// 一个路径的三种表示。
///
/// 由 [`PathForms::new`] 从任意路径字符串构造；`raw` 与 `normalized` 在
/// 「不需要转换」时会共享同一份内存（`String` 的 `Clone` 语义，不做额外优化）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathForms {
    raw: String,
    normalized: String,
    folded: String,
}

impl PathForms {
    /// 从任意路径字符串构造三种表示。
    ///
    /// 输入可以是 Windows 或 Unix 风格（`C:\a\b`、`/a/b`、`\\srv\share\x`、`a//b/`），
    /// 末尾分隔符会被去掉（根路径除外），重复分隔符会被合并。
    #[must_use]
    pub fn new(input: &str) -> Self {
        let raw = input.to_string();
        let normalized = normalize(input);
        let folded = normalized.to_lowercase();
        Self {
            raw,
            normalized,
            folded,
        }
    }

    /// 原始输入（用户看得见的样子）。
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// NFC 规范化 + 分隔符统一（`/`；UNC 前缀保留 `\\`）。
    #[must_use]
    pub fn normalized(&self) -> &str {
        &self.normalized
    }

    /// NFC + 小写折叠：**唯一索引与相等判断用这个**。
    #[must_use]
    pub fn folded(&self) -> &str {
        &self.folded
    }
}

/// 规范化一个路径：NFC + 分隔符统一 + 去末尾分隔符 + 合并重复分隔符。
///
/// 不做的事（有意为之）：
/// - 不解析 `..` / `.`（无文件系统就无法可靠解析符号链接与相对路径）；
/// - 不改变大小写（折叠是 `PathForms::folded` 的事）；
/// - 不补全相对路径为绝对路径。
#[must_use]
pub fn normalize(input: &str) -> String {
    // 1) NFC 规范化（macOS 可能给 NFD，Windows/Linux 一般是 NFC 或原样）
    let nfc: String = input.nfc().collect();

    // 2) UNC 前缀（\\server\share）要保留，否则会与 POSIX 的 // 混淆
    let (prefix, rest) = if let Some(r) = nfc.strip_prefix("\\\\") {
        ("\\\\", r)
    } else {
        ("", nfc.as_str())
    };

    // 3) 分隔符统一为 '/' + 合并重复 + 去末尾（根除外）
    let mut out = String::with_capacity(prefix.len() + rest.len());
    out.push_str(prefix);
    let mut last_was_sep = false;
    for ch in rest.chars() {
        let is_sep = ch == '/' || ch == '\\';
        if is_sep {
            if last_was_sep {
                continue; // 合并重复分隔符
            }
            out.push('/');
            last_was_sep = true;
        } else {
            out.push(ch);
            last_was_sep = false;
        }
    }

    // 4) 去掉末尾分隔符，但不能把根或盘符根吃掉："/" 与 "C:/" 保持原样
    while out.len() > prefix.len() + 1 && out.ends_with('/') {
        let candidate = out.len() - 1;
        let bytes = out.as_bytes();
        let is_drive_root = candidate == 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic();
        if is_drive_root {
            break;
        }
        out.pop();
    }
    out
}

/// 两个路径是否指向同一处（按 [`PathForms::folded`] 比较）。
#[must_use]
pub fn equivalent(a: &str, b: &str) -> bool {
    PathForms::new(a).folded == PathForms::new(b).folded
}

/// 是否为 UNC 路径（`\\server\share\...`）。
#[must_use]
pub fn is_unc(path: &str) -> bool {
    let t = path.trim_start();
    t.starts_with("\\\\") || t.starts_with("//")
}

/// 是否为（类）绝对路径：Windows 盘符、UNC、或 POSIX `/` 开头。
#[must_use]
pub fn is_absolute_like(path: &str) -> bool {
    if is_unc(path) {
        return true;
    }
    let t = path.trim_start();
    if t.starts_with('/') {
        return true;
    }
    // 盘符形式：C:\ 或 C:/（注意：单个字母后跟分隔符不算盘符，那是相对路径 a/b）
    let mut it = t.chars();
    matches!((it.next(), it.next()), (Some(c), Some(':')) if c.is_ascii_alphabetic())
}

/// 取路径的最后一段（文件名或目录名）；路径以分隔符结尾时也给出最后一段。
///
/// 返回 `String` 而不是 `&str`：内部要先做规范化，切片无处可借。
#[must_use]
pub fn file_name(path: &str) -> Option<String> {
    let n = normalize(path);
    let trimmed = n.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    // 根路径（"/" 或 "C:"）没有名字
    if trimmed == "/" || (trimmed.len() == 2 && trimmed.ends_with(':')) {
        return None;
    }
    trimmed
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

/// 取父目录（规范化后的表示）。根路径与无父目录的输入返回 `None`。
#[must_use]
pub fn parent(path: &str) -> Option<String> {
    let n = normalize(path);
    let trimmed = n.trim_end_matches('/');
    let idx = trimmed.rfind('/')?;
    if idx == 0 {
        return Some("/".to_string());
    }
    let parent = &trimmed[..idx];
    // Windows 盘符根："C:" → "C:/"
    if parent.len() == 2 && parent.ends_with(':') {
        return Some(format!("{parent}/"));
    }
    Some(parent.to_string())
}

/// 扩展名（小写、不含点）。没有扩展名时返回 `None`。
///
/// 只看**最后一段**里的最后一个点；以点开头的隐藏文件（`.gitignore`）视为无扩展名。
#[must_use]
pub fn extension(path: &str) -> Option<String> {
    let name = file_name(path)?;
    let idx = name.rfind('.')?;
    if idx == 0 || idx + 1 >= name.len() {
        return None;
    }
    Some(name[idx + 1..].to_lowercase())
}

/// 文件主名（不含扩展名，保留原始大小写）。
#[must_use]
pub fn file_stem(path: &str) -> Option<String> {
    let name = file_name(path)?;
    match name.rfind('.') {
        Some(idx) if idx > 0 => Some(name[..idx].to_string()),
        _ => Some(name),
    }
}

/// 把规范化路径（`/` 分隔）转成当前平台的 `PathBuf`。
#[must_use]
pub fn to_platform_path(normalized: &str) -> std::path::PathBuf {
    if cfg!(windows) {
        // Windows 上 '/' 与 '\\' 等价（Win32 API 都接受），UNC 前缀原样保留
        std::path::PathBuf::from(normalized.replace('/', "\\"))
    } else {
        std::path::PathBuf::from(normalized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- NFC / Unicode ----------

    #[test]
    fn nfc_composes_decomposed_input() {
        // U+0065 U+0301（e + 组合尖音符）→ U+00E9（é）
        let decomposed = "Cafe\u{0301}/x.jpg";
        let f = PathForms::new(decomposed);
        assert_eq!(f.normalized(), "Café/x.jpg");
        // 与预组合形式等价
        assert!(equivalent("Cafe\u{0301}/x.jpg", "Café/x.jpg"));
    }

    #[test]
    fn chinese_path_is_stable() {
        let p = "D:\\照片\\2026-08-15\\婚礼\\MYP0001.png";
        let f = PathForms::new(p);
        assert_eq!(f.normalized(), "D:/照片/2026-08-15/婚礼/MYP0001.png");
        assert_eq!(f.folded(), "d:/照片/2026-08-15/婚礼/myp0001.png");
        // 中文大小写折叠不改变汉字
        assert!(equivalent(p, "d:\\照片\\2026-08-15\\婚礼\\myp0001.PNG"));
    }

    #[test]
    fn unusual_unicode_is_preserved() {
        // emoji、非 BMP 字符、全角
        let f = PathForms::new("E:\\照片📷\\ＡＢＣ.png");
        assert_eq!(f.normalized(), "E:/照片📷/ＡＢＣ.png");
        // 全角 Ａ 折叠为全角 ａ（Unicode 定义如此，不做 ASCII 化）
        assert!(f.folded().contains("ａｂｃ"));
    }

    // ---------- 大小写折叠 ----------

    #[test]
    fn case_equivalence_on_all_platforms() {
        assert!(equivalent(r"C:\Users\A\Pic.JPG", r"c:/users/a/pic.jpg"));
        assert!(!equivalent(r"C:\Users\A\pic1.jpg", r"C:\Users\A\pic2.jpg"));
    }

    #[test]
    fn turkish_and_german_folding_is_self_consistent() {
        // ß 折叠为 ß（Rust 的 to_lowercase 不做 ss 展开）；只要两端一致即可
        assert!(equivalent("STRASSE", "strasse"));
        assert!(equivalent("Straße", "straße"));
        // 不能把二者混为一谈（我们不假装知道德语规则）
        assert!(!equivalent("Straße", "Strasse"));
    }

    // ---------- 分隔符 ----------

    #[test]
    fn separators_are_unified_and_deduplicated() {
        assert_eq!(normalize("C:\\a//b\\\\c"), "C:/a/b/c");
        assert_eq!(normalize("/usr//local///bin"), "/usr/local/bin");
        assert_eq!(normalize("a/b\\c"), "a/b/c");
    }

    #[test]
    fn trailing_separator_is_removed_except_root() {
        assert_eq!(normalize("C:\\a\\b\\"), "C:/a/b");
        assert_eq!(normalize("/a/b///"), "/a/b");
        assert_eq!(normalize("/"), "/");
        assert_eq!(normalize("C:\\"), "C:/");
        assert_eq!(normalize("a/"), "a");
    }

    #[test]
    fn unc_paths_keep_their_prefix() {
        let f = PathForms::new(r"\\server\share\folder\a.jpg");
        assert_eq!(f.normalized(), r"\\server/share/folder/a.jpg");
        assert!(is_unc(r"\\server\share"));
        assert!(is_unc("//server/share"));
        assert!(!is_unc(r"C:\server\share"));
        // UNC 的规范化不能被当成 POSIX 的 //
        assert!(!equivalent(r"\\srv\share\a.jpg", "//srv/share/a.jpg"));
    }

    #[test]
    fn empty_and_degenerate_inputs() {
        assert_eq!(normalize(""), "");
        assert_eq!(normalize("."), ".");
        assert_eq!(normalize(".."), "..");
        assert_eq!(normalize("a"), "a");
        assert_eq!(normalize("///"), "/");
        // 只有分隔符的 Windows 样式会退化成 "/"
        assert_eq!(normalize("\\\\"), r"\\");
    }

    #[test]
    fn relative_and_dot_segments_are_not_resolved() {
        // 有意不解析：没有文件系统就无法可靠解析
        assert_eq!(normalize("./a/../b"), "./a/../b");
        assert_eq!(normalize("../x"), "../x");
    }

    // ---------- 绝对路径判定 ----------

    #[test]
    fn absolute_like_detection() {
        assert!(is_absolute_like(r"C:\a"));
        assert!(is_absolute_like("c:/a"));
        assert!(is_absolute_like("/a"));
        assert!(is_absolute_like(r"\\srv\share"));
        assert!(!is_absolute_like("a/b"));
        assert!(!is_absolute_like("./a"));
        assert!(!is_absolute_like(""));
        // 形如 "1:\a" 不算盘符
        assert!(!is_absolute_like(r"1:\a"));
    }

    // ---------- 拆解 ----------

    #[test]
    fn file_name_and_extension() {
        assert_eq!(file_name(r"C:\a\b\photo.RAW").as_deref(), Some("photo.RAW"));
        assert_eq!(file_name("/a/b/").as_deref(), Some("b"));
        assert_eq!(file_name("/"), None);
        assert_eq!(file_name(""), None);

        assert_eq!(extension(r"C:\a\photo.RAW").as_deref(), Some("raw"));
        assert_eq!(extension("a/b.tar.gz").as_deref(), Some("gz"));
        assert_eq!(extension("a/.gitignore"), None);
        assert_eq!(extension("a/noext"), None);
        assert_eq!(extension("a/trailing."), None);

        assert_eq!(file_stem("a/b.tar.gz").as_deref(), Some("b.tar"));
        assert_eq!(file_stem("a/.gitignore").as_deref(), Some(".gitignore"));
        assert_eq!(file_stem("a/noext").as_deref(), Some("noext"));
    }

    #[test]
    fn parent_paths() {
        assert_eq!(parent(r"C:\a\b").as_deref(), Some("C:/a"));
        assert_eq!(parent(r"C:\a").as_deref(), Some("C:/"));
        assert_eq!(parent("/a").as_deref(), Some("/"));
        assert_eq!(parent("/a/b/c").as_deref(), Some("/a/b"));
        assert_eq!(parent("a"), None);
        assert_eq!(parent(""), None);
    }

    // ---------- 超长与非法字符（不 panic 即可） ----------

    #[test]
    fn very_long_path_does_not_panic() {
        let long = "a/".repeat(5000) + "x.jpg";
        let f = PathForms::new(&long);
        assert!(f.normalized().ends_with("x.jpg"));
        assert_eq!(file_name(&long).as_deref(), Some("x.jpg"));
    }

    #[test]
    fn control_characters_pass_through() {
        // 非法字符的**拒绝**是导入模版校验的事，这里只保证不 panic、不丢内容
        let weird = "a/\u{0001}\u{0007}.jpg";
        let f = PathForms::new(weird);
        assert_eq!(f.normalized(), weird);
    }

    #[test]
    fn raw_is_untouched() {
        let input = r"\\SRV\Share\Mixed\Case\F ile.JPG";
        let f = PathForms::new(input);
        assert_eq!(f.raw(), input);
        assert_ne!(f.raw(), f.normalized());
    }

    // ---------- 平台路径转换 ----------

    #[test]
    fn platform_path_conversion() {
        let p = to_platform_path("C:/照片/a.jpg");
        if cfg!(windows) {
            assert_eq!(p.to_string_lossy(), r"C:\照片\a.jpg");
        } else {
            assert_eq!(p.to_string_lossy(), "C:/照片/a.jpg");
        }
    }
}
