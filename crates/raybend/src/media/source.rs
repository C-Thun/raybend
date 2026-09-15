//! 「来源」目录的列取：给导入工作区左列（目录树）与中列（照片网格）用。
//!
//! `design/main.md` §3.1.2 / §3.2 的两条硬规则在这里落地：
//!
//! * **树里只列目录**，且**不下钻取图**（展开哪一级才读哪一级）；
//! * **中列只列该目录直属的图片**（不下钻子目录 —— 那是导入时「包含子目录」开关的事）。
//!
//! 为什么单独一层而不是直接在 Tauri 命令里写：这些是**业务规则**（跳过什么、按什么排序、
//! 拍摄时间怎么兜底），要有单测；`src-tauri` 只该做「参数转换 + 错误转字符串」。
//!
//! 拍摄时间分两级（`plans/M1-5.md` §3.1）：
//!
//! | 阶段 | 数据来源 | 代价 |
//! | --- | --- | --- |
//! | 列表（[`scan_photos`]） | 文件名 → 文件修改时间 | 一次 `read_dir` |
//! | 精确（[`read_times`]） | 真读 EXIF（并行） | 每张几毫秒 |
//!
//! 先出网格、用户按下「按时间」再补真相 —— 否则进一个大目录要干等好几秒。

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

use super::exif::{self, TakenAt};
use super::kind::{self, MediaKind};
use super::scan::{self, Cancel, ScanEvent, ScanOptions};

/// 目录树里的一个子目录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// 目录名（最后一段）。
    pub name: String,
    /// 完整路径。
    pub path: PathBuf,
}

/// 中列里的一张照片（列表阶段的信息）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceItem {
    /// 完整路径。
    pub path: PathBuf,
    /// 文件名（含扩展名，原始大小写）。
    pub file_name: String,
    /// 小写扩展名（不含点）。
    pub ext: Option<String>,
    /// RAW / 图像。
    pub kind: MediaKind,
    pub size_bytes: u64,
    /// 文件修改时间（Unix 毫秒）。
    pub mtime_ms: Option<i64>,
    /// **快速兜底**的拍摄时间（文件名 → mtime）；真相要 [`read_times`]。
    pub taken_at: Option<TakenAt>,
}

/// 一次列取的结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceListing {
    /// 照片（已按扫描顺序：目录先、同级按名字）。
    pub items: Vec<SourceItem>,
    /// 跳过的文件/目录数（隐藏、垃圾、侧车、非照片…）。
    pub skipped: usize,
    /// 读不了的位置（权限等）—— **不致命**，照 `scan` 的口径继续。
    pub problems: Vec<String>,
    /// 扫描耗时（毫秒）。
    pub elapsed_ms: i64,
}

impl SourceListing {
    /// 列到了几张。
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// 一张都没有。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// 照片计数（喂「已选择 N 张照片」）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PhotoCount {
    pub photos: usize,
    pub skipped: usize,
    /// 扫描是否在读完之前被中止（当次结果是**下界**）。
    pub truncated: bool,
}

/// 一个目录的**直接子目录**（不递归、不下钻）。
///
/// 排序：按名字**不区分大小写**升序（Windows 资源管理器的手感），同名前缀时用原名兜底，
/// 保证顺序确定（可断言、不会因为平台的 read_dir 顺序而抖动）。
///
/// 为什么不用 [`scan`]：`scan` 只为**它真的往下走的**目录发 `Dir` 事件，
/// 而「只列一层」恰恰要求它不要往下走（`max_depth = 0`）—— 那就一个事件也收不到。
/// 所以这里直接 `read_dir`，跳过规则与 `scan` 保持一致（隐藏项 / 垃圾 / 符号链接）。
pub fn list_dirs(root: &Path) -> Result<Vec<DirEntry>> {
    ensure_dir(root)?;
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();

        // 隐藏项：与 `scan` 的默认口径一致（`.` 开头一律不列）
        if name.starts_with('.') {
            continue;
        }
        // 系统目录 / 垃圾名：`$RECYCLE.BIN`、`System Volume Information` …
        if kind::junk_kind(&name).is_some() {
            continue;
        }
        // 符号链接不跟随：目录环与「链接到别处」的目录最容易把树撑爆。
        //
        // ⚠️ 用 `file_type()`（`read_dir` 自带的条目类型）而不是 `path().is_dir()`：
        // 后者会**多一次 stat 系统调用**。在一个 300 条的目录上，实测 9p 下
        // 0.9s → 0.45s（慢盘上是成倍的差别）。
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() || !file_type.is_dir() {
            continue;
        }

        out.push(DirEntry {
            name,
            path: entry.path(),
        });
    }

    out.sort_by(|a, b| {
        let folded = a.name.to_lowercase().cmp(&b.name.to_lowercase());
        folded.then_with(|| a.name.cmp(&b.name))
    });
    Ok(out)
}

/// 列出目录里的照片（`recursive = false` 时只列直属的）。
///
/// 这一步**不读 EXIF**：拍摄时间先用「文件名 → mtime」兜底（见模块文档）。
pub fn scan_photos(root: &Path, recursive: bool) -> Result<SourceListing> {
    let opts = ScanOptions {
        max_depth: if recursive { 32 } else { 0 },
        ..ScanOptions::default()
    };
    let mut listing = SourceListing::default();
    let outcome = scan::scan(root, &opts, &Cancel::new(), |event| {
        if let ScanEvent::File(file) = event {
            listing.items.push(SourceItem {
                taken_at: exif::resolve_taken_at(None, &file.file_name, file.mtime_ms),
                path: file.abs_path,
                file_name: file.file_name,
                ext: file.ext,
                kind: file.kind,
                size_bytes: file.size_bytes,
                mtime_ms: file.mtime_ms,
            });
        }
        Ok(())
    })?;

    listing.skipped = outcome.skipped - outcome.skipped_count(scan::SkipReason::TooDeep);
    listing.problems = outcome
        .problems
        .into_iter()
        .map(|p| format!("{}：{}", p.rel_path, p.message))
        .collect();
    listing.elapsed_ms = outcome.elapsed_ms;
    Ok(listing)
}

/// 只数张数（不保留清单）——「已选择 N 张照片」用它。
///
/// 与 [`scan_photos`] 的区别只是**不攒清单**：大目录下省内存。
pub fn count_photos(root: &Path, recursive: bool) -> Result<PhotoCount> {
    let opts = ScanOptions {
        max_depth: if recursive { 32 } else { 0 },
        ..ScanOptions::default()
    };
    let mut count = PhotoCount::default();
    let outcome = scan::scan(root, &opts, &Cancel::new(), |event| {
        if matches!(event, ScanEvent::File(_)) {
            count.photos += 1;
        }
        Ok(())
    })?;
    // 不递归时子目录会被记成 TooDeep —— 那不是「跳过的东西」，别混进计数里
    count.skipped = outcome.skipped - outcome.skipped_count(scan::SkipReason::TooDeep);
    count.truncated = outcome.cancelled;
    Ok(count)
}

/// 一个文件的精确拍摄时间。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeEntry {
    pub path: PathBuf,
    pub taken_at: Option<TakenAt>,
}

/// 并行读一批文件的**真实 EXIF 拍摄时间**（用户按下「按时间」时才调）。
///
/// * 结果顺序**与输入一致**（并行不影响确定性，测试与界面都靠它）；
/// * 单张失败不影响其它（读不到就是 `None`，没有错误分支）；
/// * 线程数取 `min(4, 可用核数)` —— 这是可中断的一批活，不是要压满 CPU 的基准。
#[must_use]
pub fn read_times(paths: &[PathBuf]) -> Vec<TimeEntry> {
    let workers = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .clamp(1, 4);

    if paths.is_empty() {
        return Vec::new();
    }
    if workers == 1 || paths.len() < 2 {
        return paths.iter().map(|p| read_time_one(p)).collect();
    }

    let chunk = paths.len().div_ceil(workers);
    let mut out = Vec::with_capacity(paths.len());
    std::thread::scope(|scope| {
        let handles: Vec<_> = paths
            .chunks(chunk)
            .map(|slice| {
                scope.spawn(move || {
                    slice
                        .iter()
                        .map(|p| read_time_one(p.as_path()))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for handle in handles {
            // 线程 panic 不该把整批拖垮：跳过它，后面按长度补齐
            if let Ok(entries) = handle.join() {
                out.extend(entries);
            }
        }
    });

    // 线程 panic 时长度会对不上 —— 补齐比让调用方拿到错位的数组安全
    while out.len() < paths.len() {
        out.push(TimeEntry {
            path: PathBuf::new(),
            taken_at: None,
        });
    }
    out
}

fn read_time_one(path: &Path) -> TimeEntry {
    let data = exif::read_file(path);
    let file_name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let mtime_ms = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .map(crate::store::time::from_system_time);
    TimeEntry {
        path: path.to_path_buf(),
        taken_at: exif::resolve_taken_at(Some(&data), &file_name, mtime_ms),
    }
}

/// 把 `scan` 给的相对路径（`/` 分隔）接成原生路径。
///
/// 现在只有测试用得到它（`list_dirs` 改成了直接 `read_dir`）；保留是因为
/// 「`/` 分隔的相对路径 → 原生路径」这个转换以后做递归列取时还会用到，
/// 而它有一个真的坑（`PathBuf::join` 会把 `a/b` 当成单段，得到混合分隔符）。
#[cfg(test)]
fn join_relative(root: &Path, rel: &str) -> PathBuf {
    let mut out = root.to_path_buf();
    for segment in rel.split('/').filter(|s| !s.is_empty()) {
        out.push(segment);
    }
    out
}

/// 便捷：路径必须是存在的目录（命令层用它给出可读错误）。
pub fn ensure_dir(path: &Path) -> Result<()> {
    let meta = std::fs::metadata(path).map_err(|_| Error::PathNotFound(path.to_path_buf()))?;
    if meta.is_dir() {
        Ok(())
    } else {
        Err(Error::Unsupported(format!(
            "不是目录：{}",
            path.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn write(path: &Path, bytes: usize) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, vec![7u8; bytes]).unwrap();
    }

    fn names(dirs: &[DirEntry]) -> Vec<&str> {
        dirs.iter().map(|d| d.name.as_str()).collect()
    }

    // ---------- list_dirs ----------

    #[test]
    fn list_dirs_returns_only_directories_sorted_case_insensitively() {
        let dir = tmp();
        let root = dir.path();
        fs::create_dir_all(root.join("Zebra")).unwrap();
        fs::create_dir_all(root.join("apple")).unwrap();
        fs::create_dir_all(root.join("Banana")).unwrap();
        fs::create_dir_all(root.join("apple/inner")).unwrap();
        write(&root.join("photo.jpg"), 10);
        write(&root.join("notes.txt"), 10);

        let dirs = list_dirs(root).unwrap();
        assert_eq!(names(&dirs), vec!["apple", "Banana", "Zebra"], "按名排序、只目录");
        // 不下钻：inner 不出现（它是 apple 的子目录）
        assert!(!names(&dirs).contains(&"inner"));
        assert_eq!(dirs[0].path, root.join("apple"));
    }

    #[test]
    fn list_dirs_skips_hidden_and_junk_directories() {
        let dir = tmp();
        let root = dir.path();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("$RECYCLE.BIN")).unwrap();
        fs::create_dir_all(root.join("normal")).unwrap();

        let dirs = list_dirs(root).unwrap();
        assert_eq!(names(&dirs), vec!["normal"]);
    }

    #[test]
    fn list_dirs_on_missing_path_is_an_error() {
        let dir = tmp();
        let missing = dir.path().join("nope");
        assert!(matches!(list_dirs(&missing), Err(Error::PathNotFound(_))));
        assert!(matches!(ensure_dir(&missing), Err(Error::PathNotFound(_))));
    }

    #[test]
    fn list_dirs_on_a_file_is_rejected() {
        let dir = tmp();
        let file = dir.path().join("a.jpg");
        write(&file, 4);
        assert!(matches!(list_dirs(&file), Err(Error::Unsupported(_))));
    }

    #[test]
    fn list_dirs_on_empty_directory_is_empty() {
        let dir = tmp();
        assert!(list_dirs(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn list_dirs_handles_unicode_names() {
        let dir = tmp();
        let root = dir.path();
        fs::create_dir_all(root.join("旅行")).unwrap();
        fs::create_dir_all(root.join("2026-09 归档")).unwrap();
        let dirs = list_dirs(root).unwrap();
        assert_eq!(dirs.len(), 2);
        assert!(names(&dirs).contains(&"旅行"));
        assert_eq!(dirs[0].path, root.join("2026-09 归档"));
    }

    // ---------- scan_photos ----------

    #[test]
    fn scan_photos_lists_only_direct_images_by_default() {
        let dir = tmp();
        let root = dir.path();
        write(&root.join("a.jpg"), 10);
        write(&root.join("b.RW2"), 20);
        write(&root.join("sub/c.jpg"), 30);
        write(&root.join("notes.txt"), 5);
        write(&root.join("a.xmp"), 5);

        let listing = scan_photos(root, false).unwrap();
        let files: Vec<&str> = listing
            .items
            .iter()
            .map(|i| i.file_name.as_str())
            .collect();
        assert_eq!(files, vec!["a.jpg", "b.RW2"], "只列直属照片，侧车与非照片不算");
        assert_eq!(listing.items[0].ext.as_deref(), Some("jpg"));
        assert_eq!(listing.items[1].kind, MediaKind::Raw);
        assert_eq!(listing.items[0].size_bytes, 10);
        assert_eq!(listing.len(), 2);
        assert!(!listing.is_empty());
    }

    #[test]
    fn scan_photos_recursive_descends_into_subdirectories() {
        let dir = tmp();
        let root = dir.path();
        write(&root.join("a.jpg"), 10);
        write(&root.join("sub/deep/c.jpg"), 30);
        let listing = scan_photos(root, true).unwrap();
        let files: Vec<&str> = listing
            .items
            .iter()
            .map(|i| i.file_name.as_str())
            .collect();
        assert_eq!(files, vec!["a.jpg", "c.jpg"]);
    }

    #[test]
    fn scan_photos_falls_back_to_filename_then_mtime_for_taken_at() {
        let dir = tmp();
        let root = dir.path();
        // 文件名里有日期 → 用文件名
        write(&root.join("IMG_20260815_123456.jpg"), 10);
        // 什么线索都没有 → 用 mtime
        write(&root.join("plain.jpg"), 10);

        let listing = scan_photos(root, false).unwrap();
        let by_name: Vec<(&str, Option<&'static str>)> = listing
            .items
            .iter()
            .map(|i| {
                (
                    i.file_name.as_str(),
                    i.taken_at.map(|t| match t.source {
                        exif::TakenAtSource::Exif => "exif",
                        exif::TakenAtSource::Filename => "filename",
                        exif::TakenAtSource::FileMtime => "mtime",
                    }),
                )
            })
            .collect();
        assert!(by_name.contains(&("IMG_20260815_123456.jpg", Some("filename"))));
        assert!(
            by_name.contains(&("plain.jpg", Some("mtime"))),
            "列表阶段不能去读 EXIF（太慢），必须落到 mtime：{by_name:?}"
        );
    }

    #[test]
    fn scan_photos_reports_problems_without_failing() {
        let dir = tmp();
        let root = dir.path();
        write(&root.join("a.jpg"), 10);
        let listing = scan_photos(root, false).unwrap();
        assert!(listing.problems.is_empty());
        // 目录不存在才是硬错误
        assert!(scan_photos(&root.join("missing"), false).is_err());
    }

    #[test]
    fn scan_photos_on_empty_directory_is_empty() {
        let dir = tmp();
        let listing = scan_photos(dir.path(), false).unwrap();
        assert!(listing.is_empty());
        assert_eq!(listing.len(), 0);
    }

    // ---------- count_photos ----------

    #[test]
    fn count_photos_respects_recursion_flag() {
        let dir = tmp();
        let root = dir.path();
        write(&root.join("a.jpg"), 10);
        write(&root.join("b.png"), 10);
        write(&root.join("sub/c.jpg"), 10);
        write(&root.join("sub/d.txt"), 10);

        assert_eq!(count_photos(root, false).unwrap().photos, 2);
        assert_eq!(count_photos(root, true).unwrap().photos, 3);
    }

    #[test]
    fn count_photos_on_empty_directory_is_zero() {
        let dir = tmp();
        let count = count_photos(dir.path(), true).unwrap();
        assert_eq!(count.photos, 0);
        assert!(!count.truncated);
    }

    // ---------- read_times ----------

    #[test]
    fn read_times_preserves_input_order_and_length() {
        let dir = tmp();
        let root = dir.path();
        let paths: Vec<PathBuf> = (0..9)
            .map(|i| {
                let p = root.join(format!("IMG_2026081{i}_000000.jpg"));
                write(&p, 10);
                p
            })
            .collect();

        let times = read_times(&paths);
        assert_eq!(times.len(), paths.len());
        for (entry, path) in times.iter().zip(paths.iter()) {
            assert_eq!(&entry.path, path, "顺序必须与输入一致");
            let taken = entry.taken_at.expect("文件名里有日期，应该能解析出来");
            assert_eq!(taken.source, exif::TakenAtSource::Filename);
        }
    }

    #[test]
    fn read_times_on_empty_input_is_empty() {
        assert!(read_times(&[]).is_empty());
    }

    #[test]
    fn read_times_survives_missing_files() {
        let dir = tmp();
        let missing = dir.path().join("gone.jpg");
        let times = read_times(std::slice::from_ref(&missing));
        assert_eq!(times.len(), 1);
        assert_eq!(times[0].path, missing);
        assert!(times[0].taken_at.is_none(), "文件都不在了，不该编出一个时间");
    }

    #[test]
    fn read_times_on_a_single_path_stays_single() {
        let dir = tmp();
        let p = dir.path().join("one.jpg");
        write(&p, 4);
        let times = read_times(std::slice::from_ref(&p));
        assert_eq!(times.len(), 1);
        // 没有 EXIF、文件名里也没日期 → mtime 兜底
        assert_eq!(
            times[0].taken_at.map(|t| t.source),
            Some(exif::TakenAtSource::FileMtime)
        );
    }

    // ---------- join_relative ----------

    #[test]
    fn join_relative_uses_native_separators() {
        let joined = join_relative(Path::new("root"), "a/b/c");
        assert_eq!(joined, Path::new("root").join("a").join("b").join("c"));
        assert_eq!(join_relative(Path::new("root"), ""), PathBuf::from("root"));
        assert_eq!(join_relative(Path::new("root"), "/"), PathBuf::from("root"));
        assert_eq!(
            join_relative(Path::new("root"), "//a//b/"),
            Path::new("root").join("a").join("b")
        );
    }
}
