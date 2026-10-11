//! **中心化脱敏**：日志与崩溃记录的**唯一闸门**（`specs/logging-and-crash-report.md` §8）。
//!
//! 纪律：所有写入必须先过这里 —— 不指望 120+ 个调用点各自记得（那正是必须中心化的理由）。
//!
//! 这一层管**有固定形态**的东西：盘符/UNC/绝对路径、邮箱、IP、URL。
//! 没有固定形态的（预设名、LUT 名、标签名、搜索词）由**调用点**用 [`name`] / [`text`] 助手标注
//! —— 它们在文本里长得和普通词一样，正则救不了，只能靠知道语义的那一侧。
//!
//! ## 三条性质（§8.1）
//!
//! * **确定性**：同一条路径在同一会话里永远映射到同一个 `#hash`，便于对齐日志行；
//! * **不可逆**：短哈希用**每次启动随机生成的盐**（[`Redactor::new`]），跨会话不可关联；
//! * **源码路径例外**：panic 位置这类代码信息不是用户信息 —— 含 `crates/…/src/…` 特征的路径
//!   会被**截到相对段**保留（同时丢掉 `/home/某某/…` 这种构建机前缀）。
//!
//! 已知边界（§8.4，如实写下来而不是假装没有）：用户自己写的自由文本（预设名等）不脱敏；
//! 我们只保证「固定形态」的部分不泄漏，剩下靠「不自动上传」这条红线兜底。

use std::sync::OnceLock;

use sha2::{Digest, Sha256};

/// 脱敏器。生产用一个**进程级随机盐**（[`global`]），测试可以注入固定盐。
#[derive(Debug, Clone)]
pub struct Redactor {
    salt: [u8; 32],
}

impl Redactor {
    /// 随机盐（`getrandom` 已在依赖树里，不回退、不 panic —— 拿不到随机源就用时间戳兜底，
    /// 隐私上弱一点也远好过让日志系统把应用拖崩）。
    #[must_use]
    pub fn new() -> Self {
        let mut salt = [0_u8; 32];
        if getrandom::fill(&mut salt).is_err() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            let digest = Sha256::digest(now.to_le_bytes());
            salt.copy_from_slice(&digest);
        }
        Self { salt }
    }

    /// 固定盐（单测与「把同一份日志的盐固定住」用）。
    #[must_use]
    pub fn with_salt(salt: [u8; 32]) -> Self {
        Self { salt }
    }

    /// **唯一入口**：一行文本进，脱敏后的文本出。
    #[must_use]
    pub fn line(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut prev: Option<char> = None;
        let mut rest = text;
        while !rest.is_empty() {
            match self.match_at(rest, prev) {
                Some((consumed, replacement)) => {
                    out.push_str(&replacement);
                    prev = replacement.chars().next_back();
                    rest = &rest[consumed..];
                }
                None => {
                    let Some(ch) = rest.chars().next() else { break };
                    out.push(ch);
                    prev = Some(ch);
                    rest = &rest[ch.len_utf8()..];
                }
            }
        }
        out
    }

    /// 在 `head` 的起点尝试匹配一个可脱敏形态。返回（吃掉多少字节，替换成什么）。
    fn match_at(&self, head: &str, prev: Option<char>) -> Option<(usize, String)> {
        if !is_token_start(prev, head) {
            return None;
        }
        if let Some(len) = url_len(head) {
            return Some((len, "<url>".to_string()));
        }
        if head.starts_with("\\\\") {
            let len = scan_path_end(head);
            return Some((len, "<unc>".to_string()));
        }
        if let Some(len) = self.drive_path_len(head) {
            return Some((len, self.path_token(&head[..len])));
        }
        if let Some(len) = unix_abs_path_len(head) {
            return Some((len, self.path_token(&head[..len])));
        }
        if let Some(len) = email_len(head) {
            return Some((len, "<email>".to_string()));
        }
        if let Some(len) = ipv4_len(head) {
            return Some((len, "<ip>".to_string()));
        }
        None
    }

    /// 盘符路径（`C:\…` / `C:/…`）的长度（扫描到路径结束）。
    fn drive_path_len(&self, head: &str) -> Option<usize> {
        let bytes = head.as_bytes();
        let drive = matches!(bytes.first(), Some(b) if b.is_ascii_alphabetic());
        let sep = matches!(bytes.get(1), Some(b':')) && matches!(bytes.get(2), Some(b'\\' | b'/'));
        if drive && sep {
            return Some(scan_path_end(head));
        }
        None
    }

    /// 把一个路径形态换成 token：`<path:.jpg#3f9a>` / `<path#3f9a>`；源码路径截段保留。
    fn path_token(&self, raw: &str) -> String {
        let trimmed = raw.trim_end_matches(['\\', '/']);
        if let Some(source) = source_relative_of(trimmed) {
            // 代码信息，不是用户信息（§8.2 最后一行）；截掉构建机前缀
            return source;
        }
        let ext = extension_of(trimmed);
        let hash = self.short_hash(trimmed);
        if ext.is_empty() {
            format!("<path#{hash}>")
        } else {
            format!("<path:{ext}#{hash}>")
        }
    }

    /// 盐化短哈希（4 位十六进制）。
    fn short_hash(&self, value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.salt);
        hasher.update(value.as_bytes());
        let digest = hasher.finalize();
        format!("{:02x}{:02x}", digest[0], digest[1])
    }
}

impl Default for Redactor {
    fn default() -> Self {
        Self::new()
    }
}

/// 进程级脱敏器（首次使用时生成随机盐）。
#[must_use]
pub fn global() -> &'static Redactor {
    static GLOBAL: OnceLock<Redactor> = OnceLock::new();
    GLOBAL.get_or_init(Redactor::new)
}

/// 快捷入口：用进程级盐脱敏一行。
#[must_use]
pub fn line(text: &str) -> String {
    global().line(text)
}

/// **调用点助手**：名字类（预设 / LUT / 标签 / 集合 / 库名）一律记成 `<name>`。
///
/// 它们没有可识别的形态，只有在**知道这是名字**的地方才能救。用法：
/// `log::info!(target: "presets", "套用预设 name={}", diagnostics::redact::name(&preset.name))`。
#[must_use]
pub fn name(_value: &str) -> &'static str {
    "<name>"
}

/// **调用点助手**：搜索词与其它自由文本一律记成 `<text:长度>`（只留长度，不留内容）。
#[must_use]
pub fn text(value: &str) -> String {
    format!("<text:{}>", value.chars().count())
}

/// 是不是 token 起点。
///
/// `prev` 是**前一个字符**（`None` = 行首）：只有在「上一字符是边界」时才允许开始一个新 token。
/// 这个判断防的是两种切碎：① 标识符中间切一刀（`crates` 后面的 `/raybend/...` 不该被当成路径）；
/// ② 已经脱敏过的 token 再被套一层（`<path:.jpg#ab12>` 里的字符不该重新入扫描）。
fn is_token_start(prev: Option<char>, head: &str) -> bool {
    let Some(first) = head.chars().next() else {
        return false;
    };
    // 起点自身必须像「路径或地址的开头」，否则每个字母都要跑一遍判定
    if !(first == '\\' || first == '/' || first.is_ascii_alphanumeric()) {
        return false;
    }
    match prev {
        None => true,
        Some(prev) => {
            !prev.is_ascii_alphanumeric()
                && prev != '_'
                && prev != '\\'
                && prev != '/'
                && prev != '.'
        }
    }
}

/// URL（`scheme://…`）长度。
fn url_len(head: &str) -> Option<usize> {
    let scheme_end = head.find("://")?;
    let scheme = &head[..scheme_end];
    if scheme.is_empty() || scheme.len() > 16 {
        return None;
    }
    let valid = scheme.chars().next()?.is_ascii_alphabetic()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !valid {
        return None;
    }
    Some(scan_url_end(head))
}

fn scan_url_end(head: &str) -> usize {
    let mut end = 0;
    for (idx, ch) in head.char_indices() {
        if ch.is_whitespace() || matches!(ch, '"' | '\'' | '>' | '<' | '|' | '，' | '。' | '）' | '】')
        {
            break;
        }
        end = idx + ch.len_utf8();
    }
    end
}

/// 扫描一段路径到哪里结束。
///
/// 规则：引号 / 括号 / 中文标点 / 行尾都算结束；**空格不一定结束** ——
/// 后面若跟着以 `\` 或 `/` 开头的续段（`C:\Users\张 三\图片\a.jpg`、`\\nas\My Photos\b.jpg`），
/// 说明这是「带空格的路径」而不是「路径 + 一句话」，继续吞。
fn scan_path_end(head: &str) -> usize {
    let bytes = head.as_bytes();
    let mut i = 0;
    let mut last = 0;
    while i < bytes.len() {
        let Some(ch) = head[i..].chars().next() else { break };
        if is_path_stop(ch) {
            break;
        }
        if ch.is_whitespace() {
            let mut j = i + ch.len_utf8();
            while j < bytes.len() {
                let Some(next) = head[j..].chars().next() else { break };
                if next.is_whitespace() {
                    j += next.len_utf8();
                } else {
                    break;
                }
            }
            let continues = head[j..].starts_with('\\') || head[j..].starts_with('/');
            if continues {
                i = j;
                continue;
            }
            break;
        }
        i += ch.len_utf8();
        last = i;
    }
    last
}

fn is_path_stop(ch: char) -> bool {
    matches!(
        ch,
        '"' | '\'' | '`' | '>' | '<' | '|' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | '*' | '?' | '，' | '。' | '；' | '：' | '）' | '（' | '、' | '！' | '？' | '】' | '【'
    )
}

/// Unix 绝对路径（`/mnt/…`、`/home/…`、`/Users/…`）：以 `/` 开头、第二个字符是字母数字或 `_`。
fn unix_abs_path_len(head: &str) -> Option<usize> {
    let bytes = head.as_bytes();
    if bytes.first() != Some(&b'/') {
        return None;
    }
    let second = bytes.get(1)?;
    if !(second.is_ascii_alphanumeric() || *second == b'_' || *second == b'.') {
        return None;
    }
    Some(scan_path_end(head))
}

/// 邮箱：`名字@域名.后缀`（两侧不许再出现 `@` 或空白）。
fn email_len(head: &str) -> Option<usize> {
    let mut end = 0;
    for (idx, ch) in head.char_indices() {
        if ch.is_whitespace() || is_path_stop(ch) || ch == '/' || ch == '\\' {
            break;
        }
        end = idx + ch.len_utf8();
    }
    if end == 0 {
        return None;
    }
    // 尾部可能粘着中文/标点，先裁掉再判定
    let candidate = head[..end].trim_end_matches(['.', ',', '，', '。', '！', '？', '）']);
    let (local, domain) = candidate.split_once('@')?;
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        return None;
    }
    let (host, tld) = domain.rsplit_once('.')?;
    if host.is_empty() || tld.is_empty() || !tld.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some(candidate.len())
}

/// IPv4：四段 0-255（只认 IPv4 —— IPv6 的 `:` 与时间戳/字段分隔撞车，误伤代价更大）。
fn ipv4_len(head: &str) -> Option<usize> {
    let mut end = 0;
    for (idx, ch) in head.char_indices() {
        if !(ch.is_ascii_digit() || ch == '.') {
            break;
        }
        end = idx + 1;
    }
    if end == 0 {
        return None;
    }
    let candidate = &head[..end];
    let parts: Vec<&str> = candidate.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let valid = parts.iter().all(|part| {
        !part.is_empty() && part.len() <= 3 && part.parse::<u16>().is_ok_and(|n| n <= 255)
    });
    // 别把版本号 `1.2.3.4` 之外的正常路径吃掉：调用点用 `v=1.2.3.4` 也没办法，误伤可接受
    valid.then_some(end)
}

/// 我们自己的源码路径（`crates/<name>/src/…`、`src-tauri/src/…`、`src/…`）→ 截到相对段。
///
/// 判据是**目录特征**而不是前缀：`/home/某某/repos/raybend/crates/raybend/src/editor.rs`
/// 与 `\\wsl.localhost\…\crates\raybend\src\editor.rs` 都能截出 `crates/raybend/src/editor.rs`。
fn source_relative_of(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    for anchor in ["crates/", "src-tauri/"] {
        if let Some(idx) = normalized.find(anchor) {
            let tail = &normalized[idx..];
            // 只在「像源码根」时认：锚点后确实跟着 src/（用户目录里碰巧有 crates/ 也不影响 ——
            // 我们只保留锚点之后的部分，前缀一律丢掉）
            if tail.contains("/src/") || tail.starts_with("src-tauri/") {
                return Some(tail.to_string());
            }
        }
    }
    if normalized.starts_with("src/") {
        return Some(normalized);
    }
    None
}

/// 取扩展名（小写、带点、≤8 个字母数字）；目录或没有扩展名返回空串。
fn extension_of(path: &str) -> String {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let Some((_, ext)) = file.rsplit_once('.') else {
        return String::new();
    };
    if ext.is_empty() || ext.len() > 8 || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        return String::new();
    }
    format!(".{}", ext.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed() -> Redactor {
        Redactor::with_salt([7_u8; 32])
    }

    /// 规格 §8.3 的金丝雀：真实形态语料，断言输出里**不出现**任何原串片段。
    #[test]
    fn canary_corpus_never_leaks_original_fragments() {
        let cases = [
            r"C:\Users\某某\Pictures\2024\a.jpg",
            r"C:\Users\张 三\My Photos\旅行 2024\DSC_0001.RAF",
            r"\\nas-01\photos\2024\sunset.jpg",
            "/mnt/c/src/tmp/测试库/photos/2026-09-13/a.jpg",
            "/home/alice/repos/travel-photos/2024/japan/x.nef",
            "zhang.san@example.com",
            "192.168.1.23",
            "http://192.168.1.5:8080/api?token=abc123",
            "https://nas.local/photos/a.jpg",
            r"C:\Users\😀\emoji 🎉 目录\b.png",
            r"panic at C:\Users\bob\secret\photos\raw\crash.RAF: no such file",
        ];
        for case in cases {
            let redacted = fixed().line(case);
            for fragment in [
                "某某", "张 三", "nas-01", "测试库", "alice", "zhang.san", "192.168.", "token=abc123",
                "emoji", "bob", "secret", "travel-photos",
            ] {
                assert!(
                    !redacted.contains(fragment),
                    "语料 `{case}` 脱敏后仍含 `{fragment}`：{redacted}"
                );
            }
        }
    }

    #[test]
    fn paths_keep_extension_and_align_within_a_session() {
        let r = fixed();
        let first = r.line(r"打开 C:\Users\某某\Pictures\a.JPG 失败");
        let second = r.line(r"再次失败 C:\Users\某某\Pictures\a.JPG");
        assert!(first.contains("<path:.jpg#"), "{first}");
        assert_eq!(first.split_whitespace().nth(1), second.split_whitespace().nth(1), "同一路径同会话内哈希一致");
        assert!(first.contains("<path:.jpg#") && first.ends_with("失败"));
    }

    #[test]
    fn different_paths_get_different_tokens_and_unc_is_plain() {
        let r = fixed();
        let a = r.line(r"C:\a\one.jpg");
        let b = r.line(r"C:\a\two.jpg");
        assert_ne!(a, b);
        let unc = r.line(r"\\server\share\photos\x.jpg");
        assert_eq!(unc, "<unc>");
        assert!(!unc.contains("server"));
    }

    #[test]
    fn source_paths_survive_without_build_machine_prefix() {
        let r = fixed();
        let line = r.line(r"panic at /home/alice/repos/c-thun/raybend/crates/raybend/src/develop/pipeline.rs:184");
        assert!(line.contains("crates/raybend/src/develop/pipeline.rs:184"), "{line}");
        assert!(!line.contains("alice"), "{line}");
        let win = r.line(r"panic at C:\Users\bob\raybend\crates\raybend\src\editor.rs:42");
        assert!(win.contains("crates/raybend/src/editor.rs:42"), "{win}");
        assert!(!win.contains("bob"), "{win}");
        // 相对路径本来就不动
        assert_eq!(r.line("crates/raybend/src/editor.rs:42"), "crates/raybend/src/editor.rs:42");
    }

    #[test]
    fn urls_emails_and_ips_are_flat_tokens() {
        let r = fixed();
        assert_eq!(r.line("goto https://example.com/a/b?c=1 now"), "goto <url> now");
        assert_eq!(r.line("mail bob@corp.example.com 结束"), "mail <email> 结束");
        assert_eq!(r.line("ping 10.0.0.255 失败"), "ping <ip> 失败");
        // 时间戳里的 `:` 不该被当成 IPv6
        assert_eq!(r.line("at 22:41:03.184Z"), "at 22:41:03.184Z");
        // 版本号四段数字会被当成 IPv4 —— 已知误伤（宁可多脱敏）
        assert_eq!(r.line("wgpu 30.0.1"), "wgpu 30.0.1");
    }

    #[test]
    fn names_and_text_helpers_hide_content_but_keep_evidence() {
        assert_eq!(name("客户婚礼 A"), "<name>");
        assert_eq!(text("找一只猫"), "<text:4>");
        assert_eq!(text(""), "<text:0>");
    }

    #[test]
    fn plain_text_and_code_paths_pass_through_unchanged() {
        let r = fixed();
        for plain in [
            "editor.render drain 超时 waited=2500ms frames=812",
            "crates/raybend/src/editor.rs:42 thread=render",
            "raw-worker 退出码 101",
        ] {
            assert_eq!(r.line(plain), plain);
        }
    }
}
