//! 把照片**移动到库内另一个目录**（拖到目录行 / `Ctrl+X` + `Ctrl+V`）。
//!
//! 这是「资产的文件组整体搬家」：位图、`_RAW/` 下的相机原片、以及跟着它的 `.xmp` 侧车
//! 一起走（`memory/FUNCTION-REPOSITORY.md` §4.1 的文件组口径）。**RAW 断不了**——
//! 只搬位图会让 RAW+ 变成两条资产。
//!
//! # 与回收站的关系
//!
//! 同一套文件搬运与重名规则都在 `store::recycle` 里（`files_of` / `move_file` /
//! `rollback` / `prune_ancestors_of`）—— 这里**复用**它们，不写第二份（`AGENTS.md` §2.12）。
//! 区别只有一处：回收站要改 `recycled_from` / `deleted_at`，移动只改**路径**。
//!
//! # 规则（崔总 2026-11-05）
//!
//! | 项 | 规则 |
//! | --- | --- |
//! | 目标 | 必须是 `photos/` 下的一个**真实目录**：不能是库根（`photos` 自己）、不能是回收站、不能是 `_RAW/` |
//! | 同名 | 目标里已有同名 → 整组加 `_01`、`_02`…（与导入 / 还原同一口径） |
//! | 失败 | 一组里任何一个文件动不了 → **整组回滚**（`move_file` + `rollback`） |
//! | 结果 | 源目录被搬空就逐级收掉（`prune_ancestors_of`，覆盖空的 `_RAW/`） |
//! | 缓存 | **不用清**：缩图按文件身份（同一卷 rename 身份不变）、大图按 `asset_id` + 源签名（大小/mtime 不变） |
//! | 撤销 | **不进撤销栈**（与删除一致：文件操作不在 `marking` 那套 undo 里） |

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::Result;
use crate::store::recycle::{
    absolute, files_of, folded_of, move_file, prune_ancestors_of, rollback,
};
use crate::store::path_semantics;

/// 库内照片根（与 `store::repository::DEFAULT_PHOTOS_DIR` 同值）。
pub(crate) const PHOTOS_DIR: &str = "photos";

/// 重名时最多尝试多少个后缀（与还原规则同一个上限；超出即放弃这一组）。
const MAX_RENAME_ATTEMPTS: usize = 999;

/// 移动的报告。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MoveReport {
    /// 真正移动了的资产数
    pub moved: usize,
    /// 因为目标重名而改了名的资产数
    pub renamed: usize,
    /// 失败的文件（相对路径 + 原因）
    pub failed: Vec<(String, String)>,
}

/// 把一个资产的全部文件相对路径，换算成目标目录下的相对路径。
///
/// * 位图 / 其它 → `<target>/<原名>`
/// * `_RAW/` 里的原片 → `<target>/_RAW/<原名>`
/// * 改名（`attempt > 0`）时**整组换同一个主体名**，各自保留扩展名
fn relocate_rel(rel_path: &str, target: &str, attempt: usize, base_stem: Option<&str>) -> String {
    let name = rel_path.rsplit('/').next().unwrap_or(rel_path);
    let in_raw = path_semantics::parent(rel_path)
        .is_some_and(|dir| dir.rsplit('/').next().is_some_and(|last| last.eq_ignore_ascii_case("_RAW")));
    let renamed = match (attempt, base_stem) {
        (0, _) | (_, None) => name.to_string(),
        (_, Some(stem)) => match path_semantics::extension(name) {
            Some(ext) => format!("{stem}_{attempt:02}.{ext}"),
            None => format!("{stem}_{attempt:02}"),
        },
    };
    if in_raw {
        format!("{target}/_RAW/{renamed}")
    } else {
        format!("{target}/{renamed}")
    }
}

/// 侧车目标相对路径（跟随后可能改名的主体名）。
fn sidecar_target_rel(
    primary_rel: &str,
    target: &str,
    attempt: usize,
    base_stem: Option<&str>,
) -> Option<String> {
    // 没有可推出的侧车位置 = 这个资产不带侧车
    crate::xmp::rel_sidecar_of(primary_rel)?;
    let stem = match (attempt, base_stem) {
        (0, _) => path_semantics::file_stem(primary_rel)?,
        (_, Some(base)) => format!("{base}_{attempt:02}"),
        (_, None) => return None,
    };
    Some(format!("{target}/{stem}.xmp"))
}

/// 目标位置是否已经有一个 sidecar（同名 `.xmp` 不能被覆盖）。
/// 用 `resolve_sidecar`：与读取/回收同一套大小写折叠口径。
fn sidecar_target_exists(
    root: &Path,
    primary_rel: &str,
    target: &str,
    attempt: usize,
    base_stem: Option<&str>,
) -> bool {
    let Some(rel) = sidecar_target_rel(primary_rel, target, attempt, base_stem) else {
        return false;
    };
    match crate::xmp::resolve_sidecar(&absolute(root, &rel)) {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(error) => {
            log::warn!("[relocate] 目标侧车检查失败，按冲突处理：{error}");
            true
        }
    }
}

/// 一个文件所在的目录（库内相对路径；`photos/2026/_RAW` → 原样返回）。
pub(crate) fn dir_of(rel_path: &str) -> String {
    path_semantics::parent(rel_path).unwrap_or_default()
}

/// 校验目标目录：必须是 `photos/` 下的真实目录（挡住库根、回收站、`_RAW/`、越界）。
fn resolve_target(root: &Path, target_rel: &str) -> std::result::Result<PathBuf, String> {
    let rel = target_rel.replace('\\', "/");
    let trimmed = rel.trim_matches('/');
    if trimmed.is_empty() {
        return Err("不能移动到库根".to_string());
    }
    if !trimmed.starts_with(&format!("{PHOTOS_DIR}/")) {
        return Err("只能移动到 photos/ 下的目录".to_string());
    }
    if trimmed.split('/').any(|seg| seg == ".." || seg == ".") {
        return Err("目录路径不合法".to_string());
    }
    if trimmed
        .rsplit('/')
        .next()
        .is_some_and(|last| last.eq_ignore_ascii_case("_RAW"))
    {
        return Err("不能移动到 _RAW 目录".to_string());
    }
    let dir = absolute(root, trimmed);
    let meta = std::fs::metadata(&dir).map_err(|e| format!("目标目录不可用：{e}"))?;
    if !meta.is_dir() {
        return Err("目标不是目录".to_string());
    }
    Ok(dir)
}

/// 锁住的资产（一级锁及以上）：不能动位置，交给回收站 / 其它模块去解释。
pub fn locked_for_move(conn: &Connection, ids: &[i64]) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM assets WHERE id = ?1 AND lock_level >= ?2",
    )?;
    let mut locked = Vec::new();
    for id in ids {
        let row: Option<i64> = stmt
            .query_row(
                rusqlite::params![id, i64::from(crate::store::marking::LOCK_NO_DELETE)],
                |r| r.get(0),
            )
            .ok();
        if row.is_some() {
            locked.push(*id);
        }
    }
    Ok(locked)
}

/// 搬一批照片到 `target_rel`（**自己开事务**：测试与没有外层事务的调用方用它）。
pub fn move_assets_to_dir(
    conn: &Connection,
    root: &Path,
    ids: &[i64],
    target_rel: &str,
) -> Result<MoveReport> {
    let tx = conn.unchecked_transaction()?;
    let report = move_assets_body(&tx, root, ids, target_rel, || Ok(()))?;
    tx.commit()?;
    Ok(report)
}

/// 搬一批照片到 `target_rel`（带库会话检查；**不开事务** —— 调用方的事务说了算）。
pub fn move_assets_to_dir_checked(
    conn: &Connection,
    root: &Path,
    ids: &[i64],
    target_rel: &str,
    check_session: impl Fn() -> Result<()>,
) -> Result<MoveReport> {
    move_assets_body(conn, root, ids, target_rel, check_session)
}

fn move_assets_body(
    conn: &Connection,
    root: &Path,
    ids: &[i64],
    target_rel: &str,
    check_session: impl Fn() -> Result<()>,
) -> Result<MoveReport> {
    check_session()?;
    let target_dir_rel = target_rel.replace('\\', "/").trim_matches('/').to_string();
    // 目标不合法：整批拒绝（一个都不动），原因直接给用户看
    if let Err(reason) = resolve_target(root, &target_dir_rel) {
        return Ok(MoveReport {
            moved: 0,
            renamed: 0,
            failed: vec![(target_dir_rel, reason)],
        });
    }
    let photos_root = absolute(root, PHOTOS_DIR);
    let mut report = MoveReport::default();

    /*
     * 一级锁：**不能动位置**（与移入回收站同一口径）。
     * 没锁（0）或锁低于一级时才允许移动；锁着的记录**一个字段都不许改**。
     */
    let locked = locked_for_move(conn, ids)?;
    let locked_set: std::collections::BTreeSet<i64> = locked.iter().copied().collect();
    for id in ids {
        if locked_set.contains(id) {
            let files = files_of(conn, *id)?;
            for file in &files {
                report.failed.push((file.rel_path.clone(), "照片已锁定，不能移动".to_string()));
            }
            continue;
        }
        check_session()?;
        let files = files_of(conn, *id)?;
        if files.is_empty() {
            continue;
        }
        // 已经有一半在回收站里（不该出现在浏览里）：不动它，交给回收站那套逻辑
        if files.iter().any(|file| {
            path_semantics::parent(&file.rel_path)
                .is_some_and(|dir| dir.starts_with("recycle"))
        }) {
            continue;
        }
        /*
         * 整组都已经在目标位置 = 无事发生（重复拖 / 重复粘贴是幂等的）。
         * **不能只看位图**：RAW-only 资产没有位图，空集合 `.all()` 恒真会把它整组跳过。
         */
        let already_there = files.iter().all(|file| {
            let dir = dir_of(&file.rel_path);
            if file.role == "raw" {
                dir == format!("{target_dir_rel}/_RAW")
            } else {
                dir == target_dir_rel
            }
        });
        if already_there {
            continue;
        }

        let primary = files
            .iter()
            .find(|file| file.role == "bitmap")
            .or_else(|| files.first())
            .expect("files 非空");
        let base_stem = path_semantics::file_stem(&primary.rel_path);

        // 找一个整组都不冲突的名字（**侧车也算**：同名 `.xmp` 不能被覆盖）
        let mut attempt = 0usize;
        let mut chosen: Option<usize> = None;
        while attempt <= MAX_RENAME_ATTEMPTS {
            let clash = files.iter().any(|file| {
                let to_rel = relocate_rel(&file.rel_path, &target_dir_rel, attempt, base_stem.as_deref());
                absolute(root, &to_rel).exists()
            }) || sidecar_target_exists(
                root,
                &primary.rel_path,
                &target_dir_rel,
                attempt,
                base_stem.as_deref(),
            );
            if !clash {
                chosen = Some(attempt);
                break;
            }
            attempt += 1;
        }
        let Some(attempt) = chosen else {
            for file in &files {
                report
                    .failed
                    .push((file.rel_path.clone(), "目标目录里同名文件太多".to_string()));
            }
            continue;
        };

        // 文件先走（失败整组回滚）
        let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut ok = true;
        for file in &files {
            check_session()?;
            let from = absolute(root, &file.rel_path);
            if !from.exists() {
                // 文件缺失：照常改记录（移动后表现为 missing），与回收站的做法一致
                continue;
            }
            let to_rel = relocate_rel(&file.rel_path, &target_dir_rel, attempt, base_stem.as_deref());
            let to = absolute(root, &to_rel);
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

        // 侧车跟着走（`_RAW/` 折算回上一层；名字跟随位图的新主体名）
        if let Some(orig_sidecar) = crate::xmp::rel_sidecar_of(&primary.rel_path) {
            let stem = match (attempt, base_stem.as_deref()) {
                (0, _) => path_semantics::file_stem(&primary.rel_path),
                (_, Some(base)) => Some(format!("{base}_{attempt:02}")),
                (_, None) => None,
            };
            if let Some(stem) = stem {
                let to_rel = format!("{target_dir_rel}/{stem}.xmp");
                match crate::xmp::resolve_sidecar(&absolute(root, &orig_sidecar)) {
                    Ok(Some(from)) => {
                        if let Err(reason) = move_file(&from, &absolute(root, &to_rel)) {
                            log::warn!("[relocate] 侧车未能跟随：{reason}");
                        }
                    }
                    Ok(None) => {}
                    Err(error) => log::warn!("[relocate] 侧车路径解析失败：{error}"),
                }
            }
        }

        // 记录后走
        for file in &files {
            let to_rel = relocate_rel(&file.rel_path, &target_dir_rel, attempt, base_stem.as_deref());
            conn.execute(
                "UPDATE asset_files SET rel_path = ?1, rel_path_folded = ?2 WHERE id = ?3",
                rusqlite::params![to_rel, folded_of(&to_rel), file.id],
            )?;
        }
        crate::store::fts::refresh_asset(conn, *id)?;

        report.moved += 1;
        if attempt > 0 {
            report.renamed += 1;
        }

        // 源目录搬空了就逐级收掉（`_RAW/` 也算）
        let mut dirs: Vec<String> = files
            .iter()
            .map(|file| dir_of(&file.rel_path))
            .collect();
        dirs.sort_unstable();
        dirs.dedup();
        for dir in &dirs {
            prune_ancestors_of(&absolute(root, dir), &photos_root);
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::migration::{self, DbKind};
    use crate::store::pragma;

    const T0: i64 = 1_789_516_800_000;

    fn catalog() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::Catalog, migration::Backups::none(), T0).unwrap();
        conn
    }

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "raybend-relocate-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn asset_with_files(conn: &Connection, root: &Path, rels: &[&str]) -> i64 {
        conn.execute(
            "INSERT INTO assets (imported_at, updated_at) VALUES (?1, ?1)",
            [T0],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        for rel in rels {
            let role = if rel.contains("/_RAW/") { "raw" } else { "bitmap" };
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

    fn rel_of(conn: &Connection, id: i64, role: &str) -> String {
        conn.query_row(
            "SELECT rel_path FROM asset_files WHERE asset_id = ?1 AND role = ?2",
            rusqlite::params![id, role],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn moves_bitmap_into_target_directory() {
        let root = temp_root("single");
        std::fs::create_dir_all(root.join("photos/2026/新目录")).unwrap();
        let conn = catalog();
        let id = asset_with_files(&conn, &root, &["photos/2026/IMG_0001.jpg"]);

        let report =
            move_assets_to_dir(&conn, &root, &[id], "photos/2026/新目录").unwrap();
        assert_eq!(report.moved, 1);
        assert_eq!(report.renamed, 0);
        assert_eq!(rel_of(&conn, id, "bitmap"), "photos/2026/新目录/IMG_0001.jpg");
        assert!(root.join("photos/2026/新目录/IMG_0001.jpg").exists());
        assert!(!root.join("photos/2026/IMG_0001.jpg").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn raw_plus_moves_whole_group_and_keeps_raw_in_raw_dir() {
        let root = temp_root("raw");
        std::fs::create_dir_all(root.join("photos/2026/_RAW")).unwrap();
        std::fs::create_dir_all(root.join("photos/2026/目标")).unwrap();
        let conn = catalog();
        let id = asset_with_files(
            &conn,
            &root,
            &["photos/2026/IMG_0001.jpg", "photos/2026/_RAW/IMG_0001.CR3"],
        );

        let report = move_assets_to_dir(&conn, &root, &[id], "photos/2026/目标").unwrap();
        assert_eq!(report.moved, 1);
        assert_eq!(rel_of(&conn, id, "bitmap"), "photos/2026/目标/IMG_0001.jpg");
        assert_eq!(
            rel_of(&conn, id, "raw"),
            "photos/2026/目标/_RAW/IMG_0001.CR3",
            "RAW 必须跟着走，并且仍然在 _RAW/ 里"
        );
        assert!(root.join("photos/2026/目标/_RAW/IMG_0001.CR3").exists());
        // 源目录空了 → 连 `_RAW/` 一起收掉（2026 目录里还有子目录「目标」，所以留着）
        assert!(!root.join("photos/2026/_RAW").exists(), "空的 _RAW/ 是被搬空的，要收掉");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn sidecar_follows_the_photo_and_gets_renamed_with_the_group() {
        let root = temp_root("sidecar");
        std::fs::create_dir_all(root.join("photos/2026")).unwrap();
        std::fs::create_dir_all(root.join("photos/目标")).unwrap();
        let conn = catalog();
        let id = asset_with_files(&conn, &root, &["photos/2026/IMG_0001.jpg"]);
        std::fs::write(root.join("photos/2026/IMG_0001.xmp"), b"<x/>").unwrap();
        // 目标里已经有一个同名 → 整组改名 `_01`
        std::fs::write(root.join("photos/目标/IMG_0001.jpg"), b"other").unwrap();

        let report = move_assets_to_dir(&conn, &root, &[id], "photos/目标").unwrap();
        assert_eq!(report.moved, 1);
        assert_eq!(report.renamed, 1);
        assert_eq!(rel_of(&conn, id, "bitmap"), "photos/目标/IMG_0001_01.jpg");
        assert!(root.join("photos/目标/IMG_0001_01.jpg").exists());
        assert!(root.join("photos/目标/IMG_0001_01.xmp").exists(), "侧车跟着改名");
        assert!(!root.join("photos/2026/IMG_0001.xmp").exists());
        // 原来那个同名文件完好无损
        assert_eq!(
            std::fs::read(root.join("photos/目标/IMG_0001.jpg")).unwrap(),
            b"other"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_library_root_recycle_raw_and_missing_targets() {
        let root = temp_root("reject");
        std::fs::create_dir_all(root.join("photos/2026/_RAW")).unwrap();
        std::fs::create_dir_all(root.join("recycle/2026")).unwrap();
        let conn = catalog();
        let id = asset_with_files(&conn, &root, &["photos/2026/IMG.jpg"]);

        for bad in ["", "photos", "recycle/2026", "photos/2026/_RAW", "photos/不存在", "../outside"] {
            let report = move_assets_to_dir(&conn, &root, &[id], bad).unwrap();
            assert_eq!(report.moved, 0, "{bad} 不该被接受");
            assert!(!report.failed.is_empty(), "{bad} 要给一句原因");
            assert_eq!(rel_of(&conn, id, "bitmap"), "photos/2026/IMG.jpg", "失败时一个字段都不许动");
            assert!(root.join("photos/2026/IMG.jpg").exists());
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn moving_into_the_same_directory_is_a_noop() {
        let root = temp_root("same");
        std::fs::create_dir_all(root.join("photos/2026")).unwrap();
        let conn = catalog();
        let id = asset_with_files(&conn, &root, &["photos/2026/IMG.jpg"]);

        let report = move_assets_to_dir(&conn, &root, &[id], "photos/2026").unwrap();
        assert_eq!(report.moved, 0);
        assert!(report.failed.is_empty(), "同目录不是错误，是「什么都不用做」");
        assert_eq!(rel_of(&conn, id, "bitmap"), "photos/2026/IMG.jpg");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn raw_only_asset_is_not_mistaken_for_already_there() {
        let root = temp_root("raw-only");
        std::fs::create_dir_all(root.join("photos/2026/_RAW")).unwrap();
        std::fs::create_dir_all(root.join("photos/目标")).unwrap();
        let conn = catalog();
        let id = asset_with_files(&conn, &root, &["photos/2026/_RAW/IMG.CR3"]);

        let report = move_assets_to_dir(&conn, &root, &[id], "photos/目标").unwrap();
        assert_eq!(report.moved, 1, "RAW-only 也必须能搬");
        assert_eq!(rel_of(&conn, id, "raw"), "photos/目标/_RAW/IMG.CR3");
        assert!(root.join("photos/目标/_RAW/IMG.CR3").exists());
        // 再搬一次：幂等，不再动
        let again = move_assets_to_dir(&conn, &root, &[id], "photos/目标").unwrap();
        assert_eq!(again.moved, 0);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn locked_asset_is_not_moved() {
        let root = temp_root("locked");
        std::fs::create_dir_all(root.join("photos/2026")).unwrap();
        std::fs::create_dir_all(root.join("photos/目标")).unwrap();
        let conn = catalog();
        let id = asset_with_files(&conn, &root, &["photos/2026/IMG.jpg"]);
        conn.execute("UPDATE assets SET lock_level = 1 WHERE id = ?1", [id]).unwrap();

        let report = move_assets_to_dir(&conn, &root, &[id], "photos/目标").unwrap();
        assert_eq!(report.moved, 0);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(rel_of(&conn, id, "bitmap"), "photos/2026/IMG.jpg");
        assert!(root.join("photos/2026/IMG.jpg").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn sidecar_collision_forces_the_whole_group_to_rename() {
        let root = temp_root("sidecar-collision");
        std::fs::create_dir_all(root.join("photos/2026")).unwrap();
        std::fs::create_dir_all(root.join("photos/目标")).unwrap();
        let conn = catalog();
        let id = asset_with_files(&conn, &root, &["photos/2026/IMG.jpg"]);
        std::fs::write(root.join("photos/2026/IMG.xmp"), b"ours").unwrap();
        std::fs::write(root.join("photos/目标/IMG.xmp"), b"existing").unwrap();

        let report = move_assets_to_dir(&conn, &root, &[id], "photos/目标").unwrap();
        assert_eq!(report.moved, 1);
        assert_eq!(report.renamed, 1);
        assert_eq!(rel_of(&conn, id, "bitmap"), "photos/目标/IMG_01.jpg");
        assert_eq!(std::fs::read(root.join("photos/目标/IMG.xmp")).unwrap(), b"existing", "已有侧车不能被覆盖");
        assert_eq!(std::fs::read(root.join("photos/目标/IMG_01.xmp")).unwrap(), b"ours");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_file_still_gets_its_record_moved() {
        let root = temp_root("missing");
        std::fs::create_dir_all(root.join("photos/2026")).unwrap();
        std::fs::create_dir_all(root.join("photos/目标")).unwrap();
        let conn = catalog();
        let id = asset_with_files(&conn, &root, &["photos/2026/IMG.jpg"]);
        std::fs::remove_file(root.join("photos/2026/IMG.jpg")).unwrap();

        let report = move_assets_to_dir(&conn, &root, &[id], "photos/目标").unwrap();
        assert_eq!(report.moved, 1, "磁盘上没了也要把记录搬到目标（表现为 missing）");
        assert_eq!(rel_of(&conn, id, "bitmap"), "photos/目标/IMG.jpg");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn empty_source_folder_is_pruned_after_the_move() {
        let root = temp_root("prune");
        std::fs::create_dir_all(root.join("photos/2026/08/15")).unwrap();
        std::fs::create_dir_all(root.join("photos/存档")).unwrap();
        let conn = catalog();
        let id = asset_with_files(&conn, &root, &["photos/2026/08/15/IMG.jpg"]);

        move_assets_to_dir(&conn, &root, &[id], "photos/存档").unwrap();
        assert!(!root.join("photos/2026").exists(), "搬空的源目录要逐级收掉");
        assert!(root.join("photos/存档/IMG.jpg").exists());
        std::fs::remove_dir_all(&root).ok();
    }
}
