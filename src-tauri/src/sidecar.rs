//! XMP sidecar 的进程内同步与采纳（`specs/xmp-sidecar.md` §6–§7）。
//!
//! * **写出**：编辑 / 定稿 / 标记落库后，把该资产的 `rb:` + 标准层镜像到照片旁的
//!   `<主体名>.xmp`。走**单工作线程**队列 —— 同资产严格 FIFO，后写覆盖前写，
//!   命令本身不等待文件 IO（「全是背后的工作」）。
//! * **采纳**：导入 / 重建**新建**资产时自动发现旁边的 sidecar 并读回（无需用户操作）。
//!   已存在的资产不读不覆盖（DB 为真相源）。
//!
//! 失败策略：写不出只记日志（下一次触发自然重试），**不回滚已落库的编辑**。

use crate::browse::BrowseState;
use crate::db::DbState;
use raybend::store::time;
use raybend::store::{assets, develop, issues, tags, photo_tags, organization};
use raybend::xmp::{self, SidecarContent, SidecarMetadata, SidecarProfile};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, mpsc};
use tauri::{AppHandle, Manager, Runtime};

/// `xmp:CreatorTool`（应用名 + 版本，读者能认出这是谁写的）。
const TOOL: &str = concat!("RayBend ", env!("CARGO_PKG_VERSION"));

type Task = Box<dyn FnOnce() + Send + 'static>;

static QUEUE: OnceLock<mpsc::Sender<Task>> = OnceLock::new();

fn sender() -> &'static mpsc::Sender<Task> {
    QUEUE.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Task>();
        std::thread::Builder::new()
            .name("sidecar".to_string())
            .spawn(move || {
                while let Ok(task) = rx.recv() {
                    task();
                }
            })
            .expect("sidecar 工作线程启动失败");
        tx
    })
}

/// 排一件后台活（串行执行；进程退出时未执行的就丢弃 —— 下次触发会重写）。
pub(crate) fn queue(task: impl FnOnce() + Send + 'static) {
    let _ = sender().send(Box::new(task));
}

/// 把一批资产的 sidecar 排进队列（命令层写完库之后调；空列表直接返回）。
pub(crate) fn queue_sync<R: Runtime>(app: AppHandle<R>, repository_id: &str, asset_ids: Vec<i64>) {
    if asset_ids.is_empty() {
        return;
    }
    let repository_id = repository_id.to_string();
    queue(move || {
        if let Err(error) = sync_now(&app, &repository_id, &asset_ids) {
            eprintln!("[sidecar] 同步失败（{repository_id}）：{error}");
        }
    });
}

/// 立即同步一批资产（导入线程等本来就是后台的场景直接调这个，不再排队）。
fn sync_now<R: Runtime>(
    app: &AppHandle<R>,
    repository_id: &str,
    asset_ids: &[i64],
) -> Result<(), String> {
    let state = app.state::<BrowseState>();
    let catalog = state.lease(app, repository_id)?;
    let dictionary = app.state::<DbState>().with(app, |db| db.read(tags::list_all).map_err(|e| e.to_string()))?;
    catalog.write(move |conn| organization::sync_legacy_terms(conn, &dictionary)).map_err(|e| e.to_string())?;
    let root = catalog.root().to_path_buf();
    let now = time::now_millis();
    let mut failed = 0usize;
    for asset_id in asset_ids {
        if let Err(error) = sync_one(app, &catalog, &root, *asset_id, now) {
            failed += 1;
            eprintln!("[sidecar] 资产 {asset_id} 写出失败：{error}");
        }
    }
    if failed > 0 {
        eprintln!(
            "[sidecar] 同步 {} 个资产：{failed} 失败（保留盘上文件，下次触发重试）",
            asset_ids.len()
        );
    }
    state.observe_session(app, &catalog);
    Ok(())
}

/// 一个资产的镜像写出 / 清空删除。
fn sync_one<R: Runtime>(
    _app: &AppHandle<R>,
    catalog: &std::sync::Arc<raybend::store::db::CatalogDb>,
    root: &Path,
    asset_id: i64,
    now: i64,
) -> raybend::Result<()> {
    let (files, latest, finalized, row, keywords, tag_state) = catalog.read(move |conn| {
        let files = assets::files_of_asset(conn, asset_id)?;
        let latest = develop::load(conn, asset_id)?;
        let finalized = issues::list(conn, asset_id)?;
        let row = conn.query_row(
            "SELECT rating, color_label, author, description, country, province_state, city, sublocation \
             FROM assets WHERE id = ?1",
            [asset_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            },
        )?;
        let keywords = photo_tags::effective_names(conn, asset_id)?;
        let tag_state = photo_tags::snapshot(conn, asset_id)?;
        Ok((files, latest, finalized, row, keywords, tag_state))
    })?;
    let Some(rel) = xmp::sidecar_rel_path(&files) else {
        return Ok(()); // 连文件行都没有（资产已删）—— 没什么可镜像
    };
    let abs = root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));

    let (rating, color_label, author, description, country, province_state, city, sublocation) =
        row;
    let raw_file_name = files
        .iter()
        .find(|file| file.role == "raw" && !file.missing)
        .or_else(|| files.iter().find(|file| file.role == "raw"))
        .and_then(|file| file.rel_path.rsplit('/').next().map(str::to_string));

    let content = SidecarContent {
        tag_state: Some(tag_state),
        latest: Some(latest),
        profiles: finalized
            .iter()
            .map(|issue| SidecarProfile {
                name: issue.name.clone(),
                source_base: issue.source_base,
                created_at_ms: issue.created_at,
                ordinal: Some(issue.ordinal),
                stack: issue.stack.clone(),
            })
            .collect(),
        metadata: SidecarMetadata {
            rating: rating.clamp(0, 5) as u8,
            color_label,
            keywords,
            author,
            description,
            country,
            province_state,
            city,
            sublocation,
        },
        raw_file_name,
    };

    match xmp::compose_checked(&content, now, TOOL)? {
        Some(xml) => {
            if let xmp::PublishOutcome::Preserved = xmp::publish(&abs, &xml)? {
                eprintln!("[sidecar] {rel} 是更新版本的文件，保留未改写");
            }
        }
        None => {
            if xmp::remove_if_ours(&abs)? {
                eprintln!("[sidecar] {rel} 内容已全空，删除（编辑了才有）");
            }
        }
    }
    Ok(())
}

/// 标记补丁里**会影响 sidecar 内容**的照片（喜欢 / 锁不进 sidecar，排除）。
pub(crate) fn marked_assets(ops: &[raybend::store::marking::Op]) -> Vec<i64> {
    ops.iter()
        .filter(|op| {
            !matches!(
                op,
                raybend::store::marking::Op::Like { .. } | raybend::store::marking::Op::Lock { .. }
            )
        })
        .map(|op| op.asset_id())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// —— 自家导入（`specs/xmp-sidecar.md` §7）——
/// 新登记的资产（导入 sink 与重建都会产出）：`file_abs` 是**照片文件**的绝对路径
/// （导入时在源目录、重建时在库内），sidecar 在它旁边（`_RAW/` 折算）探测。
pub(crate) struct NewAsset {
    pub asset_id: i64,
    pub file_abs: PathBuf,
}

/// 对一批新资产自动采纳旁边的自家 sidecar（同步执行，调用方已在后台线程）。
pub(crate) fn adopt_new_assets<R: Runtime>(
    app: &AppHandle<R>,
    catalog: &std::sync::Arc<raybend::store::db::CatalogDb>,
    items: &[NewAsset],
) {
    let mut adopted = 0usize;
    let mut skipped = 0usize;
    for item in items {
        let Some(sidecar) = xmp::probe_sidecar(&item.file_abs) else {
            continue; // 旁边没有 sidecar 是常态
        };
        match adopt_one(app, catalog, item.asset_id, &sidecar) {
            Ok(true) => adopted += 1,
            Ok(false) => skipped += 1,
            Err(error) => {
                skipped += 1;
                eprintln!("[sidecar] 采纳 {} 失败：{error}", sidecar.display());
            }
        }
    }
    if adopted + skipped > 0 {
        eprintln!("[sidecar] 新资产 sidecar 采纳：{adopted} 成功、{skipped} 跳过");
    }
}

/// 采纳一个资产。`Ok(false)` = 没有 / 不是我们的 / 已有数据，静默跳过。
fn adopt_one<R: Runtime>(
    app: &AppHandle<R>,
    catalog: &std::sync::Arc<raybend::store::db::CatalogDb>,
    asset_id: i64,
    sidecar: &Path,
) -> Result<bool, String> {
    let text = std::fs::read_to_string(sidecar).map_err(|e| e.to_string())?;
    let Some(parsed) = xmp::parse(&text).map_err(|e| e.to_string())? else {
        return Ok(false); // 别家的文件：不读
    };
    for warning in &parsed.warnings {
        eprintln!("[sidecar] 采纳 {}：{warning}", sidecar.display());
    }
    if parsed.profiles.is_empty() && parsed.latest.is_none() && parsed.metadata.is_empty()
        && !parsed.tag_state.as_ref().is_some_and(|state| state.has_content()) {
        return Ok(false);
    }
    // 跨库恢复保留资源 ID；缺 LUT 明确记录，不能静默清掉引用或改变 profile。
    let lut_ids: BTreeSet<String> = parsed
        .latest
        .iter()
        .chain(parsed.profiles.iter().map(|profile| &profile.stack))
        .filter_map(|stack| stack.lut_id.clone())
        .collect();
    if !lut_ids.is_empty() {
        let resources = app.state::<DbState>().with(app, |db| {
            db.read(|conn| {
                lut_ids
                    .iter()
                    .map(|id| {
                        raybend::store::luts::get(conn, id).map(|record| (id.clone(), record))
                    })
                    .collect::<raybend::Result<Vec<_>>>()
            })
            .map_err(|e| e.to_string())
        });
        match resources.and_then(|resources| crate::lut::root(app).map(|root| (root, resources))) {
            Ok((root, resources)) => {
                for (id, record) in resources {
                    if record.is_none_or(|record| !root.join(record.file_rel_path).is_file()) {
                        eprintln!(
                            "[sidecar] 采纳 {}：LUT {id} 缺失，保留引用等待资源补齐",
                            sidecar.display()
                        );
                    }
                }
            }
            Err(error) => eprintln!("[sidecar] LUT 资源检查失败，保留引用：{error}"),
        }
    }
    // 文件 IO 和 app.db 词典写入不占 catalog 事务；守卫在最终写事务内再检查。
    let names = parsed.tag_state.as_ref().map_or_else(|| parsed.metadata.keywords.clone(), |state| {
        state.manual.iter().chain(&state.ai).chain(&state.masks).cloned().collect()
    });
    let ids = if names.is_empty() {
        Vec::new()
    } else {
        app.state::<DbState>()
            .with(app, move |db: &raybend::store::db::AppDb| {
                db.write(move |conn| tags::ensure_tags(conn, &names, time::now_millis()))
                    .map_err(|e| e.to_string())
            })?
    };
    let dictionary = app.state::<DbState>().with(app, |db| {
        db.read(|conn| tags::tags_by_ids(conn, &ids)).map_err(|e| e.to_string())
    })?;
    catalog.write(move |conn| {
        organization::sync_legacy_terms(conn, &dictionary)?;
        xmp::adopt(conn, asset_id, &parsed, &ids, time::now_millis())
    })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use raybend::store::marking::Op;

    #[test]
    fn sidecar_mark_targets_exclude_private_flags_and_deduplicate_assets() {
        let ops = [
            Op::Like {
                asset_id: 1,
                before: None,
                after: Some("like".into()),
            },
            Op::Lock {
                asset_id: 2,
                before: 0,
                after: 1,
            },
            Op::Rating {
                asset_id: 3,
                before: 0,
                after: 4,
            },
            Op::Color {
                asset_id: 3,
                before: None,
                after: Some("green".into()),
            },
            Op::TagAttach {
                asset_id: 4,
                tag_id: 7,
            },
            Op::TagDetach {
                asset_id: 4,
                tag_id: 8,
            },
            Op::Text {
                asset_id: 5,
                field: raybend::store::marking::TextField::Description,
                before: None,
                after: Some("说明".into()),
            },
        ];
        assert_eq!(marked_assets(&ops), [3, 4, 5]);
        assert!(marked_assets(&[]).is_empty());
    }

    #[test]
    fn queued_tasks_keep_fifo_order() {
        let (tx, rx) = mpsc::channel();
        for value in 0..3 {
            let tx = tx.clone();
            queue(move || {
                tx.send(value).unwrap();
            });
        }
        for expected in 0..3 {
            assert_eq!(
                rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap(),
                expected
            );
        }
    }
}
