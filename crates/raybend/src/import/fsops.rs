//! 文件操作：真实实现 + 内存假实现（测试用）。
//!
//! 导入执行器（`runner`）只通过 [`FileOps`] 碰磁盘，于是「跑完一整轮导入」
//! 这件事在单测里可以**完全不碰真文件** —— `cargo test` 才能保持秒级（`AGENTS.md` §2.10）。
//!
//! ## 复制为什么要 `.part` 中转
//!
//! ```text
//! 源文件 ──复制──▶ <目标>.<随机>.part ──rename──▶ <目标>
//! ```
//!
//! 直接往目标名上写，一旦中途崩溃/断电就会留下**半截文件**，而它长得跟成品一模一样
//! （下次导入会以为已经导过了）。`.part` 中转 + rename 让「就位」变成一个原子动作：
//! 目标名要么不存在、要么是完整文件。残留的 `.part` 以 `.` 开头，扫描器本来就会跳过它。

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::import::plan::SourceExtras;
use crate::media::exif;
use crate::media::scan::{Cancel, ScanEvent, ScanOptions, ScannedFile};
use crate::store::file_id::FileId;

/// 导入要用到的文件操作。
///
/// `rel` 一律是**库内相对路径**（含 `photos/`，`/` 分隔），由实现去和库根拼起来；
/// `source` 是源盘上的绝对路径。
///
/// **全部方法只借 `&self`**：执行器要同时拿着「文件操作」与「扫描器」，
/// 而测试里它们是同一个内存假实现 —— 两个 `&mut` 借不出来。真实现本来也只需要 `&self`
/// （它只有一个库根路径），内存实现把可变状态放进 `RefCell`。
pub trait FileOps {
    /// 库内相对路径存在吗（只认文件）。
    fn exists(&self, rel: &str) -> bool;
    /// 库内相对目录存在吗。
    fn dir_exists(&self, rel: &str) -> bool;
    /// 建目录（连同父级；已存在不算错）。
    fn create_dir_all(&self, rel: &str) -> Result<()>;
    /// 复制：源绝对路径 → 库内相对路径。返回复制的字节数。
    ///
    /// **绝不覆盖**已存在的目标（[`Error::TargetExists`]），失败时不留临时文件。
    fn copy(&self, source: &Path, rel: &str) -> Result<u64>;
    /// 删一个文件（清理用；不存在不算错）。
    fn remove(&self, rel: &str) -> Result<()>;
    /// 库内文件的大小。
    fn size_of(&self, rel: &str) -> Option<u64>;
    /// 库内文件的身份（读不到 → `None`）。
    fn identity_of(&self, rel: &str) -> Option<FileId>;
    /// 目标卷剩余空间（拿不到 → `None`）。
    fn free_bytes(&self) -> Option<u64>;
}

/// 扫描源目录时要顺手读哪些信息。
///
/// **模版用到才读**：`:CYEAR` 那类变量要读 EXIF（每个文件一次盘），
/// 而纯 `:FILENAME` 模版一个字节都不用读。判重同理 —— 关掉开关就别去读身份。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanNeeds {
    /// 要不要读文件身份（判重开着才要）。
    pub identity: bool,
    /// 要不要读拍摄时间（模版里有日期变量才要）。
    pub taken_at: bool,
    /// 要不要读相机品牌/型号（模版里有 `:BRAND`/`:MODEL` 才要）。
    pub camera: bool,
}

impl ScanNeeds {
    /// 从「模版 + 判重开关」推出要读什么。
    #[must_use]
    pub fn for_template(template: &crate::import::template::Template, avoid_duplicates: bool) -> Self {
        use crate::import::template::Var;
        let vars = template.vars();
        Self {
            identity: avoid_duplicates,
            taken_at: template.needs_date(),
            camera: vars.contains(&Var::Brand) || vars.contains(&Var::Model),
        }
    }

    /// 一个都不读（纯文件名模版 + 不判重）。
    #[must_use]
    pub fn nothing() -> Self {
        Self {
            identity: false,
            taken_at: false,
            camera: false,
        }
    }

    /// 全都要读。
    #[must_use]
    pub fn everything() -> Self {
        Self {
            identity: true,
            taken_at: true,
            camera: true,
        }
    }
}

/// 走目录树的结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WalkOutcome {
    /// 报出来的文件数。
    pub files: usize,
    /// 跳过的文件/目录数（隐藏、垃圾、非照片……）。
    pub skipped: usize,
    /// 是不是被取消了（取消时结果**不完整**）。
    pub cancelled: bool,
}

/// 源目录的扫描器：把「走目录树」与「读元数据」两件事都挡在注入点后面。
///
/// 拆成两步（先 `walk` 后 `enrich`）的理由：走目录只要文件名与大小，很快；
/// 读 EXIF/身份要真的碰每个文件 —— 前者给进度与取消用，后者才花时间。
pub trait Scanner {
    /// 走一遍目录树，**边扫边回调**（回调返回 `false` 就停下）。
    fn walk(
        &self,
        root: &Path,
        opts: &ScanOptions,
        cancel: &Cancel,
        on_file: &mut dyn FnMut(ScannedFile) -> bool,
    ) -> Result<WalkOutcome>;

    /// 给一批文件补齐规划要用的信息（身份 / 拍摄时间 / 相机）。
    ///
    /// 结果**与输入同序**；单张失败就是 `None`，不该让整批失败。
    fn enrich(&self, files: &[ScannedFile], needs: &ScanNeeds, cancel: &Cancel)
    -> Vec<SourceExtras>;
}

/// 真实的扫描器：`media::scan` + 并行读元数据。
#[derive(Debug, Default, Clone, Copy)]
pub struct FsScanner;

impl Scanner for FsScanner {
    fn walk(
        &self,
        root: &Path,
        opts: &ScanOptions,
        cancel: &Cancel,
        on_file: &mut dyn FnMut(ScannedFile) -> bool,
    ) -> Result<WalkOutcome> {
        let mut out = WalkOutcome::default();
        let mut stop = false;
        let outcome = crate::media::scan::scan(root, opts, cancel, |event| {
            if let ScanEvent::File(file) = event {
                out.files += 1;
                if !on_file(file) {
                    stop = true;
                    // 回调说停：用「取消」这条既有通路把扫描停下来（不是错误）
                    cancel.cancel();
                }
            }
            Ok(())
        })?;
        out.skipped = outcome.skipped;
        out.cancelled = stop || outcome.cancelled;
        Ok(out)
    }

    fn enrich(
        &self,
        files: &[ScannedFile],
        needs: &ScanNeeds,
        cancel: &Cancel,
    ) -> Vec<SourceExtras> {
        if files.is_empty() {
            return Vec::new();
        }
        // 一个都不读：直接给空（纯文件名模版 + 不判重时的快路径）
        if *needs == ScanNeeds::nothing() {
            return vec![SourceExtras::default(); files.len()];
        }

        // 与 `media::source::read_times` 同一套办法：分块、`min(4, 核数)` 线程、结果保持顺序
        let workers = std::thread::available_parallelism()
            .map_or(1, std::num::NonZeroUsize::get)
            .clamp(1, 4);
        if workers == 1 || files.len() < 2 {
            return files
                .iter()
                .map(|f| extras_for(f, needs, cancel))
                .collect();
        }

        let chunk = files.len().div_ceil(workers);
        let mut out: Vec<SourceExtras> = Vec::with_capacity(files.len());
        std::thread::scope(|scope| {
            let handles: Vec<_> = files
                .chunks(chunk)
                .map(|slice| {
                    scope.spawn(move || {
                        slice
                            .iter()
                            .map(|f| extras_for(f, needs, cancel))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            for handle in handles {
                if let Ok(part) = handle.join() {
                    out.extend(part);
                }
            }
        });
        // 线程 panic 时长度可能对不上：补齐，绝不留下「下标错位」的隐患
        out.resize(files.len(), SourceExtras::default());
        out
    }
}

/// 读一个文件的规划信息。
fn extras_for(file: &ScannedFile, needs: &ScanNeeds, cancel: &Cancel) -> SourceExtras {
    if cancel.is_cancelled() {
        return SourceExtras::default();
    }
    let (taken_at, brand, model) = if needs.taken_at || needs.camera {
        // RAW 的拍摄时间/机身也要读得到（否则导入落点会退回文件名/mtime）
        let data = exif::read_file_for(&file.abs_path);
        let taken = if needs.taken_at {
            exif::resolve_taken_at(Some(&data), &file.file_name, file.mtime_ms).map(|t| t.millis)
        } else {
            None
        };
        (
            taken,
            if needs.camera { data.camera_make } else { None },
            if needs.camera { data.camera_model } else { None },
        )
    } else {
        (None, None, None)
    };
    SourceExtras {
        identity: if needs.identity {
            FileId::try_read(&file.abs_path)
        } else {
            None
        },
        taken_at,
        brand,
        model,
    }
}

/// 真实实现：库根 + 真文件系统。
#[derive(Debug, Clone)]
pub struct RepoFs {
    root: PathBuf,
}

impl RepoFs {
    /// 绑到一个库根目录。
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// 库根。
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 库内相对路径 → 绝对路径。
    ///
    /// 顺手挡住「跑出库外」的相对路径 —— 这不是防坏人，是防我们自己写错：
    /// 目标路径由模版算出来，模版再花哨也不该能写到库外面去。
    /// `..` 只能往上走到**库根为止**，再多的 `..` 直接忽略。
    #[must_use]
    pub fn abs(&self, rel: &str) -> PathBuf {
        let mut out = self.root.clone();
        let mut depth = 0_usize;
        for segment in rel.split(['/', '\\']) {
            match segment {
                "" | "." => {}
                ".." => {
                    if depth > 0 {
                        out.pop();
                        depth -= 1;
                    }
                    // 已经在库根：`..` 无处可去，忽略它
                }
                other => {
                    out.push(other);
                    depth += 1;
                }
            }
        }
        out
    }
}

impl FileOps for RepoFs {
    fn exists(&self, rel: &str) -> bool {
        self.abs(rel).is_file()
    }

    fn dir_exists(&self, rel: &str) -> bool {
        self.abs(rel).is_dir()
    }

    fn create_dir_all(&self, rel: &str) -> Result<()> {
        let path = self.abs(rel);
        std::fs::create_dir_all(&path).map_err(|e| {
            Error::Io(std::io::Error::new(
                e.kind(),
                format!("建目录失败 {}：{e}", path.display()),
            ))
        })
    }

    fn copy(&self, source: &Path, rel: &str) -> Result<u64> {
        let target = self.abs(rel);
        if target.exists() {
            return Err(Error::TargetExists(target.display().to_string()));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // 临时名：同一目录下，`.` 开头（扫描器按隐藏项跳过）
        let part = part_path(&target);
        let outcome = (|| -> Result<u64> {
            let written = std::fs::copy(source, &part)?;
            std::fs::rename(&part, &target)?;
            Ok(written)
        })();
        if outcome.is_err() {
            // 失败不留垃圾：临时文件清掉（目标本身没被碰过）
            let _ = std::fs::remove_file(&part);
        }
        outcome
    }

    fn remove(&self, rel: &str) -> Result<()> {
        let path = self.abs(rel);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::Io(e)),
        }
    }

    fn size_of(&self, rel: &str) -> Option<u64> {
        std::fs::metadata(self.abs(rel)).ok().map(|m| m.len())
    }

    fn identity_of(&self, rel: &str) -> Option<FileId> {
        FileId::try_read(self.abs(rel))
    }

    fn free_bytes(&self) -> Option<u64> {
        free_bytes_of(&self.root)
    }
}

impl crate::import::plan::FsProbe for RepoFs {
    fn file_exists(&self, rel_path: &str) -> bool {
        self.exists(rel_path)
    }
}

/// 一个文件在复制中途的临时名（同目录、`.` 开头）。
fn part_path(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map_or_else(|| "photo".to_string(), |n| n.to_string_lossy().to_string());
    target.with_file_name(format!(".{name}.part"))
}

/// 某个路径所在卷的剩余空间。
///
/// 拿不到就返回 `None`（导入前的预检会把它当成「不知道」，而不是「没空间」——
/// 网络盘、奇怪的 FS 上真有可能拿不到）。
///
/// Unix 侧用 `libc::statvfs`（`libc` 本来就在依赖树里，被 rusqlite/tauri 传递引入，
/// 登记成直接依赖不增加任何新代码）；Windows 侧用已在依赖里的 `windows-sys`。
#[must_use]
pub fn free_bytes_of(path: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;

        let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
        // SAFETY: `libc::statvfs` 是纯整数字段的结构体，全零是合法初值。
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        // SAFETY: `c` 以 NUL 结尾且生命周期覆盖本次调用；`st` 是本地可写变量。
        if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
            return None;
        }
        // f_bavail 是「非特权用户可用块数」，正是我们要的。
        // 这两个字段在 64 位上是 u64、32 位上是 u32 —— 统一按 u64 算。
        #[allow(clippy::useless_conversion)]
        let available = u64::from(st.f_bavail);
        #[allow(clippy::useless_conversion)]
        let block = u64::from(st.f_bsize);
        Some(available.saturating_mul(block))
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;

        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut free_to_caller: u64 = 0;
        // SAFETY: 传的是以 0 结尾的宽字符串（上面 chain 补了 0）；
        // 前两个输出参数显式传空指针（表示不关心），第三个是本地可写变量。
        let ok = unsafe {
            windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
                wide.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut free_to_caller,
            )
        };
        if ok == 0 { None } else { Some(free_to_caller) }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        None
    }
}

/// 内存里的假文件系统：单测靠它跑完整轮导入而不碰磁盘。
///
/// 「源盘」也在这个结构里（`sources`），这样才能注入「某个源文件读不了」这类故障。
#[derive(Debug, Default)]
pub struct MemoryFs {
    dirs: RefCell<HashSet<String>>,
    /// 折叠路径 →（原始路径, 字节）。键用折叠形式（模拟 Windows 的大小写不敏感），
    /// 值里留着原始写法，断言才看得出是哪条。
    files: RefCell<HashMap<String, (String, Vec<u8>)>>,
    /// 源盘上的内容（绝对路径 → 字节）。
    sources: HashMap<PathBuf, Vec<u8>>,
    /// 复制这些源路径时人为失败（测错误清单）。
    failing: RefCell<HashSet<PathBuf>>,
    /// 读「库内文件身份」时返回什么（测试显式指定；内存文件没有真身份）。
    identity_answers: RefCell<HashMap<String, FileId>>,
    /// 源根 → 扫描会看到的文件。
    scans: HashMap<PathBuf, Vec<ScannedFile>>,
    /// 源相对路径 → 读元数据会得到什么。
    extras: HashMap<String, SourceExtras>,
    /// 报告给调用方的剩余空间。
    pub free: Option<u64>,
    /// 记录每次复制（`源 → 目标`），断言「确实拷了这些」。
    copied: RefCell<Vec<(PathBuf, String)>>,
}

impl MemoryFs {
    /// 空文件系统。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 往「源盘」放一个文件。
    pub fn add_source(&mut self, path: impl Into<PathBuf>, bytes: &[u8]) {
        self.sources.insert(path.into(), bytes.to_vec());
    }

    /// 让某个源路径的复制失败。
    pub fn fail_source(&mut self, path: impl Into<PathBuf>) {
        self.failing.borrow_mut().insert(path.into());
    }

    /// 安排某个源文件的规划信息（拍摄时间 / 身份 / 相机）。
    pub fn set_extras(&mut self, rel_path: &str, extras: SourceExtras) {
        self.extras.insert(rel_path.to_string(), extras);
    }

    /// 预先放一个库内文件（模拟「目标已存在」）。
    pub fn add_existing(&mut self, rel: &str, bytes: &[u8]) {
        self.files
            .borrow_mut()
            .insert(fold(rel), (rel.to_string(), bytes.to_vec()));
        if let Some(dir) = parent_of(rel) {
            self.dirs.borrow_mut().insert(dir);
        }
    }

    /// 某个库内文件的字节（断言用）。
    #[must_use]
    pub fn bytes_of(&self, rel: &str) -> Option<Vec<u8>> {
        self.files
            .borrow()
            .get(&fold(rel))
            .map(|(_, b)| b.clone())
    }

    /// 库内文件数（断言「没多也没少」）。
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.files.borrow().len()
    }

    /// 现在有哪些库内相对路径（原始写法、排序；断言用）。
    #[must_use]
    pub fn paths(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .files
            .borrow()
            .values()
            .map(|(rel, _)| rel.clone())
            .collect();
        out.sort();
        out
    }

    /// 复制过哪些（`源 → 目标`；断言「确实拷了这些」）。
    #[must_use]
    pub fn copied(&self) -> Vec<(PathBuf, String)> {
        self.copied.borrow().clone()
    }
}

impl FileOps for MemoryFs {
    fn exists(&self, rel: &str) -> bool {
        self.files.borrow().contains_key(&fold(rel))
    }

    fn dir_exists(&self, rel: &str) -> bool {
        self.dirs.borrow().contains(&fold(rel))
    }

    fn create_dir_all(&self, rel: &str) -> Result<()> {
        let mut acc = String::new();
        for segment in rel.split('/') {
            if segment.is_empty() {
                continue;
            }
            if !acc.is_empty() {
                acc.push('/');
            }
            acc.push_str(segment);
            self.dirs.borrow_mut().insert(fold(&acc));
        }
        Ok(())
    }

    fn copy(&self, source: &Path, rel: &str) -> Result<u64> {
        if self.failing.borrow().contains(source) {
            return Err(Error::Io(std::io::Error::other(format!(
                "读不了源文件：{}",
                source.display()
            ))));
        }
        if self.exists(rel) {
            return Err(Error::TargetExists(rel.to_string()));
        }
        let Some(bytes) = self.sources.get(source).cloned() else {
            return Err(Error::PathNotFound(source.to_path_buf()));
        };
        if let Some(dir) = parent_of(rel) {
            self.dirs.borrow_mut().insert(dir);
        }
        let len = bytes.len() as u64;
        self.files
            .borrow_mut()
            .insert(fold(rel), (rel.to_string(), bytes));
        self.copied
            .borrow_mut()
            .push((source.to_path_buf(), rel.to_string()));
        Ok(len)
    }

    fn remove(&self, rel: &str) -> Result<()> {
        self.files.borrow_mut().remove(&fold(rel));
        Ok(())
    }

    fn size_of(&self, rel: &str) -> Option<u64> {
        self.files
            .borrow()
            .get(&fold(rel))
            .map(|(_, b)| u64::try_from(b.len()).unwrap_or(u64::MAX))
    }

    fn identity_of(&self, rel: &str) -> Option<FileId> {
        // 内存里的文件没有真实身份 —— 由测试显式指定（`set_identity`）
        self.identity_answers.borrow().get(rel).copied()
    }

    fn free_bytes(&self) -> Option<u64> {
        self.free
    }
}

impl MemoryFs {
    /// 指定「读某个库内文件身份时返回什么」（默认 `None`）。
    pub fn set_identity(&self, rel: &str, id: FileId) {
        self.identity_answers.borrow_mut().insert(rel.to_string(), id);
    }
}

impl MemoryFs {
    /// 安排「扫某个源根时会看到什么」。
    pub fn set_scan(&mut self, root: impl Into<PathBuf>, files: Vec<ScannedFile>) {
        self.scans.insert(root.into(), files);
    }
}

impl Scanner for MemoryFs {
    fn walk(
        &self,
        root: &Path,
        _opts: &ScanOptions,
        cancel: &Cancel,
        on_file: &mut dyn FnMut(ScannedFile) -> bool,
    ) -> Result<WalkOutcome> {
        let files = self.scans.get(root).cloned().unwrap_or_default();
        let mut out = WalkOutcome::default();
        for file in files {
            if cancel.is_cancelled() {
                out.cancelled = true;
                break;
            }
            out.files += 1;
            if !on_file(file) {
                out.cancelled = true;
                break;
            }
        }
        Ok(out)
    }

    fn enrich(
        &self,
        files: &[ScannedFile],
        _needs: &ScanNeeds,
        _cancel: &Cancel,
    ) -> Vec<SourceExtras> {
        files
            .iter()
            .map(|f| {
                self.extras
                    .get(&f.rel_path)
                    .cloned()
                    .unwrap_or_default()
            })
            .collect()
    }
}

impl crate::import::plan::FsProbe for MemoryFs {
    fn file_exists(&self, rel_path: &str) -> bool {
        self.exists(rel_path)
    }
}

fn parent_of(rel: &str) -> Option<String> {
    rel.rfind('/').map(|idx| fold(&rel[..idx]))
}

fn fold(path: &str) -> String {
    crate::store::path_semantics::PathForms::new(path)
        .folded()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /* ══════════════════════════════════════════════════════════════
     * 内存假实现（runner 的单测靠它跑完整轮导入而不碰磁盘）
     * ══════════════════════════════════════════════════════════════ */

    #[test]
    fn memory_copy_moves_bytes_and_records_what_happened() {
        let mut fs = MemoryFs::new();
        fs.add_source("/src/a.jpg", b"hello");
        let written = fs.copy(Path::new("/src/a.jpg"), "photos/a.jpg").expect("复制");
        assert_eq!(written, 5);
        assert_eq!(fs.bytes_of("photos/a.jpg").as_deref(), Some(&b"hello"[..]));
        assert_eq!(fs.paths(), vec!["photos/a.jpg"]);
        assert_eq!(
            fs.copied(),
            vec![(PathBuf::from("/src/a.jpg"), "photos/a.jpg".to_string())]
        );
        // 目录被顺手建出来了
        assert!(fs.dir_exists("photos"));
    }

    #[test]
    fn memory_copy_refuses_to_overwrite() {
        let mut fs = MemoryFs::new();
        fs.add_source("/src/a.jpg", b"new");
        fs.add_existing("photos/a.jpg", b"old");
        let err = fs
            .copy(Path::new("/src/a.jpg"), "photos/a.jpg")
            .expect_err("不许覆盖");
        assert!(matches!(err, Error::TargetExists(_)));
        assert_eq!(
            fs.bytes_of("photos/a.jpg").as_deref(),
            Some(&b"old"[..]),
            "原文件没被动"
        );
    }

    #[test]
    fn memory_copy_reports_missing_and_failing_sources() {
        let mut fs = MemoryFs::new();
        let err = fs
            .copy(Path::new("/src/gone.jpg"), "photos/gone.jpg")
            .expect_err("源不在");
        assert!(matches!(err, Error::PathNotFound(_)));
        assert_eq!(fs.file_count(), 0, "失败不留半截文件");

        fs.add_source("/src/bad.jpg", b"x");
        fs.fail_source("/src/bad.jpg");
        let err = fs
            .copy(Path::new("/src/bad.jpg"), "photos/bad.jpg")
            .expect_err("读不了");
        assert!(matches!(err, Error::Io(_)));
        assert_eq!(fs.file_count(), 0);
    }

    #[test]
    fn memory_dirs_and_sizes() {
        let mut fs = MemoryFs::new();
        fs.create_dir_all("photos/2026-08-15/Japan").unwrap();
        assert!(fs.dir_exists("photos/2026-08-15/Japan"));
        assert!(fs.dir_exists("photos/2026-08-15"), "父级也建了");
        assert!(fs.dir_exists("photos"));
        assert!(!fs.dir_exists("photos/elsewhere"));
        // 建目录不等于建文件
        assert_eq!(fs.file_count(), 0);

        fs.add_source("/src/a.jpg", b"12345");
        fs.copy(Path::new("/src/a.jpg"), "photos/a.jpg").unwrap();
        assert_eq!(fs.size_of("photos/a.jpg"), Some(5));
        assert_eq!(fs.size_of("photos/none.jpg"), None);
        fs.remove("photos/a.jpg").unwrap();
        assert_eq!(fs.file_count(), 0);
        fs.remove("photos/a.jpg").expect("删不存在的文件不算错");
    }

    #[test]
    fn memory_fake_is_case_insensitive_like_windows() {
        let mut fs = MemoryFs::new();
        fs.add_existing("Photos/A.JPG", b"x");
        assert!(fs.exists("photos/a.jpg"), "折叠比较");
        assert_eq!(fs.size_of("PHOTOS/a.JPG"), Some(1));
    }

    #[test]
    fn memory_fake_serves_both_traits() {
        // 规划阶段用 `FsProbe`，执行阶段用 `FileOps` —— 同一个假实现两边都能用
        use crate::import::plan::FsProbe;
        let mut fs = MemoryFs::new();
        fs.add_existing("photos/a.jpg", b"x");
        assert!(FsProbe::file_exists(&fs, "photos/a.jpg"));
        assert!(!FsProbe::file_exists(&fs, "photos/b.jpg"));
        fs.free = Some(1234);
        assert_eq!(fs.free_bytes(), Some(1234));
    }

    #[test]
    fn memory_identity_is_explicit() {
        let fs = MemoryFs::new();
        assert_eq!(fs.identity_of("photos/a.jpg"), None, "默认读不到身份");
        let id = FileId::new(9, [7u8; 16]);
        fs.set_identity("photos/a.jpg", id);
        assert_eq!(fs.identity_of("photos/a.jpg"), Some(id));
    }

    /* ══════════════════════════════════════════════════════════════
     * 真实实现（临时目录，跑完就删）
     * ══════════════════════════════════════════════════════════════ */

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().expect("临时目录")
    }

    fn write_source(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).expect("写源文件");
        path
    }

    #[test]
    fn repo_fs_copies_files_into_place() {
        let dir = tmp();
        let source = write_source(dir.path(), "a.jpg", b"hello world");
        let root = dir.path().join("repo");
        let fs = RepoFs::new(&root);
        fs.create_dir_all("photos").unwrap();
        assert!(fs.dir_exists("photos"));
        assert!(!fs.dir_exists("photos/x"));

        let written = fs
            .copy(&source, "photos/2026-08-15/a.jpg")
            .expect("复制应当成功");
        assert_eq!(written, 11);
        assert_eq!(fs.size_of("photos/2026-08-15/a.jpg"), Some(11));
        assert_eq!(
            std::fs::read(root.join("photos/2026-08-15/a.jpg")).unwrap(),
            b"hello world"
        );
        // 目标目录是自动建的
        assert!(fs.dir_exists("photos/2026-08-15"));
        // 不留临时文件
        let leftovers: Vec<_> = std::fs::read_dir(root.join("photos/2026-08-15"))
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains("part"))
            .collect();
        assert!(leftovers.is_empty(), "不该留下 .part：{leftovers:?}");
    }

    #[test]
    fn repo_fs_never_overwrites_an_existing_file() {
        let dir = tmp();
        let source = write_source(dir.path(), "a.jpg", b"new content");
        let root = dir.path().join("repo");
        let fs = RepoFs::new(&root);
        fs.create_dir_all("photos").unwrap();
        std::fs::write(root.join("photos/a.jpg"), b"old").unwrap();

        let err = fs.copy(&source, "photos/a.jpg").expect_err("不许覆盖");
        assert!(matches!(err, Error::TargetExists(_)), "{err:?}");
        assert_eq!(
            std::fs::read(root.join("photos/a.jpg")).unwrap(),
            b"old",
            "原文件必须是原样"
        );
    }

    #[test]
    fn repo_fs_cleans_up_when_the_source_is_unreadable() {
        let dir = tmp();
        let root = dir.path().join("repo");
        let fs = RepoFs::new(&root);
        let err = fs
            .copy(&dir.path().join("根本没有这个文件.jpg"), "photos/a.jpg")
            .expect_err("源不存在");
        assert!(matches!(err, Error::Io(_)), "{err:?}");
        // 目标与临时文件都不该留下
        assert!(!root.join("photos/a.jpg").exists());
        assert!(!root.join("photos/.a.jpg.part").exists());
        assert!(!fs.exists("photos/a.jpg"));
    }

    #[test]
    fn repo_fs_reports_identity_and_free_space() {
        let dir = tmp();
        let source = write_source(dir.path(), "a.jpg", b"x");
        let root = dir.path().join("repo");
        let fs = RepoFs::new(&root);
        fs.create_dir_all("photos").unwrap();
        fs.copy(&source, "photos/a.jpg").unwrap();

        // 身份：正常文件系统上应当读得到；读不到（特殊 FS）就当未知，不算失败
        if let Some(id) = fs.identity_of("photos/a.jpg") {
            assert_ne!(id.volume_serial, 0, "卷序列号应当是真实的");
        }
        assert_eq!(fs.identity_of("photos/none.jpg"), None);

        // 剩余空间：只要求「拿得到就为正」
        if let Some(free) = fs.free_bytes() {
            assert!(free > 0, "free={free}");
        }
        if let Some(free) = free_bytes_of(dir.path()) {
            assert!(free > 0);
        }
    }

    #[test]
    fn repo_fs_abs_refuses_to_escape_the_root() {
        let fs = RepoFs::new("/repo/root");
        assert_eq!(fs.abs("photos/a.jpg"), PathBuf::from("/repo/root/photos/a.jpg"));
        assert_eq!(fs.abs("/photos/a.jpg"), PathBuf::from("/repo/root/photos/a.jpg"));
        assert_eq!(
            fs.abs("photos/../../etc/passwd"),
            PathBuf::from("/repo/root/etc/passwd"),
            "`..` 最多退到库根，再多的忽略 —— 走不出库外"
        );
        assert_eq!(
            fs.abs("../../../../etc/passwd"),
            PathBuf::from("/repo/root/etc/passwd"),
            "一上来就是 `..` 也一样"
        );
        assert!(!fs.abs("photos/a.jpg").to_string_lossy().contains("//"));
    }

    #[test]
    fn part_path_keeps_the_name_and_hides_the_file() {
        let part = part_path(Path::new("/repo/photos/a.jpg"));
        assert_eq!(part, PathBuf::from("/repo/photos/.a.jpg.part"));
        assert!(
            part.file_name().unwrap().to_string_lossy().starts_with('.'),
            "点开头 → 扫描器会跳过它"
        );
    }
}
