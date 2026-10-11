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
//! 拍摄时间分两级（`specs/M1-5.md` §3.1）：
//!
//! | 阶段 | 数据来源 | 代价 |
//! | --- | --- | --- |
//! | 列表（[`scan_photos`]） | 文件名 → 文件修改时间 | 一次 `read_dir` |
//! | 精确（`media::meta::read_photo_meta` → `dir_meta_ensure`） | 真读 EXIF（顺路带回宽高/方向） | 每张几毫秒 |
//!
//! 先出网格（兜底时间就够了）、再由「读文件信息」**分块**把真相补回来 ——
//! 2026-10-11 崔总口径：时间并进那次读头，**不再有第二条专读时间的命令**
//!（`specs/source-time-inline.md`；以前那条 `read_times` 一次读整目录，真机 6 秒把界面按住）。

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

use super::exif::{self, TakenAt};
use super::kind::{self, MediaKind};
use super::scan::{self, Cancel, ScanEvent, ScanOptions};

/// 源文件缓存签名：同路径替换或同 inode 改写都不能命中旧派生图。
/// 不读像素；大小、精确修改时间和 FileId 在 Rust 里统一计算。
pub fn source_signature(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let metadata = std::fs::metadata(path)?;
    let mut hash = Sha256::new();
    hash.update(metadata.len().to_le_bytes());
    let (before_epoch, modified) = match metadata.modified()?.duration_since(std::time::UNIX_EPOCH)
    {
        Ok(value) => (false, value),
        Err(error) => (true, error.duration()),
    };
    hash.update([u8::from(before_epoch)]);
    hash.update(modified.as_nanos().to_le_bytes());
    if let Some(id) = crate::store::file_id::FileId::try_read(path) {
        hash.update(id.volume_serial.to_le_bytes());
        hash.update(id.file_id);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// 目录树里的一个子目录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// 目录名（最后一段）。
    pub name: String,
    /// 完整路径。
    pub path: PathBuf,
    /// 它里面**还有没有会显示出来的子目录** —— 决定树上画不画展开箭头。
    ///
    /// 为什么要多扫一层（人类 2026-09-19 定）：只列一层的话前端无从知道某个子目录是不是
    /// 空到底，只能保守地先画上箭头，点开才发现什么都没有 —— 那是误导。
    /// 代价是每个子目录多一次 `read_dir`，但**找到第一个合格项就短路**（`find`），
    /// 只有真空的目录才需要读到底；本地盘这点代价远小于让人点空箭头。
    pub has_children: bool,
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
    /// **快速兜底**的拍摄时间（文件名 → **创建时间** → mtime）；真相由「读文件信息」顺路补回
    /// （`dir_meta_ensure`，见 `specs/source-time-inline.md`）。
    ///
    /// 创建时间的位置是崔总 2026-10-11 定的：**从 SD 卡直接读时它就是拍照时间**，
    /// 搭「按时间」的初始框架靠它 —— 不读 EXIF，随目录列表一起来的。
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

/// 一个目录项是不是「树上会显示的目录」：跳过隐藏 / 垃圾名 / 符号链接 / 非目录。
///
/// `list_dirs` 与 `has_subdirectories` **共用这一处判定** —— 两边的口径必须一致，
/// 否则会出现「箭头说有、点开是空」的错（或者反过来：有子目录却不画箭头）。
fn is_listable_dir(entry: &std::fs::DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy().into_owned();

    // 隐藏项：`.` 开头 / Windows 隐藏属性（人类 2026-09-16：目录树里不显示隐藏目录）
    if kind::is_hidden_name(&name) {
        return false;
    }
    #[cfg(windows)]
    {
        if entry
            .metadata()
            .map(|meta| kind::has_hidden_attribute(&meta))
            .unwrap_or(false)
        {
            return false;
        }
    }
    // 系统目录 / 垃圾名：`$RECYCLE.BIN`、`System Volume Information` …
    if kind::junk_kind(&name).is_some() {
        return false;
    }
    /*
     * `_RAW/` 是**保留目录**：树的每一层都会把它过滤掉（`memory/FUNCTION-REPOSITORY.md` §4.1），
     * 所以它不能算进「这个目录有没有子目录」—— 否则会出现
     * 「明明没有可见子目录、却画着展开箭头，点开是空的」（人类 2026-09-19 报的）。
     * 注意只影响**列目录**这条路径：导入扫描走的是 `scan`，`_RAW` 照常被认出来。
     */
    if name.eq_ignore_ascii_case("_raw") {
        return false;
    }
    // 符号链接不跟随：目录环与「链接到别处」的目录最容易把树撑爆。
    //
    // ⚠️ 用 `file_type()`（`read_dir` 自带的条目类型）而不是 `path().is_dir()`：
    // 后者会**多一次 stat 系统调用**。在一个 300 条的目录上，实测 9p 下
    // 0.9s → 0.45s（慢盘上是成倍的差别）。
    let Ok(file_type) = entry.file_type() else {
        return false;
    };
    !file_type.is_symlink() && file_type.is_dir()
}

/// 这个目录里有没有**会显示出来的**子目录（判定口径与 `list_dirs` 完全一致）。
///
/// 读不了（权限等）一律当「没有」—— 与 `list_dirs` 在该层跳过它的行为一致，
/// 不会出现「画了箭头点开报错」。
fn has_subdirectories(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    // `any` 短路：找到第一个合格项就停，只有真空目录才读到底
    entries.flatten().any(|entry| is_listable_dir(&entry))
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
        // 跳过规则全部在 `is_listable_dir` 里 —— 与「多扫一层」的探测**共用同一口径**
        if !is_listable_dir(&entry) {
            continue;
        }

        out.push(DirEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            has_children: has_subdirectories(&entry.path()),
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
/// 这一步**不读 EXIF**：拍摄时间先用「文件名 → 创建时间 → mtime」兜底
/// （见模块文档；创建时间是 `read_dir` 顺手就有的，不额外读文件）。
pub fn scan_photos(root: &Path, recursive: bool) -> Result<SourceListing> {
    let opts = ScanOptions {
        max_depth: if recursive { 32 } else { 0 },
        ..ScanOptions::default()
    };
    let mut listing = SourceListing::default();
    let outcome = scan::scan(root, &opts, &Cancel::new(), |event| {
        if let ScanEvent::File(file) = event {
            listing.items.push(SourceItem {
                taken_at: fallback_taken_at(&file),
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

/// 列表阶段的**兜底**拍摄时间：文件名 → **创建时间** → mtime。
///
/// 为什么把创建时间放在 mtime 前面（崔总 2026-10-11）：「导入大多数是直接从相机卡读，
/// 那时文件创建时间就是拍照时间」——文件系统给，随目录列表一起到手，不用读文件。
/// ⚠️ 拷贝过的文件在 NTFS 上 `created` 是**复制时间**（比 mtime 远）；所以这条
/// **只用于来源目录的界面**，进库（`import/sink.rs`）仍按「EXIF → 文件名 → mtime」。
fn fallback_taken_at(file: &scan::ScannedFile) -> Option<TakenAt> {
    /*
     * 文件系统那两个时间**取小的那个**（崔总 2026-10-11 定）：
     * 从卡上直接读时两者一致；复制过来的文件 `created` 会变成复制那天（2026），
     * 而 mtime 还是拍摄日（2018）—— 取小值就与「按时间」的正确顺序一致，
     * 界面上显示的日期也不会比排序看起来晚一大截。
     */
    let filesystem = match (file.created_ms, file.mtime_ms) {
        (Some(created), Some(mtime)) if mtime < created => Some(TakenAt {
            millis: mtime,
            offset_min: None,
            source: exif::TakenAtSource::FileMtime,
        }),
        (Some(created), _) => Some(TakenAt {
            millis: created,
            offset_min: None,
            source: exif::TakenAtSource::FileCreated,
        }),
        (None, mtime) => mtime.map(|millis| TakenAt {
            millis,
            offset_min: None,
            source: exif::TakenAtSource::FileMtime,
        }),
    };
    exif::resolve_taken_at(None, &file.file_name, None).or(filesystem)
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
        Err(Error::Unsupported(format!("不是目录：{}", path.display())))
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
        assert_eq!(
            names(&dirs),
            vec!["apple", "Banana", "Zebra"],
            "按名排序、只目录"
        );
        // 不下钻：inner 不出现（它是 apple 的子目录）
        assert!(!names(&dirs).contains(&"inner"));
        assert_eq!(dirs[0].path, root.join("apple"));
    }

    #[test]
    fn list_dirs_reports_whether_each_child_has_subdirectories() {
        let dir = tmp();
        let root = dir.path();
        // deep/ 下面有子目录 → true
        fs::create_dir_all(root.join("deep/inner")).unwrap();
        // leaf/ 下面只有文件 → false
        fs::create_dir_all(root.join("leaf")).unwrap();
        write(&root.join("leaf/photo.jpg"), 10);
        // empty/ 空目录 → false
        fs::create_dir_all(root.join("empty")).unwrap();
        // trap/ 只有隐藏子目录 → false（口径与 `list_dirs` 一致：隐藏项不算）
        fs::create_dir_all(root.join("trap/.hidden")).unwrap();

        let dirs = list_dirs(root).unwrap();
        let flag = |name: &str| -> bool {
            dirs.iter()
                .find(|entry| entry.name == name)
                .unwrap()
                .has_children
        };
        assert!(flag("deep"), "有子目录 → true");
        assert!(!flag("leaf"), "只有文件 → false");
        assert!(!flag("empty"), "空目录 → false");
        assert!(!flag("trap"), "只有隐藏子目录 → false");
    }

    #[test]
    fn list_dirs_has_children_matches_what_expanding_will_show() {
        /*
         * 不变式：`has_children` 必须等于「展开后真的列得出东西」。
         * 两处判定同源（都用 `is_listable_dir`）就不会出现
         * 「画了箭头、点开是空」或「有子目录却不给箭头」这两种误导。
         */
        let dir = tmp();
        let root = dir.path();
        for name in ["a/b", "a/c/d", "e", "f/g"] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        write(&root.join("a/b/only-file.jpg"), 10);

        let dirs = list_dirs(root).unwrap();
        for entry in &dirs {
            let children = list_dirs(&entry.path).unwrap();
            assert_eq!(
                entry.has_children,
                !children.is_empty(),
                "{} 的箭头与展开结果不一致",
                entry.name
            );
        }
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
        let files: Vec<&str> = listing.items.iter().map(|i| i.file_name.as_str()).collect();
        assert_eq!(
            files,
            vec!["a.jpg", "b.RW2"],
            "只列直属照片，侧车与非照片不算"
        );
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
        let files: Vec<&str> = listing.items.iter().map(|i| i.file_name.as_str()).collect();
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
                        exif::TakenAtSource::FileCreated => "created",
                        exif::TakenAtSource::FileMtime => "mtime",
                        exif::TakenAtSource::Sibling => "sibling",
                    }),
                )
            })
            .collect();
        assert!(by_name.contains(&("IMG_20260815_123456.jpg", Some("filename"))));
        assert!(
            by_name.contains(&("plain.jpg", Some("created"))),
            "列表阶段不能去读 EXIF（太慢）——落到**文件创建时间**（崔总 2026-10-11 定）：{by_name:?}"
        );
    }

    #[test]
    fn 兜底次序是_文件名_创建时间_mtime() {
        let file = |name: &str, created: Option<i64>, mtime: Option<i64>| scan::ScannedFile {
            abs_path: PathBuf::from(name),
            rel_path: String::from(name),
            ext: kind::extension(name),
            stem_folded: kind::stem_folded(name),
            file_name: name.to_string(),
            kind: MediaKind::Image,
            size_bytes: 1,
            mtime_ms: mtime,
            created_ms: created,
        };

        // 文件名里有日期 → 用它
        let named = fallback_taken_at(&file("IMG_20260815_123456.jpg", Some(500), Some(900)));
        assert_eq!(named.map(|t| t.source), Some(exif::TakenAtSource::Filename));
        // 没有日期 → 创建时间（从卡上直接读时就是拍照时间）
        let created = fallback_taken_at(&file("plain.jpg", Some(500), Some(900)));
        assert_eq!(
            created.map(|t| (t.millis, t.source)),
            Some((500, exif::TakenAtSource::FileCreated))
        );
        // 两个文件系统时间都在 → **取小的那个**（复制过来的文件 created 是复制日，mtime 才是拍摄日）
        let older_mtime = fallback_taken_at(&file("plain.jpg", Some(900), Some(500)));
        assert_eq!(
            older_mtime.map(|t| (t.millis, t.source)),
            Some((500, exif::TakenAtSource::FileMtime)),
            "mtime 更小 → 用它（与「按时间」的正确顺序一致）"
        );
        // 文件系统不给创建时间 → mtime 兜底
        let only_mtime = fallback_taken_at(&file("plain.jpg", None, Some(900)));
        assert_eq!(
            only_mtime.map(|t| (t.millis, t.source)),
            Some((900, exif::TakenAtSource::FileMtime))
        );
        // 什么都没有 → 没有
        assert_eq!(fallback_taken_at(&file("plain.jpg", None, None)), None);
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

/// 一批路径**现在**还是不是目录 —— 用来把「盘没插」的最近目录标灰
/// （人类 2026-09-16：启动时要认出来，每次重新选中时也要再查一遍）。
///
/// * **一次问一批**：启动时几十条最近目录，一条一个 IPC 纯属浪费；
/// * **判据很轻**：`metadata` + `is_dir`，**不读目录内容** —— 网络盘掉线时
///   `is_dir` 会立刻失败（或很快超时），不会像 `read_dir` 那样卡住；
/// * 路径不存在、权限不足、盘没挂：一律算「不可用」（这正是界面要的语义，
///   具体原因不重要 —— 用户能做的都是「把盘插上再试」）。
pub fn paths_are_dirs(paths: &[String]) -> Vec<bool> {
    paths.iter().map(|path| Path::new(path).is_dir()).collect()
}

#[cfg(test)]
mod availability_tests {
    use super::paths_are_dirs;

    #[test]
    fn 存在的目录为真_不存在或不是目录为假() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("testrepos");
        std::fs::create_dir_all(&root).unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, b"x").unwrap();

        let paths = vec![
            root.to_string_lossy().to_string(),
            file.to_string_lossy().to_string(), // 是文件，不是目录
            dir.path().join("没有这个").to_string_lossy().to_string(),
            String::new(), // 空串也不能 panic
        ];
        assert_eq!(paths_are_dirs(&paths), vec![true, false, false, false]);
    }

    #[test]
    fn 改名等于拔盘_改回去等于插回() {
        // 人类给的测法：本地磁盘上直接改名，就等价于设备插拔
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("testrepos");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.to_string_lossy().to_string();
        assert_eq!(paths_are_dirs(std::slice::from_ref(&path)), vec![true]);

        let unplugged = dir.path().join("testrepos-unplugged");
        std::fs::rename(&root, &unplugged).unwrap();
        assert_eq!(
            paths_are_dirs(std::slice::from_ref(&path)),
            vec![false],
            "改名 → 不可用"
        );

        std::fs::rename(&unplugged, &root).unwrap();
        assert_eq!(paths_are_dirs(&[path]), vec![true], "改回来 → 又可用");
    }
}

#[cfg(test)]
mod signature_tests {
    use super::*;
    #[test]
    fn source_signature_tracks_exact_time_replacement_and_unicode_paths() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("中文.jpg");
        std::fs::write(&path, b"same size").unwrap();
        let first = source_signature(&path).unwrap();
        assert_eq!(first, source_signature(&path).unwrap());
        let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_times(
            std::fs::FileTimes::new()
                .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(123)),
        )
        .unwrap();
        assert_ne!(first, source_signature(&path).unwrap());
        assert!(source_signature(&root.path().join("missing.jpg")).is_err());
        assert_eq!(source_signature(&path).unwrap().len(), 64);
    }
}
