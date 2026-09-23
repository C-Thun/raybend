//! 资产与文件记录的读写（`catalog.db` 侧）。
//!
//! 与 [`crate::media::diff`] 的分工：那边只算「要做什么」，这边负责**真的做**
//! （并且只在事务里做 —— 调用方通过 `CatalogDb::write` / `write_tx` 进来）。
//!
//! 关键约定：
//!
//! * **一条 `assets` = 一张照片**；它下面的 `asset_files` 是同一张照片的多个文件
//!   （`bitmap` 位图 + `raw` 原始数据，见 `REPOSITORY.md` §4.1）。
//!   新建时按「同目录 + 同名主体」把位图与 RAW 归到同一个资产下。
//! * **绝不删记录**：磁盘上找不到只标 `missing_since`（`AGENTS.md` §6.4）。
//! * **路径存两份**：`rel_path`（原始大小写，展示用）与 `rel_path_folded`
//!   （NFC + 折叠，唯一索引与比较用）—— 见 `store::path_semantics`。

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{Error, Result};
use crate::media::diff::{DiffPlan, DiskFile, Evidence};
use crate::media::kind::MediaKind;
use crate::store::file_id::FileId;
use crate::store::path_semantics::PathForms;

/// `asset_files` 的一行（差分与落库都用它）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRow {
    pub row_id: i64,
    pub asset_id: i64,
    pub rel_path: String,
    pub rel_path_folded: String,
    /// `bitmap` / `raw` / `sidecar`。
    pub role: String,
    pub ext: String,
    pub size_bytes: Option<u64>,
    pub mtime_ms: Option<i64>,
    /// 文件创建时间（列在 v4 加；老库为 NULL ⇒ 上层退回 `mtime_ms`）
    pub file_created_ms: Option<i64>,
    pub identity: Option<FileId>,
    /// `missing_since`：非空 = 磁盘上暂时找不到。
    pub missing: bool,
}

impl FileRow {
    /// 转成差分输入。
    #[must_use]
    pub fn to_db_file(&self) -> crate::media::diff::DbFile {
        crate::media::diff::DbFile {
            row_id: self.row_id,
            asset_id: self.asset_id,
            rel_path: self.rel_path.clone(),
            rel_path_folded: self.rel_path_folded.clone(),
            size_bytes: self.size_bytes,
            mtime_ms: self.mtime_ms,
            file_created_ms: self.file_created_ms,
            identity: self.identity,
            missing: self.missing,
        }
    }
}

/// 文件在该资产里的角色。
#[must_use]
pub fn role_of(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Raw => "raw",
        MediaKind::Image => "bitmap",
        MediaKind::Other => "sidecar",
    }
}

/// 读全库的文件记录（含已标缺失的）。
///
/// 十万张照片这个量级下一次读全部也就几十毫秒，换来的是差分算法可以完全在内存里
/// 跑；等真的到大库了再改成分批（`limit` 已经是现成的切分点）。
pub fn list_files(conn: &Connection) -> Result<Vec<FileRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, asset_id, rel_path, rel_path_folded, role, ext, size_bytes, mtime_ms, file_created_ms,
                volume_serial, file_id, missing_since
           FROM asset_files
          ORDER BY id",
    )?;
    let rows = stmt.query_map([], |r| {
        // SQLite 只有 i64：volume_serial 按位重解释存取（`as` 双向一一对应，不会丢信息）
        let vol: Option<i64> = r.get(9)?;
        let blob: Option<Vec<u8>> = r.get(10)?;
        let missing_since: Option<i64> = r.get(11)?;
        Ok(FileRow {
            row_id: r.get(0)?,
            asset_id: r.get(1)?,
            rel_path: r.get(2)?,
            rel_path_folded: r.get(3)?,
            role: r.get(4)?,
            ext: r.get(5)?,
            size_bytes: r.get::<_, Option<i64>>(6)?.map(|v| v.max(0) as u64),
            mtime_ms: r.get(7)?,
            file_created_ms: r.get(8)?,
            identity: match (vol, blob.as_deref()) {
                (Some(v), Some(b)) => FileId::from_blob(v as u64, b),
                _ => None,
            },
            missing: missing_since.is_some(),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 建一条 `assets`，返回新 id。除必填时间外全部留空（EXIF 在后续步骤填）。
pub fn insert_asset(conn: &Connection, now_ms: i64) -> Result<i64> {
    conn.execute(
        "INSERT INTO assets (imported_at, updated_at) VALUES (?1, ?1)",
        [now_ms],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 给一个资产插入一个文件记录。
///
/// `bitmap` 与 `raw` 抢同一个 `(rel_path_folded)` 唯一索引时会报错 —— 那是**好事**：
/// 说明同一条路径被重复插了，应该在差分阶段就被拦住。
pub fn insert_file(
    conn: &Connection,
    asset_id: i64,
    disk: &DiskFile,
    role: &str,
    now_ms: i64,
) -> Result<i64> {
    let ext = crate::media::kind::extension(&disk.rel_path).unwrap_or_default();
    let (vol, blob): (Option<i64>, Option<Vec<u8>>) = match &disk.identity {
        Some(id) if !id.is_zero() => (Some(id.volume_serial as i64), Some(id.file_id.to_vec())),
        _ => (None, None),
    };
    conn.execute(
        "INSERT INTO asset_files
            (asset_id, role, rel_path, rel_path_folded, ext, size_bytes, mtime_ms,
             file_created_ms, volume_serial, file_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
        params![
            asset_id,
            role,
            disk.rel_path,
            disk.rel_path_folded,
            ext,
            disk.size_bytes as i64,
            disk.mtime_ms,
            disk.created_ms,
            vol,
            blob,
            now_ms
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 补写一个文件的「来源」列（`REPOSITORY.md` §4.3 的判重靠它）。
///
/// * `source_path`：源文件的**绝对路径**（溯源 + 兜底判重）；
/// * `source_identity`：源文件身份 `(卷序列号, 文件 ID)`（读不到就传 `None`）。
///
/// 与 `volume_serial` / `file_id` 分开存：那两列是**库内副本**的身份，
/// 差分与缺失检测靠它，不能被源身份占用（`catalog_0003` 迁移里写了原因）。
pub fn set_source(
    conn: &Connection,
    rel_path_folded: &str,
    source_path: &str,
    source_identity: Option<FileId>,
    now_ms: i64,
) -> Result<usize> {
    let forms = PathForms::new(source_path);
    let (volume, blob): (Option<i64>, Option<Vec<u8>>) = match source_identity {
        Some(id) if !id.is_zero() => (Some(id.volume_serial as i64), Some(id.file_id.to_vec())),
        _ => (None, None),
    };
    let updated = conn.execute(
        "UPDATE asset_files
            SET source_path = ?2, source_path_folded = ?3,
                source_volume_serial = ?4, source_file_id = ?5, updated_at = ?6
          WHERE rel_path_folded = ?1",
        params![
            rel_path_folded,
            forms.raw(),
            forms.folded(),
            volume,
            blob,
            now_ms
        ],
    )?;
    Ok(updated)
}

/// 更新文件路径（改名/移动）。展示路径与折叠路径都要更新。
pub fn update_path(conn: &Connection, row_id: i64, new_path: &str, now_ms: i64) -> Result<()> {
    let forms = PathForms::new(new_path);
    let ext = crate::media::kind::extension(new_path).unwrap_or_default();
    conn.execute(
        "UPDATE asset_files
            SET rel_path = ?1, rel_path_folded = ?2, ext = ?3, updated_at = ?4
          WHERE id = ?5",
        params![new_path, forms.folded(), ext, now_ms, row_id],
    )?;
    Ok(())
}

/// 更新大小与时间戳（内容变了）。
pub fn update_stat(
    conn: &Connection,
    row_id: i64,
    size_bytes: u64,
    mtime_ms: Option<i64>,
    file_created_ms: Option<i64>,
    now_ms: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE asset_files
            SET size_bytes = ?1, mtime_ms = ?2,
                file_created_ms = COALESCE(?5, file_created_ms), updated_at = ?3
          WHERE id = ?4",
        params![size_bytes as i64, mtime_ms, now_ms, row_id, file_created_ms],
    )?;
    Ok(())
}

/// 标记缺失（幂等：已经有了就不动，保留最早发现的时间）。
pub fn mark_missing(conn: &Connection, row_id: i64, now_ms: i64) -> Result<()> {
    conn.execute(
        "UPDATE asset_files SET missing_since = ?1, updated_at = ?1
          WHERE id = ?2 AND missing_since IS NULL",
        params![now_ms, row_id],
    )?;
    Ok(())
}

/// 清掉缺失标记（文件又回来了）。
pub fn clear_missing(conn: &Connection, row_id: i64, now_ms: i64) -> Result<()> {
    conn.execute(
        "UPDATE asset_files SET missing_since = NULL, updated_at = ?1
          WHERE id = ?2 AND missing_since IS NOT NULL",
        params![now_ms, row_id],
    )?;
    Ok(())
}

/// 一次落库的结果统计。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ApplyOutcome {
    pub new_assets: usize,
    pub new_files: usize,
    pub renamed: usize,
    pub modified: usize,
    pub returned: usize,
    pub missing: usize,
    /// 推测配对（[`Evidence::Heuristic`]）的条数 —— UI 该给出「可能是重命名」的措辞。
    pub heuristic: usize,
}

impl ApplyOutcome {
    #[must_use]
    pub fn total(&self) -> usize {
        self.new_files + self.renamed + self.modified + self.returned + self.missing
    }
}

/// 把一份变更计划落库。
///
/// 调用方负责事务（`CatalogDb::write` 已经是一个事务；批量导入请用 `write_tx`）。
/// `disk` 必须与生成 `plan` 时用的是同一份快照。
pub fn apply_diff(
    conn: &Connection,
    plan: &DiffPlan,
    disk: &[DiskFile],
    now_ms: i64,
) -> Result<ApplyOutcome> {
    let mut out = ApplyOutcome::default();

    // ── 新文件：按 (目录, 名字主体) 分组，配对成一张照片 ──
    // 先看库里有没有已经存在的同组资产（例如 RAW 是后补进来的），有就挂上去。
    for group in group_new_files(plan, disk) {
        let asset_id = match find_asset_for_group(conn, &group.dir_folded, &group.stem_folded)? {
            Some(id) => id,
            None => {
                let id = insert_asset(conn, now_ms)?;
                out.new_assets += 1;
                id
            }
        };
        for &idx in &group.files {
            let f = &disk[idx];
            insert_file(conn, asset_id, f, role_of(f.kind), now_ms)?;
            out.new_files += 1;
        }
    }

    // ── 改名 / 移动 ──
    for r in &plan.renamed {
        update_path(conn, r.row_id, &r.new_path, now_ms)?;
        // 顺带把大小/时间刷新一遍：改名往往伴随替换（同一次操作里做掉）
        if let Some(f) = disk.get(r.disk_index) {
            update_stat(conn, r.row_id, f.size_bytes, f.mtime_ms, f.created_ms, now_ms)?;
        }
        out.renamed += 1;
        if r.evidence == Evidence::Heuristic {
            out.heuristic += 1;
        }
    }

    // ── 内容变了 ──
    for m in &plan.modified {
        if let Some(f) = disk.get(m.disk_index) {
            update_stat(conn, m.row_id, f.size_bytes, f.mtime_ms, f.created_ms, now_ms)?;
            out.modified += 1;
        }
    }

    // ── 回归 ──
    for m in &plan.returned {
        clear_missing(conn, m.row_id, now_ms)?;
        if let Some(f) = disk.get(m.disk_index) {
            update_stat(conn, m.row_id, f.size_bytes, f.mtime_ms, f.created_ms, now_ms)?;
        }
        out.returned += 1;
    }

    // ── 缺失 ──
    for row_id in &plan.missing {
        mark_missing(conn, *row_id, now_ms)?;
        out.missing += 1;
    }

    Ok(out)
}

/// 一批新文件按 (目录, 名字主体) 分好组。
struct NewGroup {
    dir_folded: String,
    stem_folded: String,
    files: Vec<usize>,
}

fn group_new_files(plan: &DiffPlan, disk: &[DiskFile]) -> Vec<NewGroup> {
    let mut groups: Vec<NewGroup> = Vec::new();
    // plan.new_files 已升序 → 相同分组内的文件顺序确定
    for &idx in &plan.new_files {
        let f = &disk[idx];
        let dir = f
            .rel_path_folded
            .rsplit_once('/')
            .map_or(String::new(), |(d, _)| d.to_string());
        let stem = crate::media::kind::stem_folded(&f.rel_path_folded);
        match groups
            .iter_mut()
            .find(|g| g.dir_folded == dir && g.stem_folded == stem)
        {
            Some(g) => g.files.push(idx),
            None => groups.push(NewGroup {
                dir_folded: dir,
                stem_folded: stem,
                files: vec![idx],
            }),
        }
    }
    groups
}

/// 找一个已经存在的资产：同目录下已经有同名主体的文件（位图 ↔ RAW 配对）。
///
/// **`_RAW/` 折算**（`REPOSITORY.md` §4.1）：RAW 落在 `<目录>/_RAW/` 里，
/// 与位图天生不同目录 —— 两边都折算掉最后那段 `_RAW` 再比，
/// 否则同一张照片会变成两条资产（网格里出现两个格子）。
/// 于是「先导 RAW 再导位图」与「先导位图再导 RAW」都能配上对。
pub fn find_asset_for_group(
    conn: &Connection,
    dir_folded: &str,
    stem_folded: &str,
) -> Result<Option<i64>> {
    // 主体的比对在 Rust 侧做：SQLite 里没有「去扩展名 + 折叠」的函数，
    // 而按目录前缀筛完的候选很少（一个目录里的照片数）。
    let prefix = if dir_folded.is_empty() {
        String::new()
    } else {
        format!("{dir_folded}/")
    };
    let like = format!("{prefix}%");
    let mut stmt = conn.prepare(
        "SELECT asset_id, rel_path_folded FROM asset_files
          WHERE rel_path_folded LIKE ?1 AND missing_since IS NULL
          ORDER BY id",
    )?;
    let mut rows = stmt.query([&like])?;
    while let Some(row) = rows.next()? {
        let asset_id: i64 = row.get(0)?;
        let path: String = row.get(1)?;
        // 只在同一层目录里比（LIKE 会带上更深的子目录）；
        // 候选行如果在 `_RAW/` 里，折算回上一层再比
        let (dir, name) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
        let dir = normalize_raw_dir(dir);
        if dir == normalize_raw_dir(dir_folded) && crate::media::kind::stem_folded(name) == stem_folded
        {
            return Ok(Some(asset_id));
        }
    }
    Ok(None)
}

/// 把「RAW 分流目录」折算掉：`photos/2026/_RAW` → `photos/2026`。
///
/// 只有**最后一段**是 `_RAW` 才折算（中间叫 `_RAW` 的目录是用户自己的命名）。
/// **大小写不敏感**：库里存的是折叠路径（`photos/2026/_raw`），
/// 而调用方手里可能是原文（`photos/2026/_RAW`）—— 两头都得认（这里踩过一次坑）。
fn normalize_raw_dir(dir: &str) -> &str {
    const SUFFIX: &str = "/_RAW";
    let start = dir.len().checked_sub(SUFFIX.len());
    match start {
        Some(at) if dir[at..].eq_ignore_ascii_case(SUFFIX) => &dir[..at],
        _ => dir,
    }
}

/// 把一个资产的 EXIF 元数据写进去。
///
/// **只写「从文件推出来」的字段**：拍摄时间/器材/曝光/尺寸/朝向/GPS。
/// 绝不碰用户自己写的（`author` / `description` / `rating` / 色标 / 标签）——
/// 重新读一遍 EXIF 不该把用户的劳动冲掉。
///
/// GPS 是「从文件推出来的」⇒ 在这里写（人类 2026-09-19 要在右栏看经纬度）。
/// 而 `country` / `province_state` / `city` / `sublocation` 是**人写的**（EXIF 里没有），
/// 所以不在这里，走 `marking::set_text`。
pub fn apply_exif(
    conn: &Connection,
    asset_id: i64,
    exif: &crate::media::exif::ExifData,
    taken: Option<crate::media::exif::TakenAt>,
    now_ms: i64,
) -> Result<()> {
    let (taken_at, taken_src, offset) = match taken {
        Some(t) => (
            Some(t.millis),
            Some(match t.source {
                crate::media::exif::TakenAtSource::Exif => "exif",
                crate::media::exif::TakenAtSource::Filename => "filename",
                crate::media::exif::TakenAtSource::FileMtime => "file_mtime",
                // 从同名姊妹文件（位图）继承来的（见 `media::pairing`）。
                // 列是自由文本（`catalog_0001_init.sql` 里的枚举只是注释），所以不需要迁移。
                crate::media::exif::TakenAtSource::Sibling => "sibling",
            }),
            t.offset_min,
        ),
        None => (None, None, None),
    };
    conn.execute(
        "UPDATE assets SET
            taken_at = ?1, taken_at_source = ?2, taken_at_offset_min = ?3,
            camera_make = ?4, camera_model = ?5, lens = ?6,
            focal_mm = ?7, f_number = ?8, exposure_ms = ?9, iso = ?10,
            width = ?11, height = ?12, orientation = ?13,
            gps_lat = COALESCE(?14, gps_lat), gps_lon = COALESCE(?15, gps_lon),
            updated_at = ?16
          WHERE id = ?17",
        params![
            taken_at,
            taken_src,
            offset,
            exif.camera_make,
            exif.camera_model,
            exif.lens,
            exif.focal_mm,
            exif.f_number,
            exif.exposure_ms,
            exif.iso,
            exif.width,
            exif.height,
            exif.orientation,
            exif.gps.map(|g| g.lat),
            exif.gps.map(|g| g.lon),
            now_ms,
            asset_id
        ],
    )?;
    Ok(())
}

/// 按**库内相对路径**反查资产 id（缩略图 / 看图那两条命令只有路径）。
///
/// 用**折叠**形式比（`rel_path_folded`）：大小写与 Unicode 规范化的差异不算两回事
/// （`AGENTS.md` §7.3）。找不到返回 `None`。
///
/// # Errors
/// 数据库读失败。
pub fn find_by_rel_path(conn: &Connection, rel_path: &str) -> Result<Option<i64>> {
    let folded = crate::store::path_semantics::PathForms::new(rel_path)
        .folded()
        .to_string();
    Ok(conn
        .query_row(
            "SELECT asset_id FROM asset_files WHERE rel_path_folded = ?1",
            [folded],
            |row| row.get::<_, i64>(0),
        )
        .optional()?)
}

/// 一个资产下有哪些文件（按角色）。
pub fn files_of_asset(conn: &Connection, asset_id: i64) -> Result<Vec<FileRow>> {
    Ok(list_files(conn)?
        .into_iter()
        .filter(|f| f.asset_id == asset_id)
        .collect())
}

/// 库里有几个资产、几个文件（含缺失的）。
pub fn counts(conn: &Connection) -> Result<(i64, i64)> {
    let assets: i64 = conn.query_row("SELECT count(*) FROM assets", [], |r| r.get(0))?;
    let files: i64 = conn.query_row("SELECT count(*) FROM asset_files", [], |r| r.get(0))?;
    Ok((assets, files))
}

/// 某个资产当前是否「在线」（至少有一个文件没被标记缺失）。
pub fn is_online(conn: &Connection, asset_id: i64) -> Result<bool> {
    let n: i64 = conn
        .query_row(
            "SELECT count(*) FROM asset_files WHERE asset_id = ?1 AND missing_since IS NULL",
            [asset_id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);
    Ok(n > 0)
}

/// 删除一个资产（连带其文件记录）。**本阶段界面上不提供**，只给维护/测试用。
pub fn delete_asset(conn: &Connection, asset_id: i64) -> Result<bool> {
    Ok(conn.execute("DELETE FROM assets WHERE id = ?1", [asset_id])? > 0)
}

/// 把一个已存在但不在任何资产下的文件挂到指定资产（给未来的手动整理留口）。
pub fn reattach_file(conn: &Connection, row_id: i64, asset_id: i64, now_ms: i64) -> Result<()> {
    let exists: Option<i64> = conn
        .query_row("SELECT id FROM assets WHERE id = ?1", [asset_id], |r| {
            r.get(0)
        })
        .optional()?;
    if exists.is_none() {
        return Err(Error::Unsupported(format!("资产 {asset_id} 不存在")));
    }
    conn.execute(
        "UPDATE asset_files SET asset_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![asset_id, now_ms, row_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::diff;
    use crate::store::migration::{self, DbKind};
    use crate::store::pragma;

    const T0: i64 = 1_789_516_800_000;

    fn catalog() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::Catalog, migration::Backups::none(), T0).unwrap();
        conn
    }

    fn disk(path: &str, size: u64, mtime: i64) -> DiskFile {
        DiskFile::new(path, size, Some(mtime))
    }

    fn id(n: u8) -> FileId {
        let mut bytes = [0u8; 16];
        bytes[0] = n;
        FileId {
            volume_serial: 7,
            file_id: bytes,
        }
    }

    /// 跑一轮「扫描 → 差分 → 落库」，返回落库结果。
    fn round(conn: &Connection, files: &[DiskFile], now: i64) -> ApplyOutcome {
        let rows: Vec<FileRow> = list_files(conn).unwrap();
        let db: Vec<_> = rows.iter().map(FileRow::to_db_file).collect();
        let plan = diff::diff(files, &db);
        apply_diff(conn, &plan, files, now).unwrap()
    }

    // ---------- 新建 ----------

    #[test]
    fn new_bitmap_and_raw_pair_up_into_one_asset() {
        let conn = catalog();
        let files = [disk("photos/a.jpg", 100, 5), disk("photos/a.rw2", 900, 5)];
        let out = round(&conn, &files, T0);

        assert_eq!(out.new_files, 2);
        assert_eq!(out.new_assets, 1, "同名位图与 RAW 是同一张照片");
        assert_eq!(counts(&conn).unwrap(), (1, 2));

        let row: (i64, i64) = conn
            .query_row(
                "SELECT count(DISTINCT asset_id), count(*) FROM asset_files",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(row, (1, 2));
        // 角色分得清
        let roles: Vec<String> = list_files(&conn)
            .unwrap()
            .into_iter()
            .map(|f| f.role)
            .collect();
        assert!(roles.contains(&"bitmap".to_string()));
        assert!(roles.contains(&"raw".to_string()));
    }

    #[test]
    fn lone_files_get_their_own_assets() {
        let conn = catalog();
        let files = [disk("photos/a.jpg", 100, 5), disk("photos/b.jpg", 200, 5)];
        let out = round(&conn, &files, T0);
        assert_eq!((out.new_assets, out.new_files), (2, 2));
    }

    #[test]
    fn same_name_in_different_directories_are_different_photos() {
        let conn = catalog();
        let files = [disk("2025/a.jpg", 100, 5), disk("2026/a.jpg", 100, 5)];
        let out = round(&conn, &files, T0);
        assert_eq!(out.new_assets, 2, "同名不同目录不算配对");
    }

    #[test]
    fn raw_arriving_later_joins_the_existing_asset() {
        let conn = catalog();
        let first = [disk("photos/a.jpg", 100, 5)];
        round(&conn, &first, T0);
        assert_eq!(counts(&conn).unwrap(), (1, 1));

        // 第二次扫描时 RAW 出现了
        let both = [disk("photos/a.jpg", 100, 5), disk("photos/a.rw2", 900, 5)];
        let out = round(&conn, &both, T0 + 1);
        assert_eq!(out.new_files, 1);
        assert_eq!(out.new_assets, 0, "应当挂到已存在的资产上");
        assert_eq!(counts(&conn).unwrap(), (1, 2));
    }

    #[test]
    fn second_round_with_no_changes_writes_nothing() {
        let conn = catalog();
        let files = [disk("a.jpg", 100, 5).with_identity(id(1))];
        round(&conn, &files, T0);
        let out = round(&conn, &files, T0 + 1);
        assert_eq!(out.total(), 0);
        assert_eq!(counts(&conn).unwrap(), (1, 1));
    }

    // ---------- 改名 / 移动 ----------

    #[test]
    fn identity_reveals_a_rename() {
        let conn = catalog();
        round(
            &conn,
            &[disk("photos/old.jpg", 100, 5).with_identity(id(3))],
            T0,
        );

        let out = round(
            &conn,
            &[disk("photos/new.jpg", 100, 5).with_identity(id(3))],
            T0 + 1,
        );
        assert_eq!((out.renamed, out.new_files, out.new_assets), (1, 0, 0));
        let f = &list_files(&conn).unwrap()[0];
        assert_eq!(f.rel_path, "photos/new.jpg");
        assert_eq!(f.rel_path_folded, "photos/new.jpg");
        assert!(!f.missing, "改名不是缺失");
        assert_eq!(counts(&conn).unwrap(), (1, 1), "不该多出记录");
    }

    #[test]
    fn rename_updates_the_folded_path_too() {
        let conn = catalog();
        round(
            &conn,
            &[disk("photos/Old.JPG", 100, 5).with_identity(id(4))],
            T0,
        );
        round(
            &conn,
            &[disk("其它目录/新名字.JPG", 100, 5).with_identity(id(4))],
            T0 + 1,
        );
        let f = &list_files(&conn).unwrap()[0];
        assert_eq!(f.rel_path, "其它目录/新名字.JPG");
        assert_eq!(
            f.rel_path_folded,
            PathForms::new("其它目录/新名字.JPG").folded()
        );
        assert_eq!(f.ext, "jpg", "扩展名要跟着更新");
    }

    // ---------- 内容变化 ----------

    #[test]
    fn modified_file_updates_stat_not_identity() {
        let conn = catalog();
        round(&conn, &[disk("a.jpg", 100, 5).with_identity(id(9))], T0);
        let out = round(&conn, &[disk("a.jpg", 200, 9).with_identity(id(9))], T0 + 1);
        assert_eq!(out.modified, 1);
        let f = &list_files(&conn).unwrap()[0];
        assert_eq!(f.size_bytes, Some(200));
        assert_eq!(f.mtime_ms, Some(9));
        assert_eq!(f.identity, Some(id(9)));
    }

    // ---------- 缺失与回归 ----------

    #[test]
    fn disappeared_file_is_marked_not_deleted() {
        let conn = catalog();
        round(&conn, &[disk("a.jpg", 100, 5)], T0);
        let out = round(&conn, &[], T0 + 1000);
        assert_eq!(out.missing, 1);
        assert_eq!(counts(&conn).unwrap(), (1, 1), "记录必须还在");
        let f = &list_files(&conn).unwrap()[0];
        assert!(f.missing);
        assert!(!is_online(&conn, f.asset_id).unwrap(), "全缺失 = 离线");

        // 再扫一次仍然不在：不重复计
        let again = round(&conn, &[], T0 + 2000);
        assert_eq!(again.missing, 0, "已经标过了不必再标");

        // missing_since 保留最早那次的时刻
        let since: i64 = conn
            .query_row("SELECT missing_since FROM asset_files", [], |r| r.get(0))
            .unwrap();
        assert_eq!(since, T0 + 1000);
    }

    #[test]
    fn returning_file_clears_the_missing_flag() {
        let conn = catalog();
        round(&conn, &[disk("a.jpg", 100, 5)], T0);
        round(&conn, &[], T0 + 1000);
        let out = round(&conn, &[disk("a.jpg", 100, 5)], T0 + 2000);
        assert_eq!(out.returned, 1);
        let f = &list_files(&conn).unwrap()[0];
        assert!(!f.missing);
        assert!(is_online(&conn, f.asset_id).unwrap());
    }

    #[test]
    fn bitmap_missing_does_not_take_the_asset_offline_when_raw_is_still_there() {
        let conn = catalog();
        let both = [disk("a.jpg", 100, 5), disk("a.rw2", 900, 5)];
        round(&conn, &both, T0);
        let asset_id = list_files(&conn).unwrap()[0].asset_id;

        round(&conn, &[disk("a.rw2", 900, 5)], T0 + 1000);
        assert!(
            is_online(&conn, asset_id).unwrap(),
            "RAW 还在 → 资产仍可查看"
        );
        assert_eq!(counts(&conn).unwrap(), (1, 2), "缺失的那个文件记录还在");
    }

    // ---------- 推测配对 ----------

    #[test]
    fn heuristic_rename_is_counted_as_heuristic() {
        let conn = catalog();
        // 没有身份信息（网络盘）→ 靠同目录 + 同大小 + 同时间 + 名字相似
        round(&conn, &[disk("photos/IMG_0001.jpg", 100, 5)], T0);
        let out = round(&conn, &[disk("photos/IMG_0002.jpg", 100, 5)], T0 + 1);
        assert_eq!(out.renamed, 1);
        assert_eq!(
            out.heuristic, 1,
            "推测配对要记下来，UI 才能说「可能是重命名」"
        );
    }

    #[test]
    fn identity_matches_are_not_counted_as_heuristic() {
        let conn = catalog();
        round(
            &conn,
            &[disk("photos/a.jpg", 100, 5).with_identity(id(5))],
            T0,
        );
        let out = round(
            &conn,
            &[disk("photos/b.jpg", 100, 5).with_identity(id(5))],
            T0 + 1,
        );
        assert_eq!((out.renamed, out.heuristic), (1, 0));
    }

    // ---------- 路径语义 ----------

    #[test]
    fn unique_index_on_folded_path_is_enforced() {
        // 同一路径插两次必须被数据库挡住（差分层不该让它发生，但防线要在）
        let conn = catalog();
        let a = insert_asset(&conn, T0).unwrap();
        insert_file(&conn, a, &disk("photos/a.jpg", 1, 1), "bitmap", T0).unwrap();
        let err = insert_file(&conn, a, &disk("Photos/A.JPG", 1, 1), "bitmap", T0);
        assert!(err.is_err(), "折叠后同路径 → 唯一索引必须报错");
    }

    #[test]
    fn unicode_paths_roundtrip() {
        let conn = catalog();
        let files = [disk("照片/2026年9月/海边 清晨.JPG", 100, 5)];
        round(&conn, &files, T0);
        let f = &list_files(&conn).unwrap()[0];
        assert_eq!(f.rel_path, "照片/2026年9月/海边 清晨.JPG");
        assert_eq!(f.ext, "jpg");
    }

    #[test]
    fn missing_identity_still_works() {
        let conn = catalog();
        let out = round(&conn, &[disk("a.jpg", 100, 5)], T0);
        assert_eq!(out.new_files, 1);
        let f = &list_files(&conn).unwrap()[0];
        assert_eq!(f.identity, None, "没有身份信息也要能入库");
    }

    // ---------- 其它 ----------

    #[test]
    fn files_of_asset_and_counts() {
        let conn = catalog();
        round(
            &conn,
            &[
                disk("a.jpg", 1, 1),
                disk("a.rw2", 2, 1),
                disk("b.jpg", 3, 1),
            ],
            T0,
        );
        let (assets, files) = counts(&conn).unwrap();
        assert_eq!((assets, files), (2, 3));
        let first = list_files(&conn).unwrap()[0].asset_id;
        assert_eq!(files_of_asset(&conn, first).unwrap().len(), 2);
    }

    #[test]
    fn delete_asset_cascades_files() {
        let conn = catalog();
        round(&conn, &[disk("a.jpg", 1, 1), disk("a.rw2", 2, 1)], T0);
        let asset_id = list_files(&conn).unwrap()[0].asset_id;
        assert!(delete_asset(&conn, asset_id).unwrap());
        assert!(list_files(&conn).unwrap().is_empty(), "文件记录要级联删掉");
        assert_eq!(counts(&conn).unwrap(), (0, 0));
        assert!(!delete_asset(&conn, asset_id).unwrap(), "再删就没有了");
    }

    #[test]
    fn reattach_rejects_unknown_asset() {
        let conn = catalog();
        round(&conn, &[disk("a.jpg", 1, 1)], T0);
        let row = list_files(&conn).unwrap()[0].row_id;
        assert!(reattach_file(&conn, row, 999, T0).is_err());
    }

    #[test]
    fn apply_exif_writes_derived_fields_and_leaves_user_fields_alone() {
        use crate::media::exif::{ExifData, TakenAt, TakenAtSource};
        let conn = catalog();
        round(&conn, &[disk("a.jpg", 1, 1)], T0);
        let asset_id = list_files(&conn).unwrap()[0].asset_id;

        // 用户先写了作者与描述（EXIF 不该冲掉它们）
        conn.execute(
            "UPDATE assets SET author = '张三', description = '海边的清晨' WHERE id = ?1",
            [asset_id],
        )
        .unwrap();

        let exif = ExifData {
            camera_make: Some("Panasonic".into()),
            camera_model: Some("DC-S5M2".into()),
            lens: Some("LUMIX S 20-60".into()),
            focal_mm: Some(35.0),
            f_number: Some(5.6),
            exposure_ms: Some(4.0),
            iso: Some(800),
            width: Some(6000),
            height: Some(4000),
            orientation: Some(1),
            ..ExifData::default()
        };
        let taken = Some(TakenAt {
            millis: T0,
            offset_min: Some(480),
            source: TakenAtSource::Exif,
        });
        apply_exif(&conn, asset_id, &exif, taken, T0 + 5).unwrap();

        let row = conn
            .query_row(
                "SELECT taken_at, taken_at_source, taken_at_offset_min, camera_model, iso,
                        width, height, orientation, author, description
                   FROM assets WHERE id = ?1",
                [asset_id],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, i64>(4)?,
                        r.get::<_, i64>(5)?,
                        r.get::<_, i64>(6)?,
                        r.get::<_, i64>(7)?,
                        r.get::<_, String>(8)?,
                        r.get::<_, String>(9)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(row.0, T0);
        assert_eq!(row.1, "exif", "来源要如实记下来");
        assert_eq!(row.2, 480);
        assert_eq!(row.3, "DC-S5M2");
        assert_eq!((row.4, row.5, row.6, row.7), (800, 6000, 4000, 1));
        assert_eq!(row.8, "张三", "用户写的作者不能被冲掉");
        assert_eq!(row.9, "海边的清晨", "用户写的描述不能被冲掉");
    }

    #[test]
    fn apply_exif_without_time_clears_stale_values() {
        // 文件被换成另一张、新文件没有拍摄时间：旧时间不能留着骗人
        use crate::media::exif::ExifData;
        let conn = catalog();
        round(&conn, &[disk("a.jpg", 1, 1)], T0);
        let asset_id = list_files(&conn).unwrap()[0].asset_id;
        conn.execute(
            "UPDATE assets SET taken_at = 1, taken_at_source = 'exif' WHERE id = ?1",
            [asset_id],
        )
        .unwrap();

        apply_exif(&conn, asset_id, &ExifData::default(), None, T0 + 9).unwrap();
        let (t, src): (Option<i64>, Option<String>) = conn
            .query_row(
                "SELECT taken_at, taken_at_source FROM assets WHERE id = ?1",
                [asset_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(t, None);
        assert_eq!(src, None);
    }

    #[test]
    fn a_thousand_files_land_in_one_round() {
        let conn = catalog();
        let files: Vec<DiskFile> = (0..1000)
            .map(|i| disk(&format!("p/{i:04}.jpg"), 100 + i, 5))
            .collect();
        let out = round(&conn, &files, T0);
        assert_eq!((out.new_assets, out.new_files), (1000, 1000));
        // 第二轮应当无变化（差分 + 落库都是幂等的）
        let again = round(&conn, &files, T0 + 1);
        assert_eq!(again.total(), 0);
    }
}
