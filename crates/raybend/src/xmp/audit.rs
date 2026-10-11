//! 巡检（`specs/xmp-audit-tool.md`）：把盘上的 sidecar 与库里应写的内容对一遍。
//!
//! **只读**：不写、不删、不改盘上任何文件。判定复用运行时同一份实现
//! （`collect` / `compose` / `crs_fragments` / `issues::profile_hash`），不另立一套规则。
//!
//! 用途是验收辅助：文件层（路径规则/孤儿/哈希/元数据/`crs:` 规则/大小写折叠）由它一次核对，
//! GUI 交互、视觉与回收站仍由人确认（`AGENTS.md` §2.8）。

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::Connection;

use super::{
    AssetSidecar, ParsedSidecar, collect, compose, crs_fragments, is_ours, latest_is_recorded,
    mapping, parse, resolve_sidecar,
};
use crate::Result;
use crate::store::path_semantics::PathForms;
use crate::store::{issues, time};

/// 巡检结果里一条发现的级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// 盘上与库不一致 —— 必须处理。
    Fail,
    /// 可疑但不一定是错（陈旧副本、文本漂移）。
    Warn,
    /// 说明性（外来文件、正常状态）。
    Info,
}

impl Severity {
    fn tag(self) -> &'static str {
        match self {
            Self::Fail => "FAIL",
            Self::Warn => "WARN",
            Self::Info => "INFO",
        }
    }
}

/// 一条发现。`subject` 是库内相对路径（或说明性文字）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    pub subject: String,
    pub message: String,
}

/// 一次巡检的报告（纯数据：CLI 打印，测试断言，将来界面也能用）。
#[derive(Debug, Clone, Default)]
pub struct AuditReport {
    /// 库里的资产数。
    pub assets: usize,
    /// 其中「有可表达内容、应有 sidecar」的资产数。
    pub expected: usize,
    /// `photos/` 下找到的 `.xmp` 文件数。
    pub on_disk: usize,
    pub findings: Vec<Finding>,
}

impl AuditReport {
    #[must_use]
    pub fn count(&self, severity: Severity) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == severity)
            .count()
    }

    /// 人读文本（CLI 直接打印）。
    #[must_use]
    pub fn render(&self, root: &Path) -> String {
        let mut out = format!(
            "XMP 巡检：{}\n资产 {}（应写 {}，盘上 .xmp {}）\n",
            root.display(),
            self.assets,
            self.expected,
            self.on_disk,
        );
        for finding in &self.findings {
            out.push_str(&format!(
                "[{}] {}：{}\n",
                finding.severity.tag(),
                finding.subject,
                finding.message
            ));
        }
        out.push_str(&format!(
            "汇总：FAIL {} / WARN {} / INFO {}\n",
            self.count(Severity::Fail),
            self.count(Severity::Warn),
            self.count(Severity::Info),
        ));
        out
    }
}

/// 巡检一个库根。`root` 是 `catalog.db` 所在目录。
pub fn audit(conn: &Connection, root: &Path) -> Result<AuditReport> {
    let mut report = AuditReport::default();
    let now = time::now_millis();
    let tool = concat!("RayBend ", env!("CARGO_PKG_VERSION"));

    // 全部资产：期望的落点（含「不该有文件」的）与内容。
    let asset_ids: Vec<i64> = conn
        .prepare("SELECT id FROM assets ORDER BY id")?
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;

    let mut owned: BTreeMap<String, String> = BTreeMap::new(); // folded rel → rel
    for asset_id in asset_ids {
        report.assets += 1;
        let Some(asset) = collect(conn, asset_id)? else {
            continue; // 没有文件行：无法确定落点
        };
        owned.insert(
            PathForms::new(&asset.rel).folded().to_string(),
            asset.rel.clone(),
        );
        let expected_xml = compose(&asset.content, now, tool);
        if expected_xml.is_some() {
            report.expected += 1;
        }
        verify_asset(&mut report, root, &asset, expected_xml);
    }

    scan_disk(&mut report, root, &owned)?;
    Ok(report)
}

/// 单个资产：存在性 → 归属 → 内容一致性。
fn verify_asset(
    report: &mut AuditReport,
    root: &Path,
    asset: &AssetSidecar,
    expected_xml: Option<String>,
) {
    let rel = asset.rel.clone();
    let abs = root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    let existing = match resolve_sidecar(&abs) {
        Ok(found) => found,
        Err(error) => {
            report.findings.push(Finding {
                severity: Severity::Fail,
                subject: rel,
                message: format!("路径解析失败：{error}"),
            });
            return;
        }
    };

    match (expected_xml, existing) {
        (Some(_), None) => report.findings.push(Finding {
            severity: Severity::Fail,
            subject: rel,
            message: "应有 sidecar，盘上没有".into(),
        }),
        (Some(expected), Some(path)) => verify_file(report, asset, &rel, &path, &expected),
        (None, None) => {}
        (None, Some(path)) => {
            // 库里没内容却有文件：自家多半是陈旧副本；别家文件我们本来就不动。
            if is_ours(&path) {
                report.findings.push(Finding {
                    severity: Severity::Warn,
                    subject: rel,
                    message: "库里没有可表达内容，文件仍在（陈旧副本，下次触发会删除）".into(),
                });
            } else {
                report.findings.push(Finding {
                    severity: Severity::Info,
                    subject: rel,
                    message: "外来文件（无 rb:，不动它）".into(),
                });
            }
        }
    }
}

/// 盘上已存在（且应为自家）的文件：解析 + 逐项比对。
fn verify_file(
    report: &mut AuditReport,
    asset: &AssetSidecar,
    rel: &str,
    path: &Path,
    expected_xml: &str,
) {
    let mut push = |severity: Severity, message: String| {
        report.findings.push(Finding {
            severity,
            subject: rel.to_string(),
            message,
        });
    };

    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return push(Severity::Fail, format!("读取失败：{error}")),
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return push(Severity::Fail, "文件不是合法 UTF-8".into());
    };
    let parsed = match parse(text) {
        Ok(Some(parsed)) => parsed,
        Ok(None) => {
            return push(
                Severity::Fail,
                "该位置不是自家 sidecar（没有 rb: 域）".into(),
            );
        }
        Err(error) => return push(Severity::Fail, format!("解析失败：{error}")),
    };
    for warning in &parsed.warnings {
        push(Severity::Info, format!("解析提示：{warning}"));
    }

    compare_latest(report, asset, &parsed, rel);
    compare_profiles(report, asset, &parsed, rel);
    compare_metadata(report, asset, &parsed, rel);
    compare_tag_state(report, asset, &parsed, rel);
    compare_crs(report, asset, text, rel);
    compare_text(report, text, expected_xml, rel);
}

fn compare_latest(
    report: &mut AuditReport,
    asset: &AssetSidecar,
    parsed: &ParsedSidecar,
    rel: &str,
) {
    let db = asset
        .content
        .latest
        .as_ref()
        .filter(|stack| latest_is_recorded(stack));
    match (db, parsed.latest.as_ref()) {
        (Some(db), Some(file)) => {
            if file.source_base != db.source_base {
                report.findings.push(Finding {
                    severity: Severity::Fail,
                    subject: rel.to_string(),
                    message: "rb:latest 的编辑源与库不一致".into(),
                });
            }
            match (issues::profile_hash(db), issues::profile_hash(file)) {
                (Ok(db_hash), Ok(file_hash)) if db_hash != file_hash => {
                    report.findings.push(Finding {
                        severity: Severity::Fail,
                        subject: rel.to_string(),
                        message: "rb:latest 与库里的工作副本不一致".into(),
                    });
                }
                (Err(error), _) | (_, Err(error)) => report.findings.push(Finding {
                    severity: Severity::Fail,
                    subject: rel.to_string(),
                    message: format!("rb:latest 无法比对：{error}"),
                }),
                _ => {}
            }
        }
        (Some(_), None) => report.findings.push(Finding {
            severity: Severity::Fail,
            subject: rel.to_string(),
            message: "文件缺少 rb:latest（库里应有）".into(),
        }),
        (None, Some(_)) => report.findings.push(Finding {
            severity: Severity::Fail,
            subject: rel.to_string(),
            message: "文件有 rb:latest，库里已没有".into(),
        }),
        (None, None) => {}
    }
}

fn compare_profiles(
    report: &mut AuditReport,
    asset: &AssetSidecar,
    parsed: &ParsedSidecar,
    rel: &str,
) {
    let db_issues = &asset.content.profiles;
    for issue in db_issues {
        let Some(file) = parsed
            .profiles
            .iter()
            .find(|profile| profile.name == issue.name)
        else {
            report.findings.push(Finding {
                severity: Severity::Fail,
                subject: rel.to_string(),
                message: format!("定稿「{}」在文件里缺失", issue.name),
            });
            continue;
        };
        match (
            issues::profile_hash(&issue.stack),
            issues::profile_hash(&file.stack),
        ) {
            (Ok(db_hash), Ok(file_hash)) if db_hash != file_hash => {
                report.findings.push(Finding {
                    severity: Severity::Fail,
                    subject: rel.to_string(),
                    message: format!("定稿「{}」与库里的配置不一致", issue.name),
                });
            }
            (Err(error), _) | (_, Err(error)) => report.findings.push(Finding {
                severity: Severity::Fail,
                subject: rel.to_string(),
                message: format!("定稿「{}」无法比对：{error}", issue.name),
            }),
            _ => {}
        }
        if file.source_base != issue.source_base {
            report.findings.push(Finding {
                severity: Severity::Fail,
                subject: rel.to_string(),
                message: format!("定稿「{}」的编辑源与库不一致", issue.name),
            });
        }
        if file.ordinal != issue.ordinal {
            report.findings.push(Finding {
                severity: Severity::Fail,
                subject: rel.to_string(),
                message: format!(
                    "定稿「{}」的尾号不一致（文件 {:?} / 库里 {:?}）",
                    issue.name, file.ordinal, issue.ordinal
                ),
            });
        }
    }
    for file in &parsed.profiles {
        if !db_issues.iter().any(|issue| issue.name == file.name) {
            report.findings.push(Finding {
                severity: Severity::Warn,
                subject: rel.to_string(),
                message: format!("文件里的定稿「{}」库里没有", file.name),
            });
        }
    }
}

fn compare_metadata(
    report: &mut AuditReport,
    asset: &AssetSidecar,
    parsed: &ParsedSidecar,
    rel: &str,
) {
    let db = &asset.content.metadata;
    let file = &parsed.metadata;
    let mut mismatch = |field: &str, db_value: String, file_value: String| {
        report.findings.push(Finding {
            severity: Severity::Fail,
            subject: rel.to_string(),
            message: format!("{field} 不一致（文件 {file_value} / 库里 {db_value}）"),
        });
    };
    let text = |value: &Option<String>| value.clone().unwrap_or_else(|| "空".into());

    if file.rating != db.rating {
        mismatch("评级", db.rating.to_string(), file.rating.to_string());
    }
    if file.color_label != db.color_label {
        mismatch("色标", text(&db.color_label), text(&file.color_label));
    }
    if file.author != db.author {
        mismatch("作者", text(&db.author), text(&file.author));
    }
    if file.description != db.description {
        mismatch("说明", text(&db.description), text(&file.description));
    }
    if file.country != db.country {
        mismatch("国家", text(&db.country), text(&file.country));
    }
    if file.province_state != db.province_state {
        mismatch("省份", text(&db.province_state), text(&file.province_state));
    }
    if file.city != db.city {
        mismatch("城市", text(&db.city), text(&file.city));
    }
    if file.sublocation != db.sublocation {
        mismatch("地点", text(&db.sublocation), text(&file.sublocation));
    }
    if file.keywords != db.keywords {
        mismatch("关键词", db.keywords.join("、"), file.keywords.join("、"));
    }
}

fn compare_tag_state(
    report: &mut AuditReport,
    asset: &AssetSidecar,
    parsed: &ParsedSidecar,
    rel: &str,
) {
    let db = asset
        .content
        .tag_state
        .as_ref()
        .and_then(|state| serde_json::to_value(state).ok());
    let file = parsed
        .tag_state
        .as_ref()
        .and_then(|state| serde_json::to_value(state).ok());
    match (db, file) {
        (Some(db), Some(file)) if db != file => report.findings.push(Finding {
            severity: Severity::Fail,
            subject: rel.to_string(),
            message: "标签来源快照（rb:tagState）与库不一致".into(),
        }),
        (Some(_), None) => report.findings.push(Finding {
            severity: Severity::Fail,
            subject: rel.to_string(),
            message: "文件缺少 rb:tagState（库里应有）".into(),
        }),
        (None, Some(_)) => report.findings.push(Finding {
            severity: Severity::Warn,
            subject: rel.to_string(),
            message: "文件有 rb:tagState，库里已没有".into(),
        }),
        _ => {}
    }
}

/// `crs:` 规则（`specs/xmp-sidecar.md` §5.1）：只有 raw 基、有可映射项的 latest 才写。
fn compare_crs(report: &mut AuditReport, asset: &AssetSidecar, text: &str, rel: &str) {
    let mut push = |message: String| {
        report.findings.push(Finding {
            severity: Severity::Fail,
            subject: rel.to_string(),
            message,
        });
    };
    let latest = asset.content.latest.as_ref();
    let block = latest
        .filter(|stack| {
            stack.source_base == crate::store::develop::EditBase::Raw && !stack.is_empty()
        })
        .map(|stack| mapping::crs_block(stack, asset.content.raw_file_name.as_deref()));
    let Some(block) = block.filter(|block| block.has_any) else {
        if text.contains(" crs:") {
            push("不该有 crs: 调整（sooc 基或没有可映射项），文件里却出现".into());
        }
        return;
    };
    let (attrs, elements) = crs_fragments(&block);
    for attr in attrs.split(' ').filter(|part| !part.is_empty()) {
        if !text.contains(attr) {
            push(format!("crs: 调整缺失或与库不符：{attr}"));
        }
    }
    for element in elements
        .split("><")
        .map(|part| part.trim_start_matches('<'))
    {
        let element = element.trim_end_matches('>');
        if !element.is_empty() && !text.contains(element) {
            push(format!("crs: 曲线缺失或与库不符：{element}"));
        }
    }
}

/// 整体文本漂移（WARN）：去掉每次写出都会变的两个属性后，应当与重新组合的结果一致。
///
/// 逐项检查已经覆盖语义差异；这里只是把「形状漂移」（例如跨版本的序列化变化）提示出来。
fn compare_text(report: &mut AuditReport, text: &str, expected: &str, rel: &str) {
    let actual = strip_volatile(text);
    let expected = strip_volatile(expected);
    if actual == expected {
        return;
    }
    let hint = first_difference(&expected, &actual).map_or_else(String::new, |(left, right)| {
        format!("\n      期望 …{left}…\n      文件 …{right}…")
    });
    report.findings.push(Finding {
        severity: Severity::Warn,
        subject: rel.to_string(),
        message: format!("文本与重新组合的结果不同（{hint}）"),
    });
}

/// 去掉每次写出/工具版本都会变的属性（`xmp:MetadataDate` / `xmp:CreatorTool` / `x:xmptk`）。
fn strip_volatile(xml: &str) -> String {
    let mut out = xml.to_string();
    for name in ["xmp:MetadataDate", "xmp:CreatorTool", "x:xmptk"] {
        while let Some(start) = out.find(&format!(" {name}=\"")) {
            let value_start = start + name.len() + 3;
            let Some(value_end) = out[value_start..].find('"') else {
                break;
            };
            out.replace_range(start..value_start + value_end + 1, "");
        }
    }
    out
}

/// 首个差异位置与两侧窗口（窗口按字符边界切，避免切坏 UTF-8）。
fn first_difference(expected: &str, actual: &str) -> Option<(String, String)> {
    let expected_chars: Vec<char> = expected.chars().collect();
    let actual_chars: Vec<char> = actual.chars().collect();
    let at = expected_chars
        .iter()
        .zip(&actual_chars)
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| expected_chars.len().min(actual_chars.len()));
    let window = |chars: &[char]| {
        chars
            .iter()
            .skip(at.saturating_sub(20))
            .take(60)
            .collect::<String>()
            .replace('\n', "⏎")
    };
    Some((window(&expected_chars), window(&actual_chars)))
}

/// 盘面扫描：`photos/` 下所有 `.xmp` 的归属、落点与大小写折叠歧义。
fn scan_disk(
    report: &mut AuditReport,
    root: &Path,
    owned: &BTreeMap<String, String>,
) -> Result<()> {
    let photos = root.join("photos");
    if !photos.is_dir() {
        report.findings.push(Finding {
            severity: Severity::Info,
            subject: "photos/".into(),
            message: "库内没有 photos/ 目录，跳过盘面扫描".into(),
        });
        return Ok(());
    }

    let mut found: Vec<String> = Vec::new();
    let mut stack = vec![photos.clone()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) => {
                report.findings.push(Finding {
                    severity: Severity::Warn,
                    subject: rel_to_root(root, &dir),
                    message: format!("目录读不到：{error}"),
                });
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if is_xmp_file(&path) {
                found.push(rel_to_root(root, &path));
            }
        }
    }
    report.on_disk = found.len();

    // 大小写折叠歧义：同一目标名出现 ≥2 次（`resolve_sidecar` 会拒绝，发布也不能猜）。
    let mut by_folded: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for rel in &found {
        by_folded
            .entry(PathForms::new(rel).folded().to_string())
            .or_default()
            .push(rel.clone());
    }
    for group in by_folded.values().filter(|group| group.len() > 1) {
        report.findings.push(Finding {
            severity: Severity::Fail,
            subject: group[0].clone(),
            message: format!("同一目标名有多个大小写折叠变体：{}", group.join(" / ")),
        });
    }

    for rel in &found {
        let abs = root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let ours = is_ours(&abs);
        if in_raw_dir(rel) {
            report.findings.push(Finding {
                severity: if ours { Severity::Fail } else { Severity::Info },
                subject: rel.clone(),
                message: if ours {
                    "_RAW/ 里出现自家 sidecar（规则禁止写进 _RAW/）".into()
                } else {
                    "外来文件在 _RAW/（我们不动它）".into()
                },
            });
            continue;
        }
        if let Some(target) = owned.get(PathForms::new(rel).folded()) {
            if *target != *rel {
                report.findings.push(Finding {
                    severity: Severity::Info,
                    subject: rel.clone(),
                    message: format!("实际文件大小写与库内路径不同（库内是 {target}）"),
                });
            }
            continue; // 资产比对已在 verify_asset 里做过
        }
        report.findings.push(Finding {
            severity: if ours { Severity::Fail } else { Severity::Info },
            subject: rel.clone(),
            message: if ours {
                "孤儿 sidecar：找不到对应资产".into()
            } else {
                "外来文件（无 rb:，不动它）".into()
            },
        });
    }
    Ok(())
}

fn is_xmp_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("xmp"))
}

fn in_raw_dir(rel: &str) -> bool {
    rel.split('/')
        .rev()
        .skip(1)
        .any(|part| part.eq_ignore_ascii_case("_RAW"))
}

fn rel_to_root(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(rel) => rel
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/"),
        Err(_) => path
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::develop::{DevelopStack, EditBase};
    use crate::store::migration::{self, DbKind};
    use crate::store::{assets, develop};
    use crate::xmp::{PublishOutcome, publish};
    use std::path::PathBuf;

    struct Fixture {
        _dir: tempfile::TempDir,
        root: PathBuf,
        conn: Connection,
        asset_id: i64,
        sidecar: PathBuf,
    }

    /// 真实临时库：一个 RAW 基的照片（位图 + `_RAW/` RAW）+ 编辑 + 定稿 + 元数据，
    /// 并用**运行时同一份实现**写出 sidecar —— 这是「干净状态」。
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("photos/2026-08-15/_RAW")).unwrap();
        std::fs::write(root.join("photos/2026-08-15/MYP0001.JPG"), b"jpg").unwrap();
        std::fs::write(root.join("photos/2026-08-15/_RAW/MYP0001.ORF"), b"raw").unwrap();

        let mut conn = Connection::open_in_memory().unwrap();
        crate::store::pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::Catalog, migration::Backups::none(), 1).unwrap();
        let asset_id = assets::insert_asset(&conn, 10).unwrap();
        conn.execute(
            "INSERT INTO asset_files(asset_id, role, rel_path, rel_path_folded, ext, created_at, updated_at) \
             VALUES(?1, 'bitmap', 'photos/2026-08-15/MYP0001.JPG', lower('photos/2026-08-15/MYP0001.JPG'), 'jpg', 1, 1)",
            [asset_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO asset_files(asset_id, role, rel_path, rel_path_folded, ext, created_at, updated_at) \
             VALUES(?1, 'raw', 'photos/2026-08-15/_RAW/MYP0001.ORF', lower('photos/2026-08-15/_RAW/MYP0001.ORF'), 'orf', 1, 1)",
            [asset_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE assets SET rating = 4, color_label = 'green', author = '崔总', city = '杭州' WHERE id = ?1",
            [asset_id],
        )
        .unwrap();

        let mut latest = DevelopStack {
            source_base: EditBase::Raw,
            ..Default::default()
        };
        latest.params.insert("exposure".into(), 0.35);
        develop::save(&conn, asset_id, &latest, 20).unwrap();
        issues::create(&conn, asset_id, "暖调", &latest, 30).unwrap();

        let sidecar = root.join("photos/2026-08-15/MYP0001.xmp");
        let asset = collect(&conn, asset_id).unwrap().unwrap();
        let xml = compose(&asset.content, 40, "RayBend test").unwrap();
        assert_eq!(publish(&sidecar, &xml).unwrap(), PublishOutcome::Written);

        Fixture {
            _dir: dir,
            root,
            conn,
            asset_id,
            sidecar,
        }
    }

    fn messages(report: &AuditReport, severity: Severity) -> Vec<String> {
        report
            .findings
            .iter()
            .filter(|finding| finding.severity == severity)
            .map(|finding| finding.message.clone())
            .collect()
    }

    #[test]
    fn clean_library_passes() {
        let fixture = fixture();
        let report = audit(&fixture.conn, &fixture.root).unwrap();
        assert_eq!(report.assets, 1);
        assert_eq!(report.expected, 1);
        assert_eq!(report.on_disk, 1);
        assert_eq!(report.count(Severity::Fail), 0, "{:?}", report.findings);
        assert_eq!(report.count(Severity::Warn), 0, "{:?}", report.findings);
        assert!(report.render(&fixture.root).contains("汇总：FAIL 0"));
    }

    #[test]
    fn missing_file_is_a_failure() {
        let fixture = fixture();
        std::fs::remove_file(&fixture.sidecar).unwrap();
        let report = audit(&fixture.conn, &fixture.root).unwrap();
        assert!(
            messages(&report, Severity::Fail)
                .iter()
                .any(|m| m.contains("应写未写") || m.contains("应有 sidecar"))
        );
    }

    #[test]
    fn metadata_drift_is_a_failure() {
        let fixture = fixture();
        fixture
            .conn
            .execute(
                "UPDATE assets SET rating = 5 WHERE id = ?1",
                [fixture.asset_id],
            )
            .unwrap();
        let report = audit(&fixture.conn, &fixture.root).unwrap();
        let failures = messages(&report, Severity::Fail);
        assert!(failures.iter().any(|m| m.contains("评级")), "{failures:?}");
    }

    #[test]
    fn foreign_and_orphan_files_are_reported_by_ownership() {
        let fixture = fixture();
        // 外来文件：不进 FAIL。
        std::fs::write(
            fixture.root.join("photos/2026-08-15/OTHER.xmp"),
            "<x/> lightroom",
        )
        .unwrap();
        // 孤儿自家文件：FAIL。
        std::fs::copy(
            &fixture.sidecar,
            fixture.root.join("photos/2026-08-15/ORPHAN.xmp"),
        )
        .unwrap();
        // `_RAW/` 里出现自家文件：FAIL。
        std::fs::copy(
            &fixture.sidecar,
            fixture.root.join("photos/2026-08-15/_RAW/MYP0001.xmp"),
        )
        .unwrap();

        let report = audit(&fixture.conn, &fixture.root).unwrap();
        assert_eq!(report.on_disk, 4);
        let failures = messages(&report, Severity::Fail);
        assert!(failures.iter().any(|m| m.contains("孤儿")), "{failures:?}");
        assert!(failures.iter().any(|m| m.contains("_RAW/")), "{failures:?}");
        let infos = messages(&report, Severity::Info);
        assert!(infos.iter().any(|m| m.contains("外来")), "{infos:?}");
    }

    #[test]
    fn folded_duplicates_are_a_failure() {
        let fixture = fixture();
        std::fs::write(
            fixture.root.join("photos/2026-08-15/MYP0001.XMP"),
            std::fs::read(&fixture.sidecar).unwrap(),
        )
        .unwrap();
        let report = audit(&fixture.conn, &fixture.root).unwrap();
        let failures = messages(&report, Severity::Fail);
        assert!(
            failures.iter().any(|m| m.contains("大小写折叠变体")),
            "{failures:?}"
        );
    }

    #[test]
    fn crs_layer_follows_raw_base_only() {
        let fixture = fixture();
        // 库里切到 SOOC 基（文件没重写）：文件里的 crs: 就成了不该有的东西。
        let mut sooc = DevelopStack {
            source_base: EditBase::Sooc,
            ..Default::default()
        };
        sooc.params.insert("exposure".into(), 0.35);
        develop::save(&fixture.conn, fixture.asset_id, &sooc, 50).unwrap();
        let report = audit(&fixture.conn, &fixture.root).unwrap();
        let failures = messages(&report, Severity::Fail);
        assert!(
            failures.iter().any(|m| m.contains("不该有 crs:")),
            "{failures:?}"
        );
    }

    #[test]
    fn text_drift_is_only_a_warning() {
        let fixture = fixture();
        let text = std::fs::read_to_string(&fixture.sidecar).unwrap();
        // 手工插入一个无意义的空白差异：语义全对，只有形状变了。
        std::fs::write(&fixture.sidecar, text.replace("<rdf:RDF", "<rdf:RDF  ")).unwrap();
        let report = audit(&fixture.conn, &fixture.root).unwrap();
        assert_eq!(report.count(Severity::Fail), 0, "{:?}", report.findings);
        assert!(
            messages(&report, Severity::Warn)
                .iter()
                .any(|m| m.contains("文本")),
            "{:?}",
            report.findings
        );
    }
}
