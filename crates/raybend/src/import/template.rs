//! 导入模版：解析、校验与渲染（`REPOSITORY.md` §3）。
//!
//! 模版是**库级设置**，决定每张照片落进 `photos/` 里的哪条相对路径。默认值：
//!
//! ```text
//! :CYEAR-:CMONTH-:CDAY/MY:FILENAME   →   photos/2026-08-15/MYP0001.png
//! ```
//!
//! 本模块是**纯函数**：不认识文件系统、不认识数据库、不认识 Tauri。
//! 它只做三件事：把模版串切成片段、把片段渲染成字符串、把「这串东西能不能当路径」判出来。
//!
//! ## 三条容易做错的规则（都在测试里钉着）
//!
//! 1. **变量名按最长前缀匹配**：`:FILENAMEGO` 是 `FILENAME` + 字面量 `GO`，
//!    不是「未知变量 FILENAMEGO」（`REPOSITORY.md` §3.2 用户特别说明过）。
//! 2. **序号宽度即身份**：`:SEQ000` 与 `:SEQ0000` 是两个不同的计数器，
//!    所以 [`Template::seq_widths`] 返回的是**用到的宽度集合**，而不是一个位数。
//! 3. **变量取「每张照片自己」的值**：同一天的照片才共享目标根目录，
//!    因此渲染是**逐文件**的，模版的目录部分只是对每个文件各求一次值。
//!
//! ## 缺值怎么办
//!
//! 拍摄时间缺失时日期变量渲染成 `0000` / `00`（而不是拒绝导入：用户的照片里
//! 总会有几张读不出时间），同时把缺的变量记在 [`Rendered::missing`] 里 ——
//! 让上层决定是「照导但记一笔」还是「跳过」。相机品牌/型号缺失渲染成 `Unknown`
//! （用英文是因为它要落进**文件路径**，不该随界面语言变）。

use std::fmt;

use crate::store::time;

/// 模版里能用的变量。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Var {
    /// 创建年份（4 位）。
    CYear,
    /// 创建月份（2 位）。
    CMonth,
    /// 创建日（2 位）。
    CDay,
    /// 一年中的第几周（ISO-8601，2 位）。
    CWeek,
    /// 文件主名（**不含扩展名**）。
    FileName,
    /// 相机品牌。
    Brand,
    /// 相机型号。
    Model,
}

impl Var {
    /// 模版里的写法（不含 `:`）。
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::CYear => "CYEAR",
            Self::CMonth => "CMONTH",
            Self::CDay => "CDAY",
            Self::CWeek => "CWEEK",
            Self::FileName => "FILENAME",
            Self::Brand => "BRAND",
            Self::Model => "MODEL",
        }
    }

    /// 是不是日期类变量（决定要不要去读拍摄时间）。
    #[must_use]
    pub fn is_date(self) -> bool {
        matches!(self, Self::CYear | Self::CMonth | Self::CDay | Self::CWeek)
    }
}

/// **全部**已知变量名，按**长度降序**（最长前缀匹配要用这个顺序）。
pub const KNOWN_VARS: &[Var] = &[
    Var::FileName, // 8
    Var::CMonth,   // 6
    Var::CYear,    // 5
    Var::CWeek,    // 5
    Var::Brand,    // 5
    Var::Model,    // 5
    Var::CDay,     // 4
];

/// 序号变量的前缀（后面跟的全是 `0`，几个 `0` 就是几位数）。
pub const SEQ_PREFIX: &str = "SEQ";

/// 序号最多几位。再宽没有意义（计数器写满会回绕，见 `REPOSITORY.md` §3.3）。
pub const MAX_SEQ_WIDTH: usize = 9;

/// 一个路径分段的长度上限。
///
/// Windows 的 FAT/NTFS 单段上限是 255 个 UTF-16 单元；这里按**字符数**近似
/// （BMP 之外的字符在 UTF-16 里占两个单元，所以这是个宽松的上界）。
pub const MAX_COMPONENT_CHARS: usize = 255;

/// 值缺失时的占位符。
const UNKNOWN: &str = "Unknown";

/// 模版里的一段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// 原文照抄。
    Literal(String),
    /// 一个变量。
    Var(Var),
    /// 一个序号（宽度 = 位数）。
    Seq {
        /// 位数。
        width: usize,
    },
}

/// 解析出来的模版。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    source: String,
    parts: Vec<Part>,
    warnings: Vec<Warning>,
}

/// 不致命、但要给用户看一眼的问题（`REPOSITORY.md` §3.5）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// 不认识的变量 —— 会原文照抄，几乎肯定不是用户想要的。
    UnknownVar {
        /// 用户写的名字（尽量取到最长的可疑片段）。
        name: String,
        /// 在模版串里的位置（字符下标）。
        at: usize,
    },
    /// 同一个宽度出现多次 —— 允许（会连号），但值得提醒。
    RepeatedSeq {
        /// 重复的位数。
        width: usize,
    },
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownVar { name, at } => write!(
                f,
                "不认识变量「:{name}」（第 {at} 个字符）—— 可用变量：{}",
                VarList
            ),
            Self::RepeatedSeq { width } => write!(
                f,
                "模版里用了两次 :{SEQ_PREFIX}{}（同宽度的序号会连号）",
                "0".repeat(*width)
            ),
        }
    }
}

/// 可用变量的一行提示（错误信息与界面提示共用，避免两处各写一份）。
pub struct VarList;

impl fmt::Display for VarList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<String> = KNOWN_VARS
            .iter()
            .map(|v| format!(":{}", v.name()))
            .chain(std::iter::once(format!(":{SEQ_PREFIX}000")))
            .collect();
        write!(f, "{}", names.join(" "))
    }
}

/// 模版哪里不对。带 `at` 的都用**字符下标**（不是字节下标），方便界面上高亮。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateErrorKind {
    /// 空模版。
    Empty,
    /// 结尾是 `/`（会生成空的文件名）。
    TrailingSeparator,
    /// 出现了非法字符（Windows：`< > : " | ? *` 与控制字符）。
    IllegalChar {
        /// 那个字符。
        ch: char,
        /// 位置。
        at: usize,
    },
    /// `:SEQ` 后面不是数字。
    MalformedSeq {
        /// 位置。
        at: usize,
    },
    /// 序号位数超出 [`MAX_SEQ_WIDTH`]。
    TooWideSeq {
        /// 用户写的位数。
        width: usize,
        /// 位置。
        at: usize,
    },
    /// 渲染出来的路径里有空分段（例如 `a//b`，或变量渲染成了空串）。
    EmptySegment {
        /// 那个空段自己的下标（`a//b` 里是第二个 `/` 的位置）。
        at: usize,
    },
    /// 分段以 `.` 或空格结尾 —— Windows 会把它悄悄吃掉，必须拦。
    ComponentEndsWithDotOrSpace {
        /// 位置。
        at: usize,
    },
    /// 分段太长。
    TooLongComponent {
        /// 位置。
        at: usize,
    },
}

/// 模版错误（带给人看的说明）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateError {
    /// 是哪一类。
    pub kind: TemplateErrorKind,
    /// 给用户看的一句话。
    pub message: String,
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TemplateError {}

impl TemplateError {
    fn new(kind: TemplateErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

/// 各宽度序号的值（`(宽度, 值)` 对）—— 缺的宽度按 0 渲染。
///
/// 用「宽度 → 值」而不是单个数字：`:SEQ000` 与 `:SEQ0000` 是两个独立的计数器
/// （`REPOSITORY.md` §3.3），一个模版里同时出现时各自的当前值不一样。
#[derive(Debug, Clone, Copy, Default)]
pub struct SeqValues<'a> {
    pairs: &'a [(usize, u64)],
}

impl<'a> SeqValues<'a> {
    /// 一个都没有（渲染成 0 占位）。
    pub const NONE: Self = Self { pairs: &[] };

    /// 由 `(宽度, 值)` 对构造。
    #[must_use]
    pub const fn new(pairs: &'a [(usize, u64)]) -> Self {
        Self { pairs }
    }

    /// 取某个宽度的值（没有就当 0）。
    #[must_use]
    pub fn get(&self, width: usize) -> u64 {
        self.pairs
            .iter()
            .find(|(w, _)| *w == width)
            .map_or(0, |(_, v)| *v)
    }
}

/// 渲染一张照片需要的值。
#[derive(Debug, Clone, Copy, Default)]
pub struct RenderCtx<'a> {
    /// 拍摄时间（Unix 毫秒，UTC 取年月日 —— 与 `store::time` 的口径一致）。
    pub taken_at: Option<i64>,
    /// 文件主名（不含扩展名，原始大小写）。
    pub stem: &'a str,
    /// 相机品牌。
    pub brand: Option<&'a str>,
    /// 相机型号。
    pub model: Option<&'a str>,
    /// 各宽度序号的值（由分配器给；模版里没用序号时是空的）。
    pub seqs: SeqValues<'a>,
}

/// 渲染结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// 结果路径（`/` 分隔，**不含扩展名**）。
    pub text: String,
    /// 因为**缺值**而用了占位符的变量。
    pub missing: Vec<Var>,
}

impl Rendered {
    /// 拆成 `(目录, 文件名)`；模版里没有目录部分时目录是 `None`。
    #[must_use]
    pub fn split_dir_name(&self) -> (Option<&str>, &str) {
        match self.text.rfind('/') {
            Some(idx) => (Some(&self.text[..idx]), &self.text[idx + 1..]),
            None => (None, self.text.as_str()),
        }
    }
}

/// 解析模版串。**硬错误**（空、非法字符、格式不对）在这里就报出来，
/// 不认识的变量只算警告（见 [`Template::warnings`]）。
pub fn parse(source: &str) -> Result<Template, TemplateError> {
    if source.trim().is_empty() {
        return Err(TemplateError::new(
            TemplateErrorKind::Empty,
            "导入模版不能为空",
        ));
    }
    if source.ends_with('/') {
        return Err(TemplateError::new(
            TemplateErrorKind::TrailingSeparator,
            "导入模版不能以「/」结尾 —— 那样算不出文件名",
        ));
    }

    let chars: Vec<char> = source.chars().collect();
    let mut parts: Vec<Part> = Vec::new();
    let mut warnings: Vec<Warning> = Vec::new();
    let mut literal = String::new();
    let mut seq_widths: Vec<usize> = Vec::new();
    let mut idx = 0;

    while idx < chars.len() {
        let ch = chars[idx];

        // 非法字符：`:` 是变量标记，单独处理；`/` 是分隔符，允许。
        if ch != ':' && ch != '/' && is_illegal_in_component(ch) {
            return Err(TemplateError::new(
                TemplateErrorKind::IllegalChar { ch, at: idx },
                format!("导入模版里有不能用于文件名的字符「{ch}」（第 {idx} 个字符）"),
            ));
        }

        if ch != ':' {
            literal.push(ch);
            idx += 1;
            continue;
        }

        // ── `:` 开头：先试序号，再按最长前缀匹配变量 ──
        let rest: String = chars[idx + 1..].iter().collect();

        if let Some(after_seq) = rest.strip_prefix(SEQ_PREFIX) {
            let digits = after_seq.chars().take_while(|c| *c == '0').count();
            if digits == 0 {
                return Err(TemplateError::new(
                    TemplateErrorKind::MalformedSeq { at: idx },
                    format!(
                        "序号要写成「:{SEQ_PREFIX}000」（几个 0 就是几位数），第 {idx} 个字符处的不完整"
                    ),
                ));
            }
            if digits > MAX_SEQ_WIDTH {
                return Err(TemplateError::new(
                    TemplateErrorKind::TooWideSeq {
                        width: digits,
                        at: idx,
                    },
                    format!("序号最多 {MAX_SEQ_WIDTH} 位，这里写了 {digits} 位"),
                ));
            }
            flush_literal(&mut parts, &mut literal);
            parts.push(Part::Seq { width: digits });
            if seq_widths.contains(&digits) {
                warnings.push(Warning::RepeatedSeq { width: digits });
            }
            seq_widths.push(digits);
            idx += 1 + SEQ_PREFIX.len() + digits;
            continue;
        }

        // 最长前缀匹配：`KNOWN_VARS` 已经是长度降序，第一个命中的就是最长的那个。
        match KNOWN_VARS.iter().find(|v| rest.starts_with(v.name())) {
            Some(var) => {
                flush_literal(&mut parts, &mut literal);
                parts.push(Part::Var(*var));
                idx += 1 + var.name().len();
            }
            None => {
                // 不认识的变量：**原文照抄**，并取一个最长的可疑名字用于提示
                // （`:CAMERA123` 里 `CAMERA123` 整体当名字，提示才看得懂）。
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                let name = if name.is_empty() {
                    ch.to_string()
                } else {
                    name
                };
                warnings.push(Warning::UnknownVar {
                    name: name.clone(),
                    at: idx,
                });
                literal.push(ch);
                literal.push_str(&name);
                idx += 1 + name.chars().count();
            }
        }
    }

    flush_literal(&mut parts, &mut literal);

    // 全部是分隔符 / 空片段（例如 `//`）—— 空目录名不是合法路径
    if parts.is_empty() {
        return Err(TemplateError::new(
            TemplateErrorKind::Empty,
            "导入模版只算出了空路径",
        ));
    }

    Ok(Template {
        source: source.to_string(),
        parts,
        warnings,
    })
}

impl Template {
    /// 原文。
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// 解析出来的片段（测试与调试用）。
    #[must_use]
    pub fn parts(&self) -> &[Part] {
        &self.parts
    }

    /// 不致命的问题。
    #[must_use]
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    /// 用到的**序号宽度集合**（去重、保持出现顺序）。
    ///
    /// 空的表示模版里没有序号 —— 上层就不必去动 `seq_counters`。
    #[must_use]
    pub fn seq_widths(&self) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        for part in &self.parts {
            if let Part::Seq { width } = part
                && !out.contains(width)
            {
                out.push(*width);
            }
        }
        out
    }

    /// 用到的变量（去重、保持出现顺序）。
    #[must_use]
    pub fn vars(&self) -> Vec<Var> {
        let mut out: Vec<Var> = Vec::new();
        for part in &self.parts {
            if let Part::Var(v) = part
                && !out.contains(v)
            {
                out.push(*v);
            }
        }
        out
    }

    /// 模版会不会用到拍摄时间（上层据此决定要不要读 EXIF）。
    #[must_use]
    pub fn needs_date(&self) -> bool {
        self.vars().iter().any(|v| v.is_date())
    }

    /// 渲染一张照片的目标相对路径（**不含扩展名**，`/` 分隔）。
    #[must_use]
    pub fn render(&self, ctx: &RenderCtx<'_>) -> Rendered {
        let mut text = String::new();
        let mut missing: Vec<Var> = Vec::new();
        let date = ctx.taken_at.map(time::civil);

        for part in &self.parts {
            match part {
                Part::Literal(s) => text.push_str(s),
                Part::Seq { width } => {
                    let value = ctx.seqs.get(*width);
                    // `{:0width$}` 不足位补零；超出位数就原样展开（宁可长，不可截断）
                    text.push_str(&format!("{value:0width$}"));
                }
                Part::Var(var) => match var {
                    Var::FileName => text.push_str(ctx.stem),
                    Var::Brand => push_str_or_unknown(&mut text, ctx.brand, *var, &mut missing),
                    Var::Model => push_str_or_unknown(&mut text, ctx.model, *var, &mut missing),
                    Var::CYear => match date {
                        Some((y, ..)) => text.push_str(&format!("{y:04}")),
                        None => {
                            text.push_str("0000");
                            note_missing(&mut missing, *var);
                        }
                    },
                    Var::CMonth => match date {
                        Some((_, m, ..)) => text.push_str(&format!("{m:02}")),
                        None => {
                            text.push_str("00");
                            note_missing(&mut missing, *var);
                        }
                    },
                    Var::CDay => match date {
                        Some((_, _, d, ..)) => text.push_str(&format!("{d:02}")),
                        None => {
                            text.push_str("00");
                            note_missing(&mut missing, *var);
                        }
                    },
                    Var::CWeek => match date {
                        Some((y, m, d, ..)) => {
                            text.push_str(&format!("{:02}", iso_week(y, m, d)));
                        }
                        None => {
                            text.push_str("00");
                            note_missing(&mut missing, *var);
                        }
                    },
                },
            }
        }

        Rendered { text, missing }
    }
}

/// 渲染出来的字符串能不能当库内相对路径（逐分段检查）。
///
/// 与解析期检查的分工：解析期看的是**模版原文**（能提前告诉用户哪里写错了），
/// 这里看的是**渲染结果**（变量值里也可能夹带非法字符 —— EXIF 里的型号什么都可能写）。
pub fn check_output(text: &str) -> Result<(), TemplateError> {
    if text.is_empty() {
        return Err(TemplateError::new(
            TemplateErrorKind::Empty,
            "算出来的路径是空的",
        ));
    }
    if text.ends_with('/') {
        return Err(TemplateError::new(
            TemplateErrorKind::TrailingSeparator,
            "算出来的路径以「/」结尾",
        ));
    }

    let mut at = 0;
    let chars: Vec<char> = text.chars().collect();
    let mut seg_start = 0;
    for (idx, ch) in chars.iter().enumerate() {
        if *ch == '/' {
            check_segment(&chars[seg_start..idx], at, seg_start)?;
            at += idx - seg_start + 1;
            seg_start = idx + 1;
            continue;
        }
        if is_illegal_in_component(*ch) {
            return Err(TemplateError::new(
                TemplateErrorKind::IllegalChar { ch: *ch, at: idx },
                format!("路径里有不能用于文件名的字符「{ch}」（第 {idx} 个字符）"),
            ));
        }
    }
    check_segment(&chars[seg_start..], at, seg_start)?;
    Ok(())
}

fn check_segment(segment: &[char], _at: usize, seg_start: usize) -> Result<(), TemplateError> {
    if segment.is_empty() {
        return Err(TemplateError::new(
            TemplateErrorKind::EmptySegment { at: seg_start },
            format!("第 {} 个字符处是空目录名", seg_start + 1),
        ));
    }
    if segment.len() > MAX_COMPONENT_CHARS {
        return Err(TemplateError::new(
            TemplateErrorKind::TooLongComponent { at: seg_start },
            format!(
                "有一段名字太长（{} 个字符，上限 {MAX_COMPONENT_CHARS}）",
                segment.len()
            ),
        ));
    }
    if matches!(segment.last(), Some('.') | Some(' ')) {
        return Err(TemplateError::new(
            TemplateErrorKind::ComponentEndsWithDotOrSpace { at: seg_start },
            format!(
                "第 {} 个字符处的那一段以「{}」结尾 —— Windows 会把它吃掉，得改名",
                seg_start + 1,
                segment.last().copied().unwrap_or(' ')
            ),
        ));
    }
    Ok(())
}

/// Windows 文件名里不能出现的字符（`/` 是分隔符，`:` 已被变量解析吃掉，都不算）。
fn is_illegal_in_component(ch: char) -> bool {
    matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\\')
        || (ch.is_control())
}

fn flush_literal(parts: &mut Vec<Part>, literal: &mut String) {
    if !literal.is_empty() {
        parts.push(Part::Literal(std::mem::take(literal)));
    }
}

fn push_str_or_unknown(
    out: &mut String,
    value: Option<&str>,
    var: Var,
    missing: &mut Vec<Var>,
) {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => out.push_str(v),
        None => {
            out.push_str(UNKNOWN);
            note_missing(missing, var);
        }
    }
}

fn note_missing(missing: &mut Vec<Var>, var: Var) {
    if !missing.contains(&var) {
        missing.push(var);
    }
}

/// ISO-8601 的周号（周一为一周之始，第 1 周是含 1 月 4 日的那周）。
///
/// 用 ISO 而不是「第几天 / 7」：按周分目录时，ISO 的周界与相机的做法一致
/// （跨年那几天会归到相邻年份的周里，这也是 `:CYEAR` 与 `:CWEEK` 并列时唯一说得通的组合）。
#[must_use]
pub fn iso_week(year: i64, month: u32, day: u32) -> u32 {
    let days = time::days_from_civil(year, month, day);
    let doy = days - time::days_from_civil(year, 1, 1) + 1;
    // Jan 1 1970 是周四 → 周一为 1 的编号下是 4
    let weekday = (days + 3).rem_euclid(7) + 1;
    let week = (doy - weekday + 10).div_euclid(7);
    if week < 1 {
        iso_weeks_in_year(year - 1)
    } else if week > i64::from(iso_weeks_in_year(year)) {
        1
    } else {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            week as u32
        }
    }
}

/// 某一年有 52 还是 53 个 ISO 周。
fn iso_weeks_in_year(year: i64) -> u32 {
    let jan1 = (time::days_from_civil(year, 1, 1) + 3).rem_euclid(7) + 1; // Mon=1
    let leap = is_leap_year(year);
    // 1 月 1 日是周四 → 53 周；是周三且闰年 → 也是 53 周
    if jan1 == 4 || (leap && jan1 == 3) { 53 } else { 52 }
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(stem: &str) -> RenderCtx<'_> {
        RenderCtx {
            // 2026-08-15T12:34:56Z
            taken_at: Some(1_786_797_296_000),
            stem,
            brand: Some("NIKON CORPORATION"),
            model: Some("Z7II"),
            seqs: SeqValues::NONE,
        }
    }

    fn render(source: &str, ctx: &RenderCtx<'_>) -> String {
        parse(source).expect("模版应当能解析").render(ctx).text
    }

    /* ── 默认模版与示例（REPOSITORY.md §3.1） ───────────────────────── */

    #[test]
    fn default_template_matches_the_spec_example() {
        assert_eq!(
            render(":CYEAR-:CMONTH-:CDAY/MY:FILENAME", &ctx("P0001")),
            "2026-08-15/MYP0001"
        );
    }

    #[test]
    fn single_variable_and_pure_literal_templates() {
        assert_eq!(render(":FILENAME", &ctx("P0001")), "P0001");
        assert_eq!(render("固定名", &ctx("P0001")), "固定名");
        assert_eq!(
            render(":MODEL/:BRAND/:FILENAME", &ctx("P0001")),
            "Z7II/NIKON CORPORATION/P0001"
        );
    }

    /* ── 规则 1：最长前缀匹配（用户特别说明过的那条） ────────────────── */

    #[test]
    fn variable_names_match_by_longest_prefix() {
        // `:FILENAMEGO` = FILENAME + 字面量 "GO"
        assert_eq!(render(":FILENAMEGO", &ctx("P0001")), "P0001GO");
        assert_eq!(render(":CYEARX", &ctx("P0001")), "2026X");
        // 前后都粘字面量
        assert_eq!(render("P-:CDAY-END", &ctx("x")), "P-15-END");
        // 没有歧义时逐个变量都对
        assert_eq!(render(":CMONTH:CDAY", &ctx("x")), "0815");
    }

    #[test]
    fn unknown_variable_is_a_warning_and_printed_verbatim() {
        let tpl = parse(":CAMERA/:FILENAME").expect("不认识的变量不该让解析失败");
        assert_eq!(tpl.render(&ctx("P0001")).text, ":CAMERA/P0001");
        assert_eq!(
            tpl.warnings(),
            &[Warning::UnknownVar {
                name: "CAMERA".into(),
                at: 0
            }]
        );
        // 警告里要列出可用变量，否则用户不知道该写什么
        let msg = tpl.warnings()[0].to_string();
        assert!(msg.contains(":FILENAME"), "提示里要有可用变量：{msg}");
        assert!(msg.contains(":SEQ000"), "序号也是可用变量：{msg}");
    }

    #[test]
    fn unknown_variable_name_stops_at_the_first_separator() {
        let tpl = parse(":NOPE/foo").expect("能解析");
        assert_eq!(
            tpl.warnings(),
            &[Warning::UnknownVar {
                name: "NOPE".into(),
                at: 0
            }]
        );
    }

    /* ── 规则 2：序号宽度即身份 ────────────────────────────────────── */

    #[test]
    fn sequence_pads_to_its_width() {
        let values = [(3usize, 7u64), (4, 7), (5, 7), (1, 7)];
        let c = RenderCtx {
            seqs: SeqValues::new(&values),
            ..ctx("P0001")
        };
        assert_eq!(render(":SEQ000", &c), "007");
        assert_eq!(render(":SEQ0000", &c), "0007");
        assert_eq!(render(":SEQ00000", &c), "00007");
        // 一位数也允许（`:SEQ0`）
        assert_eq!(render(":SEQ0", &c), "7");
    }

    #[test]
    fn each_width_has_its_own_value() {
        // 同一张照片、同一个模版：两个宽度的计数器各走各的（REPOSITORY.md §3.3）
        let values = [(3usize, 12u64), (4, 345)];
        let c = RenderCtx {
            seqs: SeqValues::new(&values),
            ..ctx("P0001")
        };
        assert_eq!(render(":SEQ000-:SEQ0000", &c), "012-0345");
        // 没给的宽度按 0 顶
        assert_eq!(render(":SEQ00000", &c), "00000");
    }

    #[test]
    fn sequence_keeps_growing_past_its_width() {
        let values = [(3usize, 1234u64)];
        let c = RenderCtx {
            seqs: SeqValues::new(&values),
            ..ctx("P0001")
        };
        assert_eq!(
            render(":SEQ000", &c),
            "1234",
            "写满之后不截断 —— 宁可名字长一点，也不能两张照片同名"
        );
    }

    #[test]
    fn seq_widths_reports_every_distinct_width() {
        let tpl = parse(":SEQ000/:SEQ0000/:SEQ000").expect("能解析");
        assert_eq!(tpl.seq_widths(), vec![3, 4], "去重且保持出现顺序");
        // 同宽度用了两次 → 提醒（会连号）
        assert_eq!(tpl.warnings(), &[Warning::RepeatedSeq { width: 3 }]);
        // 没有序号时是空的 —— 上层据此跳过 seq_counters
        assert!(parse(":FILENAME").expect("能解析").seq_widths().is_empty());
    }

    #[test]
    fn malformed_sequence_is_an_error() {
        let err = parse(":SEQ/x").expect_err("`:SEQ` 后面没有 0");
        assert!(matches!(err.kind, TemplateErrorKind::MalformedSeq { at: 0 }));
        assert!(err.message.contains("SEQ000"), "要说清怎么写：{}", err.message);

        let err = parse(":SEQ0000000000").expect_err("10 位超过上限 9");
        assert!(matches!(
            err.kind,
            TemplateErrorKind::TooWideSeq { width: 10, at: 0 }
        ));
        // 刚好到上限要能过
        assert!(parse(":SEQ000000000").is_ok(), "9 位是允许的");
    }

    #[test]
    fn sequence_is_matched_before_variables() {
        // `:SEQ000` 不能因为「以 S 开头的变量」之类的原因被拆错
        let values = [(3usize, 1u64)];
        let c = RenderCtx {
            seqs: SeqValues::new(&values),
            ..ctx("P0001")
        };
        assert_eq!(render(":SEQ000:FILENAME", &c), "001P0001");
    }

    /* ── 规则 3：值来自每张照片自己 ────────────────────────────────── */

    #[test]
    fn different_photos_render_different_roots() {
        let tpl = parse(":CYEAR-:CMONTH-:CDAY/MY:FILENAME").expect("能解析");
        let a = RenderCtx {
            taken_at: Some(1_786_797_296_000), // 2026-08-15
            ..ctx("a")
        };
        let b = RenderCtx {
            taken_at: Some(1_786_883_696_000), // 2026-08-16
            ..ctx("b")
        };
        assert_eq!(tpl.render(&a).text, "2026-08-15/MYa");
        assert_eq!(tpl.render(&b).text, "2026-08-16/MYb");
        assert_ne!(
            tpl.render(&a).split_dir_name().0,
            tpl.render(&b).split_dir_name().0,
            "不同日期 → 不同目标根目录"
        );
    }

    #[test]
    fn split_dir_name_handles_both_shapes() {
        let with_dir = parse(":CYEAR/:FILENAME")
            .expect("能解析")
            .render(&ctx("P0001"));
        assert_eq!(with_dir.split_dir_name(), (Some("2026"), "P0001"));
        let flat = parse(":FILENAME").expect("能解析").render(&ctx("P0001"));
        assert_eq!(flat.split_dir_name(), (None, "P0001"));
    }

    /* ── ISO 周（:CWEEK） ─────────────────────────────────────────── */

    #[test]
    fn iso_week_reference_values() {
        // 跨年那几天归到相邻年份的周 —— 这是 ISO 的定义，也是相机的做法
        assert_eq!(iso_week(2021, 1, 1), 53, "2021-01-01 属于 2020 的第 53 周");
        assert_eq!(iso_week(2020, 12, 31), 53);
        assert_eq!(iso_week(2019, 12, 30), 1, "2019-12-30 已经是 2020 年第 1 周");
        assert_eq!(iso_week(2018, 1, 1), 1, "2018-01-01 是周一");
        assert_eq!(iso_week(2016, 1, 4), 1);
        assert_eq!(iso_week(2015, 12, 31), 53);
        assert_eq!(iso_week(2017, 1, 1), 52, "2017-01-01 是周日，属 2016 年最后一周");
    }

    #[test]
    fn iso_week_is_constant_within_a_week() {
        // 2026-08-10 是周一
        let monday = iso_week(2026, 8, 10);
        for day in 11..=16 {
            assert_eq!(iso_week(2026, 8, day), monday, "8 月 {day} 日");
        }
        assert_ne!(iso_week(2026, 8, 17), monday, "下一个周一要 +1");
        assert_eq!(iso_week(2026, 8, 17), monday + 1);
    }

    #[test]
    fn iso_week_stays_in_range_for_every_day_of_a_year() {
        for year in [2019, 2020, 2021, 2024, 2026, 2100] {
            for month in 1..=12 {
                for day in 1..=28 {
                    let w = iso_week(year, month, day);
                    assert!((1..=53).contains(&w), "{year}-{month}-{day} → {w}");
                }
            }
        }
    }

    #[test]
    fn week_renders_two_digits() {
        // 2026-01-01 是周四（2026 年第 1 周）
        let c = RenderCtx {
            taken_at: Some(1_767_225_600_000),
            ..ctx("a")
        };
        assert_eq!(render(":CWEEK", &c), "01");
    }

    /* ── 缺值 ─────────────────────────────────────────────────────── */

    #[test]
    fn missing_taken_at_renders_zeros_and_records_why() {
        let tpl = parse(":CYEAR-:CMONTH-:CDAY/:FILENAME").expect("能解析");
        let out = tpl.render(&RenderCtx {
            taken_at: None,
            ..ctx("P0001")
        });
        assert_eq!(out.text, "0000-00-00/P0001");
        assert_eq!(out.missing, vec![Var::CYear, Var::CMonth, Var::CDay]);
        assert!(tpl.needs_date());
    }

    #[test]
    fn missing_brand_and_model_render_unknown() {
        let out = parse(":BRAND/:MODEL/:FILENAME")
            .expect("能解析")
            .render(&RenderCtx {
                brand: None,
                model: Some("   "),
                ..ctx("P0001")
            });
        assert_eq!(out.text, "Unknown/Unknown/P0001", "空白也算缺值");
        assert_eq!(out.missing, vec![Var::Brand, Var::Model]);
        assert!(!parse(":FILENAME").expect("能解析").needs_date());
    }

    /* ── 解析期的硬错误（REPOSITORY.md §3.5） ──────────────────────── */

    #[test]
    fn empty_template_is_rejected() {
        for bad in ["", "   ", "\t\n"] {
            let err = parse(bad).expect_err("空模版要报错");
            assert!(matches!(err.kind, TemplateErrorKind::Empty));
        }
    }

    #[test]
    fn trailing_separator_is_rejected() {
        let err = parse(":CYEAR/:CMONTH/").expect_err("结尾不能是 /");
        assert!(matches!(err.kind, TemplateErrorKind::TrailingSeparator));
    }

    #[test]
    fn illegal_characters_are_rejected_with_a_position() {
        for (bad, ch) in [
            ("a<b", '<'),
            ("a>b", '>'),
            ("a\"b", '"'),
            ("a|b", '|'),
            ("a?b", '?'),
            ("a*b", '*'),
            ("a\\b", '\\'),
        ] {
            let err = parse(bad).expect_err("非法字符要报错");
            match err.kind {
                TemplateErrorKind::IllegalChar { ch: got, at } => {
                    assert_eq!(got, ch, "{bad}");
                    assert_eq!(at, 1, "{bad} 的位置");
                }
                other => panic!("{bad} 应报非法字符，实际 {other:?}"),
            }
        }
    }

    #[test]
    fn colon_is_not_treated_as_an_illegal_character() {
        // `:` 是变量标记；不认识的变量是警告而不是错误（用户可能只是打错）
        let tpl = parse(":UNKNOWN").expect(":` 开头的未知变量不该报非法字符");
        assert_eq!(tpl.warnings().len(), 1);
    }

    /* ── 渲染结果的路径检查 ───────────────────────────────────────── */

    #[test]
    fn check_output_accepts_the_spec_examples() {
        for good in [
            "2026-08-15/MYP0001",
            "2026-08-15/Japan/Kyoto/MYP0001",
            "MYP0001",
            "照片/我的名字",
        ] {
            assert!(check_output(good).is_ok(), "{good}");
        }
    }

    #[test]
    fn check_output_rejects_bad_shapes() {
        assert!(matches!(
            check_output("").expect_err("空").kind,
            TemplateErrorKind::Empty
        ));
        assert!(matches!(
            check_output("a/b/").expect_err("结尾 /").kind,
            TemplateErrorKind::TrailingSeparator
        ));
        assert!(
            matches!(
                check_output("a//b").expect_err("空分段").kind,
                TemplateErrorKind::EmptySegment { at: 2 }
            ),
            "`at` 是那个空段自己的下标（`a//b` 里是第二个 `/`）"
        );
        assert!(matches!(
            check_output("/a").expect_err("开头就是空分段").kind,
            TemplateErrorKind::EmptySegment { at: 0 }
        ));
        assert!(matches!(
            check_output("a./b").expect_err("以点结尾").kind,
            TemplateErrorKind::ComponentEndsWithDotOrSpace { at: 0 }
        ));
        assert!(matches!(
            check_output("a /b").expect_err("以空格结尾").kind,
            TemplateErrorKind::ComponentEndsWithDotOrSpace { at: 0 }
        ));
        // 变量值里夹带的非法字符（EXIF 型号什么都可能写）
        assert!(matches!(
            check_output("2026/\u{7}\u{7}").expect_err("控制字符").kind,
            TemplateErrorKind::IllegalChar { .. }
        ));
    }

    #[test]
    fn check_output_rejects_too_long_components() {
        let long = "字".repeat(MAX_COMPONENT_CHARS);
        assert!(check_output(&long).is_ok(), "刚好到上限应当通过");
        let too_long = "字".repeat(MAX_COMPONENT_CHARS + 1);
        assert!(matches!(
            check_output(&too_long).expect_err("超长").kind,
            TemplateErrorKind::TooLongComponent { at: 0 }
        ));
        // 长分段在后面时报的位置要对
        let nested = format!("ok/{too_long}");
        match check_output(&nested).expect_err("超长").kind {
            TemplateErrorKind::TooLongComponent { at } => assert_eq!(at, 3),
            other => panic!("{other:?}"),
        }
    }

    /* ── 组合与边界 ───────────────────────────────────────────────── */

    #[test]
    fn chinese_and_unicode_stems_pass_through() {
        assert_eq!(
            render(":CYEAR/:FILENAME", &ctx("照片-001")),
            "2026/照片-001"
        );
        // 全角字符、emoji 不算非法字符（Windows 也允许）
        assert_eq!(render(":FILENAME", &ctx("海边🌊")), "海边🌊");
        assert!(check_output("2026/海边🌊").is_ok());
    }

    #[test]
    fn very_long_stem_passes_through_and_is_flagged_by_check_output() {
        let stem = "x".repeat(300);
        let out = parse(":FILENAME").expect("能解析").render(&ctx(&stem));
        assert_eq!(out.text.len(), 300, "渲染本身不截断");
        assert!(
            check_output(&out.text).is_err(),
            "但路径检查要拦下来 —— 落盘的路径由上层负责判"
        );
    }

    #[test]
    fn render_is_deterministic_for_the_same_input() {
        let tpl = parse(":CYEAR-:CMONTH-:CDAY/MY:FILENAME").expect("能解析");
        assert_eq!(tpl.render(&ctx("P0001")), tpl.render(&ctx("P0001")));
    }

    #[test]
    fn vars_lists_what_the_template_uses() {
        let tpl = parse(":CYEAR/:MODEL/:FILENAME/:SEQ000").expect("能解析");
        assert_eq!(tpl.vars(), vec![Var::CYear, Var::Model, Var::FileName]);
        assert_eq!(tpl.seq_widths(), vec![3]);
    }

    #[test]
    fn var_names_all_parse_back_to_themselves() {
        // 变量表与解析器不能各写一份 —— 每个已知名字都得能解析回它自己
        let s: Vec<String> = KNOWN_VARS.iter().map(|v| format!(":{}", v.name())).collect();
        let tpl = parse(&s.join("/")).expect("全是已知变量，应当能解析");
        assert!(tpl.warnings().is_empty(), "{:?}", tpl.warnings());
        assert_eq!(tpl.vars().len(), KNOWN_VARS.len());
    }

    #[test]
    fn error_messages_are_human_readable() {
        let err = parse(":CYEAR/<bad>").expect_err("非法字符");
        assert!(err.message.contains('<'), "要说清是哪个字符：{}", err.message);
        assert!(err.to_string().contains("第"), "要给出位置：{err}");
    }
}
