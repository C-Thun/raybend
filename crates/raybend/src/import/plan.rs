//! 目标路径规划：一张源照片该落到库里的哪条路径（`memory/FUNCTION-REPOSITORY.md` §3.3–§4.3）。
//!
//! 这一步**不碰磁盘、不写数据库**（复制与登记是 `runner` 的事），它只回答两个问题：
//!
//! ```text
//! 1. 这张源文件要不要导？（重复 / 被「不含子目录」排除 → 跳过）
//! 2. 要导的话，落到 photos/ 下的哪条相对路径？
//! ```
//!
//! ## 三步算法（顺序不能换，`memory/FUNCTION-REPOSITORY.md` §4.2）
//!
//! ```text
//! ① 目录透传：目标目录**从文件反推** —— 每张照片算出自己的目标根，再把源相对层级原样搬下去
//!    （于是「空目录不建、只含文档的目录不建」自动成立，不需要单独枚举一遍目录树）
//! ② 模版求值出「该文件的目标根」：模版的**目录部分**只在这里求值一次
//! ③ 位图/RAW 分流、重名后缀、序号分配都作用在最终目标目录上
//! ```
//!
//! ## 三条容易被忽略的规则
//!
//! * **序号按「最终目标目录 + 宽度」独立计数** —— `Japan/Kyoto` 与 `Japan/Tokyo` 各有各的 `0001`。
//! * **位图与 RAW 共享渲染出来的名字**：`MYP0001.png` 与 `_RAW/MYP0001.ORF` 同名是有意的
//!   （编辑位图时要靠它找到同名 RAW）。所以**序号在「组」这一级分配一次**，RAW 不再另取号。
//! * **跳过的文件不消耗序号**：否则一次「重复导入」的操作会把库里的编号往后推。
//!
//! 注入点（测试与真实环境的分界）：
//!
//! * [`FsProbe`] —— 目标路径在**盘上**是不是已经占着了（真实实现查盘，测试用内存集合）；
//! * [`KnownTargets`] —— 目标路径在**库里**是不是已经占着了（真实实现读 `asset_files`，见 sink）；
//! * [`Sequences`] —— 序号计数（真实实现落 `seq_counters` 表，见 runner）。

use std::collections::{HashMap, HashSet};

use crate::import::template::{self, RenderCtx, Template, Var};
use crate::media::fingerprint::ContentFingerprint;
use crate::media::kind::MediaKind;
use crate::media::scan::ScannedFile;
use crate::store::file_id::FileId;
use crate::store::path_semantics::PathForms;

/// 一张待导入的源文件（`plan` 的输入）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    /// 源文件绝对路径（复制时用）。
    pub abs_path: std::path::PathBuf,
    /// 相对**源根**的路径（`/` 分隔，原始大小写）。
    pub rel_path: String,
    /// 文件名原文（含扩展名，原始大小写）。
    pub file_name: String,
    /// 扩展名（**原始大小写**，不含点；没有扩展名时是空串）。
    pub ext: String,
    /// 大类（位图 / RAW）。
    pub kind: MediaKind,
    /// 去扩展名并折叠的主体 —— 配对位图与 RAW 用（同一组的这两个值相同）。
    pub stem_folded: String,
    /// 字节数。
    pub size_bytes: u64,
    /// 修改时间（Unix 毫秒）。
    pub mtime_ms: Option<i64>,
    /// 创建时间（Unix 毫秒；文件系统不给出生时间时为 `None`）。
    pub created_ms: Option<i64>,
    /// 文件身份（读不到时为 `None`）。
    pub identity: Option<FileId>,
    /// 拍摄时间（Unix 毫秒；缺了模版就渲染成 0）。
    pub taken_at: Option<i64>,
    /// 相机品牌。
    pub brand: Option<String>,
    /// 相机型号。
    pub model: Option<String>,
}

impl SourceFile {
    /// 由扫描结果 + 额外读到的信息拼出来。
    #[must_use]
    pub fn from_scanned(scan: &ScannedFile, extras: SourceExtras) -> Self {
        Self {
            abs_path: scan.abs_path.clone(),
            rel_path: scan.rel_path.clone(),
            file_name: scan.file_name.clone(),
            // 扩展名要**原样**透传（`P0001.JPG` 落库后还是 `.JPG`），
            // 所以不能走 `kind::extension` —— 那个按定义返回小写（判类型用）
            ext: original_extension(&scan.file_name),
            kind: scan.kind,
            // 扫描给的 stem_folded 已经是「去扩展名 + NFC + 折叠」，直接用
            stem_folded: scan.stem_folded.clone(),
            size_bytes: scan.size_bytes,
            mtime_ms: scan.mtime_ms,
            created_ms: scan.created_ms,
            identity: extras.identity,
            taken_at: extras.taken_at,
            brand: extras.brand,
            model: extras.model,
        }
    }

    /// 它所在的源目录（相对源根，`/` 分隔；直接躺在源根里时是空串）。
    #[must_use]
    pub fn source_dir(&self) -> &str {
        match self.rel_path.rfind('/') {
            Some(idx) => &self.rel_path[..idx],
            None => "",
        }
    }

    /// 文件名主体（不含扩展名，原文大小写）。
    #[must_use]
    pub fn stem(&self) -> &str {
        match self.ext.is_empty() {
            true => &self.file_name,
            false => &self.file_name[..self.file_name.len() - self.ext.len() - 1],
        }
    }

    fn role(&self) -> Option<Role> {
        Role::of(self.kind)
    }
}

/// 从文件名里取**原始大小写**的扩展名（没有就是空串）。
///
/// 判类型用 `kind::extension`（小写），落盘用这个 —— `P0001.JPG` 复制进库后
/// 还得叫 `.JPG`，用户看得见的东西不该被我们悄悄改名。
#[must_use]
pub fn original_extension(file_name: &str) -> String {
    let name = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    match name.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < name.len() => name[dot + 1..].to_string(),
        _ => String::new(),
    }
}

/// 扫描结果之外、规划还需要的信息。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceExtras {
    /// 文件身份。
    pub identity: Option<FileId>,
    /// 拍摄时间（Unix 毫秒）。
    pub taken_at: Option<i64>,
    /// 相机品牌。
    pub brand: Option<String>,
    /// 相机型号。
    pub model: Option<String>,
}

/// 文件在资产里的角色（与 `asset_files.role` 同义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// 位图。
    Bitmap,
    /// 原始数据。
    Raw,
}

impl Role {
    /// 由大类推出角色；`MediaKind::Other` 不是照片，返回 `None`。
    #[must_use]
    pub fn of(kind: MediaKind) -> Option<Self> {
        match kind {
            MediaKind::Image => Some(Self::Bitmap),
            MediaKind::Raw => Some(Self::Raw),
            MediaKind::Other => None,
        }
    }

    /// 库里 `asset_files.role` 的写法。
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bitmap => "bitmap",
            Self::Raw => "raw",
        }
    }
}

/// 「这张源文件之前导进来过吗」——判重用（`memory/FUNCTION-REPOSITORY.md` §4.3）。
///
/// 两层：**源身份**优先，**源绝对路径折叠 + 大小 + mtime** 兜底
/// （网络盘/权限不足时读不到身份，那也不能退化成「每张都当新的」）。
///
/// 兜底键用**绝对路径**：库里存的 `source_path` 就是它（顺带能回答「这张照片从哪来的」），
/// 而且用相对路径的话，两个不同源根里的同名文件会互相撞。
#[derive(Debug, Default, Clone)]
pub struct KnownSources {
    identities: HashSet<FileId>,
    fallback: HashSet<(String, u64, Option<i64>)>,
}

impl KnownSources {
    /// 空集（第一次导入）。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 记一个已导入过的源身份。
    pub fn insert_identity(&mut self, id: FileId) {
        if !id.is_zero() {
            self.identities.insert(id);
        }
    }

    /// 记一条兜底记录（源路径 + 大小 + mtime）。
    pub fn insert_fallback(&mut self, source_path: &str, size: u64, mtime: Option<i64>) {
        self.fallback
            .insert((fold(source_path), size, mtime));
    }

    /// 这张源文件在库里有了吗？
    #[must_use]
    pub fn contains(&self, file: &SourceFile) -> bool {
        if let Some(id) = file.identity
            && !id.is_zero()
            && self.identities.contains(&id)
        {
            return true;
        }
        self.fallback.contains(&(
            fold(&file.abs_path.to_string_lossy()),
            file.size_bytes,
            file.mtime_ms,
        ))
    }

    /// 是不是空的（空的话判重可以整个跳过）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.identities.is_empty() && self.fallback.is_empty()
    }
}

/// 「这些目标路径是我们自己以前留下的」——续跑时用（`specs/M1-6.md` §3.3）。
///
/// 场景：上次导入在「复制完了但还没登记」那一步被杀，库里留下了文件
/// 与一条 `pending` 记录。这次再导同一批源时，规划**必须算出同一个目标路径**
/// （否则重名规则会给它加 `_01`，等于把同一张照片导了两份）。
///
/// 所以这些路径在规划眼里是**可用的**（哪怕盘上已经有了）：
/// 执行器会认出「这条是我们自己的」，直接补登记而不是重拷。
/// 只有**大小相符**的那些才会被放进来 —— 大小不符说明那文件不是我们写的。
#[derive(Debug, Default, Clone)]
pub struct Reserved {
    paths: HashSet<String>,
}

impl Reserved {
    /// 空集。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 记一条（内部按折叠比较，跟重名规则一致）。
    pub fn insert(&mut self, target_rel: &str) {
        self.paths.insert(fold(target_rel));
    }

    /// 是不是我们自己留下的。
    #[must_use]
    pub fn contains(&self, target_rel: &str) -> bool {
        self.paths.contains(&fold(target_rel))
    }

    /// 有没有。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

/// 目标路径规划要问磁盘的一件事：**这条库内相对路径在盘上已经有了吗**。
pub trait FsProbe {
    /// 库内相对路径（含 `photos/`）是否已存在。
    fn file_exists(&self, rel_path: &str) -> bool;
}

/// 库里已经占着的目标路径（folded → 那一行）—— 冲突判定必须**同时**看盘与库。
///
/// 只看盘为什么不够：库里有一行、盘上没有的路径（标了 missing、被手动删、
/// 或者**文件监听差分刚刚登记**）在盘探针看来是「空位」，而登记时的
/// `INSERT` 会撞 `UNIQUE(rel_path_folded)`（`store::assets::insert_file` 是裸 INSERT，
/// 且这条 op 在批事务里 —— 一条撞库会带走整批，2026-10-12 真机事故）。
///
/// 反过来的情况（盘上有、库里没有）由 [`FsProbe`] 负责；两者**都要**问。
/// 带上各自的大小与已建立的内容指纹：判定「同一份内容」时能直接用它，
/// 不必再去读库内文件（`specs/import-dedup-fingerprint.md` §3.2）。
#[derive(Debug, Default, Clone)]
pub struct KnownTargets {
    entries: HashMap<String, LibraryEntry>,
}

/// 库内一个文件行（冲突判定要的三件事）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryEntry {
    /// 库内相对路径（**原文**，给人看；键是它的折叠形态）。
    pub rel_path: String,
    /// 库里记的大小（老行可能是 `NULL`）。
    pub size_bytes: Option<u64>,
    /// 已建立的内容指纹（`NULL` = 还没建立）。
    pub fingerprint: Option<ContentFingerprint>,
}

impl KnownTargets {
    /// 空集。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 记一条（内部按折叠比较，跟重名规则一致）。
    pub fn insert(&mut self, entry: LibraryEntry) {
        self.entries.insert(fold(&entry.rel_path), entry);
    }

    /// 这条路径库里占着吗；占着就回那一行。
    #[must_use]
    pub fn get(&self, rel_path: &str) -> Option<&LibraryEntry> {
        self.entries.get(&fold(rel_path))
    }

    /// 这条路径库里占着吗。
    #[must_use]
    pub fn contains(&self, rel_path: &str) -> bool {
        self.entries.contains_key(&fold(rel_path))
    }

    /// 有没有。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// 内容指纹的来源（判重的注入点；真实实现读盘，测试用内存表）。
///
/// 只在**目标路径被占**时才被调用（`specs/import-dedup-fingerprint.md` §3.2）——
/// 新照片的导入路径一个字节都不多读。
pub trait ContentProbe {
    /// 源文件的内容指纹（口径按类型选）；读不到 → `None`（那就按冲突处理）。
    fn source_fingerprint(&self, file: &SourceFile) -> Option<ContentFingerprint>;
    /// 库内文件的内容指纹（读盘上那一份）；读不到 → `None`。
    fn library_fingerprint(&self, rel_path: &str) -> Option<ContentFingerprint>;
    /// 库内文件的大小（读盘上那一份）；盘上没有 → `None`。
    fn library_size(&self, rel_path: &str) -> Option<u64>;
}

/// 什么都不读的探针（关掉判重、以及不关心指纹的测试用）。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoContent;

impl ContentProbe for NoContent {
    fn source_fingerprint(&self, _file: &SourceFile) -> Option<ContentFingerprint> {
        None
    }
    fn library_fingerprint(&self, _rel_path: &str) -> Option<ContentFingerprint> {
        None
    }
    fn library_size(&self, _rel_path: &str) -> Option<u64> {
        None
    }
}

/// 序号分配：按「最终目标目录 + 位数」独立计数（`memory/FUNCTION-REPOSITORY.md` §3.3）。
#[derive(Debug, Default, Clone)]
pub struct Sequences {
    values: HashMap<(String, usize), u64>,
}

impl Sequences {
    /// 空计数（每个键都从 1 开始）。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 用库里已有的计数播种（`seq_counters` 表读出来的值）。
    pub fn seed(&mut self, directory: &str, width: usize, last: u64) {
        self.values
            .insert((fold(directory), width), last.min(max_value(width)));
    }

    /// 当前值（没分配过就是 0）。
    #[must_use]
    pub fn last(&self, directory: &str, width: usize) -> u64 {
        self.values
            .get(&(fold(directory), width))
            .copied()
            .unwrap_or(0)
    }

    /// 取下一个值：**从 1 开始**，写满（全 9）回绕到 1（`memory/FUNCTION-REPOSITORY.md` §3.3）。
    pub fn next(&mut self, directory: &str, width: usize) -> u64 {
        let cap = max_value(width);
        let entry = self
            .values
            .entry((fold(directory), width))
            .or_insert(0);
        *entry = if *entry >= cap { 1 } else { *entry + 1 };
        *entry
    }

    /// 把某个计数器**还回**到之前的值。
    ///
    /// 用在「名字渲染完了、结果整组都被跳过（内容与库里一致）」这一条路上：
    /// 跳过的文件不消耗序号（`memory/FUNCTION-REPOSITORY.md` §3.3），
    /// 否则一次「重复导入」会把库里的编号白白往后推。
    pub fn restore(&mut self, directory: &str, width: usize, value: u64) {
        self.values
            .insert((fold(directory), width), value.min(max_value(width)));
    }

    /// 导出所有非零计数（runner 据此写回 `seq_counters`）。
    #[must_use]
    pub fn snapshot(&self) -> Vec<(String, usize, u64)> {
        let mut out: Vec<(String, usize, u64)> = self
            .values
            .iter()
            .filter(|(_, v)| **v > 0)
            .map(|((dir, width), value)| (dir.clone(), *width, *value))
            .collect();
        out.sort();
        out
    }
}

/// 某个宽度能表示的最大编号（3 位 → 999）。到它就该回绕了。
fn max_value(width: usize) -> u64 {
    let width = u32::try_from(width.clamp(1, template::MAX_SEQ_WIDTH)).unwrap_or(9);
    10u64.saturating_pow(width).saturating_sub(1)
}

/// 规划参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanOptions {
    /// 库内放照片的目录名（`memory/FUNCTION-REPOSITORY.md` §1，默认 `photos`）。
    pub photos_dir: String,
    /// 是否透传子目录（界面上的「包含子目录」）。
    pub include_subdirs: bool,
    /// 是否避免重复导入（界面上的复选框；关掉就整批重导一份）。
    pub avoid_duplicates: bool,
}

impl Default for PlanOptions {
    fn default() -> Self {
        Self {
            photos_dir: "photos".to_string(),
            include_subdirs: true,
            avoid_duplicates: true,
        }
    }
}

/// 一条源文件规划出来的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedItem {
    /// 在输入数组里的下标（调用方拿它取回源文件）。
    pub index: usize,
    /// 源相对路径（错误清单里给人看的）。
    pub source_rel: String,
    /// 角色（`MediaKind::Other` 不是照片，没有角色）。
    pub role: Option<Role>,
    /// 结果。
    pub outcome: ItemOutcome,
    /// 模版里**缺值**的变量（用占位符顶了，值得记一笔）。
    pub missing: Vec<Var>,
    /// 它的**整组**是不是「完整导入」了（`specs/import-dedup-fingerprint.md` §9）。
    ///
    /// 判据：这一组在源里的每个成员都真的导入了（没跳过、没失败）、而且**没改名**。
    /// 边车（`.xmp`）采纳靠它把门 —— 拆开的情况（例：位图改名 `_01`、RAW 跳过）
    /// 不算完整导入，不采纳配置。
    pub complete: bool,
}

impl PlannedItem {
    /// 规划成功时它的库内相对路径。
    #[must_use]
    pub fn target_rel(&self) -> Option<&str> {
        match &self.outcome {
            ItemOutcome::Plan { target_rel } => Some(target_rel),
            _ => None,
        }
    }

    /// 要写进 `import_items.status` 的词（`memory/FUNCTION-REPOSITORY.md` §4.5）。
    #[must_use]
    pub fn status_str(&self) -> &'static str {
        match &self.outcome {
            ItemOutcome::Plan { .. } => "pending",
            ItemOutcome::Skipped(_) => "skipped",
            ItemOutcome::Failed(_) => "failed",
        }
    }

    /// 要写进 `import_items.reason` 的说明。
    #[must_use]
    pub fn reason(&self) -> Option<String> {
        match &self.outcome {
            ItemOutcome::Plan { .. } => None,
            ItemOutcome::Skipped(reason) | ItemOutcome::Failed(reason) => Some(reason.clone()),
        }
    }
}

/// 一条源文件的下场。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemOutcome {
    /// 规划好了：库内相对路径（含 `photos/`，含扩展名）。
    Plan {
        /// 目标路径。
        target_rel: String,
    },
    /// 不导，但不算失败（已在库里 / 被「不含子目录」排除）。
    Skipped(String),
    /// 这条落不了地（算出来的路径不合法）。
    Failed(String),
}

/// 规划结果的计数。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanCounts {
    /// 要导的。
    pub planned: usize,
    /// 跳过的（含重复）。
    pub skipped: usize,
    /// 其中「已在库中」的。
    pub duplicates: usize,
    /// 失败的。
    pub failed: usize,
    /// 模版缺值的文件数（`0000-00-00` 那种）。
    pub missing_meta: usize,
}

/// 规划结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanResult {
    /// 与输入同序、一一对应。
    pub items: Vec<PlannedItem>,
    /// 计数。
    pub counts: PlanCounts,
    /// 这一批触及的目标目录（去重、排序；runner 据此建目录）。
    pub target_dirs: Vec<String>,
    /// 判重时**顺手为库内老行建立的**内容指纹：`(库内相对路径, 指纹)`。
    ///
    /// plan 不写库（它只算），由 runner 随这一批事务回填（`Op::SetContentHash`）——
    /// 下次再判到同一条就省一次读盘。
    pub backfilled: Vec<(String, ContentFingerprint)>,
}

impl PlanResult {
    /// 要真的复制的那些（跳过与失败的不在其中）。
    pub fn planned(&self) -> impl Iterator<Item = &PlannedItem> {
        self.items
            .iter()
            .filter(|i| matches!(i.outcome, ItemOutcome::Plan { .. }))
    }
}

/// 为什么这张源文件不导（不算失败的那些）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Skip {
    /// 已在库里（判重命中）。
    Duplicate,
    /// 关掉了「包含子目录」，而它在子目录里。
    SubdirsExcluded,
    /// 不是可导入的图片（扫描层已过滤，这里是防御）。
    NotAPhoto,
}

impl Skip {
    fn reason(self) -> &'static str {
        match self {
            Self::Duplicate => "已在库中（按文件身份判重）",
            Self::SubdirsExcluded => "已关闭「包含子目录」，子目录里的文件不导入",
            Self::NotAPhoto => "不是可导入的图片",
        }
    }
}

/// 续跑时要「落回上次那个名字」的目标：源文件（`rel_path`）→ 上次计划的目标。
///
/// 崩溃可能正好停在「复制完了、还没登记」之间：文件已经在 `photos/` 里，
/// 而 `import_items` 那行还是 `pending`。这时若照常编号，同一张照片就会在磁盘上
/// 出现**两份**（一份没人认识）——`specs/M1-7.md` 步骤 2 的崩溃演练抓到的就是这个。
///
/// runner 见到「目标在且大小相符」会直接登记、不重拷（`runner.rs` ④），
/// 所以这里只要把名字复用上，重复副本就不会产生。
pub type Adopt = HashMap<String, String>;

/// 同一张照片的一组文件（位图 + RAW）。分组键是「源目录 + 折叠主体」。
#[derive(Debug, Default)]
struct Group {
    members: Vec<usize>,
    /// 组里**在源里**有没有位图 —— 决定它的 RAW 去不去 `_RAW/`
    /// （只跟源有关：位图哪怕被跳过了，RAW 该分流的还是分流）。
    has_bitmap: bool,
}

/// 规划一批源文件（同一次导入里的**一个源目录**）。
///
/// 多个源目录各自调用一次（每个源目录一个 run，见 `specs/M1-6.md` §3.4），
/// 所以这里不必考虑「不同源根的同名文件」—— 那是下一个目录的事了。
// 参数确实多，但每一条都是规划必需的外部事实（文件、模版、选项、探针、判重集、
// 序号计数器、我们占着的位置、上次留下的目标名）。把它们打包成结构体只是把参数藏起来，
// 反而看不清规划到底依赖什么 —— 所以这里显式列出并豁免这条建议。
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn plan(
    files: &[SourceFile],
    tpl: &Template,
    opts: &PlanOptions,
    fs: &dyn FsProbe,
    known: &KnownSources,
    known_targets: &KnownTargets,
    content: &dyn ContentProbe,
    seq: &mut Sequences,
    reserved: &Reserved,
    adopt: &Adopt,
) -> PlanResult {
    let mut outcomes: Vec<Option<ItemOutcome>> = vec![None; files.len()];
    let mut counts = PlanCounts::default();
    let mut groups: Vec<Group> = Vec::new();
    let mut group_index: HashMap<(String, String), usize> = HashMap::new();

    // ── 第 1 遍：分组 + 判「跳」──────────────────────────────
    // 跳过的文件不参与后面的序号分配与命名，但**分组信息要留着**：
    // 组里在源里有没有位图，决定同组的 RAW 去不去 `_RAW/`。
    for (idx, file) in files.iter().enumerate() {
        let key = (file.source_dir().to_string(), file.stem_folded.clone());
        let group = *group_index.entry(key).or_insert_with(|| {
            groups.push(Group::default());
            groups.len() - 1
        });
        groups[group].members.push(idx);
        if file.role() == Some(Role::Bitmap) {
            groups[group].has_bitmap = true;
        }

        if let Some(skip) = skip_reason(file, opts, known) {
            if skip == Skip::Duplicate {
                counts.duplicates += 1;
            }
            counts.skipped += 1;
            outcomes[idx] = Some(ItemOutcome::Skipped(skip.reason().to_string()));
        }
    }

    // ── 第 2 遍：逐组算路径 ─────────────────────────────────
    // 组长（组里第一个要导的位图，没有位图就是第一个要导的文件）去问序号分配器，
    // 组里其他文件复用它的名字：`MYP0001.png` 与 `_RAW/MYP0001.ORF` 同名是有意的。
    //
    // 判重（**逐成员**）与改名（**整组**）都在这一遍收口（`specs/import-dedup-fingerprint.md` §3）：
    // 只要有一个要导入的成员「同名但内容不同」，**整组一起**加同一个后缀 —— 否则
    // `A/MYP1_01.JPG` 与 `A/_RAW/MYP1.RW2` 名字对不上，库侧按主体配对时
    // 会把同一张照片拆成两个格子。
    let mut claimed: HashSet<String> = HashSet::new();
    let mut dirs: Vec<String> = Vec::new();
    let mut missing_by_item: Vec<Vec<Var>> = vec![Vec::new(); files.len()];
    let mut complete_by_item: Vec<bool> = vec![false; files.len()];
    let mut backfilled: Vec<(String, ContentFingerprint)> = Vec::new();
    // 源文件指纹缓存：同一次规划里一个源文件最多读一次
    let mut source_fingerprints: HashMap<String, Option<ContentFingerprint>> = HashMap::new();
    let context = VerdictContext {
        fs,
        known_targets,
        content,
        avoid_duplicates: opts.avoid_duplicates,
    };

    for group in &groups {
        let members: Vec<usize> = group
            .members
            .iter()
            .copied()
            .filter(|idx| outcomes[*idx].is_none())
            .collect();
        if members.is_empty() {
            continue;
        }
        let leader = members
            .iter()
            .copied()
            .find(|idx| files[*idx].role() == Some(Role::Bitmap))
            .unwrap_or(members[0]);

        // **组长排到最前**：组里其他人要复用它的名字与目录，顺序不能靠运气
        // （位图与 RAW 谁先扫到不确定，RAW 先扫到时若不重排就会拿自己的名字当基准）
        let mut order = members.clone();
        if let Some(pos) = order.iter().position(|idx| *idx == leader) {
            order.swap(0, pos);
        }

        // 组员的公共信息：名字（组长的渲染结果）与目录（组长的目标根）。
        //
        // 续跑（`specs/import-dedup-fingerprint.md` §11.3）：组里**任一**成员上次已经落过位置
        // → 整组沿用那个名字与目录，因为「位图与同组 RAW 同名」是配对不变量，
        // 只让一个成员落回原名、另一个重新编号就会把它打散。
        // 成员是 RAW 时它的落点在 `<组目录>/_RAW` 下 —— 组目录要折回上一层。
        let mut shared: Option<(String, String, Vec<Var>)> = None;
        if let Some((idx, target)) = order
            .iter()
            .find_map(|idx| adopt.get(&files[*idx].rel_path).map(|target| (*idx, target)))
        {
            let adopted = adopted_target(target);
            let dir = match (files[idx].role(), group.has_bitmap) {
                (Some(Role::Raw), true) => {
                    crate::store::assets::normalize_raw_dir(&adopted.dir).to_string()
                }
                _ => adopted.dir.clone(),
            };
            shared = Some((dir, adopted.stem.clone(), adopted.missing.clone()));
        }
        // 组长这次取走的号（整组都被跳过时还回去：跳过的文件不消耗序号）
        let mut consumed: Vec<(String, usize, u64)> = Vec::new();
        let mut plan_members: Vec<GroupMember> = Vec::new();

        for idx in order {
            let file = &files[idx];
            let missing = match &shared {
                Some((_, _, missing)) => missing.clone(),
                None => {
                    // 还没人替这一组占过位置 → 让组长的模版渲一次，全组复用它的名字
                    match render_target(file, tpl, opts, seq) {
                        Ok(rendered) => {
                            consumed = rendered.consumed.clone();
                            shared = Some((rendered.dir.clone(), rendered.stem.clone(), rendered.missing.clone()));
                            rendered.missing
                        }
                        Err(reason) => {
                            counts.failed += 1;
                            outcomes[idx] = Some(ItemOutcome::Failed(reason));
                            continue;
                        }
                    }
                }
            };
            let Some((dir, stem, _)) = shared.clone() else {
                continue;
            };

            // RAW 有同组位图时进 `_RAW/`；其余一律跟着组长的目录
            let target_dir = match (file.role(), group.has_bitmap) {
                (Some(Role::Raw), true) => join_rel(&[dir.as_str(), RAW_DIR]),
                _ => dir.clone(),
            };
            plan_members.push(GroupMember {
                idx,
                dir: target_dir,
                stem,
                ext: file.ext.clone(),
                missing,
            });
        }
        if plan_members.is_empty() {
            continue;
        }

        // 逐成员判「原名字这条路通不通」（占用 + 内容）
        let verdicts: Vec<Verdict> = plan_members
            .iter()
            .map(|member| {
                member_verdict(
                    &files[member.idx],
                    member,
                    &claimed,
                    reserved,
                    &context,
                    &mut source_fingerprints,
                    &mut backfilled,
                )
            })
            .collect();

        // 整组消解：任一要导入的成员冲突 → 整组共用一个后缀
        let suffix = if verdicts.iter().any(Verdict::is_conflict) {
            find_group_suffix(&plan_members, &verdicts, &claimed, reserved, &context)
        } else {
            None
        };

        let mut planned_here = 0usize;
        let mut skipped_here = 0usize;
        for (member, verdict) in plan_members.iter().zip(verdicts.iter()) {
            match verdict {
                Verdict::Skip { library_rel } => {
                    skipped_here += 1;
                    counts.skipped += 1;
                    counts.duplicates += 1;
                    outcomes[member.idx] = Some(ItemOutcome::Skipped(format!(
                        "同一张照片（内容一致）：库内 {library_rel}"
                    )));
                }
                Verdict::Free | Verdict::Conflict => {
                    let stem = match suffix {
                        Some(n) => collision_stem(&member.stem, n),
                        None => member.stem.clone(),
                    };
                    let mut target_rel = compose(&member.dir, &stem, &member.ext);
                    // 同一批里已经有别的成员落到这个名字上（同一组里出现同名成员 ——
                    // 只可能来自重复输入）→ 按既有重名规则往后让位，
                    // **绝不**让两个成员算出同一个路径。
                    if !is_path_free(&target_rel, &claimed, reserved, &context) {
                        for n in 1..=99_999u64 {
                            let candidate =
                                compose(&member.dir, &collision_stem(&member.stem, n), &member.ext);
                            if is_path_free(&candidate, &claimed, reserved, &context) {
                                target_rel = candidate;
                                break;
                            }
                        }
                    }
                    claimed.insert(fold(&target_rel));
                    if !dirs.contains(&member.dir) {
                        dirs.push(member.dir.clone());
                    }
                    if !member.missing.is_empty() {
                        counts.missing_meta += 1;
                    }
                    missing_by_item[member.idx] = member.missing.clone();
                    counts.planned += 1;
                    planned_here += 1;
                    outcomes[member.idx] = Some(ItemOutcome::Plan { target_rel });
                }
            }
        }

        // 整组一个都没导（全被内容判重跳过）→ 把这一组取走的号还回去。
        // 只跳过一部分时不还：还有成员在用那个名字（位图跳过、RAW 补号也落回它）。
        if planned_here == 0 && skipped_here > 0 {
            for (key_dir, width, before) in &consumed {
                seq.restore(key_dir, *width, *before);
            }
        }

        // 「整组完整导入」：组里每个成员都真的导了、而且没改名 ——
        // 边车（`.xmp`）采纳靠它把门（`specs/import-dedup-fingerprint.md` §9）。
        let complete = suffix.is_none()
            && group
                .members
                .iter()
                .all(|idx| matches!(outcomes[*idx], Some(ItemOutcome::Plan { .. })));
        for member in &plan_members {
            complete_by_item[member.idx] = complete;
        }
    }

    // ── 汇总 ───────────────────────────────────────────────
    let items: Vec<PlannedItem> = files
        .iter()
        .enumerate()
        .map(|(idx, file)| {
            let outcome = outcomes[idx]
                .clone()
                .unwrap_or_else(|| ItemOutcome::Failed("内部错误：这条没算出结果".to_string()));
            PlannedItem {
                index: idx,
                source_rel: file.rel_path.clone(),
                role: file.role(),
                missing: missing_by_item[idx].clone(),
                complete: complete_by_item[idx],
                outcome,
            }
        })
        .collect();

    dirs.sort();
    PlanResult {
        items,
        counts,
        target_dirs: dirs,
        backfilled,
    }
}

/// `RAW` 的分流目录名（`memory/FUNCTION-REPOSITORY.md` §4.1）。
const RAW_DIR: &str = "_RAW";

/// 这张源文件要不要跳。
fn skip_reason(file: &SourceFile, opts: &PlanOptions, known: &KnownSources) -> Option<Skip> {
    if file.role().is_none() {
        return Some(Skip::NotAPhoto);
    }
    if !opts.include_subdirs && !file.source_dir().is_empty() {
        return Some(Skip::SubdirsExcluded);
    }
    if opts.avoid_duplicates && known.contains(file) {
        return Some(Skip::Duplicate);
    }
    None
}

/// 组长渲染出来的东西。
struct RenderedTarget {
    /// 目标根目录（含 `photos/` 与源相对层级；**不含** `_RAW`）。
    dir: String,
    /// 文件名主体（含序号，不含扩展名）。
    stem: String,
    /// 缺值的模版变量。
    missing: Vec<Var>,
    /// 这次渲染取走的序号（`(计数器键目录, 宽度, 取号前的值)`）——
    /// 整组都被跳过时要原样还回去（`Sequences::restore`）。
    consumed: Vec<(String, usize, u64)>,
}

/// 从「上次留下的目标路径」反推目录与主干名（扩展名一律取自源文件，
/// 与 `render_target` 的口径一致：名字主体不含扩展名）。
fn adopted_target(target: &str) -> RenderedTarget {
    let (dir, name) = target.rsplit_once('/').unwrap_or(("", target));
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    RenderedTarget {
        dir: dir.to_string(),
        stem: stem.to_string(),
        missing: Vec::new(),
        // 沿用上次的名字：这次没有向计数器取号，也就没什么可还的
        consumed: Vec::new(),
    }
}

/// 把一张源文件渲染成目标目录 + 文件名主体（**只给组长用**）。
///
/// 分两步渲染：先用「序号全 0」的形状算出计数器键（序号若被写进目录部分会自指，
/// 所以键取零形态），再带着真值渲染一次拿最终结果。
fn render_target(
    file: &SourceFile,
    tpl: &Template,
    opts: &PlanOptions,
    seq: &mut Sequences,
) -> Result<RenderedTarget, String> {
    let shape = tpl.render(&render_ctx(file, template::SeqValues::NONE));
    let (shape_dir, shape_name) = shape.split_dir_name();
    if shape_name.is_empty() {
        return Err("模版没算出文件名".to_string());
    }
    if let Err(err) = template::check_output(&shape.text) {
        return Err(err.message.clone());
    }

    // **计数器键**用零形态的目录：序号若被写进目录部分，拿真值当键就会自指
    // （键取决于它的值、值又取决于键）。行为后果见 `sequence_in_the_directory_part_*` 那条测试。
    let key_dir = join_rel(&[
        opts.photos_dir.as_str(),
        shape_dir.unwrap_or(""),
        file.source_dir(),
    ]);

    // 每个宽度各取一个号（`memory/FUNCTION-REPOSITORY.md` §3.3：宽度不同就是不同的计数器）
    // 顺手记下取号前的值：整组都被跳过时要原样还回去（`Sequences::restore`）
    let mut consumed: Vec<(String, usize, u64)> = Vec::new();
    let values: Vec<(usize, u64)> = tpl
        .seq_widths()
        .into_iter()
        .map(|width| {
            let before = seq.last(&key_dir, width);
            let value = seq.next(&key_dir, width);
            consumed.push((key_dir.clone(), width, before));
            (width, value)
        })
        .collect();

    // 带真值再渲染一次 —— **目录部分要跟着走**（序号可能出现在目录里）
    let rendered = tpl.render(&render_ctx(file, template::SeqValues::new(&values)));
    let (rendered_dir, name) = rendered.split_dir_name();
    let dir = join_rel(&[
        opts.photos_dir.as_str(),
        rendered_dir.unwrap_or(""),
        file.source_dir(),
    ]);
    if let Err(err) = template::check_output(&join_rel(&[&dir, name])) {
        return Err(err.message.clone());
    }
    Ok(RenderedTarget {
        dir,
        stem: name.to_string(),
        missing: rendered.missing,
        consumed,
    })
}

/// 给一张源文件搭渲染上下文。
fn render_ctx<'a>(file: &'a SourceFile, seqs: template::SeqValues<'a>) -> RenderCtx<'a> {
    RenderCtx {
        taken_at: file.taken_at,
        stem: file.stem(),
        brand: file.brand.as_deref(),
        model: file.model.as_deref(),
        seqs,
    }
}

/// 组里一个成员的落点（第 2 遍里用）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct GroupMember {
    idx: usize,
    dir: String,
    stem: String,
    ext: String,
    missing: Vec<Var>,
}

/// 一个成员在**原（不带后缀）目标路径**上的判定。
#[derive(Debug, Clone, PartialEq, Eq)]
enum Verdict {
    /// 这条路通（盘上库里都没有，或者本来就是我们自己留下的）。
    Free,
    /// 路占着，且内容与库里那份一致 —— 跳过这一条。
    Skip { library_rel: String },
    /// 路占着，但内容不同 / 核对不了 —— 要改名。
    Conflict,
}

impl Verdict {
    fn is_conflict(&self) -> bool {
        matches!(self, Self::Conflict)
    }
}

/// 判定要用的一堆外部事实（打包一下，免得参数排到十几个）。
struct VerdictContext<'a> {
    fs: &'a dyn FsProbe,
    known_targets: &'a KnownTargets,
    content: &'a dyn ContentProbe,
    avoid_duplicates: bool,
}

/// 这条路径现在被谁占着（库里那一行优先，其次盘上那个文件）。
enum Occupant {
    /// 库里有行（带着库里记的大小与已有指纹）。
    Row(LibraryEntry),
    /// 只有盘上有文件：库里没登记（手动拷进库、或差分还没跑）—— 大小与指纹现读。
    DiskOnly { rel_path: String },
}

impl Occupant {
    fn rel_path(&self) -> &str {
        match self {
            Self::Row(entry) => &entry.rel_path,
            Self::DiskOnly { rel_path } => rel_path,
        }
    }

    fn size_bytes(&self, ctx: &VerdictContext<'_>) -> Option<u64> {
        match self {
            Self::Row(entry) => entry.size_bytes,
            Self::DiskOnly { rel_path } => ctx.content.library_size(rel_path),
        }
    }

    /// 库内那一份的内容指纹：库里存着就用存的，否则现读一次（读到的回填）。
    fn fingerprint(
        &self,
        ctx: &VerdictContext<'_>,
        backfilled: &mut Vec<(String, ContentFingerprint)>,
    ) -> Option<ContentFingerprint> {
        match self {
            Self::Row(entry) => {
                if let Some(fingerprint) = &entry.fingerprint {
                    return Some(fingerprint.clone());
                }
                let computed = ctx.content.library_fingerprint(&entry.rel_path)?;
                backfilled.push((entry.rel_path.clone(), computed.clone()));
                Some(computed)
            }
            // 盘上多出来的文件：库里没行可回填，只用于这次判定
            Self::DiskOnly { rel_path } => ctx.content.library_fingerprint(rel_path),
        }
    }
}

/// 这条路径现在被谁占着；`None` = 空着。
fn occupancy(rel_path: &str, ctx: &VerdictContext<'_>) -> Option<Occupant> {
    if let Some(entry) = ctx.known_targets.get(rel_path) {
        return Some(Occupant::Row(entry.clone()));
    }
    if ctx.fs.file_exists(rel_path) {
        return Some(Occupant::DiskOnly {
            rel_path: rel_path.to_string(),
        });
    }
    None
}

/// 判一个成员在**原名字**上的路通不通（`specs/import-dedup-fingerprint.md` §3.2）。
fn member_verdict(
    file: &SourceFile,
    member: &GroupMember,
    claimed: &HashSet<String>,
    reserved: &Reserved,
    ctx: &VerdictContext<'_>,
    source_cache: &mut HashMap<String, Option<ContentFingerprint>>,
    backfilled: &mut Vec<(String, ContentFingerprint)>,
) -> Verdict {
    let plain = compose(&member.dir, &member.stem, &member.ext);
    // 本批里已经有人占了这个名字（同一批两个源目录落到同一个名字）→ 改名。
    // 这条要放在 `reserved` 之前：`claimed` 是「这一批真实的落点」，优先级最高。
    if claimed.contains(&fold(&plain)) {
        return Verdict::Conflict;
    }
    // 我们自己留下的名字（续跑）：落回去，不算占用
    if reserved.contains(&plain) {
        return Verdict::Free;
    }
    let Some(occupant) = occupancy(&plain, ctx) else {
        return Verdict::Free;
    };
    // 关掉判重：不读指纹，直接重名让位
    if !ctx.avoid_duplicates {
        return Verdict::Conflict;
    }
    // 大小先筛：不同一定不是同一份（省掉绝大多数读盘）
    if occupant.size_bytes(ctx) != Some(file.size_bytes) {
        return Verdict::Conflict;
    }
    let Some(library) = occupant.fingerprint(ctx, backfilled) else {
        // 库内那份读不到（缺失 / 权限）→ 核对不了，宁可改名导入也不覆盖
        return Verdict::Conflict;
    };
    let source = source_cache
        .entry(file.rel_path.clone())
        .or_insert_with(|| ctx.content.source_fingerprint(file));
    match source {
        Some(source) if source.same_content(&library) => Verdict::Skip {
            library_rel: occupant.rel_path().to_string(),
        },
        _ => Verdict::Conflict,
    }
}

/// 给**整组**找最小可用后缀：组里每个要导入的成员都得空着（盘上库里都没有）。
///
/// 找不到（1–99999 全被占）时返回 `None`：调用方把不加后缀的名字交出去，
/// runner 用 `create_new` 写文件会以「目标已存在」失败 —— **绝不静默覆盖**。
fn find_group_suffix(
    members: &[GroupMember],
    verdicts: &[Verdict],
    claimed: &HashSet<String>,
    reserved: &Reserved,
    ctx: &VerdictContext<'_>,
) -> Option<u64> {
    for n in 1..=99_999u64 {
        let free = members
            .iter()
            .zip(verdicts)
            .filter(|(_, verdict)| !matches!(verdict, Verdict::Skip { .. }))
            .all(|(member, _)| {
                let candidate = compose(&member.dir, &collision_stem(&member.stem, n), &member.ext);
                is_path_free(&candidate, claimed, reserved, ctx)
            });
        if free {
            return Some(n);
        }
    }
    None
}

fn is_path_free(
    candidate: &str,
    claimed: &HashSet<String>,
    reserved: &Reserved,
    ctx: &VerdictContext<'_>,
) -> bool {
    if claimed.contains(&fold(candidate)) {
        return false;
    }
    // 自己留下的半成品算「可用」（续跑要落回同一个名字）—— 这条**优先于**库内占用：
    // 文件监听差分可能已经把我们自己写下的半成品登记进库了（2026-10-12 事故），
    // 那时库里有行不代表这个位置「是别人的」—— 执行器会认出「这条是我们自己的」直接补登记。
    if reserved.contains(candidate) {
        return true;
    }
    !ctx.known_targets.contains(candidate) && !ctx.fs.file_exists(candidate)
}

/// Shared no-overwrite naming convention for import and export.
pub fn collision_stem(stem: &str, index: u64) -> String {
    if index == 0 { stem.into() } else { format!("{stem}_{index:02}") }
}

fn compose(dir: &str, stem: &str, ext: &str) -> String {
    let name = if ext.is_empty() {
        stem.to_string()
    } else {
        format!("{stem}.{ext}")
    };
    join_rel(&[dir, &name])
}

/// 拼库内相对路径：跳过空段、去掉多余的 `/`。
fn join_rel(parts: &[&str]) -> String {
    let mut out = String::new();
    for part in parts {
        for segment in part.split('/') {
            let segment = segment.trim();
            if segment.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.push('/');
            }
            out.push_str(segment);
        }
    }
    out
}

/// 路径的比较形式（NFC + 折叠）：`AGENTS.md` §7.3 的跨平台规则。
fn fold(path: &str) -> String {
    PathForms::new(path).folded().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::template::parse as parse_template;
    use crate::media::kind;
    use std::cell::{Cell, RefCell};

    /// 2026-08-15T12:00:00Z
    const T2026_08_15: i64 = 1_786_795_200_000;
    /// 2026-08-16T12:00:00Z
    const T2026_08_16: i64 = 1_786_881_600_000;

    /// 内存里的「盘」：只记哪些库内相对路径已经存在。
    #[derive(Default)]
    struct FakeFs {
        existing: HashSet<String>,
    }

    impl FakeFs {
        fn with(paths: &[&str]) -> Self {
            Self {
                existing: paths.iter().map(|p| fold(p)).collect(),
            }
        }
    }

    impl FsProbe for FakeFs {
        fn file_exists(&self, rel_path: &str) -> bool {
            self.existing.contains(&fold(rel_path))
        }
    }

    /// 内存里的「指纹表」：源文件与库内文件各一张，并记下被问了几次
    /// —— 「没被占用就别读盘」这条靠调用计数验证。
    #[derive(Default)]
    struct FakeContent {
        source: RefCell<HashMap<String, ContentFingerprint>>,
        library: RefCell<HashMap<String, ContentFingerprint>>,
        source_calls: Cell<usize>,
        library_calls: Cell<usize>,
    }

    impl FakeContent {
        fn new() -> Self {
            Self::default()
        }

        /// 给一个源文件（按 `rel_path` 认）配上指纹。
        fn source(self, rel_path: &str, fp: ContentFingerprint) -> Self {
            self.source.borrow_mut().insert(rel_path.to_string(), fp);
            self
        }

        /// 给一个库内路径配上指纹（模拟「读盘算出来」）。
        fn library(self, rel_path: &str, fp: ContentFingerprint) -> Self {
            // 键用**折叠**形式：真实实现查库内文件也走折叠比较
            self.library.borrow_mut().insert(fold(rel_path), fp);
            self
        }

        fn source_calls(&self) -> usize {
            self.source_calls.get()
        }

        fn library_calls(&self) -> usize {
            self.library_calls.get()
        }
    }

    impl ContentProbe for FakeContent {
        fn source_fingerprint(&self, file: &SourceFile) -> Option<ContentFingerprint> {
            self.source_calls.set(self.source_calls.get() + 1);
            self.source.borrow().get(&file.rel_path).cloned()
        }

        fn library_fingerprint(&self, rel_path: &str) -> Option<ContentFingerprint> {
            self.library_calls.set(self.library_calls.get() + 1);
            self.library.borrow().get(&fold(rel_path)).cloned()
        }

        fn library_size(&self, rel_path: &str) -> Option<u64> {
            // 内存「盘」上的大小：测试里只要指纹对得上就当大小也一致（大小单独用 KnownTargets 验）
            self.library
                .borrow()
                .contains_key(&fold(rel_path))
                .then_some(1000)
        }
    }

    fn fp(hex: &str) -> ContentFingerprint {
        ContentFingerprint::new(
            crate::media::fingerprint::HashKind::Full,
            hex.to_string(),
        )
    }

    fn file(rel: &str, kind: MediaKind, taken_at: Option<i64>) -> SourceFile {
        let file_name = rel.rsplit('/').next().unwrap_or(rel).to_string();
        // 与 `from_scanned` 一样：用**原始大小写**的扩展名（曾在这里踩过坑：
        // 测试替身用了 `kind::extension`（小写），于是「扩展名原样」这条规则被假绿了很久）
        let ext = original_extension(&file_name);
        SourceFile {
            abs_path: std::path::PathBuf::from(format!("/src/{rel}")),
            rel_path: rel.to_string(),
            stem_folded: kind::stem_folded(&file_name),
            file_name,
            ext,
            kind,
            size_bytes: 1000,
            mtime_ms: Some(1_700_000_000_000),
            created_ms: None,
            identity: None,
            taken_at,
            brand: None,
            model: None,
        }
    }

    fn bitmap(rel: &str) -> SourceFile {
        file(rel, MediaKind::Image, Some(T2026_08_15))
    }

    fn raw(rel: &str) -> SourceFile {
        file(rel, MediaKind::Raw, Some(T2026_08_15))
    }

    fn plan_with(files: &[SourceFile], template: &str) -> PlanResult {
        plan_default(files, template, &PlanOptions::default(), &FakeFs::default())
    }

    fn plan_default(
        files: &[SourceFile],
        template: &str,
        opts: &PlanOptions,
        fs: &FakeFs,
    ) -> PlanResult {
        plan_full(files, template, opts, fs, &KnownSources::new())
    }

    fn plan_full(
        files: &[SourceFile],
        template: &str,
        opts: &PlanOptions,
        fs: &FakeFs,
        known: &KnownSources,
    ) -> PlanResult {
        let tpl = parse_template(template).expect("模版应当能解析");
        let mut seq = Sequences::new();
        plan(
            files,
            &tpl,
            opts,
            fs,
            known,
            &KnownTargets::new(),
            &NoContent,
            &mut seq,
            &Reserved::new(),
            &Adopt::new(),
        )
    }

    /// 「库里已经占着这些目标路径」（盘上什么都没有）。
    fn plan_occupied(
        files: &[SourceFile],
        template: &str,
        fs: &FakeFs,
        occupied: &[&str],
    ) -> PlanResult {
        let tpl = parse_template(template).expect("模版应当能解析");
        let mut seq = Sequences::new();
        let mut targets = KnownTargets::new();
        for path in occupied {
            targets.insert(LibraryEntry {
                rel_path: (*path).to_string(),
                // 库里没记大小、没建立指纹的**老行**：判定只能走冲突（改名）
                size_bytes: None,
                fingerprint: None,
            });
        }
        plan(
            files,
            &tpl,
            &PlanOptions::default(),
            fs,
            &KnownSources::new(),
            &targets,
            &NoContent,
            &mut seq,
            &Reserved::new(),
            &Adopt::new(),
        )
    }

    fn plan_reserved(
        files: &[SourceFile],
        template: &str,
        fs: &FakeFs,
        reserved: &[&str],
    ) -> PlanResult {
        let tpl = parse_template(template).expect("模版应当能解析");
        let mut seq = Sequences::new();
        let mut set = Reserved::new();
        for path in reserved {
            set.insert(path);
        }
        plan(
            files,
            &tpl,
            &PlanOptions::default(),
            fs,
            &KnownSources::new(),
            &KnownTargets::new(),
            &NoContent,
            &mut seq,
            &set,
            &Adopt::new(),
        )
    }

    fn targets(result: &PlanResult) -> Vec<String> {
        result
            .items
            .iter()
            .map(|item| match &item.outcome {
                ItemOutcome::Plan { target_rel } => target_rel.clone(),
                ItemOutcome::Skipped(r) => format!("跳过：{r}"),
                ItemOutcome::Failed(r) => format!("失败：{r}"),
            })
            .collect()
    }

    const DEFAULT_TPL: &str = ":CYEAR-:CMONTH-:CDAY/MY:FILENAME";

    /* ══════════════════════════════════════════════════════════════
     * 目录透传（memory/FUNCTION-REPOSITORY.md §4.2 那张表，逐条对照）
     * ══════════════════════════════════════════════════════════════ */

    #[test]
    fn passthrough_keeps_the_source_shape_below_the_template_root() {
        let files = [
            bitmap("a.jpg"),
            bitmap("Japan/Kyoto/b.jpg"),
            bitmap("Japan/Tokyo/c.jpg"),
            bitmap("Japan/Kyoto/d.jpg"),
        ];
        let result = plan_with(&files, DEFAULT_TPL);
        assert_eq!(
            targets(&result),
            vec![
                "photos/2026-08-15/MYa.jpg",
                "photos/2026-08-15/Japan/Kyoto/MYb.jpg",
                "photos/2026-08-15/Japan/Tokyo/MYc.jpg",
                "photos/2026-08-15/Japan/Kyoto/MYd.jpg",
            ]
        );
        assert_eq!(result.counts.planned, 4);
    }

    #[test]
    fn template_is_evaluated_per_photo_so_dates_split_the_tree() {
        let mut d = bitmap("Japan/Kyoto/d.jpg");
        d.taken_at = Some(T2026_08_16);
        let files = [bitmap("Japan/Kyoto/b.jpg"), d];
        let result = plan_with(&files, DEFAULT_TPL);
        assert_eq!(
            targets(&result),
            vec![
                "photos/2026-08-15/Japan/Kyoto/MYb.jpg",
                "photos/2026-08-16/Japan/Kyoto/MYd.jpg",
            ],
            "模版按每张照片求值：不同日期 → 不同目标根，层级各自独立"
        );
    }

    #[test]
    fn template_is_not_applied_level_by_level() {
        // 绝不会出现 photos/2026-08-15/Japan/2026-08-15/Kyoto/
        let result = plan_with(&[bitmap("Japan/Kyoto/b.jpg")], DEFAULT_TPL);
        assert_eq!(
            targets(&result),
            vec!["photos/2026-08-15/Japan/Kyoto/MYb.jpg"]
        );
    }

    #[test]
    fn target_dirs_lists_only_directories_that_get_files() {
        let result = plan_with(
            &[bitmap("a.jpg"), bitmap("Japan/Kyoto/b.jpg")],
            DEFAULT_TPL,
        );
        assert_eq!(
            result.target_dirs,
            vec!["photos/2026-08-15", "photos/2026-08-15/Japan/Kyoto"],
            "空目录 / 只含文档的目录不在其中（它们不会被创建）"
        );
    }

    #[test]
    fn include_subdirs_off_skips_nested_files() {
        let opts = PlanOptions {
            include_subdirs: false,
            ..PlanOptions::default()
        };
        let files = [bitmap("a.jpg"), bitmap("sub/b.jpg")];
        let result = plan_default(&files, DEFAULT_TPL, &opts, &FakeFs::default());
        assert_eq!(
            targets(&result),
            vec![
                "photos/2026-08-15/MYa.jpg",
                "跳过：已关闭「包含子目录」，子目录里的文件不导入",
            ]
        );
        assert_eq!(result.counts.skipped, 1);
        assert_eq!(result.counts.planned, 1);
    }

    /* ══════════════════════════════════════════════════════════════
     * 位图 / RAW 分流（memory/FUNCTION-REPOSITORY.md §4.1）
     * ══════════════════════════════════════════════════════════════ */

    #[test]
    fn bitmap_and_raw_of_one_photo_share_the_name_and_split_into_raw_dir() {
        let files = [bitmap("P0001.png"), raw("P0001.ORF")];
        let result = plan_with(&files, DEFAULT_TPL);
        assert_eq!(
            targets(&result),
            vec![
                "photos/2026-08-15/MYP0001.png",
                "photos/2026-08-15/_RAW/MYP0001.ORF",
            ]
        );
        assert_eq!(result.counts.planned, 2, "一张照片两个文件，都算导入项");
    }

    #[test]
    fn raw_alone_stays_in_the_normal_directory() {
        let result = plan_with(&[raw("P0002.ORF")], DEFAULT_TPL);
        assert_eq!(
            targets(&result),
            vec!["photos/2026-08-15/MYP0002.ORF"],
            "只有 RAW → 正常进目标目录，不套 _RAW/"
        );
    }

    #[test]
    fn pairing_is_case_insensitive_and_extension_agnostic() {
        let files = [bitmap("IMG_1.JPG"), raw("img_1.orf")];
        let result = plan_with(&files, DEFAULT_TPL);
        assert_eq!(
            targets(&result),
            vec![
                "photos/2026-08-15/MYIMG_1.JPG",
                "photos/2026-08-15/_RAW/MYIMG_1.orf",
            ],
            "大小写与扩展名都不影响配对；扩展名原样透传"
        );
    }

    #[test]
    fn raw_is_placed_near_the_bitmap_even_in_subdirectories() {
        let files = [bitmap("Trip/a.jpg"), raw("Trip/a.CR3")];
        let result = plan_with(&files, DEFAULT_TPL);
        assert_eq!(
            targets(&result),
            vec![
                "photos/2026-08-15/Trip/MYa.jpg",
                "photos/2026-08-15/Trip/_RAW/MYa.CR3",
            ]
        );
    }

    #[test]
    fn raw_split_happens_under_raw_dir_and_counts_its_own_conflicts() {
        // 两张同主体的 RAW（不同扩展名）+ 一张位图：两个 RAW 都进 _RAW/，第二个加后缀
        let files = [
            bitmap("P0003.jpg"),
            raw("P0003.ORF"),
            raw("P0003.DNG"),
        ];
        let result = plan_with(&files, DEFAULT_TPL);
        assert_eq!(
            targets(&result),
            vec![
                "photos/2026-08-15/MYP0003.jpg",
                "photos/2026-08-15/_RAW/MYP0003.ORF",
                "photos/2026-08-15/_RAW/MYP0003.DNG",
            ],
            "两个 RAW 扩展名不同 → 文件名本来就不一样，不算冲突"
        );
    }

    /* ══════════════════════════════════════════════════════════════
     * 序号（memory/FUNCTION-REPOSITORY.md §3.3）
     * ══════════════════════════════════════════════════════════════ */

    #[test]
    fn sequence_counts_per_final_directory() {
        let files = [
            bitmap("Japan/Kyoto/a.jpg"),
            bitmap("Japan/Kyoto/b.jpg"),
            bitmap("Japan/Tokyo/c.jpg"),
        ];
        let result = plan_with(&files, ":CYEAR-:CMONTH-:CDAY/MY:SEQ000");
        assert_eq!(
            targets(&result),
            vec![
                "photos/2026-08-15/Japan/Kyoto/MY001.jpg",
                "photos/2026-08-15/Japan/Kyoto/MY002.jpg",
                "photos/2026-08-15/Japan/Tokyo/MY001.jpg",
            ],
            "序号在每个最终目标目录里独立计数"
        );
    }

    #[test]
    fn bitmap_and_raw_share_one_sequence_number() {
        let files = [bitmap("a.jpg"), raw("a.CR3")];
        let result = plan_with(&files, ":CYEAR-:CMONTH-:CDAY/MY:SEQ000");
        assert_eq!(
            targets(&result),
            vec![
                "photos/2026-08-15/MY001.jpg",
                "photos/2026-08-15/_RAW/MY001.CR3",
            ],
            "同组的 RAW 用位图那个号 —— 否则 _RAW/ 下的同名约定就断了"
        );
    }

    #[test]
    fn skipped_files_do_not_consume_sequence_numbers() {
        let mut known = KnownSources::new();
        known.insert_fallback("/src/a.jpg", 1000, Some(1_700_000_000_000));
        let files = [bitmap("a.jpg"), bitmap("b.jpg")];
        let result = plan_full(
            &files,
            ":CYEAR-:CMONTH-:CDAY/MY:SEQ000",
            &PlanOptions::default(),
            &FakeFs::default(),
            &known,
        );
        assert_eq!(
            targets(&result),
            vec![
                "跳过：已在库中（按文件身份判重）",
                "photos/2026-08-15/MY001.jpg",
            ],
            "重复导入不该把库里的编号往后推"
        );
    }

    #[test]
    fn sequence_wraps_around_after_filling_up() {
        let mut seq = Sequences::new();
        assert_eq!(seq.next("photos/x", 3), 1, "从 1 开始");
        seq.seed("photos/x", 3, 999);
        assert_eq!(seq.next("photos/x", 3), 1, "写满（999）后回绕到 1");
        assert_eq!(seq.last("photos/x", 3), 1);
        assert_eq!(seq.next("photos/x", 4), 1, "宽度不同 = 不同计数器");
        assert_eq!(seq.last("photos/x", 4), 1);
    }

    #[test]
    fn sequences_snapshot_skips_untouched_counters() {
        let mut seq = Sequences::new();
        seq.next("photos/a", 3);
        seq.next("photos/a", 3);
        seq.next("photos/b", 4);
        assert_eq!(
            seq.snapshot(),
            vec![("photos/a".to_string(), 3, 2), ("photos/b".to_string(), 4, 1)]
        );
    }

    /* ══════════════════════════════════════════════════════════════
     * 重名（memory/FUNCTION-REPOSITORY.md §3.4）
     * ══════════════════════════════════════════════════════════════ */

    #[test]
    fn conflict_with_a_file_on_disk_gets_a_two_digit_suffix() {
        let fs = FakeFs::with(&["photos/2026-08-15/MYa.jpg"]);
        let result = plan_default(&[bitmap("a.jpg")], DEFAULT_TPL, &PlanOptions::default(), &fs);
        assert_eq!(targets(&result), vec!["photos/2026-08-15/MYa_01.jpg"]);
    }

    #[test]
    fn suffix_keeps_growing_and_widens_after_ninety_nine() {
        let mut existing: Vec<String> = vec!["photos/2026-08-15/MYa.jpg".to_string()];
        for i in 1..=99 {
            existing.push(format!("photos/2026-08-15/MYa_{i:02}.jpg"));
        }
        let refs: Vec<&str> = existing.iter().map(String::as_str).collect();
        let fs = FakeFs::with(&refs);
        let result = plan_default(&[bitmap("a.jpg")], DEFAULT_TPL, &PlanOptions::default(), &fs);
        assert_eq!(
            targets(&result),
            vec!["photos/2026-08-15/MYa_100.jpg"],
            "_99 也被占之后继续涨位数"
        );
    }

    #[test]
    fn two_planned_files_never_collide_within_one_run() {
        // 两个源目录只差大小写（Windows 上它们是同一个目录）→ 目标路径折叠后相同，
        // 必须在**同一批内**就加上后缀，否则第二个会把第一个覆盖掉
        let files = [bitmap("Trip/a.jpg"), bitmap("trip/a.jpg")];
        let result = plan_with(&files, ":FILENAME");
        let names = targets(&result);
        assert_eq!(names, vec!["photos/Trip/a.jpg", "photos/trip/a_01.jpg"]);
        let mut unique = HashSet::new();
        for name in &names {
            assert!(unique.insert(fold(name)), "同一次导入里不能重名：{name}");
        }
    }

    #[test]
    fn same_stem_with_different_extensions_is_not_a_conflict() {
        // 同一组里 .png 与 .jpg 是两个不同的文件名 —— 不需要后缀
        let result = plan_with(
            &[bitmap("dir1/x.png"), bitmap("dir1/x.jpg")],
            "MY:SEQ000",
        );
        assert_eq!(
            targets(&result),
            vec!["photos/dir1/MY001.png", "photos/dir1/MY001.jpg"]
        );
    }

    #[test]
    fn suffix_goes_before_the_extension_and_keeps_its_original_case() {
        let fs = FakeFs::with(&["photos/2026-08-15/MYP0001.JPG"]);
        let result = plan_default(
            &[bitmap("P0001.JPG")],
            DEFAULT_TPL,
            &PlanOptions::default(),
            &fs,
        );
        assert_eq!(targets(&result), vec!["photos/2026-08-15/MYP0001_01.JPG"]);
    }

    #[test]
    fn reserved_paths_from_a_previous_run_are_reused_not_suffixed() {
        // 上次「复制完没登记」留下的文件：续跑要落回同一个名字，
        // 否则重名规则会给它加 _01 —— 同一张照片就有两份了
        let fs = FakeFs::with(&["photos/2026-08-15/MYa.jpg"]);
        let files = [bitmap("a.jpg")];
        assert_eq!(
            targets(&plan_default(&files, DEFAULT_TPL, &PlanOptions::default(), &fs)),
            vec!["photos/2026-08-15/MYa_01.jpg"],
            "正常情况下确实要避让已存在的文件"
        );
        assert_eq!(
            targets(&plan_reserved(
                &files,
                DEFAULT_TPL,
                &fs,
                &["photos/2026-08-15/MYa.jpg"]
            )),
            vec!["photos/2026-08-15/MYa.jpg"],
            "但它如果是我们自己留的，就该复用"
        );
    }

    #[test]
    fn raw_and_bitmap_are_not_treated_as_conflicts() {
        // 同一张照片的两个文件走不同扩展名（一个在 _RAW/），不算冲突
        let result = plan_with(&[bitmap("a.jpg"), raw("a.ORF")], DEFAULT_TPL);
        assert!(targets(&result).iter().all(|t| !t.contains("_01")));
    }

    #[test]
    fn original_extension_case_is_preserved() {
        assert_eq!(original_extension("P0001.JPG"), "JPG");
        assert_eq!(original_extension("a.Rw2"), "Rw2");
        assert_eq!(original_extension("noext"), "", "没有扩展名");
        assert_eq!(original_extension(".hidden"), "", "隐藏文件不算扩展名");
        assert_eq!(original_extension("trailing."), "", "末尾点不算扩展名");
        assert_eq!(original_extension(r"D:\\pics\\a.JPG"), "JPG", "带路径也只看最后一段");
    }

    #[test]
    fn files_without_extension_still_work() {
        let result = plan_with(&[bitmap("noext")], "MY:FILENAME");
        assert_eq!(targets(&result), vec!["photos/MYnoext"]);
    }

    /* ══════════════════════════════════════════════════════════════
     * 避免重复导入（memory/FUNCTION-REPOSITORY.md §4.3）
     * ══════════════════════════════════════════════════════════════ */

    #[test]
    fn known_identity_skips_the_file() {
        let mut known = KnownSources::new();
        let mut f = bitmap("a.jpg");
        f.identity = Some(FileId::new(7, [9u8; 16]));
        known.insert_identity(FileId::new(7, [9u8; 16]));
        let result = plan_full(
            &[f],
            DEFAULT_TPL,
            &PlanOptions::default(),
            &FakeFs::default(),
            &known,
        );
        assert_eq!(result.counts.duplicates, 1);
        assert_eq!(result.counts.planned, 0);
    }

    #[test]
    fn fallback_key_matches_path_size_and_mtime() {
        let mut known = KnownSources::new();
        known.insert_fallback("/src/sub/a.jpg", 1000, Some(1_700_000_000_000));
        let hit = bitmap("sub/a.jpg");
        let miss_size = SourceFile {
            size_bytes: 2000,
            ..bitmap("sub/a.jpg")
        };
        let miss_mtime = SourceFile {
            mtime_ms: Some(1_700_000_000_001),
            ..bitmap("sub/a.jpg")
        };
        let result = plan_full(
            &[hit, miss_size, miss_mtime],
            "MY:FILENAME",
            &PlanOptions::default(),
            &FakeFs::default(),
            &known,
        );
        assert_eq!(
            targets(&result),
            vec![
                "跳过：已在库中（按文件身份判重）",
                "photos/sub/MYa.jpg",
                "photos/sub/MYa_01.jpg",
            ],
            "兜底键要三项都对上才算重复；源目录层级照旧透传"
        );
    }

    #[test]
    fn fallback_key_is_case_folded() {
        let mut known = KnownSources::new();
        known.insert_fallback("/SRC/SUB/A.JPG", 1000, Some(1_700_000_000_000));
        let result = plan_full(
            &[bitmap("sub/a.jpg")],
            DEFAULT_TPL,
            &PlanOptions::default(),
            &FakeFs::default(),
            &known,
        );
        assert_eq!(result.counts.duplicates, 1, "Windows 上大小写不敏感");
    }

    #[test]
    fn avoid_duplicates_off_imports_everything_again() {
        let mut known = KnownSources::new();
        known.insert_fallback("/src/a.jpg", 1000, Some(1_700_000_000_000));
        let fs = FakeFs::with(&["photos/2026-08-15/MYa.jpg"]);
        let opts = PlanOptions {
            avoid_duplicates: false,
            ..PlanOptions::default()
        };
        let result = plan_full(&[bitmap("a.jpg")], DEFAULT_TPL, &opts, &fs, &known);
        assert_eq!(
            targets(&result),
            vec!["photos/2026-08-15/MYa_01.jpg"],
            "关掉判重=真的再导一份（撞名加后缀，绝不覆盖）"
        );
        assert_eq!(result.counts.duplicates, 0);
    }

    #[test]
    fn zero_identity_is_not_a_duplicate() {
        let mut known = KnownSources::new();
        known.insert_identity(FileId::ZERO);
        assert!(known.is_empty(), "零身份不该进集合");
        let result = plan_full(
            &[bitmap("a.jpg")],
            "MY:FILENAME",
            &PlanOptions::default(),
            &FakeFs::default(),
            &known,
        );
        assert_eq!(result.counts.planned, 1);
    }

    /* ══════════════════════════════════════════════════════════════
     * 模版缺值与落不了地的项
     * ══════════════════════════════════════════════════════════════ */

    #[test]
    fn missing_taken_at_is_planned_with_zeros_and_recorded() {
        let result = plan_with(&[file("a.jpg", MediaKind::Image, None)], DEFAULT_TPL);
        assert_eq!(targets(&result), vec!["photos/0000-00-00/MYa.jpg"]);
        assert_eq!(result.counts.missing_meta, 1);
        assert!(
            !result.items[0].missing.is_empty(),
            "缺值的变量要记下来（写进 import_items.reason 给用户看）"
        );
    }

    #[test]
    fn brand_and_model_come_from_each_photo() {
        let mut a = bitmap("a.jpg");
        a.brand = Some("NIKON".into());
        a.model = Some("Z7II".into());
        let b = bitmap("b.jpg");
        let result = plan_with(
            &[a, b],
            ":BRAND-:MODEL/:FILENAME",
            );
        assert_eq!(
            targets(&result),
            vec!["photos/NIKON-Z7II/a.jpg", "photos/Unknown-Unknown/b.jpg"]
        );
        assert_eq!(result.counts.missing_meta, 1);
    }

    #[test]
    fn illegal_rendered_name_fails_only_that_item() {
        // 相机型号里夹带了非法字符 —— 只有这一条失败，别的照常
        let mut bad = bitmap("bad.jpg");
        bad.model = Some("Z7<II>".into());
        let files = [bad, bitmap("good.jpg")];
        let result = plan_with(&files, ":MODEL/:FILENAME");
        assert_eq!(result.counts.failed, 1);
        assert_eq!(result.counts.planned, 1);
        assert!(matches!(result.items[0].outcome, ItemOutcome::Failed(_)));
        assert!(result.items[0].reason().is_some());
        assert_eq!(
            result.items[1].target_rel(),
            Some("photos/Unknown/good.jpg"),
            "另一条不受影响"
        );
    }

    #[test]
    fn too_long_component_fails_the_item() {
        let long = "x".repeat(300);
        let result = plan_with(&[bitmap(&format!("{long}.jpg"))], ":FILENAME");
        assert_eq!(result.counts.failed, 1);
        assert!(
            result.items[0].reason().unwrap_or_default().contains("太长"),
            "{:?}",
            result.items[0].reason()
        );
    }

    /* ══════════════════════════════════════════════════════════════
     * 边界与杂项
     * ══════════════════════════════════════════════════════════════ */

    #[test]
    fn empty_input_is_fine() {
        let result = plan_with(&[], DEFAULT_TPL);
        assert!(result.items.is_empty());
        assert!(result.target_dirs.is_empty());
        assert_eq!(result.counts, PlanCounts::default());
        assert_eq!(result.planned().count(), 0);
    }

    #[test]
    fn result_order_matches_input_order() {
        let files = [bitmap("b.jpg"), bitmap("a.jpg"), bitmap("c.jpg")];
        let result = plan_with(&files, ":FILENAME");
        let order: Vec<&str> = result
            .items
            .iter()
            .map(|i| i.source_rel.as_str())
            .collect();
        assert_eq!(order, vec!["b.jpg", "a.jpg", "c.jpg"], "顺序要保持源序");
        for (idx, item) in result.items.iter().enumerate() {
            assert_eq!(item.index, idx, "index 指回输入数组");
        }
    }

    #[test]
    fn chinese_and_emoji_names_survive_the_round_trip() {
        let result = plan_with(&[bitmap("照片/海边🌊.jpg")], DEFAULT_TPL);
        assert_eq!(
            targets(&result),
            vec!["photos/2026-08-15/照片/MY海边🌊.jpg"]
        );
    }

    #[test]
    fn non_photos_are_skipped_not_failed() {
        let result = plan_with(&[file("notes.txt", MediaKind::Other, None)], DEFAULT_TPL);
        assert_eq!(targets(&result), vec!["跳过：不是可导入的图片"]);
        assert_eq!(result.counts.skipped, 1);
        assert_eq!(result.counts.failed, 0);
    }

    #[test]
    fn planned_items_report_pending_status_for_the_db() {
        let result = plan_with(&[bitmap("a.jpg")], DEFAULT_TPL);
        assert_eq!(result.items[0].status_str(), "pending");
        assert!(result.items[0].reason().is_none());
        let skipped = plan_with(&[file("x.txt", MediaKind::Other, None)], DEFAULT_TPL);
        assert_eq!(skipped.items[0].status_str(), "skipped");
        assert!(skipped.items[0].reason().is_some());
    }

    #[test]
    fn join_rel_normalizes_separators() {
        assert_eq!(join_rel(&["photos", "2026-08-15", "a.jpg"]), "photos/2026-08-15/a.jpg");
        assert_eq!(join_rel(&["photos/", "/2026-08-15/", "a.jpg"]), "photos/2026-08-15/a.jpg");
        assert_eq!(join_rel(&["", "photos", "", "a.jpg"]), "photos/a.jpg");
        assert_eq!(join_rel(&[]), "");
    }

    #[test]
    fn sequence_in_the_directory_part_uses_the_zero_shape_as_the_counter_key() {
        // 序号写进目录部分时，计数器键只能取「序号位全 0」的形状 —— 否则它会自指。
        // 后果是：同一个零形态目录下的文件共用一个计数器，各自取号、落在同级的相邻目录里。
        let files = [bitmap("a.jpg"), bitmap("b.jpg")];
        let result = plan_with(&files, ":CYEAR/SEQ:SEQ000/:FILENAME");
        assert_eq!(
            targets(&result),
            vec!["photos/2026/SEQ001/a.jpg", "photos/2026/SEQ002/b.jpg"]
        );
    }

    /* ══════════════════════════════════════════════════════════════
     * 库内占用（`specs/import-dedup-fingerprint.md` §3 第 4 步）
     *
     * 2026-10-12 真机事故：库里有一行、盘上没有时，盘探针说「空位」→
     * 算出同一个路径 → 登记时撞 `UNIQUE(rel_path_folded)`，整批回滚。
     * ══════════════════════════════════════════════════════════════ */

    #[test]
    fn catalog_occupancy_forces_a_suffix_even_when_the_disk_is_free() {
        let result = plan_occupied(
            &[bitmap("a.jpg")],
            DEFAULT_TPL,
            &FakeFs::default(),
            &["photos/2026-08-15/MYa.jpg"],
        );
        assert_eq!(targets(&result), vec!["photos/2026-08-15/MYa_01.jpg"]);
    }

    #[test]
    fn catalog_and_disk_occupancy_both_advance_the_suffix() {
        // 库里占着 `_01`、盘上占着原名：两个都不能用，于是落到 `_02`。
        let fs = FakeFs::with(&["photos/2026-08-15/MYa.jpg"]);
        let result = plan_occupied(
            &[bitmap("a.jpg")],
            DEFAULT_TPL,
            &fs,
            &["photos/2026-08-15/MYa_01.jpg"],
        );
        assert_eq!(targets(&result), vec!["photos/2026-08-15/MYa_02.jpg"]);
    }

    #[test]
    fn our_own_leftovers_win_over_catalog_occupancy() {
        // 续跑场景：文件监听差分可能已经把我们自己的半成品登记进库了，
        // 但那个名字是**我们自己**留下的 —— 必须落回它，不能改成 `_01`。
        let files = [bitmap("a.jpg")];
        let tpl = parse_template(DEFAULT_TPL).expect("模版应当能解析");
        let mut seq = Sequences::new();
        let mut reserved = Reserved::new();
        reserved.insert("photos/2026-08-15/MYa.jpg");
        let mut targets_set = KnownTargets::new();
        targets_set.insert(LibraryEntry {
            rel_path: "photos/2026-08-15/MYa.jpg".to_string(),
            size_bytes: None,
            fingerprint: None,
        });
        let result = plan(
            &files,
            &tpl,
            &PlanOptions::default(),
            &FakeFs::default(),
            &KnownSources::new(),
            &targets_set,
            &NoContent,
            &mut seq,
            &reserved,
            &Adopt::new(),
        );
        assert_eq!(targets(&result), vec!["photos/2026-08-15/MYa.jpg"]);
    }

    #[test]
    fn catalog_occupancy_is_folded_like_everything_else() {
        // 库里的列是折叠的（小写）；规划出来的路径保留原文 —— 两边要能对上。
        let result = plan_occupied(
            &[bitmap("A.JPG")],
            DEFAULT_TPL,
            &FakeFs::default(),
            &["photos/2026-08-15/mya.jpg"],
        );
        assert_eq!(targets(&result), vec!["photos/2026-08-15/MYA_01.JPG"]);
    }

    /* ══════════════════════════════════════════════════════════════
     * 内容判重（`specs/import-dedup-fingerprint.md` §3.2–§3.3）
     *
     * 判据只在「目标路径被占」时才读盘：大小先筛，再比指纹。
     * 调用计数就是「别多读一个字节」那条红线的测据。
     * ══════════════════════════════════════════════════════════════ */

    /// 库里有这一行（大小/指纹由调用方给）。
    fn library_row(
        target: &str,
        size: Option<u64>,
        fingerprint: Option<ContentFingerprint>,
    ) -> LibraryEntry {
        LibraryEntry {
            rel_path: target.to_string(),
            size_bytes: size,
            fingerprint,
        }
    }

    fn plan_content(
        files: &[SourceFile],
        template: &str,
        fs: &FakeFs,
        targets_in: &KnownTargets,
        content: &dyn ContentProbe,
    ) -> PlanResult {
        let tpl = parse_template(template).expect("模版应当能解析");
        let mut seq = Sequences::new();
        plan(
            files,
            &tpl,
            &PlanOptions::default(),
            fs,
            &KnownSources::new(),
            targets_in,
            content,
            &mut seq,
            &Reserved::new(),
            &Adopt::new(),
        )
    }

    #[test]
    fn a_free_path_never_reads_a_fingerprint() {
        let content = FakeContent::new();
        let result = plan_content(
            &[bitmap("a.jpg")],
            DEFAULT_TPL,
            &FakeFs::default(),
            &KnownTargets::new(),
            &content,
        );
        assert_eq!(targets(&result), vec!["photos/2026-08-15/MYa.jpg"]);
        assert_eq!(content.source_calls(), 0, "没被占用就一个字节都不多读");
        assert_eq!(content.library_calls(), 0);
        assert!(result.items[0].complete, "干净导入 = 整组完整");
    }

    #[test]
    fn a_same_name_with_a_different_size_is_not_the_same_photo() {
        let mut targets_in = KnownTargets::new();
        targets_in.insert(library_row("photos/2026-08-15/MYa.jpg", Some(999), None));
        let content = FakeContent::new().library("photos/2026-08-15/MYa.jpg", fp("a"));
        let result = plan_content(
            &[bitmap("a.jpg")],
            DEFAULT_TPL,
            &FakeFs::default(),
            &targets_in,
            &content,
        );
        assert_eq!(
            targets(&result),
            vec!["photos/2026-08-15/MYa_01.jpg"],
            "大小不同 → 不是同一份，改名导入"
        );
        assert_eq!(content.library_calls(), 0, "大小先筛：不必读指纹");
        assert!(!result.items[0].complete, "改过名 = 不是完整导入（边车不采纳）");
    }

    #[test]
    fn the_same_content_at_the_same_name_is_skipped() {
        let mut targets_in = KnownTargets::new();
        targets_in.insert(library_row(
            "photos/2026-08-15/MYa.jpg",
            Some(1000),
            Some(fp("deadbeef")),
        ));
        let content = FakeContent::new().source("a.jpg", fp("deadbeef"));
        let result = plan_content(
            &[bitmap("a.jpg")],
            DEFAULT_TPL,
            &FakeFs::default(),
            &targets_in,
            &content,
        );
        let rendered = targets(&result);
        assert!(
            rendered[0].starts_with("跳过：同一张照片（内容一致）"),
            "{rendered:?}"
        );
        assert!(
            rendered[0].contains("photos/2026-08-15/MYa.jpg"),
            "原因要指出库内那一条：{rendered:?}"
        );
        assert_eq!(result.counts.skipped, 1);
        assert_eq!(result.counts.duplicates, 1);
        assert_eq!(result.counts.planned, 0);
        assert_eq!(content.library_calls(), 0, "库里存着指纹就不必读盘");
        assert_eq!(content.source_calls(), 1, "源文件读一次");
    }

    #[test]
    fn the_same_name_with_other_content_is_renamed() {
        let mut targets_in = KnownTargets::new();
        targets_in.insert(library_row(
            "photos/2026-08-15/MYa.jpg",
            Some(1000),
            Some(fp("aaaa")),
        ));
        let content = FakeContent::new().source("a.jpg", fp("bbbb"));
        let result = plan_content(
            &[bitmap("a.jpg")],
            DEFAULT_TPL,
            &FakeFs::default(),
            &targets_in,
            &content,
        );
        assert_eq!(targets(&result), vec!["photos/2026-08-15/MYa_01.jpg"]);
        assert_eq!(result.counts.duplicates, 0);
        assert!(!result.items[0].complete);
    }

    #[test]
    fn a_library_row_without_a_fingerprint_is_measured_and_handed_back() {
        let mut targets_in = KnownTargets::new();
        targets_in.insert(library_row("photos/2026-08-15/MYa.jpg", Some(1000), None));
        let content = FakeContent::new()
            .library("photos/2026-08-15/MYa.jpg", fp("same"))
            .source("a.jpg", fp("same"));
        let result = plan_content(
            &[bitmap("a.jpg")],
            DEFAULT_TPL,
            &FakeFs::default(),
            &targets_in,
            &content,
        );
        assert_eq!(content.library_calls(), 1, "库里没指纹就现算一次");
        assert_eq!(result.backfilled.len(), 1, "算出来的要带回去回填");
        assert_eq!(result.backfilled[0].0, "photos/2026-08-15/MYa.jpg");
        assert!(targets(&result)[0].starts_with("跳过："));
    }

    #[test]
    fn an_unreadable_library_copy_means_conflict_not_skip() {
        let mut targets_in = KnownTargets::new();
        targets_in.insert(library_row("photos/2026-08-15/MYa.jpg", Some(1000), None));
        // 库内那份读不到（fake 里没给）→ 核对不了
        let content = FakeContent::new().source("a.jpg", fp("same"));
        let result = plan_content(
            &[bitmap("a.jpg")],
            DEFAULT_TPL,
            &FakeFs::default(),
            &targets_in,
            &content,
        );
        assert_eq!(
            targets(&result),
            vec!["photos/2026-08-15/MYa_01.jpg"],
            "核对不了宁可改名，也不覆盖"
        );
        assert!(result.backfilled.is_empty());
    }

    #[test]
    fn avoiding_duplicates_off_never_reads_a_fingerprint() {
        let tpl = parse_template(DEFAULT_TPL).expect("模版应当能解析");
        let mut seq = Sequences::new();
        let mut targets_in = KnownTargets::new();
        targets_in.insert(library_row(
            "photos/2026-08-15/MYa.jpg",
            Some(1000),
            Some(fp("deadbeef")),
        ));
        let content = FakeContent::new().source("a.jpg", fp("deadbeef"));
        let opts = PlanOptions {
            avoid_duplicates: false,
            ..PlanOptions::default()
        };
        let result = plan(
            &[bitmap("a.jpg")],
            &tpl,
            &opts,
            &FakeFs::default(),
            &KnownSources::new(),
            &targets_in,
            &content,
            &mut seq,
            &Reserved::new(),
            &Adopt::new(),
        );
        assert_eq!(content.source_calls(), 0);
        assert_eq!(
            targets(&result),
            vec!["photos/2026-08-15/MYa_01.jpg"],
            "关掉判重：纯重名让位"
        );
    }

    #[test]
    fn a_conflicting_bitmap_renames_its_raw_sibling_too() {
        // 库里有同名 jpg（内容不同）、`_RAW/` 下那条路空着 —— 两个仍然要**一起**改名，
        // 否则 `A/MYP1_01.JPG` 与 `A/_RAW/MYP1.RW2` 名字对不上，配对就断了。
        let files = [bitmap("P1.JPG"), raw("P1.RW2")];
        let mut targets_in = KnownTargets::new();
        targets_in.insert(library_row(
            "photos/2026-08-15/MYP1.JPG",
            Some(1000),
            Some(fp("other")),
        ));
        let content = FakeContent::new().source("P1.JPG", fp("mine"));
        let result = plan_content(
            &files,
            DEFAULT_TPL,
            &FakeFs::default(),
            &targets_in,
            &content,
        );
        assert_eq!(
            targets(&result),
            vec![
                "photos/2026-08-15/MYP1_01.JPG",
                "photos/2026-08-15/_RAW/MYP1_01.RW2"
            ]
        );
        assert!(result.items.iter().all(|item| !item.complete));
    }

    #[test]
    fn a_skipped_bitmap_lets_its_raw_keep_the_plain_name() {
        // 库里的 jpg 与源 jpg 内容一致 → 跳过；raw 落原名，正好与库里那张配成对。
        let files = [bitmap("P1.JPG"), raw("P1.RW2")];
        let mut targets_in = KnownTargets::new();
        targets_in.insert(library_row(
            "photos/2026-08-15/MYP1.JPG",
            Some(1000),
            Some(fp("same")),
        ));
        let content = FakeContent::new()
            .source("P1.JPG", fp("same"))
            .source("P1.RW2", fp("raw"));
        let result = plan_content(
            &files,
            DEFAULT_TPL,
            &FakeFs::default(),
            &targets_in,
            &content,
        );
        assert_eq!(
            targets(&result),
            vec![
                "跳过：同一张照片（内容一致）：库内 photos/2026-08-15/MYP1.JPG",
                "photos/2026-08-15/_RAW/MYP1.RW2"
            ]
        );
        assert!(
            result.items.iter().all(|item| !item.complete),
            "一半跳过 = 不是完整导入"
        );
    }

    #[test]
    fn a_skipped_group_does_not_consume_a_sequence_number() {
        let files = [bitmap("P1.JPG"), raw("P1.RW2")];
        let mut targets_in = KnownTargets::new();
        targets_in.insert(library_row(
            "photos/2026-08-15/MY0001.JPG",
            Some(1000),
            Some(fp("same")),
        ));
        targets_in.insert(library_row(
            "photos/2026-08-15/_RAW/MY0001.RW2",
            Some(1000),
            Some(fp("raw")),
        ));
        let content = FakeContent::new()
            .source("P1.JPG", fp("same"))
            .source("P1.RW2", fp("raw"));
        let tpl = parse_template(":CYEAR-:CMONTH-:CDAY/MY:SEQ0000").expect("模版应当能解析");
        let mut seq = Sequences::new();
        let result = plan(
            &files,
            &tpl,
            &PlanOptions::default(),
            &FakeFs::default(),
            &KnownSources::new(),
            &targets_in,
            &content,
            &mut seq,
            &Reserved::new(),
            &Adopt::new(),
        );
        assert_eq!(result.counts.planned, 0, "{:?}", targets(&result));
        assert_eq!(
            seq.last("photos/2026-08-15", 4),
            0,
            "整组都被跳过 → 取走的号要还回去（跳过的文件不消耗序号）"
        );
    }

    #[test]
    fn an_adopted_raw_sibling_keeps_the_group_name() {
        // 续跑：上次只把 RAW 放到位（位图还没轮到）、而且计数器已经往后走了 ——
        // 整组仍要沿用那个名字，否则 `_RAW/MY0001.RW2` 会变成 `_RAW/MY0006.RW2`，
        // 与位图配对断掉（`specs/import-dedup-fingerprint.md` §11.3）。
        let files = [bitmap("P1.JPG"), raw("P1.RW2")];
        let tpl = parse_template(":CYEAR-:CMONTH-:CDAY/MY:SEQ0000").expect("模版应当能解析");
        let mut seq = Sequences::new();
        seq.seed("photos/2026-08-15", 4, 5);
        let mut adopt = Adopt::new();
        adopt.insert(
            "P1.RW2".to_string(),
            "photos/2026-08-15/_RAW/MY0001.RW2".to_string(),
        );
        let result = plan(
            &files,
            &tpl,
            &PlanOptions::default(),
            &FakeFs::default(),
            &KnownSources::new(),
            &KnownTargets::new(),
            &NoContent,
            &mut seq,
            &Reserved::new(),
            &adopt,
        );
        assert_eq!(
            targets(&result),
            vec![
                "photos/2026-08-15/MY0001.JPG",
                "photos/2026-08-15/_RAW/MY0001.RW2"
            ]
        );
        assert_eq!(
            seq.last("photos/2026-08-15", 4),
            5,
            "沿用上次的名字，这次不再取号"
        );
    }

    #[test]
    fn a_lone_raw_is_judged_in_the_parent_directory() {
        // 源里只有 RAW（没位图）→ 落 `A/`，判定也在 `A/` 上（不是 `A/_RAW/`）。
        let mut targets_in = KnownTargets::new();
        targets_in.insert(library_row(
            "photos/2026-08-15/MYP1.RW2",
            Some(1000),
            Some(fp("same")),
        ));
        let content = FakeContent::new().source("P1.RW2", fp("same"));
        let result = plan_content(
            &[raw("P1.RW2")],
            DEFAULT_TPL,
            &FakeFs::default(),
            &targets_in,
            &content,
        );
        assert!(
            targets(&result)[0].starts_with("跳过：同一张照片（内容一致）"),
            "{:?}",
            targets(&result)
        );
    }
}
