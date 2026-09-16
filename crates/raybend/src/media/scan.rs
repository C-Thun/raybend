//! 目录扫描：把一棵目录树里的媒体文件枚举出来。
//!
//! 用途：建库后的首次扫描、导入前的源目录清点、以及「库刷新」。**只管枚举，
//! 不碰数据库** —— 落库与差分是 `media::diff` 的事。
//!
//! # 写在代码里的取舍（免得以后被当性能问题「优化」掉）
//!
//! * **刻意单线程**：顺序读对 HDD / SMB / 云盘友好（随机寻道是机械盘与网络盘的
//!   主要成本），而且**结果顺序确定**（同一目录内按折叠文件名排序）——
//!   后者是导入时序号分配可复现的前提。真正的并发留给后面的 EXIF / 缩略图阶段
//!   （任务队列），那里才是 CPU 瓶颈所在。
//! * **符号链接默认不跟随**：目录环会让扫描无限深入，而用户放软链通常是
//!   「同一个目录的另一个入口」，重复扫进来只会造成重复导入。
//! * **子目录读不了不该毁掉整次扫描**（权限、损坏、拔盘中）：记一笔
//!   [`ScanProblem`] 继续走，最后汇总给用户看。
//! * 路径一律用 `/` 分隔、相对于扫描根；是否再存「折叠形式」由调用方决定
//!   （`store::path_semantics`）。

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::error::{Error, Result};
use crate::media::kind::{self, MediaKind};
use crate::store::time;

/// 取消令牌。克隆出来的副本指向同一个标志，因此可以跨线程传递。
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 请求取消。已经在跑的操作会在下一个检查点停下。
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// 扫描参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanOptions {
    /// 是否把 `.` 开头的隐藏文件/目录也算进来（默认否）。
    pub include_hidden: bool,
    /// 是否报告非照片文件（默认否：只报 RAW 与普通图像）。
    pub include_other: bool,
    /// 是否跟随符号链接（默认否，见模块文档）。
    pub follow_symlinks: bool,
    /// 最大目录深度：根为 0，其直接子目录为 1（默认 32）。
    pub max_depth: usize,
    /// 是否跳过 0 字节文件（默认是：空文件必然是坏文件）。
    pub skip_empty_files: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            include_hidden: false,
            include_other: false,
            follow_symlinks: false,
            max_depth: 32,
            skip_empty_files: true,
        }
    }
}

/// 扫到的一个目录。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ScannedDir {
    /// 相对扫描根的路径（`/` 分隔）。根自己为 `""`。
    pub rel_path: String,
    /// 深度：根 0，其直接子目录 1。
    pub depth: usize,
}

/// 扫到的一个文件。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ScannedFile {
    /// 绝对路径。
    pub abs_path: PathBuf,
    /// 相对扫描根的路径（`/` 分隔，原始大小写）。
    pub rel_path: String,
    /// 文件名的原始写法。
    pub file_name: String,
    /// 小写扩展名（不含点）；无扩展名时为 `None`。
    pub ext: Option<String>,
    /// 大类（RAW / 图像 / 其它）。
    pub kind: MediaKind,
    /// 去扩展名并折叠后的主体（配对位图与 RAW 用，见 `kind::stem_folded`）。
    pub stem_folded: String,
    pub size_bytes: u64,
    /// 修改时间（Unix 毫秒）。取不到时为 `None`。
    pub mtime_ms: Option<i64>,
}

/// 跳过某个文件/目录的理由（统计与日志用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// `.` 开头的隐藏项（`include_hidden` 关时）。
    Hidden,
    /// 系统元数据 / 临时文件 / 自家库文件（见 [`JunkKind`]）。
    Junk,
    /// 不是照片（`include_other` 关时）。
    NotAPhoto,
    /// 0 字节。
    Empty,
    /// 符号链接（`follow_symlinks` 关时）。
    Symlink,
    /// 超过 [`ScanOptions::max_depth`]。
    TooDeep,
}

/// 扫描过程中遇到的问题（不致命，计入 [`ScanOutcome::problems`]）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ScanProblem {
    /// 出问题的相对路径（`/` 分隔）。
    pub rel_path: String,
    /// 给用户看的说明。
    pub message: String,
}

/// 扫描事件：目录与文件**按遍历顺序**流式回调（目录先于其内容）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanEvent {
    Dir(ScannedDir),
    File(ScannedFile),
}

/// 一次扫描的结果统计。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ScanOutcome {
    pub dirs: usize,
    pub files: usize,
    /// 被跳过的文件/目录总数（可按 [`SkipReason`] 细分统计）。
    pub skipped: usize,
    /// 各跳过理由的计数（顺序固定，便于断言与展示）。
    pub skipped_by_reason: Vec<(SkipReason, usize)>,
    pub problems: Vec<ScanProblem>,
    /// 是否被 [`Cancel`] 中止（中止时结果是**不完整**的）。
    pub cancelled: bool,
    pub elapsed_ms: i64,
}

impl ScanOutcome {
    fn skip(&mut self, reason: SkipReason) {
        self.skipped += 1;
        match self
            .skipped_by_reason
            .iter_mut()
            .find(|(r, _)| *r == reason)
        {
            Some((_, n)) => *n += 1,
            None => self.skipped_by_reason.push((reason, 1)),
        }
    }

    /// 某个理由跳过了多少（没出现则 0）。
    #[must_use]
    pub fn skipped_count(&self, reason: SkipReason) -> usize {
        self.skipped_by_reason
            .iter()
            .find(|(r, _)| *r == reason)
            .map_or(0, |(_, n)| *n)
    }
}

/// 扫描一棵目录树，把每个目录/文件通过 `on_event` 回调出去。
///
/// * `root` 不存在 → [`Error::PathNotFound`]。
/// * 回调返回 `Err` 会**立刻中止**扫描并把错误抛出来（调用方一般在落库失败时这么做）。
/// * 回调拿到的文件是**流式**的，调用方可以边扫边批量写库，不必等整棵树扫完。
pub fn scan<F>(
    root: &Path,
    opts: &ScanOptions,
    cancel: &Cancel,
    mut on_event: F,
) -> Result<ScanOutcome>
where
    F: FnMut(ScanEvent) -> Result<()>,
{
    let started = Instant::now();
    let mut outcome = ScanOutcome::default();

    let root_meta = std::fs::metadata(root).map_err(|_| Error::PathNotFound(root.to_path_buf()))?;
    if !root_meta.is_dir() {
        return Err(Error::Unsupported(format!(
            "扫描目标不是目录：{}",
            root.display()
        )));
    }

    // 目录队列：(绝对路径, 相对路径, 深度)。
    // 用 VecDeque 做**广度优先**：同一层的目录一起处理，用户先看到「顶层有什么」，
    // 对导入界面更友好；同一目录内再按名字排序 → 总体顺序确定。
    let mut queue: VecDeque<(PathBuf, String, usize)> =
        VecDeque::from([(root.to_path_buf(), String::new(), 0)]);

    'walk: while let Some((dir, rel, depth)) = queue.pop_front() {
        if cancel.is_cancelled() {
            outcome.cancelled = true;
            break;
        }
        on_event(ScanEvent::Dir(ScannedDir {
            rel_path: rel.clone(),
            depth,
        }))?;
        outcome.dirs += 1;

        let entries = match std::fs::read_dir(&dir) {
            Ok(it) => it,
            Err(e) => {
                // 子目录读不了：记一笔继续（根目录读不了也算，反正还有别的目录）
                outcome.problems.push(ScanProblem {
                    rel_path: if rel.is_empty() {
                        ".".into()
                    } else {
                        rel.clone()
                    },
                    message: e.to_string(),
                });
                continue;
            }
        };

        // 收集 + 排序：`read_dir` 的顺序是文件系统给的，不确定
        let mut items: Vec<(String, std::fs::DirEntry)> = Vec::new();
        for e in entries {
            match e {
                Ok(entry) => {
                    let name = entry.file_name().to_string_lossy().to_string();
                    items.push((name, entry));
                }
                Err(e) => outcome.problems.push(ScanProblem {
                    rel_path: rel.clone(),
                    message: e.to_string(),
                }),
            }
        }
        items.sort_by(|a, b| {
            kind::stem_folded(&a.0)
                .cmp(&kind::stem_folded(&b.0))
                .then_with(|| a.0.cmp(&b.0))
        });

        for (name, entry) in items {
            // 取消检查必须**逐个文件**做：一个大目录里可能有几万张照片，
            // 只在「取下一个目录」时检查会让取消延迟到难以接受。
            if cancel.is_cancelled() {
                outcome.cancelled = true;
                break 'walk;
            }
            let child_rel = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };

            // 隐藏项（`include_hidden` 关时直接跳过，目录也一样）
            #[cfg(windows)]
            let hidden_by_attribute = !opts.include_hidden
                && entry
                    .metadata()
                    .map(|meta| kind::has_hidden_attribute(&meta))
                    .unwrap_or(false);
            #[cfg(not(windows))]
            let hidden_by_attribute = false;

            if !opts.include_hidden && (name.starts_with('.') || hidden_by_attribute) {
                outcome.skip(SkipReason::Hidden);
                continue;
            }

            // 先看符号链接：`file_type()` 来自 read_dir，不额外发系统调用
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(e) => {
                    outcome.problems.push(ScanProblem {
                        rel_path: child_rel,
                        message: e.to_string(),
                    });
                    continue;
                }
            };
            if ft.is_symlink() && !opts.follow_symlinks {
                outcome.skip(SkipReason::Symlink);
                continue;
            }

            // 符号链接要跟随的话，得用 metadata()（会解析到目标）
            let meta = if ft.is_symlink() {
                std::fs::metadata(entry.path())
            } else {
                entry.metadata()
            };
            let meta = match meta {
                Ok(m) => m,
                Err(e) => {
                    outcome.problems.push(ScanProblem {
                        rel_path: child_rel,
                        message: e.to_string(),
                    });
                    continue;
                }
            };

            if meta.is_dir() {
                if depth + 1 > opts.max_depth {
                    outcome.skip(SkipReason::TooDeep);
                } else {
                    queue.push_back((entry.path(), child_rel, depth + 1));
                }
                continue;
            }

            if !meta.is_file() {
                // 设备文件、FIFO、socket 之类：跳过
                outcome.skip(SkipReason::NotAPhoto);
                continue;
            }

            // 垃圾文件（系统元数据 / 临时文件 / 自家库文件）
            let mut junked = false;
            for part in std::slice::from_ref(&name) {
                if kind::junk_kind(part).is_some() {
                    outcome.skip(SkipReason::Junk);
                    junked = true;
                }
            }
            if junked {
                continue;
            }

            // 侧车不算照片本体（xmp/aae/thm/lrv/dop/pp3）
            let file_kind = kind::kind_of_file(&name);
            if kind::is_sidecar(&name) || (!file_kind.is_photo() && !opts.include_other) {
                outcome.skip(SkipReason::NotAPhoto);
                continue;
            }

            let size = meta.len();
            if size == 0 && opts.skip_empty_files {
                outcome.skip(SkipReason::Empty);
                continue;
            }

            outcome.files += 1;
            on_event(ScanEvent::File(ScannedFile {
                abs_path: entry.path(),
                rel_path: child_rel,
                ext: kind::extension(&name),
                stem_folded: kind::stem_folded(&name),
                file_name: name,
                kind: file_kind,
                size_bytes: size,
                mtime_ms: meta.modified().ok().map(time::from_system_time),
            }))?;
        }
    }

    outcome.elapsed_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    Ok(outcome)
}

/// 扫描并把结果收进内存（测试与「只要清单」的场景用；大目录请用 [`scan`] 流式）。
pub fn scan_collect(
    root: &Path,
    opts: &ScanOptions,
    cancel: &Cancel,
) -> Result<(Vec<ScannedFile>, ScanOutcome)> {
    let mut files = Vec::new();
    let outcome = scan(root, opts, cancel, |ev| {
        if let ScanEvent::File(f) = ev {
            files.push(f);
        }
        Ok(())
    })?;
    Ok((files, outcome))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    /// 建文件（自动建父目录），返回绝对路径。
    fn write(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&p, bytes).unwrap();
        p
    }

    fn rels(files: &[ScannedFile]) -> Vec<String> {
        files.iter().map(|f| f.rel_path.clone()).collect()
    }

    // ---------- 基本行为 ----------

    #[test]
    fn finds_photos_and_ignores_everything_else() {
        let dir = tmp();
        let root = dir.path();
        write(root, "a.jpg", b"x");
        write(root, "b.RW2", b"x");
        write(root, "notes.txt", b"x");
        write(root, "sub/c.png", b"x");

        let (files, out) = scan_collect(root, &ScanOptions::default(), &Cancel::new()).unwrap();
        assert_eq!(rels(&files), vec!["a.jpg", "b.RW2", "sub/c.png"]);
        assert_eq!(out.files, 3);
        assert_eq!(out.dirs, 2, "根 + sub");
        assert_eq!(out.skipped_count(SkipReason::NotAPhoto), 1);
        assert!(!out.cancelled);
        assert!(out.problems.is_empty());
        assert!(out.elapsed_ms >= 0, "耗时不应为负");

        let kinds: Vec<MediaKind> = files.iter().map(|f| f.kind).collect();
        assert_eq!(
            kinds,
            vec![MediaKind::Image, MediaKind::Raw, MediaKind::Image]
        );
        assert_eq!(files[0].ext.as_deref(), Some("jpg"));
        assert_eq!(files[0].stem_folded, "a");
    }

    #[test]
    fn order_is_deterministic_and_sorted_ignoring_case() {
        let dir = tmp();
        let root = dir.path();
        // 反着建文件，验证输出按名字（折叠）排序，且 BFS 让同层目录排在更深目录之前
        write(root, "zebra.jpg", b"x");
        write(root, "Apple.jpg", b"x");
        write(root, "mango.jpg", b"x");
        write(root, "sub/inner.jpg", b"x");

        let (a, _) = scan_collect(root, &ScanOptions::default(), &Cancel::new()).unwrap();
        let (b, _) = scan_collect(root, &ScanOptions::default(), &Cancel::new()).unwrap();
        assert_eq!(rels(&a), rels(&b), "两次扫描结果必须一致");
        assert_eq!(
            rels(&a),
            vec!["Apple.jpg", "mango.jpg", "zebra.jpg", "sub/inner.jpg"]
        );
    }

    #[test]
    fn include_other_reports_everything_but_still_skips_junk() {
        let dir = tmp();
        let root = dir.path();
        write(root, "a.jpg", b"x");
        write(root, "notes.txt", b"x");
        write(root, "Thumbs.db", b"x");

        let opts = ScanOptions {
            include_other: true,
            ..Default::default()
        };
        let (files, out) = scan_collect(root, &opts, &Cancel::new()).unwrap();
        assert_eq!(rels(&files), vec!["a.jpg", "notes.txt"]);
        assert_eq!(out.skipped_count(SkipReason::Junk), 1);
    }

    #[test]
    fn hidden_items_are_skipped_unless_asked_for() {
        let dir = tmp();
        let root = dir.path();
        write(root, "visible.jpg", b"x");
        write(root, ".hidden.jpg", b"x");
        write(root, ".hiddendir/inside.jpg", b"x");

        let (files, out) = scan_collect(root, &ScanOptions::default(), &Cancel::new()).unwrap();
        assert_eq!(rels(&files), vec!["visible.jpg"]);
        assert!(
            out.skipped_count(SkipReason::Hidden) >= 2,
            "隐藏文件与隐藏目录各算一个"
        );

        let opts = ScanOptions {
            include_hidden: true,
            ..Default::default()
        };
        let (files2, _) = scan_collect(root, &opts, &Cancel::new()).unwrap();
        assert_eq!(
            rels(&files2),
            vec![".hidden.jpg", "visible.jpg", ".hiddendir/inside.jpg"],
            "广度优先：根目录的文件先出，子目录内容随后"
        );
    }

    #[test]
    fn junk_files_are_skipped() {
        let dir = tmp();
        let root = dir.path();
        write(root, "a.jpg", b"x");
        write(root, "Thumbs.db", b"x");
        write(root, "desktop.ini", b"x");
        write(root, ".DS_Store", b"x");
        write(root, "~$doc.docx", b"x");
        write(root, "clip.tmp", b"x");
        write(root, "catalog.db", b"x");
        write(root, "catalog.db-wal", b"x");
        write(root, "IMG.jpg:Zone.Identifier", b"x");

        // 即便「全都要」（含其它类型与隐藏项），垃圾也必须被挡掉
        let opts = ScanOptions {
            include_other: true,
            include_hidden: true,
            ..Default::default()
        };
        let (files, out) = scan_collect(root, &opts, &Cancel::new()).unwrap();
        assert_eq!(rels(&files), vec!["a.jpg"], "只有真照片留下");
        assert_eq!(out.skipped_count(SkipReason::Junk), 8, "八件垃圾各记一笔");
    }

    #[test]
    fn sidecars_are_not_photos() {
        let dir = tmp();
        let root = dir.path();
        write(root, "IMG.RW2", b"x");
        write(root, "IMG.RW2.xmp", b"x");
        write(root, "clip.aae", b"x");

        let (files, out) = scan_collect(root, &ScanOptions::default(), &Cancel::new()).unwrap();
        assert_eq!(rels(&files), vec!["IMG.RW2"]);
        assert_eq!(out.skipped_count(SkipReason::NotAPhoto), 2);
    }

    #[test]
    fn empty_files_are_skipped_by_default() {
        let dir = tmp();
        let root = dir.path();
        write(root, "full.jpg", b"abc");
        write(root, "empty.jpg", b"");

        let (files, out) = scan_collect(root, &ScanOptions::default(), &Cancel::new()).unwrap();
        assert_eq!(rels(&files), vec!["full.jpg"]);
        assert_eq!(out.skipped_count(SkipReason::Empty), 1);

        let opts = ScanOptions {
            skip_empty_files: false,
            ..Default::default()
        };
        let (files2, _) = scan_collect(root, &opts, &Cancel::new()).unwrap();
        assert_eq!(files2.len(), 2);
    }

    #[test]
    fn depth_limit_stops_descending() {
        let dir = tmp();
        let root = dir.path();
        write(root, "top.jpg", b"x");
        write(root, "a/b/two.jpg", b"x");
        write(root, "a/b/c/d/three.jpg", b"x");

        let opts = ScanOptions {
            max_depth: 1,
            ..Default::default()
        };
        let (files, out) = scan_collect(root, &opts, &Cancel::new()).unwrap();
        assert_eq!(rels(&files), vec!["top.jpg"]);
        assert_eq!(
            out.skipped_count(SkipReason::TooDeep),
            1,
            "a/b 这层不该再进去"
        );

        let opts2 = ScanOptions {
            max_depth: 3,
            ..Default::default()
        };
        let (files2, _) = scan_collect(root, &opts2, &Cancel::new()).unwrap();
        assert_eq!(
            rels(&files2),
            vec!["top.jpg", "a/b/two.jpg"],
            "深度 3 到不了第 4 层"
        );
    }

    #[test]
    fn unicode_names_and_paths_survive() {
        let dir = tmp();
        let root = dir.path();
        write(root, "照片/2026年9月/海边 清晨.JPG", b"x");
        write(root, "写真/テスト.RW2", b"x");
        write(root, "cafe\u{301}.jpg", b"x"); // NFD 形式的名字

        let (files, _) = scan_collect(root, &ScanOptions::default(), &Cancel::new()).unwrap();
        assert_eq!(files.len(), 3);
        assert!(
            files
                .iter()
                .any(|f| f.rel_path == "照片/2026年9月/海边 清晨.JPG")
        );
        assert!(files.iter().any(|f| f.rel_path == "写真/テスト.RW2"));
        // 分隔符统一 `/`（即使在 Windows 上）
        assert!(files.iter().all(|f| !f.rel_path.contains('\\')));
    }

    #[test]
    fn missing_root_is_an_error() {
        let dir = tmp();
        let err = scan_collect(
            &dir.path().join("nope"),
            &ScanOptions::default(),
            &Cancel::new(),
        );
        assert!(
            matches!(err, Err(Error::PathNotFound(_))),
            "缺目录要明确报错"
        );
    }

    #[test]
    fn a_file_as_root_is_rejected() {
        let dir = tmp();
        let f = write(dir.path(), "a.jpg", b"x");
        let err = scan_collect(&f, &ScanOptions::default(), &Cancel::new());
        assert!(matches!(err, Err(Error::Unsupported(_))));
    }

    #[test]
    fn empty_directory_yields_nothing_but_reports_itself() {
        let dir = tmp();
        let (files, out) =
            scan_collect(dir.path(), &ScanOptions::default(), &Cancel::new()).unwrap();
        assert!(files.is_empty());
        assert_eq!(out.dirs, 1, "根目录本身也算一个");
        assert_eq!(out.files, 0);
    }

    // ---------- 取消 ----------

    #[test]
    fn cancel_stops_the_walk_and_marks_the_result() {
        let dir = tmp();
        let root = dir.path();
        for i in 0..20 {
            write(root, &format!("p{i:02}.jpg"), b"x");
        }
        let cancel = Cancel::new();
        let mut seen = 0;
        let out = scan(root, &ScanOptions::default(), &cancel, |ev| {
            if let ScanEvent::File(_) = ev {
                seen += 1;
                if seen == 3 {
                    cancel.cancel();
                }
            }
            Ok(())
        })
        .unwrap();
        assert!(out.cancelled, "取消后必须标记出来（结果是不完整的）");
        assert_eq!(seen, 3, "取消后不再回调新文件");
        assert!(out.files < 20);
    }

    #[test]
    fn already_cancelled_does_nothing() {
        let dir = tmp();
        write(dir.path(), "a.jpg", b"x");
        let cancel = Cancel::new();
        cancel.cancel();
        let (files, out) = scan_collect(dir.path(), &ScanOptions::default(), &cancel).unwrap();
        assert!(files.is_empty());
        assert!(out.cancelled);
        assert_eq!(out.dirs, 0, "一开始就取消 → 连根目录都不报");
    }

    #[test]
    fn callback_error_aborts_the_scan() {
        let dir = tmp();
        let root = dir.path();
        write(root, "a.jpg", b"x");
        write(root, "b.jpg", b"x");
        let err = scan(root, &ScanOptions::default(), &Cancel::new(), |ev| {
            if let ScanEvent::File(_) = ev {
                return Err(Error::Unsupported("落库失败".into()));
            }
            Ok(())
        });
        assert!(err.is_err(), "回调出错必须把错误抛出来（而不是静默跳过）");
    }

    // ---------- 流式回调 ----------

    #[test]
    fn dir_events_come_before_their_files() {
        let dir = tmp();
        let root = dir.path();
        write(root, "sub/a.jpg", b"x");
        let mut events: Vec<String> = Vec::new();
        scan(root, &ScanOptions::default(), &Cancel::new(), |ev| {
            match ev {
                ScanEvent::Dir(d) => events.push(format!("D:{}", d.rel_path)),
                ScanEvent::File(f) => events.push(format!("F:{}", f.rel_path)),
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(events, vec!["D:", "D:sub", "F:sub/a.jpg"]);
    }

    // ---------- 符号链接（Unix）----------

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed_by_default() {
        let dir = tmp();
        let root = dir.path();
        let target = root.join("real");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("inside.jpg"), b"x").unwrap();
        std::os::unix::fs::symlink(&target, root.join("link_dir")).unwrap();
        std::os::unix::fs::symlink(target.join("inside.jpg"), root.join("link_file.jpg")).unwrap();

        let (files, out) = scan_collect(root, &ScanOptions::default(), &Cancel::new()).unwrap();
        assert_eq!(rels(&files), vec!["real/inside.jpg"], "软链一个都不该跟进");
        assert_eq!(out.skipped_count(SkipReason::Symlink), 2);

        let opts = ScanOptions {
            follow_symlinks: true,
            ..Default::default()
        };
        let (files2, _) = scan_collect(root, &opts, &Cancel::new()).unwrap();
        assert_eq!(files2.len(), 3, "显式要求时才跟随");
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_subdirectory_becomes_a_problem_not_a_failure() {
        use std::os::unix::fs::PermissionsExt;
        // root 用户不受权限限制，这种环境下测不了
        if running_as_root() {
            return;
        }
        let dir = tmp();
        let root = dir.path();
        write(root, "ok.jpg", b"x");
        let locked = root.join("locked");
        fs::create_dir_all(&locked).unwrap();
        fs::write(locked.join("hidden.jpg"), b"x").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        let (files, out) = scan_collect(root, &ScanOptions::default(), &Cancel::new()).unwrap();
        // 还原权限，避免临时目录删不掉
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(rels(&files), vec!["ok.jpg"], "别的文件照常扫到");
        assert_eq!(out.problems.len(), 1, "读不了的目录记一笔问题");
        assert_eq!(out.problems[0].rel_path, "locked");
    }

    /// 当前是不是 root？（root 不受权限限制，那种环境下跳过本条测试）
    ///
    /// 不引 libc：直接读 `/proc/self/status` 的 `Uid:` 行。
    #[cfg(unix)]
    fn running_as_root() -> bool {
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find_map(|l| l.strip_prefix("Uid:").map(|v| v.trim().to_string()))
            })
            .and_then(|v| v.split_whitespace().next().map(|u| u == "0"))
            .unwrap_or(false)
    }
}
