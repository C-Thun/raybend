//! 错误报告包（W3-α，「本地导出」，崔总 2026-10-12 追加）：
//! 崩溃后再次启动 → 弹窗问用户 → 用户点「保存报告」→ 系统另存为 → 在所选目录产出
//!
//! ```text
//! <用户选的目录>/
//! ├── RayBend-错误报告-RB-<日期>-<短码>.zip     ← 发给开发者
//! └── RayBend-错误报告提交指南.txt              ← 怎么发、发到哪
//! ```
//!
//! **不发送、不联网**：这只是把已脱敏的记录打成一个包（规格 §7.2 的 T0 下限）。
//! 上传（T1 自建接收服务）与报告对话框属第三层其余部分，仍留在 `memory/FUTURE.md`。

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::crash::CrashReport;

/// 已提示过的标记文件（同一次崩溃只提示一次 —— 规格 §9）。
pub const PROMPTED_MARK: &str = "prompted";
/// 报告包在 zip 里的文件名。
const REPORT_JSON: &str = "report.json";
const REPORT_TEXT: &str = "report.txt";
/// 指南文件名（同时放进 zip 与用户选的目录）。
pub const GUIDE_NAME: &str = "RayBend-错误报告提交指南.txt";

/// 崩溃记录摘要（前端弹窗与默认文件名用；字段进 DTO，序列化成 camelCase）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashSummary {
    /// `RB-20261012-3F9A21`
    pub report_id: String,
    /// `panic` / `unclean-exit` / `hard-crash`
    pub kind: String,
    /// UTC ISO 毫秒
    pub created_at: String,
    /// 崩溃记录目录（前端回传给我们导出用）
    pub dir: String,
    /// 有没有 dump（决定弹窗里那个勾选框要不要出现）
    pub has_dump: bool,
}

/// 导出结果。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleOutcome {
    pub zip_path: String,
    pub guide_path: String,
    pub zip_bytes: u64,
    pub entries: Vec<String>,
}

/// 目录里最近的、**还没提示过**的崩溃记录。
///
/// 判据：目录名以 `RB-` 开头、含 `report.json` 或 `dump.dmp`、且没有 [`PROMPTED_MARK`] 标记。
#[must_use]
pub fn latest_unprompted(crashes_dir: &Path) -> Option<CrashSummary> {
    let entries = std::fs::read_dir(crashes_dir).ok()?;
    let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !path.is_dir() || !name.starts_with("RB-") {
            continue;
        }
        if path.join(PROMPTED_MARK).exists() {
            continue;
        }
        let has_report = path.join(REPORT_JSON).is_file();
        let has_dump = path.join("dump.dmp").is_file();
        if !has_report && !has_dump {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        candidates.push((modified, path));
    }
    // 目录 mtime 可能撞在同一刻度上（同一次启动里连续两次崩溃、或文件系统时间粒度粗），
    // 那时按**目录名**兜底：`RB-<日期>-<短码>` 里的日期就是崩溃日，名字越大越新。
    // 不这么办的话这里就是不确定的（测试在本机曾稳定选错、真机上会「提示的不是最新那份」）。
    candidates.sort_by(|(a_time, a_path), (b_time, b_path)| {
        b_time.cmp(a_time).then_with(|| b_path.cmp(a_path))
    });
    let (_, dir) = candidates.into_iter().next()?;
    Some(summarize(&dir))
}

/// 读一个崩溃目录 → 摘要（读不到 report.json 也能给出一份最小信息）。
#[must_use]
pub fn summarize(dir: &Path) -> CrashSummary {
    let report = std::fs::read_to_string(dir.join(REPORT_JSON))
        .ok()
        .and_then(|body| serde_json::from_str::<CrashReport>(&body).ok());
    CrashSummary {
        report_id: report
            .as_ref()
            .map_or_else(|| dir_name(dir), |value| value.report_id.clone()),
        kind: report
            .as_ref()
            .map_or_else(|| "hard-crash".to_string(), |value| value.kind.clone()),
        created_at: report
            .as_ref()
            .map_or_else(String::new, |value| value.created_at.clone()),
        dir: dir.to_string_lossy().into_owned(),
        has_dump: dir.join("dump.dmp").is_file(),
    }
}

/// 标记「已经问过用户了」（不报告也算 —— 规格 §9：同一次崩溃只提示一次）。
pub fn mark_prompted(crash_dir: &Path) {
    let _ = std::fs::write(crash_dir.join(PROMPTED_MARK), b"1");
}

fn dir_name(dir: &Path) -> String {
    dir.file_name()
        .map_or_else(String::new, |value| value.to_string_lossy().into_owned())
}

/// 把一份崩溃记录打成 zip，并在**同一目录**写一份提交指南。
///
/// # Errors
/// 目标路径建不出来、zip 写失败时返回可直接显示给用户的原因。
pub fn export(
    crash_dir: &Path,
    logs_dir: &Path,
    zip_path: &Path,
    include_dump: bool,
) -> Result<BundleOutcome, String> {
    if !crash_dir.is_dir() {
        return Err("崩溃记录目录不存在".to_string());
    }
    if let Some(parent) = zip_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("建不了目标目录：{error}"))?;
    }
    let summary = summarize(crash_dir);
    let guide = guide_text(&summary.report_id, &summary.kind, &summary.created_at);
    let guide_path = zip_path.with_file_name(GUIDE_NAME);
    std::fs::write(&guide_path, &guide).map_err(|error| format!("写不了提交指南：{error}"))?;

    let file = std::fs::File::create(zip_path).map_err(|error| format!("建不了 zip：{error}"))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut entries = Vec::new();

    // ① 崩溃记录本体（report.json / report.txt）
    for name in [REPORT_JSON, REPORT_TEXT] {
        if add_file(&mut zip, &crash_dir.join(name), name, options)? {
            entries.push(name.to_string());
        }
    }
    // ② dump（用户勾了才带：可能含已解码像素片段，规格 §5.2/§8.4）
    if include_dump {
        for name in ["dump.dmp", "dump.json"] {
            if add_file(&mut zip, &crash_dir.join(name), name, options)? {
                entries.push(name.to_string());
            }
        }
    }
    // ③ 日志（当前会话 + 上一次会话；都已脱敏，且各自 ≤2 MiB）
    for name in ["raybend.log", "raybend.1.log"] {
        let entry_name = format!("logs/{name}");
        if add_file(&mut zip, &logs_dir.join(name), &entry_name, options)? {
            entries.push(entry_name);
        }
    }
    // ④ 指南也塞一份进 zip：用户只转发 zip 时，收到的人也知道怎么联系我们
    zip.start_file(GUIDE_NAME, options)
        .map_err(|error| format!("写 zip 失败：{error}"))?;
    zip.write_all(guide.as_bytes())
        .map_err(|error| format!("写 zip 失败：{error}"))?;
    entries.push(GUIDE_NAME.to_string());

    let file = zip
        .finish()
        .map_err(|error| format!("收尾 zip 失败：{error}"))?;
    let zip_bytes = file
        .metadata()
        .map(|meta| meta.len())
        .unwrap_or_default();
    drop(file);

    Ok(BundleOutcome {
        zip_path: zip_path.to_string_lossy().into_owned(),
        guide_path: guide_path.to_string_lossy().into_owned(),
        zip_bytes,
        entries,
    })
}

/// 往 zip 里加一个文件；文件不存在返回 `Ok(false)`（缺哪个都不该让导出失败）。
fn add_file(
    zip: &mut ZipWriter<std::fs::File>,
    path: &Path,
    name: &str,
    options: SimpleFileOptions,
) -> Result<bool, String> {
    if !path.is_file() {
        return Ok(false);
    }
    let mut body = Vec::new();
    std::fs::File::open(path)
        .and_then(|mut file| file.read_to_end(&mut body))
        .map_err(|error| format!("读不了 {}：{error}", path.display()))?;
    zip.start_file(name, options)
        .map_err(|error| format!("写 zip 失败：{error}"))?;
    zip.write_all(&body)
        .map_err(|error| format!("写 zip 失败：{error}"))?;
    Ok(true)
}

/// 提交指南正文（纯函数，可单测）。
///
/// ❗这是给**普通用户**看的：不写术语，只写「你该做什么」。
/// QQ 群还没建（崔总 2026-10-12：暂放），所以第一版不放那一行 —— 建了再加。
#[must_use]
pub fn guide_text(report_id: &str, kind: &str, created_at: &str) -> String {
    let what = match kind {
        "panic" => "一次程序内部的错误（panic）",
        "unclean-exit" => "一次没有正常关闭的退出（可能是被强制结束或直接崩掉）",
        _ => "一次程序崩溃",
    };
    format!(
        "RayBend 错误报告提交指南\n\
         ============================\n\n\
         这份报告记录的是{what}。\n\
         报告 ID：{report_id}\n\
         发生时间：{created_at}（UTC）\n\n\
         报告里已经做过脱敏：不含你的照片路径、用户名、库名与照片内容。\n\
         把它发给我们，能帮我们定位并修掉这个问题。谢谢！\n\n\
         ── 怎么发（任选一种）────────────────────────\n\n\
         1) 邮件（推荐，最直接）\n\
            收件人：raybend+andares@outlook.com\n\
            主题：[RayBend] 错误报告 {report_id}\n\
            附件：把同目录下的 zip 拖进去即可\n\n\
         2) GitHub 提 issue\n\
            地址：https://github.com/C-Thun/raybend/issues\n\
            标题：写上报告 ID（{report_id}），正文里说一句当时在做什么\n\
            （zip 太大可以先不提附件，我们看到 ID 会联系你）\n\n\
         ── 写的时候麻烦带上这几句 ──────────────────\n\n\
         · 出现问题前你在做什么（例如：在编辑界面拖白平衡时突然退出）\n\
         · 报告 ID：{report_id}\n\
         · 如果方便，说一下你的系统与版本（应用里「设置 → 关于」能看到）\n\n\
         本指南随报告一起生成，可直接转发给需要的人。\n",
        what = what,
        report_id = report_id,
        created_at = created_at,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_crash(dir: &Path) {
        std::fs::create_dir_all(dir).expect("建目录");
        let report = format!(
            "{{\"schema\":1,\"kind\":\"panic\",\"reportId\":\"{}\",\"session\":\"s-1\",\"createdAt\":\"2026-10-12T01:39:51.218Z\",\"app\":{{\"version\":\"0.1.1\",\"profile\":\"dev\",\"os\":\"windows\",\"arch\":\"x86_64\"}},\"panic\":{{\"message\":\"boom\",\"count\":1}},\"backtrace\":[],\"logTail\":[\"line\"]}}",
            "RB-20261012-3F9A21"
        );
        std::fs::write(dir.join(REPORT_JSON), report).expect("写 report");
        std::fs::write(dir.join(REPORT_TEXT), "人读一页").expect("写 txt");
        std::fs::write(dir.join("dump.dmp"), b"DUMPDATA").expect("写 dump");
    }

    #[test]
    fn latest_unprompted_skips_marked_and_prefers_newest() {
        let dir = tempfile::tempdir().expect("临时目录");
        let older = dir.path().join("RB-20261011-AAAAAA");
        let newer = dir.path().join("RB-20261012-BBBBBB");
        seed_crash(&older);
        seed_crash(&newer);
        let found = latest_unprompted(dir.path()).expect("应当找到");
        assert!(found.dir.ends_with("RB-20261012-BBBBBB"), "取最新的一份：{}", found.dir);
        assert_eq!(found.report_id, "RB-20261012-3F9A21");
        assert!(found.has_dump);
        mark_prompted(Path::new(&found.dir));
        let next = latest_unprompted(dir.path()).expect("还有更老的一份");
        assert!(next.dir.ends_with("RB-20261011-AAAAAA"));
        mark_prompted(Path::new(&next.dir));
        assert!(latest_unprompted(dir.path()).is_none(), "都提示过就不再打扰");
    }

    #[test]
    fn export_writes_zip_with_expected_entries_and_a_guide_next_to_it() {
        let dir = tempfile::tempdir().expect("临时目录");
        let crash = dir.path().join("crashes").join("RB-20261012-3F9A21");
        let logs = dir.path().join("logs");
        std::fs::create_dir_all(&logs).expect("建日志目录");
        seed_crash(&crash);
        std::fs::write(logs.join("raybend.log"), "会话头\n一行日志\n").expect("写日志");
        std::fs::write(logs.join("raybend.1.log"), "上一次会话\n").expect("写日志");
        let target = dir.path().join("导出").join("RayBend-错误报告-RB-20261012-3F9A21.zip");

        let outcome = export(&crash, &logs, &target, true).expect("导出");
        assert!(target.is_file(), "zip 生成了");
        assert!(outcome.zip_bytes > 0);
        for entry in [
            "report.json",
            "report.txt",
            "dump.dmp",
            "logs/raybend.log",
            "logs/raybend.1.log",
            GUIDE_NAME,
        ] {
            assert!(outcome.entries.iter().any(|item| item == entry), "缺条目 {entry}");
        }
        assert!(Path::new(&outcome.guide_path).is_file(), "指南落在 zip 旁边");
        let guide = std::fs::read_to_string(&outcome.guide_path).expect("读指南");
        assert!(guide.contains("raybend+andares@outlook.com"));
        assert!(guide.contains("https://github.com/C-Thun/raybend/issues"));
        assert!(guide.contains("RB-20261012-3F9A21"));
        assert!(!guide.contains("QQ"), "QQ 群还没建，先不写占位（崔总 2026-10-12）");

        // zip 真能打开、条目对得上
        let file = std::fs::File::open(&target).expect("开 zip");
        let mut archive = zip::ZipArchive::new(file).expect("读 zip");
        let names: Vec<String> = (0..archive.len())
            .filter_map(|index| archive.by_index(index).ok().map(|entry| entry.name().to_string()))
            .collect();
        assert!(names.contains(&"report.json".to_string()), "{names:?}");
    }

    #[test]
    fn export_without_dump_omits_it() {
        let dir = tempfile::tempdir().expect("临时目录");
        let crash = dir.path().join("RB-20261012-3F9A21");
        seed_crash(&crash);
        let logs = dir.path().join("logs");
        std::fs::create_dir_all(&logs).expect("建目录");
        let target = dir.path().join("out.zip");
        let outcome = export(&crash, &logs, &target, false).expect("导出");
        assert!(!outcome.entries.iter().any(|item| item == "dump.dmp"), "不勾就不带 dump");
        assert!(outcome.entries.iter().any(|item| item == "report.json"));
    }

    #[test]
    fn guide_mentions_the_two_channels_and_the_id() {
        let text = guide_text("RB-20261012-3F9A21", "panic", "2026-10-12T01:39:51.218Z");
        assert!(text.contains("raybend+andares@outlook.com"));
        assert!(text.contains("https://github.com/C-Thun/raybend/issues"));
        assert!(text.contains("脱敏"));
        assert!(text.contains("RB-20261012-3F9A21"));
        let hard = guide_text("RB-1", "hard-crash", "x");
        assert!(hard.contains("崩溃"));
    }
}
