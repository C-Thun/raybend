//! 变更追踪：把「磁盘上有什么」与「库里记着什么」做差分。
//!
//! 这是「刷新库」的核心，也是**用户最不可原谅的失败**（照片被误删记录）的防线。
//! 因此本模块的纪律是：
//!
//! * **纯函数**：输入两组快照，输出一份变更计划；不碰数据库、不碰磁盘。
//!   落库由 `store::assets` 负责（走单写者批量事务）。
//! * **绝不产生「删除」**：磁盘上找不到的只是标 `missing_since`（`AGENTS.md` §6.4）。
//!   拔了移动硬盘、云盘离线、权限过期都会让文件「消失」，把记录删掉才是灾难。
//! * **身份优先于路径**：`(volume_serial, file_id)` 在改名/移动后不变
//!   （`AGENTS.md` §7.3），所以先按身份配对、再按路径、最后才用启发式。
//! * **启发式必须保守**：没有身份信息时（网络盘、部分 Linux FS），只有
//!   「同目录 + 同大小 + 时间戳接近 + 名字相似」四条同时满足才算重命名，
//!   且结果标记为 [`Evidence::Heuristic`] 供 UI 提示；宁可当成「一删一新」，
//!   也不要把两张不同的照片认成同一张。
//! * **结果确定**：同样输入必得同样输出（配对顺序、并列时的取舍都按稳定规则）。

use std::collections::HashMap;

use crate::media::kind::{self, MediaKind};
use crate::store::file_id::FileId;

/// 时间戳容差（毫秒）：不同文件系统的时间精度不同（FAT 2 秒、网络盘更粗），
/// 启发式匹配时允许这点偏差。
pub const MTIME_TOLERANCE_MS: i64 = 2_000;

/// 启发式配对要求文件名主体至少有这么多个字符。
///
/// 短名字（`a.jpg` / `IMG.jpg`）之间的编辑距离天然很小，拿相似度当证据会把
/// 不相干的照片认成同一张；长名字（`IMG_0001`）才具备区分力。
const MIN_STEM_CHARS_FOR_HEURISTIC: usize = 4;

/// 磁盘上的一个媒体文件（扫描结果 + 尝试读取的文件身份）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskFile {
    /// 相对扫描根的路径（原始大小写，`/` 分隔）。
    pub rel_path: String,
    /// NFC + 折叠后的路径（比较用）。
    pub rel_path_folded: String,
    pub kind: MediaKind,
    pub size_bytes: u64,
    pub mtime_ms: Option<i64>,
    /// 文件的**创建时间**（右栏「文件基础信息 → 创建日期」）。
    ///
    /// 不是所有文件系统都有出生时间（ext4 视内核/挂载参数而定）⇒ `None` 是常态，
    /// 显示端退回 `mtime_ms`，所以这里**不编造**值。
    pub created_ms: Option<i64>,
    /// 文件身份；读不到时为 `None`（网络盘、权限、非 NTFS/无 inode 的 FS）。
    pub identity: Option<FileId>,
}

impl DiskFile {
    /// 便捷构造：自己去算折叠路径与类型。
    #[must_use]
    pub fn new(rel_path: impl Into<String>, size_bytes: u64, mtime_ms: Option<i64>) -> Self {
        let rel_path = rel_path.into();
        let folded = crate::store::path_semantics::PathForms::new(&rel_path)
            .folded()
            .to_string();
        Self {
            rel_path_folded: folded,
            // kind_of_file 自己会剥掉目录部分（防御有人把相对路径直接传进来）
            kind: kind::kind_of_file(&rel_path),
            rel_path,
            size_bytes,
            created_ms: None,
            mtime_ms,
            identity: None,
        }
    }

    #[must_use]
    pub fn with_identity(mut self, id: FileId) -> Self {
        self.identity = Some(id);
        self
    }

    /// 所在目录（折叠形式，`""` = 根）。
    fn dir_folded(&self) -> &str {
        self.rel_path_folded.rsplit_once('/').map_or("", |(d, _)| d)
    }

    /// 去扩展名的主体（折叠形式）。
    fn stem_folded(&self) -> String {
        kind::stem_folded(&self.rel_path_folded)
    }
}

/// 库里的一条 `asset_files` 记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbFile {
    /// `asset_files.id`。
    pub row_id: i64,
    pub asset_id: i64,
    pub rel_path: String,
    pub rel_path_folded: String,
    pub size_bytes: Option<u64>,
    pub mtime_ms: Option<i64>,
    /// 文件创建时间（catalog v4 加的列；老库为 NULL）
    pub file_created_ms: Option<i64>,
    pub identity: Option<FileId>,
    /// 已经标记过缺失（`missing_since IS NOT NULL`）。
    pub missing: bool,
}

/// 配对依据：可信度不同，UI 该给出不同措辞。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    /// 文件身份相同（`volume_serial` + `file_id`）—— 改名/移动后身份不变，**确定**。
    Identity,
    /// 路径相同（折叠后）—— 同一位置的文件，**确定**。
    Path,
    /// 同目录 + 同大小 + 时间戳接近 + 名字相似 —— **推测**。
    Heuristic,
}

/// 配对成功的一对。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Matched {
    pub row_id: i64,
    pub asset_id: i64,
    /// `disk` 数组里的下标。
    pub disk_index: usize,
    pub evidence: Evidence,
}

/// 被判为「同一文件换了路径」的一对。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Renamed {
    pub row_id: i64,
    pub asset_id: i64,
    pub disk_index: usize,
    pub evidence: Evidence,
    /// 原来的路径（写日志/给用户看：「A 改名成了 B」）。
    pub old_path: String,
    /// 新路径。
    pub new_path: String,
}

/// 一份变更计划（**只描述要做什么，不改任何东西**）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffPlan {
    /// 库里没有的磁盘文件（下标指向 `disk` 数组）—— 需要新建资产。
    pub new_files: Vec<usize>,
    /// 同一身份/推测同一文件但路径变了 —— 需要更新路径（不做删除+插入）。
    pub renamed: Vec<Renamed>,
    /// 路径没变、但大小或时间变了 —— 内容可能被改过（重新生成缩略图时要看它）。
    pub modified: Vec<Matched>,
    /// 之前标记缺失、现在又找到了 —— 需要清掉 `missing_since`。
    pub returned: Vec<Matched>,
    /// 路径与内容都没变。
    pub unchanged: Vec<Matched>,
    /// **新发现**缺失：磁盘上找不到，需要标 `missing_since`。
    pub missing: Vec<i64>,
    /// 早就标记缺失、这次仍然没找到（不变）。
    pub still_missing: Vec<i64>,
}

impl DiffPlan {
    /// 有没有需要落库的变化（`still_missing` / `unchanged` 不算）。
    #[must_use]
    pub fn has_changes(&self) -> bool {
        !(self.new_files.is_empty()
            && self.renamed.is_empty()
            && self.modified.is_empty()
            && self.returned.is_empty()
            && self.missing.is_empty())
    }

    /// 变化条数（给用户看的一句话：「发现 N 处变化」）。
    #[must_use]
    pub fn change_count(&self) -> usize {
        self.new_files.len()
            + self.renamed.len()
            + self.modified.len()
            + self.returned.len()
            + self.missing.len()
    }
}

/// 计算变更计划。
///
/// `disk` 与 `db` 的顺序不影响结果（内部按折叠路径/身份建表，配对结果按 row_id
/// 与 disk 下标排序输出）。
#[must_use]
pub fn diff(disk: &[DiskFile], db: &[DbFile]) -> DiffPlan {
    let mut plan = DiffPlan::default();
    let mut db_taken = vec![false; db.len()];
    let mut disk_taken = vec![false; disk.len()];

    // ── 索引：身份 → db 行 / disk 文件；折叠路径 → 同理 ──
    let mut db_by_identity: HashMap<(u64, [u8; 16]), usize> = HashMap::new();
    let mut db_by_path: HashMap<&str, usize> = HashMap::new();
    for (i, f) in db.iter().enumerate() {
        if let Some(id) = &f.identity
            && !id.is_zero()
        {
            db_by_identity.insert(identity_key(id), i);
        }
        db_by_path.entry(f.rel_path_folded.as_str()).or_insert(i);
    }
    let mut disk_by_identity: HashMap<(u64, [u8; 16]), usize> = HashMap::new();
    let mut disk_by_path: HashMap<&str, usize> = HashMap::new();
    for (i, f) in disk.iter().enumerate() {
        if let Some(id) = &f.identity
            && !id.is_zero()
        {
            disk_by_identity.insert(identity_key(id), i);
        }
        disk_by_path.entry(f.rel_path_folded.as_str()).or_insert(i);
    }

    // ── 第一轮：身份配对（最强证据：改名/移动都认得出来）──
    for (i, f) in db.iter().enumerate() {
        let Some(id) = f.identity.as_ref().filter(|v| !v.is_zero()) else {
            continue;
        };
        let Some(&j) = disk_by_identity.get(&identity_key(id)) else {
            continue;
        };
        if disk_taken[j] {
            continue;
        }
        db_taken[i] = true;
        disk_taken[j] = true;
        classify(&mut plan, db, disk, i, j, Evidence::Identity);
    }

    // ── 第二轮：路径配对（折叠后相同；跨大小写也算同一处）──
    for (i, f) in db.iter().enumerate() {
        if db_taken[i] {
            continue;
        }
        let Some(&j) = disk_by_path.get(f.rel_path_folded.as_str()) else {
            continue;
        };
        if disk_taken[j] {
            continue;
        }
        db_taken[i] = true;
        disk_taken[j] = true;
        classify(&mut plan, db, disk, i, j, Evidence::Path);
    }

    // ── 第三轮：启发式（没有身份信息时）──
    // 只对**两边都没配上**的做，且按 (目录, 大小) 分桶，避免 O(n×m)。
    let mut buckets: HashMap<(&str, u64), Vec<usize>> = HashMap::new();
    for (j, f) in disk.iter().enumerate() {
        if !disk_taken[j] {
            buckets
                .entry((f.dir_folded(), f.size_bytes))
                .or_default()
                .push(j);
        }
    }
    for (i, f) in db.iter().enumerate() {
        if db_taken[i] {
            continue;
        }
        let Some(size) = f.size_bytes else { continue };
        let dir = f.rel_path_folded.rsplit_once('/').map_or("", |(d, _)| d);
        let Some(candidates) = buckets.get(&(dir, size)) else {
            continue;
        };
        let db_stem = kind::stem_folded(&f.rel_path_folded);
        // 候选按 disk 下标排序，保证「并列时任取」也是确定的
        let hit = candidates.iter().copied().find(|&j| {
            !disk_taken[j]
                && timestamps_close(f.mtime_ms, disk[j].mtime_ms)
                && names_similar(&db_stem, &disk[j].stem_folded())
        });
        let Some(j) = hit else { continue };
        db_taken[i] = true;
        disk_taken[j] = true;
        classify(&mut plan, db, disk, i, j, Evidence::Heuristic);
    }

    // ── 收尾：没配上的两边 ──
    for (j, taken) in disk_taken.iter().enumerate() {
        if !taken {
            plan.new_files.push(j);
        }
    }
    for (i, f) in db.iter().enumerate() {
        if db_taken[i] {
            continue;
        }
        if f.missing {
            plan.still_missing.push(f.row_id);
        } else {
            plan.missing.push(f.row_id);
        }
    }

    plan.new_files.sort_unstable();
    plan.missing.sort_unstable();
    plan.still_missing.sort_unstable();
    plan
}

fn identity_key(id: &FileId) -> (u64, [u8; 16]) {
    (id.volume_serial, id.file_id)
}

/// 一对已配对成功：按「路径变没变 / 内容变没变 / 之前是不是缺失」分类。
fn classify(
    plan: &mut DiffPlan,
    db: &[DbFile],
    disk: &[DiskFile],
    i: usize,
    j: usize,
    evidence: Evidence,
) {
    let d = &db[i];
    let s = &disk[j];
    let matched = Matched {
        row_id: d.row_id,
        asset_id: d.asset_id,
        disk_index: j,
        evidence,
    };

    // 「路径真的变了」的判据用**规范化后**的形式，而**不是**原始串：
    //   * 大小写变化（`Photos/IMG.JPG` vs `photos/img.jpg`）规范化后仍不同
    //     → 算改名（用户看得见，展示路径要更新）；
    //   * 纯 NFC/NFD 差异（macOS 写 NFD）规范化后相同 → **不算**改名，
    //     否则每次扫描都会报一堆「改过」的噪声。
    let path_really_changed = normalized(&d.rel_path) != normalized(&s.rel_path);
    if path_really_changed {
        plan.renamed.push(Renamed {
            row_id: d.row_id,
            asset_id: d.asset_id,
            disk_index: j,
            evidence,
            old_path: d.rel_path.clone(),
            new_path: s.rel_path.clone(),
        });
        return;
    }

    if d.missing {
        // 之前标过缺失、现在又在原路径找到了
        plan.returned.push(matched);
        return;
    }

    // 同一处位置但**文件身份变了**：文件被换成了另一个（重新导出、覆盖保存）。
    // 大小与时间可能碰巧一模一样，所以这条必须单独判 —— 它意味着缩略图要重做。
    let identity_changed = match (&d.identity, &s.identity) {
        (Some(a), Some(b)) => !a.is_zero() && !b.is_zero() && a != b,
        _ => false,
    };

    let size_same = d.size_bytes == Some(s.size_bytes);
    let mtime_same = match (d.mtime_ms, s.mtime_ms) {
        (Some(a), Some(b)) => (a - b).abs() <= MTIME_TOLERANCE_MS,
        // 有一边读不到时间戳：不据此判定「改过」，免得把只读不出的盘判成天天在变
        _ => true,
    };
    if size_same && mtime_same && !identity_changed {
        plan.unchanged.push(matched);
    } else {
        plan.modified.push(matched);
    }
}

/// 路径的规范化形式（NFC + 分隔符统一），用于判断「路径是不是真的变了」。
fn normalized(p: &str) -> String {
    crate::store::path_semantics::PathForms::new(p)
        .normalized()
        .to_string()
}

/// 时间戳是否「接近」（缺一边时按「无法判断 → 不算冲突」处理）。
fn timestamps_close(a: Option<i64>, b: Option<i64>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => (x - y).abs() <= MTIME_TOLERANCE_MS,
        _ => true,
    }
}

/// 两个文件名主体是否「相似」——保守判据：
/// 折叠后相同，或长度差 ≤ 1 且编辑距离 ≤ 1（`IMG_0001` vs `IMG_0001-1` 这种不算，
/// 因为长度差已 > 1；`IMG_0001` vs `IMG_001` 也不算，编辑距离是 2）。
fn names_similar(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    // **短名字不用启发式**：`a.jpg` 与 `z.jpg` 在编辑距离上只差一处，
    // 但它们是两张毫不相干的照片。宁可当「一删一新」，也不能把两张照片认成一张。
    if a.chars().count() < MIN_STEM_CHARS_FOR_HEURISTIC
        || b.chars().count() < MIN_STEM_CHARS_FOR_HEURISTIC
    {
        return false;
    }
    let (la, lb) = (a.chars().count(), b.chars().count());
    if la.abs_diff(lb) > 1 {
        return false;
    }
    edit_distance_at_most_one(a, b)
}

/// 编辑距离是否 ≤ 1（只判「是不是」，无需算完整距离）。
fn edit_distance_at_most_one(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (n, m) = (a.len(), b.len());
    if n.abs_diff(m) > 1 {
        return false;
    }
    if n == m {
        // 替换一处
        return a.iter().zip(b).filter(|(x, y)| x != y).count() <= 1;
    }
    // 插入/删除一处
    let (short, long) = if n < m { (a, b) } else { (b, a) };
    let mut i = 0;
    let mut skipped = false;
    for &c in long {
        if i < short.len() && short[i] == c {
            i += 1;
        } else if skipped {
            return false;
        } else {
            skipped = true;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(vol: u64, n: u8) -> FileId {
        let mut bytes = [0u8; 16];
        bytes[0] = n;
        FileId {
            volume_serial: vol,
            file_id: bytes,
        }
    }

    fn disk(path: &str, size: u64, mtime: i64) -> DiskFile {
        DiskFile::new(path, size, Some(mtime))
    }

    fn db(row: i64, asset: i64, path: &str, size: u64, mtime: i64) -> DbFile {
        DbFile {
            row_id: row,
            asset_id: asset,
            rel_path: path.into(),
            rel_path_folded: crate::store::path_semantics::PathForms::new(path)
                .folded()
                .to_string(),
            size_bytes: Some(size),
            mtime_ms: Some(mtime),
            identity: None,
            missing: false,
        }
    }

    fn db_missing(row: i64, path: &str, size: u64) -> DbFile {
        let mut f = db(row, row, path, size, 1_000);
        f.missing = true;
        f
    }

    // ---------- 空输入 ----------

    #[test]
    fn empty_inputs_produce_no_changes() {
        let plan = diff(&[], &[]);
        assert!(!plan.has_changes());
        assert_eq!(plan.change_count(), 0);
    }

    #[test]
    fn all_disk_files_are_new_when_db_is_empty() {
        let files = [disk("a.jpg", 1, 10), disk("b.jpg", 2, 20)];
        let plan = diff(&files, &[]);
        assert_eq!(plan.new_files, vec![0, 1]);
        assert_eq!(plan.change_count(), 2);
    }

    #[test]
    fn all_db_files_are_missing_when_disk_is_empty() {
        let rows = [db(1, 1, "a.jpg", 1, 10), db(2, 2, "b.jpg", 2, 20)];
        let plan = diff(&[], &rows);
        assert_eq!(plan.missing, vec![1, 2], "必须标缺失而不是删除");
        assert!(plan.still_missing.is_empty());
    }

    // ---------- 路径配对 ----------

    #[test]
    fn identical_file_is_unchanged() {
        let files = [disk("photos/a.jpg", 100, 5_000)];
        let rows = [db(7, 3, "photos/a.jpg", 100, 5_000)];
        let plan = diff(&files, &rows);
        assert_eq!(plan.unchanged.len(), 1);
        assert_eq!(plan.unchanged[0].evidence, Evidence::Path);
        assert!(!plan.has_changes());
    }

    #[test]
    fn size_change_is_modified() {
        let files = [disk("a.jpg", 999, 5_000)];
        let rows = [db(7, 3, "a.jpg", 100, 5_000)];
        let plan = diff(&files, &rows);
        assert_eq!(plan.modified.len(), 1);
        assert_eq!(plan.modified[0].row_id, 7);
        assert!(plan.has_changes());
    }

    #[test]
    fn mtime_change_beyond_tolerance_is_modified() {
        let files = [disk("a.jpg", 100, 9_000)];
        let rows = [db(7, 3, "a.jpg", 100, 5_000)];
        assert_eq!(diff(&files, &rows).modified.len(), 1);
    }

    #[test]
    fn mtime_jitter_within_tolerance_is_unchanged() {
        // 网络盘/FAT 的时间精度可能差一两秒，不该被判成「天天都在变」
        let files = [disk("a.jpg", 100, 6_000)];
        let rows = [db(7, 3, "a.jpg", 100, 5_000)];
        assert_eq!(diff(&files, &rows).unchanged.len(), 1);
    }

    #[test]
    fn missing_mtime_is_not_treated_as_change() {
        let mut d = disk("a.jpg", 100, 5_000);
        d.mtime_ms = None;
        let rows = [db(7, 3, "a.jpg", 100, 5_000)];
        assert_eq!(diff(&[d], &rows).unchanged.len(), 1);
    }

    #[test]
    fn case_difference_in_path_is_a_rename_not_a_new_file() {
        // Windows 大小写不敏感：折叠后同一条路径；但展示名要更新
        let files = [disk("Photos/IMG.JPG", 100, 5_000)];
        let rows = [db(7, 3, "photos/img.jpg", 100, 5_000)];
        let plan = diff(&files, &rows);
        assert!(plan.new_files.is_empty(), "不该当成新文件");
        assert_eq!(plan.renamed.len(), 1, "大小写变化要更新展示路径");
        assert_eq!(plan.renamed[0].old_path, "photos/img.jpg");
        assert_eq!(plan.renamed[0].new_path, "Photos/IMG.JPG");
    }

    // ---------- 身份配对 ----------

    #[test]
    fn same_identity_different_path_is_a_rename() {
        let files = [disk("photos/新名字.jpg", 100, 5_000).with_identity(id(1, 9))];
        let rows = [DbFile {
            identity: Some(id(1, 9)),
            ..db(7, 3, "photos/老名字.jpg", 100, 5_000)
        }];
        let plan = diff(&files, &rows);
        assert_eq!(plan.renamed.len(), 1);
        assert_eq!(plan.renamed[0].evidence, Evidence::Identity);
        assert_eq!(plan.renamed[0].old_path, "photos/老名字.jpg");
        assert_eq!(plan.renamed[0].new_path, "photos/新名字.jpg");
        assert!(plan.new_files.is_empty(), "身份相同 → 不是新文件");
        assert!(plan.missing.is_empty(), "身份相同 → 不算缺失");
    }

    #[test]
    fn same_identity_in_another_directory_is_a_move() {
        let files = [disk("photos/2026/a.jpg", 100, 5_000).with_identity(id(2, 5))];
        let rows = [DbFile {
            identity: Some(id(2, 5)),
            ..db(7, 3, "photos/2025/a.jpg", 100, 5_000)
        }];
        let plan = diff(&files, &rows);
        assert_eq!(plan.renamed.len(), 1);
        assert_eq!(plan.renamed[0].new_path, "photos/2026/a.jpg");
    }

    #[test]
    fn identity_wins_over_path_even_when_size_changed() {
        // 同一个文件被替换过内容、又改了名：身份还在 → 认得出是同一张
        let files = [disk("b.jpg", 500, 9_000).with_identity(id(3, 1))];
        let rows = [DbFile {
            identity: Some(id(3, 1)),
            ..db(7, 3, "a.jpg", 100, 1_000)
        }];
        let plan = diff(&files, &rows);
        assert_eq!(plan.renamed.len(), 1, "身份优先：先是改名");
        assert!(plan.modified.is_empty(), "改名与改内容一次说清，不重复计");
    }

    #[test]
    fn different_identity_same_path_is_a_replacement() {
        // 同一个位置、不同 inode：文件被换成另一张 → 按「改过」处理（重新生成缩略图）
        let files = [disk("a.jpg", 100, 5_000).with_identity(id(4, 2))];
        let rows = [DbFile {
            identity: Some(id(4, 1)),
            ..db(7, 3, "a.jpg", 100, 5_000)
        }];
        let plan = diff(&files, &rows);
        assert!(plan.unchanged.is_empty());
        assert_eq!(plan.modified.len(), 1, "大小时间都一样也当改过（身份变了）");
    }

    #[test]
    fn zero_identity_is_treated_as_unknown() {
        // 有些文件系统会给出全 0 身份 —— 那是「读不到」，不能当有效身份来配对
        let files = [disk("a.jpg", 100, 5_000).with_identity(id(0, 0))];
        let rows = [DbFile {
            identity: Some(id(0, 0)),
            ..db(7, 3, "z.jpg", 100, 5_000)
        }];
        let plan = diff(&files, &rows);
        assert_eq!(plan.new_files, vec![0]);
        assert_eq!(plan.missing, vec![7]);
    }

    // ---------- 启发式配对 ----------

    #[test]
    fn heuristic_matches_renamed_file_without_identity() {
        let files = [disk("photos/IMG_0001.jpg", 100, 5_000)];
        let rows = [db(7, 3, "photos/IMG_0002.jpg", 100, 5_000)];
        let plan = diff(&files, &rows);
        assert_eq!(plan.renamed.len(), 1);
        assert_eq!(plan.renamed[0].evidence, Evidence::Heuristic);
    }

    #[test]
    fn heuristic_requires_same_directory() {
        let files = [disk("photos/b/x.jpg", 100, 5_000)];
        let rows = [db(7, 3, "photos/a/x.jpg", 100, 5_000)];
        let plan = diff(&files, &rows);
        assert!(plan.renamed.is_empty(), "跨目录相似名字不该乱配对");
        assert_eq!(plan.new_files, vec![0]);
        assert_eq!(plan.missing, vec![7]);
    }

    #[test]
    fn heuristic_requires_same_size() {
        let files = [disk("photos/img1.jpg", 101, 5_000)];
        let rows = [db(7, 3, "photos/img2.jpg", 100, 5_000)];
        assert!(diff(&files, &rows).renamed.is_empty());
    }

    #[test]
    fn heuristic_requires_close_mtime() {
        let files = [disk("photos/img1.jpg", 100, 50_000)];
        let rows = [db(7, 3, "photos/img2.jpg", 100, 5_000)];
        assert!(diff(&files, &rows).renamed.is_empty());
    }

    #[test]
    fn heuristic_requires_similar_name() {
        let files = [disk("photos/vacation.jpg", 100, 5_000)];
        let rows = [db(7, 3, "photos/portrait.jpg", 100, 5_000)];
        assert!(
            diff(&files, &rows).renamed.is_empty(),
            "名字差太多 → 宁可当一删一新"
        );
    }

    #[test]
    fn pair_before_rename_is_not_a_rename() {
        // 同一次扫描里文件从 a.jpg 变 b.jpg、又出现新的 a.jpg：
        // b.jpg 是真的新文件，a.jpg 仍是原文件 → 不该把 b 认成 a 的改名
        let files = [disk("a.jpg", 100, 5_000), disk("b.jpg", 100, 5_000)];
        let rows = [db(7, 3, "a.jpg", 100, 5_000)];
        let plan = diff(&files, &rows);
        assert_eq!(plan.unchanged.len(), 1, "a.jpg 照旧");
        assert_eq!(plan.new_files, vec![1], "b.jpg 是新文件");
        assert!(plan.renamed.is_empty());
    }

    #[test]
    fn names_similar_edge_cases() {
        assert!(names_similar("img_0001", "img_0001"));
        assert!(names_similar("img_0001", "img_0002"), "一字之差");
        assert!(names_similar("img0001", "img_0001"), "插入一个字符");
        // 注意：`img_0001` vs `img_001` 只差一次插入，**算相似**（别凭直觉当不同）
        assert!(names_similar("img_0001", "img_001"));
        assert!(!names_similar("img_0001", "img_9999"), "差四处");
        assert!(!names_similar("a", "abc"));
        assert!(!names_similar("", "ab"));
        assert!(
            !names_similar("a", "z"),
            "短名字一律不配 —— 编辑距离小但毫无关联"
        );
        assert!(!names_similar("", "a"));
        assert!(names_similar("照片游记", "照片游记"), "中文同样适用");
        assert!(names_similar("照片游记1", "照片游记2"));
    }

    // ---------- 缺失与回归 ----------

    #[test]
    fn newly_missing_is_separated_from_still_missing() {
        let rows = [
            db(1, 1, "a.jpg", 10, 1),
            db_missing(2, "b.jpg", 20),
            db_missing(3, "c.jpg", 30),
        ];
        let plan = diff(&[], &rows);
        assert_eq!(plan.missing, vec![1], "只有 1 是新发现的缺失");
        assert_eq!(plan.still_missing, vec![2, 3], "早就缺失的保持原样");
        assert_eq!(plan.change_count(), 1, "没变化的不算进「变化数」");
    }

    #[test]
    fn reappearing_file_is_returned() {
        let files = [disk("b.jpg", 20, 1_000)];
        let rows = [db_missing(2, "b.jpg", 20)];
        let plan = diff(&files, &rows);
        assert_eq!(plan.returned.len(), 1);
        assert_eq!(plan.returned[0].row_id, 2);
        assert_eq!(plan.returned[0].evidence, Evidence::Path);
        assert!(plan.missing.is_empty());
        assert!(plan.unchanged.is_empty(), "回归不重复算进 unchanged");
    }

    #[test]
    fn returned_file_that_also_moved_is_a_rename() {
        let files = [disk("photos/new.jpg", 20, 1_000).with_identity(id(1, 1))];
        let rows = [DbFile {
            missing: true,
            identity: Some(id(1, 1)),
            ..db(2, 2, "photos/old.jpg", 20, 1_000)
        }];
        let plan = diff(&files, &rows);
        assert_eq!(plan.renamed.len(), 1, "路径变了优先报改名");
        assert!(plan.returned.is_empty());
    }

    // ---------- 规模与确定性 ----------

    #[test]
    fn identical_inputs_always_give_identical_plans() {
        let files: Vec<DiskFile> = (0..50)
            .map(|i| disk(&format!("p/{i}.jpg"), i, 100))
            .collect();
        let rows: Vec<DbFile> = (0..50)
            .map(|i| db(i as i64 + 1, 1, &format!("p/{}.jpg", i - 1), 1, 100))
            .collect();
        let a = diff(&files, &rows);
        let b = diff(&files, &rows);
        assert_eq!(a, b);
    }

    #[test]
    fn handles_a_large_directory_without_blowing_up() {
        // 一万个文件的差分应当瞬间完成（索引是 HashMap，配对是线性的）
        let files: Vec<DiskFile> = (0..10_000)
            .map(|i| disk(&format!("p/{i:05}.jpg"), 1000 + i, 5_000))
            .collect();
        let rows: Vec<DbFile> = (0..10_000)
            .map(|i| {
                db(
                    i as i64,
                    i as i64,
                    &format!("p/{i:05}.jpg"),
                    1000 + i,
                    5_000,
                )
            })
            .collect();
        let plan = diff(&files, &rows);
        assert_eq!(plan.unchanged.len(), 10_000);
        assert!(!plan.has_changes());
    }

    #[test]
    fn pairs_bitmap_and_raw_as_two_db_rows_in_one_directory() {
        // 位图与 RAW 是同名一对：差分层面上是两条独立的文件记录
        let files = [
            disk("photos/IMG_0001.jpg", 100, 5_000).with_identity(id(1, 1)),
            disk("photos/IMG_0001.rw2", 900, 5_000).with_identity(id(1, 2)),
        ];
        let rows = [
            DbFile {
                identity: Some(id(1, 1)),
                ..db(10, 5, "photos/IMG_0001.jpg", 100, 5_000)
            },
            DbFile {
                identity: Some(id(1, 2)),
                ..db(11, 5, "photos/IMG_0001.rw2", 900, 5_000)
            },
        ];
        let plan = diff(&files, &rows);
        assert_eq!(plan.unchanged.len(), 2);
        assert!(!plan.has_changes());
        assert_eq!(
            plan.unchanged[0].asset_id, plan.unchanged[1].asset_id,
            "同一个资产"
        );
    }

    #[test]
    fn unicode_and_chinese_paths_work() {
        let files = [disk("照片/2026年9月/海边.JPG", 100, 5_000)];
        let rows = [db(7, 3, "照片/2026年9月/海边.JPG", 100, 5_000)];
        assert_eq!(diff(&files, &rows).unchanged.len(), 1);

        // NFC/NFD 变体也算同一路径（macOS 会存 NFD）
        let files = [disk("照片/café.jpg", 100, 5_000)];
        let rows = [db(7, 3, "照片/cafe\u{301}.jpg", 100, 5_000)];
        assert_eq!(
            diff(&files, &rows).unchanged.len(),
            1,
            "规范化差异不该算改名"
        );
    }
}
