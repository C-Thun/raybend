//! 库级回收站（`specs/library-recycle-bin.md`）：删除 = 移入 `recycle/`，还原 = 移回原位，
//! 真删 / 清空 = 永久删除。
//!
//! 语义链：`photos/` →（删除）→ `recycle/`（可还原）→（真删 / 清空）→ 磁盘消失。
//!
//! # 不变量（测试逐条守着）
//!
//! > `asset_files.rel_path` 永远描述文件的**真实位置**；`assets.deleted_at` 描述它**是否在回收站**。
//!
//! # 文件层 vs 数据层
//!
//! 移入在**文件层是实质性动作**（真实 `rename`），在**数据层只是标记**：
//! `deleted_at` + `rel_path` 前缀改写 + `recycled_from` 记原路径 —— 别的表一律不动。
//! 评级 / 标签 / 编辑栈 / issue / 桶成员因此全保留，还原后一切照旧。
//!
//! # 目录映射
//!
//! `photos/<相对>` ⇄ `recycle/<同一相对>`（`recycle` 取代 `photos` 的角色；`_RAW/` 自然跟着走）。
//! 原路径以 `recycled_from` 为准，不靠字符串反推。
//!
//! # 顺序
//!
//! 文件先走、记录后走；单个资产一个事务。移文件失败 → 回滚已移动的、不改该资产的任何 DB。

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::Result;
use crate::store::marking::LOCK_NO_DELETE;
use crate::store::path_semantics::{self, PathForms};

/// 回收站目录名（与 `photos/` 同级；`memory/FUNCTION-REPOSITORY.md` §1 的保留目录）。
pub const RECYCLE_DIR: &str = "recycle";

/// 库内相对路径 → 绝对路径（**只做拼接**，不检查存在性）。
#[must_use]
pub fn absolute(root: &Path, rel_path: &str) -> PathBuf {
    root.join(rel_path.replace('\\', "/"))
}

/// `photos/...` → `recycle/...`（剥掉 `photos/` 前缀；recycle 取代它的位置）。
fn recycle_rel_of(rel_path: &str) -> String {
    let rest = rel_path
        .strip_prefix("photos")
        .map(|rest| rest.trim_start_matches('/'))
        .unwrap_or(rel_path);
    if rest.is_empty() {
        RECYCLE_DIR.to_string()
    } else {
        format!("{RECYCLE_DIR}/{rest}")
    }
}

fn folded_of(rel_path: &str) -> String {
    PathForms::new(rel_path).folded().to_string()
}

/// 移入回收站的报告。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecycleReport {
    /// 成功移入的资产数（文件已在 `recycle/`、DB 已标记）。
    pub moved: usize,
    /// 被锁挡住的资产（一级及以上）。
    pub blocked_locked: Vec<i64>,
    /// 文件本来就已不在磁盘上的资产数（照常标记；进回收站后显示为缺失）。
    pub already_gone: usize,
    /// 失败的「相对路径 + 原因」；这些资产**没有任何改动**。
    pub failed: Vec<(String, String)>,
}

impl RecycleReport {
    #[must_use]
    pub fn touched_anything(&self) -> bool {
        self.moved > 0 || self.already_gone > 0
    }
}

/// 还原报告。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RestoreReport {
    /// 成功还原的资产数。
    pub restored: usize,
    /// 因为原位已有同名而改名的资产数（前端据此提示「已改名还原」）。
    pub renamed: usize,
    pub failed: Vec<(String, String)>,
}

/// 真删 / 清空的报告。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PurgeReport {
    pub purged: usize,
    pub failed: Vec<(String, String)>,
}

/// 回收站概况（入口行与清空确认用）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecycleSummary {
    pub count: i64,
    /// 登记文件的大小合计（`size_bytes` 缺失时按 0 计）。
    pub bytes: i64,
}

struct FileRow {
    id: i64,
    role: String,
    rel_path: String,
    recycled_from: Option<String>,
}

fn files_of(conn: &Connection, asset_id: i64) -> Result<Vec<FileRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, role, rel_path, recycled_from FROM asset_files \
         WHERE asset_id = ?1 ORDER BY role, rel_path",
    )?;
    let rows = stmt
        .query_map([asset_id], |r| {
            Ok(FileRow {
                id: r.get(0)?,
                role: r.get(1)?,
                rel_path: r.get(2)?,
                recycled_from: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 移动一个文件（同卷 `rename`；目标目录按需创建）。
fn move_file(from: &Path, to: &Path) -> std::result::Result<(), String> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("建目录失败：{e}"))?;
    }
    std::fs::rename(from, to).map_err(|e| format!("{e}"))
}

fn rollback(pairs: &[(PathBuf, PathBuf)]) {
    for (from, to) in pairs.iter().rev() {
        if let Err(error) = std::fs::rename(to, from) {
            log::warn!(
                "[recycle] 回滚失败：{} → {}：{error}",
                to.display(),
                from.display()
            );
        }
    }
}

/// 移入回收站（**自己负责事务**的入口：测试与没有外层事务的调用方用它）。
pub fn move_to_recycle(conn: &Connection, root: &Path, ids: &[i64], now_ms: i64) -> Result<RecycleReport> {
    let tx = conn.unchecked_transaction()?;
    let report = move_to_recycle_body(&tx, root, ids, now_ms, || Ok(()))?;
    tx.commit()?;
    Ok(report)
}

/// 移入回收站（带库会话检查；**不开事务** —— 调用方的事务说了算）。
///
/// 生产路径（`browse_delete`）跑在 `catalog.write_tx` 的事务里：在这里再开一个事务
/// 会直接报 `cannot start a transaction within a transaction`（2026-10-11 真机抓到的 bug ——
/// 文件已经移进 `recycle/`，事务却整个回滚，界面看起来「点了没反应」）。
pub fn move_to_recycle_checked(
    conn: &Connection,
    root: &Path,
    ids: &[i64],
    now_ms: i64,
    check_session: impl Fn() -> Result<()>,
) -> Result<RecycleReport> {
    move_to_recycle_body(conn, root, ids, now_ms, check_session)
}

fn move_to_recycle_body(
    conn: &Connection,
    root: &Path,
    ids: &[i64],
    now_ms: i64,
    check_session: impl Fn() -> Result<()>,
) -> Result<RecycleReport> {
    check_session()?;
    let mut report = RecycleReport::default();
    if ids.is_empty() {
        return Ok(report);
    }

    // ① 锁与状态过滤（已在回收站的跳过 —— 幂等）
    let mut candidates: Vec<i64> = Vec::new();
    for id in ids {
        let state: Option<(i64, Option<i64>)> = conn
            .query_row(
                "SELECT lock_level, deleted_at FROM assets WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        match state {
            None => continue,               // 记录已经不在了（别的地方删过）
            Some((_, Some(_))) => continue, // 已经在回收站里
            Some((level, _)) if level >= i64::from(LOCK_NO_DELETE) => {
                report.blocked_locked.push(*id);
            }
            Some(_) => candidates.push(*id),
        }
    }

    // ② 文件先走
    for id in candidates {
        check_session()?;
        let files = files_of(conn, id)?;
        let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut ok = true;
        let mut gone = false;
        for file in &files {
            check_session()?;
            let from = absolute(root, &file.rel_path);
            if !from.exists() {
                gone = true;
                continue;
            }
            let to = absolute(root, &recycle_rel_of(&file.rel_path));
            if to.exists() {
                report
                    .failed
                    .push((file.rel_path.clone(), "回收站里已有同名文件".to_string()));
                ok = false;
                break;
            }
            match move_file(&from, &to) {
                Ok(()) => moved.push((from, to)),
                Err(reason) => {
                    report.failed.push((file.rel_path.clone(), reason));
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            rollback(&moved);
            continue;
        }

        // sidecar 随行：**我们的**（含 rb: 域）才动，别家的不碰；移不动只记日志
        // （`specs/xmp-sidecar.md` §6.2：sidecar 是镜像资料，不是真相源）。
        let mut sidecars: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for file in &files {
            if let Some(rel) = crate::xmp::rel_sidecar_of(&file.rel_path) {
                sidecars.insert(rel);
            }
        }
        for rel in sidecars {
            check_session()?;
            let from = match crate::xmp::resolve_sidecar(&absolute(root, &rel)) {
                Ok(Some(abs)) => abs,
                Ok(None) => continue,
                Err(error) => {
                    log::warn!("[recycle] sidecar {rel} 路径解析失败，保留：{error}");
                    continue;
                }
            };
            if !crate::xmp::is_ours(&from) {
                continue;
            }
            let to = absolute(root, &recycle_rel_of(&rel));
            if let Err(reason) = move_file(&from, &to) {
                log::warn!("[recycle] sidecar {rel} 未能移入回收站：{reason}");
            }
        }

        // ③ 记录后走（deleted_at + 前缀改写 + 原路径）—— 事务由调用方提供，这里不再开
        for file in &files {
            let recycled = recycle_rel_of(&file.rel_path);
            conn.execute(
                "UPDATE asset_files SET recycled_from = ?1, rel_path = ?2, rel_path_folded = ?3 \
                 WHERE id = ?4",
                rusqlite::params![file.rel_path, recycled, folded_of(&recycled), file.id],
            )?;
        }
        conn.execute(
            "UPDATE assets SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
            rusqlite::params![now_ms, id],
        )?;
        crate::store::fts::refresh_asset(conn, id)?;

        /*
         * 原位置可能空掉：往上一路收掉空目录（`_RAW/` 也在覆盖内）。
         * 触发式 —— 只在这次移动真的发生之后做（崔总 2026-10-11：不要主动扫空目录，
         * 否则会跟用户自建的空目录打架）。
         */
        for file in &files {
            if let Some(dir) = absolute(root, &file.rel_path).parent() {
                prune_ancestors_of(
                    dir,
                    &root.join(crate::store::repository::DEFAULT_PHOTOS_DIR),
                );
            }
        }

        if gone {
            report.already_gone += 1;
        }
        report.moved += 1;
    }
    Ok(report)
}

/// 还原（生产入口）。
pub fn restore_assets(conn: &Connection, root: &Path, ids: &[i64]) -> Result<RestoreReport> {
    let tx = conn.unchecked_transaction()?;
    let report = restore_assets_body(&tx, root, ids, || Ok(()))?;
    tx.commit()?;
    Ok(report)
}

/// 计算一个文件在**改名**时的目标相对路径（`attempt = 0` 表示不改名，用原路径）。
fn rename_target_rel(orig: &str, attempt: usize, base_stem: &str) -> String {
    if attempt == 0 {
        return orig.to_string();
    }
    let (dir, name) = orig.rsplit_once('/').unwrap_or(("", orig));
    let stem = path_semantics::file_stem(name).unwrap_or_else(|| name.to_string());
    let ext = path_semantics::extension(name);
    let new_name = match ext {
        Some(ext) if !stem.is_empty() => format!("{base_stem}_{attempt:02}.{ext}"),
        _ => format!("{base_stem}_{attempt:02}"),
    };
    if dir.is_empty() {
        new_name
    } else {
        format!("{dir}/{new_name}")
    }
}

/// 还原一批照片（带库会话检查；**不开事务** —— 调用方的事务说了算）。
pub fn restore_assets_checked(
    conn: &Connection,
    root: &Path,
    ids: &[i64],
    check_session: impl Fn() -> Result<()>,
) -> Result<RestoreReport> {
    restore_assets_body(conn, root, ids, check_session)
}

fn restore_assets_body(
    conn: &Connection,
    root: &Path,
    ids: &[i64],
    check_session: impl Fn() -> Result<()>,
) -> Result<RestoreReport> {
    check_session()?;
    let mut report = RestoreReport::default();
    for id in ids {
        check_session()?;
        let state: Option<Option<i64>> = conn
            .query_row("SELECT deleted_at FROM assets WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .ok();
        match state {
            Some(Some(_)) => {}
            // 记录不在 / 本来就不在回收站：当作无事发生（幂等）
            _ => continue,
        }
        let files = files_of(conn, *id)?;
        if files.is_empty() {
            continue;
        }

        // 组主名（冲突改名时统一用它）：位图优先，否则第一个文件。
        let primary = files
            .iter()
            .find(|f| f.role == "bitmap")
            .or_else(|| files.first())
            .expect("files 非空");
        let base_stem = primary
            .recycled_from
            .as_deref()
            .and_then(path_semantics::file_stem)
            .unwrap_or_default();

        // 找不冲突的名字：attempt = 0 用原名；冲突则 `_01`、`_02`…（与导入重名规则 §3.4 同口径）
        let mut attempt = 0usize;
        let mut chosen_attempt: Option<usize> = None;
        while attempt <= 999 {
            let clash = files.iter().any(|file| {
                let Some(orig) = file.recycled_from.as_deref() else {
                    return false;
                };
                absolute(root, &rename_target_rel(orig, attempt, &base_stem)).exists()
            });
            if !clash {
                chosen_attempt = Some(attempt);
                break;
            }
            attempt += 1;
        }
        let Some(attempt) = chosen_attempt else {
            for file in &files {
                report
                    .failed
                    .push((file.rel_path.clone(), "无法找到可用的还原名".to_string()));
            }
            continue;
        };

        // 文件先走
        let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut ok = true;
        for file in &files {
            check_session()?;
            let Some(orig) = file.recycled_from.as_deref() else {
                continue;
            };
            let from = absolute(root, &file.rel_path);
            if !from.exists() {
                continue; // 文件缺失：照常改记录（还原后表现为 missing）
            }
            let to_rel = rename_target_rel(orig, attempt, &base_stem);
            let to = absolute(root, &to_rel);
            if to.exists() {
                report
                    .failed
                    .push((to_rel, "目标位置已有同名文件".to_string()));
                ok = false;
                break;
            }
            match move_file(&from, &to) {
                Ok(()) => moved.push((from, to)),
                Err(reason) => {
                    report.failed.push((to_rel, reason));
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            rollback(&moved);
            continue;
        }

        // sidecar 跟着回来（名字跟随图片的目标名）
        for file in &files {
            let Some(orig) = file.recycled_from.as_deref() else {
                continue;
            };
            let Some(orig_sidecar) = crate::xmp::rel_sidecar_of(orig) else {
                continue;
            };
            let from = match crate::xmp::resolve_sidecar(&absolute(root, &recycle_rel_of(&orig_sidecar)))
            {
                Ok(Some(abs)) => abs,
                Ok(None) => continue,
                Err(error) => {
                    log::warn!("[recycle] sidecar {orig_sidecar} 路径解析失败，留在回收站：{error}");
                    continue;
                }
            };
            let target_rel = rename_target_rel(orig, attempt, &base_stem);
            let (dir, name) = target_rel.rsplit_once('/').unwrap_or(("", target_rel.as_str()));
            let stem = path_semantics::file_stem(name).unwrap_or_else(|| name.to_string());
            let to_rel = if dir.is_empty() {
                format!("{stem}.xmp")
            } else {
                format!("{dir}/{stem}.xmp")
            };
            if let Err(reason) = move_file(&from, &absolute(root, &to_rel)) {
                log::warn!("[recycle] sidecar {orig_sidecar} 未能还原：{reason}");
            }
        }

        // 记录后走（事务由调用方提供，这里不再开）
        for file in &files {
            let Some(orig) = file.recycled_from.as_deref() else {
                continue;
            };
            let to_rel = rename_target_rel(orig, attempt, &base_stem);
            conn.execute(
                "UPDATE asset_files SET rel_path = ?1, rel_path_folded = ?2, recycled_from = NULL \
                 WHERE id = ?3",
                rusqlite::params![to_rel, folded_of(&to_rel), file.id],
            )?;
        }
        conn.execute(
            "UPDATE assets SET deleted_at = NULL, updated_at = ?1 WHERE id = ?2",
            rusqlite::params![crate::store::time::now_millis(), id],
        )?;
        crate::store::fts::refresh_asset(conn, *id)?;

        report.restored += 1;
        if attempt > 0 {
            report.renamed += 1;
        }
    }

    prune_empty_dirs(&root.join(RECYCLE_DIR));
    Ok(report)
}

/// 从某个目录起**往上**逐级删空目录（删到 `stop_at` 为止，不含它）。
///
/// **触发式清理**（崔总 2026-10-11 定）：只在「照片被移走 / 被删掉」之后调用一次，
/// 不做全盘扫描 —— 用户自己新建的空目录不会被顺手清掉（那是他的东西，不是残留）。
/// `_RAW/` 也在覆盖内：位图与 RAW 都移走后它空了就一起收掉。
fn prune_ancestors_of(from_dir: &Path, stop_at: &Path) {
    let mut current = from_dir.to_path_buf();
    while current != stop_at && current.starts_with(stop_at) {
        if std::fs::remove_dir(&current).is_err() {
            return; // 非空（还有别的照片）或权限问题：到此为止
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => return,
        }
    }
}

/// 递归清掉空目录（只删空的；`remove_dir` 对非空目录会失败，正好保留）。
fn prune_empty_dirs(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            prune_empty_dirs(&path);
            let _ = std::fs::remove_dir(&path);
        }
    }
}

/// 真删一批（只处理在回收站里的；幂等）。
pub fn purge_assets(conn: &Connection, root: &Path, ids: &[i64]) -> Result<PurgeReport> {
    purge_assets_checked(conn, root, ids, || Ok(()))
}

/// 真删一批（带库会话检查）。
pub fn purge_assets_checked(
    conn: &Connection,
    root: &Path,
    ids: &[i64],
    check_session: impl Fn() -> Result<()>,
) -> Result<PurgeReport> {
    check_session()?;
    let mut report = PurgeReport::default();
    for id in ids {
        check_session()?;
        let state: Option<Option<i64>> = conn
            .query_row("SELECT deleted_at FROM assets WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .ok();
        if !matches!(state, Some(Some(_))) {
            continue; // 不在回收站的资产绝不真删
        }
        purge_one(conn, root, *id, &mut report, &check_session)?;
    }
    Ok(report)
}

/// 清空回收站（全部真删 + 清掉回收站目录里剩下的内容）。
pub fn empty_recycle(conn: &Connection, root: &Path) -> Result<PurgeReport> {
    empty_recycle_checked(conn, root, || Ok(()))
}

/// 清空回收站（带库会话检查）。
pub fn empty_recycle_checked(
    conn: &Connection,
    root: &Path,
    check_session: impl Fn() -> Result<()>,
) -> Result<PurgeReport> {
    check_session()?;
    let mut report = PurgeReport::default();
    let ids: Vec<i64> = {
        let mut stmt = conn.prepare("SELECT id FROM assets WHERE deleted_at IS NOT NULL ORDER BY id")?;
        
        stmt
            .query_map([], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    for id in ids {
        check_session()?;
        purge_one(conn, root, id, &mut report, &check_session)?;
    }
    // 「清空」的强语义：连未登记的残留一起清掉（`specs/library-recycle-bin.md` §2.1）。
    let dir = root.join(RECYCLE_DIR);
    if dir.exists()
        && let Err(error) = std::fs::remove_dir_all(&dir)
    {
        log::warn!("[recycle] 清空残留失败：{error}");
    }
    Ok(report)
}

/// 真删一个资产：文件 → 缓存 → 记录（外键级联）→ 全文索引。
fn purge_one(
    conn: &Connection,
    root: &Path,
    id: i64,
    report: &mut PurgeReport,
    check_session: &impl Fn() -> Result<()>,
) -> Result<bool> {
    let files = files_of(conn, id)?;
    for file in &files {
        check_session()?;
        let abs = absolute(root, &file.rel_path);
        if abs.exists()
            && let Err(error) = std::fs::remove_file(&abs)
        {
            report
                .failed
                .push((file.rel_path.clone(), format!("删除失败：{error}")));
            return Ok(false); // 文件没删掉就不动记录（记录没了、文件还在更糟）
        }
        if let Some(sidecar_rel) = crate::xmp::rel_sidecar_of(&file.rel_path)
            && let Ok(Some(sidecar_abs)) =
                crate::xmp::resolve_sidecar(&absolute(root, &sidecar_rel))
            && crate::xmp::is_ours(&sidecar_abs)
            && let Err(error) = std::fs::remove_file(&sidecar_abs)
        {
            log::warn!("[recycle] sidecar {sidecar_rel} 删除失败：{error}");
        }
    }

    // 大图缓存整目录删（派生数据，可重建；删不掉不算错）。
    let cache_dir = root.join("cache").join("full").join(id.to_string());
    if cache_dir.exists()
        && let Err(error) = std::fs::remove_dir_all(&cache_dir)
    {
        log::warn!("[recycle] 大图缓存清理失败（{}）：{error}", cache_dir.display());
    }

    // 回收站里那条链空了就收掉（同一个触发式规矩）
    for file in &files {
        if let Some(dir) = absolute(root, &file.rel_path).parent() {
            prune_ancestors_of(dir, &root.join(RECYCLE_DIR));
        }
    }

    check_session()?;
    conn.execute("DELETE FROM assets WHERE id = ?1", [id])?;
    crate::store::fts::refresh_asset(conn, id)?; // 记录已没了 → 索引行一并清
    report.purged += 1;
    Ok(true)
}

/// 回收站概况（入口行与清空确认用）。
pub fn summary(conn: &Connection) -> Result<RecycleSummary> {
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM assets WHERE deleted_at IS NOT NULL",
        [],
        |r| r.get(0),
    )?;
    let bytes: i64 = conn.query_row(
        "SELECT COALESCE(SUM(f.size_bytes), 0) FROM asset_files f \
         JOIN assets a ON a.id = f.asset_id WHERE a.deleted_at IS NOT NULL",
        [],
        |r| r.get(0),
    )?;
    Ok(RecycleSummary { count, bytes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::migration::{self, DbKind};
    use crate::store::pragma;
    use crate::store::query::{self, Query, Scope, Visibility};

    const T0: i64 = 1_789_516_800_000;

    fn catalog() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::Catalog, migration::Backups::none(), T0).unwrap();
        conn
    }

    fn asset_with_files(conn: &Connection, root: &Path, rels: &[&str]) -> i64 {
        conn.execute(
            "INSERT INTO assets (imported_at, updated_at) VALUES (?1, ?1)",
            [T0],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        for (i, rel) in rels.iter().enumerate() {
            let role = if i == 0 { "bitmap" } else { "raw" };
            let ext = rel.rsplit('.').next().unwrap().to_lowercase();
            conn.execute(
                "INSERT INTO asset_files
                   (asset_id, role, rel_path, rel_path_folded, ext, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                rusqlite::params![id, role, rel, rel.to_lowercase(), ext, T0],
            )
            .unwrap();
            let abs = root.join(rel);
            std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
            std::fs::write(&abs, b"photo").unwrap();
        }
        id
    }

    fn write_our_sidecar(root: &Path, rel: &str) {
        let abs = root.join(rel);
        std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
        std::fs::write(
            &abs,
            format!(
                "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" xmlns:rb=\"{}\"></x:xmpmeta>",
                crate::xmp::packet::RB_NS
            ),
        )
        .unwrap();
    }

    fn rel_of(conn: &Connection, id: i64, role: &str) -> String {
        conn.query_row(
            "SELECT rel_path FROM asset_files WHERE asset_id = ?1 AND role = ?2",
            rusqlite::params![id, role],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn recycled_from_of(conn: &Connection, id: i64, role: &str) -> Option<String> {
        conn.query_row(
            "SELECT recycled_from FROM asset_files WHERE asset_id = ?1 AND role = ?2",
            rusqlite::params![id, role],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn deleted_at_of(conn: &Connection, id: i64) -> Option<i64> {
        conn.query_row("SELECT deleted_at FROM assets WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .unwrap()
    }

    fn visible_counts(conn: &Connection) -> (i64, i64) {
        let mut q = Query::new(Scope::Repository);
        let live = query::count(conn, &q).unwrap();
        q.visibility = Visibility::Recycled;
        let recycled = query::count(conn, &q).unwrap();
        (live, recycled)
    }

    #[test]
    fn move_relocates_files_marks_records_and_keeps_associations() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let id = asset_with_files(&conn, root, &["photos/2026-08-15/a.jpg", "photos/2026-08-15/_RAW/a.orf"]);
        write_our_sidecar(root, "photos/2026-08-15/a.xmp");
        conn.execute("UPDATE assets SET rating = 4 WHERE id = ?1", [id])
            .unwrap();

        let report = move_to_recycle(&conn, root, &[id], T0 + 1).unwrap();

        assert_eq!(report.moved, 1);
        assert!(report.failed.is_empty());
        assert_eq!(deleted_at_of(&conn, id), Some(T0 + 1));
        assert_eq!(rel_of(&conn, id, "bitmap"), "recycle/2026-08-15/a.jpg");
        assert_eq!(rel_of(&conn, id, "raw"), "recycle/2026-08-15/_RAW/a.orf");
        assert_eq!(
            recycled_from_of(&conn, id, "bitmap").as_deref(),
            Some("photos/2026-08-15/a.jpg")
        );
        assert!(!root.join("photos/2026-08-15/a.jpg").exists());
        assert!(root.join("recycle/2026-08-15/a.jpg").exists());
        assert!(root.join("recycle/2026-08-15/_RAW/a.orf").exists());
        assert!(!root.join("photos/2026-08-15/a.xmp").exists(), "sidecar 跟着走");
        assert!(root.join("recycle/2026-08-15/a.xmp").exists());
        let rating: i64 = conn
            .query_row("SELECT rating FROM assets WHERE id = ?1", [id], |r| r.get(0))
            .unwrap();
        assert_eq!(rating, 4, "关联数据一个都不动");
        assert_eq!(visible_counts(&conn), (0, 1), "常规查询看不见、回收站查询看得见");
    }

    /*
     * 生产调用形状：`browse_delete` 在 `write_tx` 的**外层事务里**调 `move_to_recycle_checked`
     * （restore / purge / empty 同理）。`_checked` 若自己再开事务，SQLite 会直接报
     * 「cannot start a transaction within a transaction」—— 2026-10-11 真机 bug：
     * 文件进了 `recycle/`、记录整个回滚、界面看着「点了没反应」。
     * 这条测试把「事务归调用方」钉死。
     */
    #[test]
    fn checked_entries_run_inside_an_outer_transaction() {        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let id = asset_with_files(&conn, root, &["photos/2026-08-15/a.jpg"]);

        let tx = conn.unchecked_transaction().unwrap();
        let report = move_to_recycle_checked(&tx, root, &[id], T0 + 1, || Ok(())).unwrap();
        tx.commit().unwrap();
        assert_eq!(report.moved, 1);
        assert_eq!(deleted_at_of(&conn, id), Some(T0 + 1));
        assert!(root.join("recycle/2026-08-15/a.jpg").exists());

        let tx = conn.unchecked_transaction().unwrap();
        let report = restore_assets_checked(&tx, root, &[id], || Ok(())).unwrap();
        tx.commit().unwrap();
        assert_eq!(report.restored, 1);
        assert_eq!(deleted_at_of(&conn, id), None);
        assert!(root.join("photos/2026-08-15/a.jpg").exists());

        // 再删一次，走 purge / empty 的事务形状
        let tx = conn.unchecked_transaction().unwrap();
        move_to_recycle_checked(&tx, root, &[id], T0 + 2, || Ok(())).unwrap();
        tx.commit().unwrap();
        let tx = conn.unchecked_transaction().unwrap();
        let report = purge_assets_checked(&tx, root, &[id], || Ok(())).unwrap();
        tx.commit().unwrap();
        assert_eq!(report.purged, 1);
        assert!(!root.join("recycle/2026-08-15/a.jpg").exists());
        let rows: i64 = conn
            .query_row("SELECT count(*) FROM assets WHERE id = ?1", [id], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0, "真删后记录不再存在");

        let tx = conn.unchecked_transaction().unwrap();
        let report = empty_recycle_checked(&tx, root, || Ok(())).unwrap();
        tx.commit().unwrap();
        assert_eq!(report.purged, 0);
    }

    /*
     * 触发式空目录清理（崔总 2026-10-11）：照片移走后原位置不留空目录，`_RAW/` 也一样；
     * 并且**只**在删除/还原这些动作里清理 —— 不主动扫，所以用户自建的空目录不会被顺手删。
     */
    #[test]
    fn prune_removes_emptied_dirs_but_keeps_user_made_ones() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let id = asset_with_files(
            &conn,
            root,
            &["photos/2026-08-15/a.jpg", "photos/2026-08-15/_RAW/a.orf"],
        );
        // 用户自建的空目录（与本次动作无关）：不该被顺手清掉
        let spare = root.join("photos/2026-08-15/自建空目录");
        std::fs::create_dir_all(&spare).unwrap();

        move_to_recycle(&conn, root, &[id], T0 + 1).unwrap();

        assert!(!root.join("photos/2026-08-15/_RAW").exists(), "_RAW 空了要收掉");
        assert!(!root.join("photos/2026-08-15/a.jpg").exists());
        assert!(root.join("recycle/2026-08-15/a.jpg").exists());
        assert!(
            spare.exists(),
            "用户自建的空目录不动（触发式清理只认这次动作走掉的东西）"
        );
        assert!(
            root.join("photos/2026-08-15").exists(),
            "目录里还有用户的东西，不能整层删掉"
        );

        restore_assets(&conn, root, &[id]).unwrap();
        assert!(root.join("photos/2026-08-15/a.jpg").exists(), "还原回原位");
        assert!(
            !root.join("recycle/2026-08-15").exists(),
            "回收站里腾空的目录链收掉"
        );
        assert!(spare.exists());
    }

    #[test]
    fn locks_block_the_move() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let id = asset_with_files(&conn, root, &["photos/a.jpg"]);
        conn.execute("UPDATE assets SET lock_level = 1 WHERE id = ?1", [id])
            .unwrap();

        let report = move_to_recycle(&conn, root, &[id], T0 + 1).unwrap();

        assert_eq!(report.moved, 0);
        assert_eq!(report.blocked_locked, vec![id]);
        assert_eq!(deleted_at_of(&conn, id), None);
        assert!(root.join("photos/a.jpg").exists());
        assert!(!root.join("recycle/a.jpg").exists());
    }

    #[test]
    fn a_gone_file_is_still_marked() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let id = asset_with_files(&conn, root, &["photos/a.jpg"]);
        std::fs::remove_file(root.join("photos/a.jpg")).unwrap();

        let report = move_to_recycle(&conn, root, &[id], T0 + 1).unwrap();

        assert_eq!(report.moved, 1);
        assert_eq!(report.already_gone, 1);
        assert_eq!(rel_of(&conn, id, "bitmap"), "recycle/a.jpg");
        assert_eq!(deleted_at_of(&conn, id), Some(T0 + 1));
    }

    #[test]
    fn move_is_idempotent_for_already_recycled_assets() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let id = asset_with_files(&conn, root, &["photos/a.jpg"]);
        move_to_recycle(&conn, root, &[id], T0 + 1).unwrap();

        let again = move_to_recycle(&conn, root, &[id], T0 + 2).unwrap();
        assert_eq!(again.moved, 0);
        assert!(again.failed.is_empty());
        assert_eq!(deleted_at_of(&conn, id), Some(T0 + 1), "时间不被二次写");
    }

    #[test]
    fn restore_returns_files_and_records() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let id = asset_with_files(&conn, root, &["photos/2026-08-15/a.jpg", "photos/2026-08-15/_RAW/a.orf"]);
        write_our_sidecar(root, "photos/2026-08-15/a.xmp");
        move_to_recycle(&conn, root, &[id], T0 + 1).unwrap();

        let report = restore_assets(&conn, root, &[id]).unwrap();

        assert_eq!(report.restored, 1);
        assert_eq!(report.renamed, 0);
        assert_eq!(deleted_at_of(&conn, id), None);
        assert_eq!(rel_of(&conn, id, "bitmap"), "photos/2026-08-15/a.jpg");
        assert_eq!(rel_of(&conn, id, "raw"), "photos/2026-08-15/_RAW/a.orf");
        assert_eq!(recycled_from_of(&conn, id, "bitmap"), None);
        assert!(root.join("photos/2026-08-15/a.jpg").exists());
        assert!(root.join("photos/2026-08-15/a.xmp").exists(), "sidecar 回来");
        assert_eq!(visible_counts(&conn), (1, 0));
    }

    #[test]
    fn restore_renames_with_suffix_on_conflict() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let id = asset_with_files(&conn, root, &["photos/a.jpg", "photos/_RAW/a.orf"]);
        move_to_recycle(&conn, root, &[id], T0 + 1).unwrap();
        // 原位被占：同名文件又出现了（导入/手放都行）
        std::fs::write(root.join("photos/a.jpg"), b"other").unwrap();

        let report = restore_assets(&conn, root, &[id]).unwrap();

        assert_eq!(report.restored, 1);
        assert_eq!(report.renamed, 1);
        assert_eq!(rel_of(&conn, id, "bitmap"), "photos/a_01.jpg");
        assert_eq!(rel_of(&conn, id, "raw"), "photos/_RAW/a_01.orf");
        assert_eq!(std::fs::read(root.join("photos/a.jpg")).unwrap(), b"other");
        assert_eq!(std::fs::read(root.join("photos/a_01.jpg")).unwrap(), b"photo");
    }

    #[test]
    fn purge_removes_files_cache_and_record() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let id = asset_with_files(&conn, root, &["photos/a.jpg"]);
        move_to_recycle(&conn, root, &[id], T0 + 1).unwrap();
        let cache_dir = root.join("cache").join("full").join(id.to_string());
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(cache_dir.join("latest-v6.avif"), b"x").unwrap();

        let report = purge_assets(&conn, root, &[id]).unwrap();

        assert_eq!(report.purged, 1);
        assert!(!root.join("recycle/a.jpg").exists());
        assert!(!cache_dir.exists(), "大图缓存整目录清掉");
        let left: i64 = conn
            .query_row("SELECT count(*) FROM assets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0, "记录真删");
    }

    #[test]
    fn purge_never_touches_live_assets() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let id = asset_with_files(&conn, root, &["photos/a.jpg"]);

        let report = purge_assets(&conn, root, &[id]).unwrap();

        assert_eq!(report.purged, 0);
        assert!(root.join("photos/a.jpg").exists());
    }

    #[test]
    fn empty_clears_everything_including_unknown_residue() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let a = asset_with_files(&conn, root, &["photos/a.jpg"]);
        let b = asset_with_files(&conn, root, &["photos/b.jpg"]);
        move_to_recycle(&conn, root, &[a, b], T0 + 1).unwrap();
        // 未登记的残留（用户手动放的）
        std::fs::write(root.join("recycle/manual.txt"), b"junk").unwrap();

        let report = empty_recycle(&conn, root).unwrap();

        assert_eq!(report.purged, 2);
        assert!(!root.join("recycle").exists(), "回收站目录整体清掉");
        assert_eq!(visible_counts(&conn), (0, 0));
    }

    #[test]
    fn summary_counts_recycled_assets() {
        let conn = catalog();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let id = asset_with_files(&conn, root, &["photos/a.jpg"]);
        conn.execute("UPDATE asset_files SET size_bytes = 123 WHERE asset_id = ?1", [id])
            .unwrap();
        move_to_recycle(&conn, root, &[id], T0 + 1).unwrap();

        let summary = summary(&conn).unwrap();
        assert_eq!(summary.count, 1);
        assert_eq!(summary.bytes, 123);
    }
}
