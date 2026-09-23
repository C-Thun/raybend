//! 编辑栈的 IPC（M3-W3）：读 / 落库 / 重置。
//!
//! # 一条铁律：**松手才落库**
//!
//! 拖动过程中参数每帧都在变，但落库只在**松手**（或点重置）时发生一次 ——
//! 这是 `AGENTS.md` 的既定口径，也是「拖动不掉帧」的前提：SQLite 的单写者线程
//! 不该被每秒 60 次的参数写淹掉。
//!
//! 所以这里的命令是**低频**的（一次拖动一次），高频的那条在 `editor::editor_set_params`
//! （只发内存里的参数给渲染线程，不碰数据库）。
//!
//! # 与 `browse` 的关系
//!
//! 库是**同一个**（`BrowseState::with_catalog` 缓存着当前打开的 catalog），
//! 所以这里不另开库、不另起写者线程 —— 编辑与浏览共用一套连接与写者。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

use raybend::store::assets;
use raybend::store::develop::{self, DevelopStack, IssueChoice};
use raybend::store::marking::{ChangeSet, Op};
use raybend::store::repository::{self, RepositoryState};
use raybend::store::time;

use crate::browse::BrowseState;
use crate::db::DbState;
use crate::source::blocking;

/// 一张照片的编辑栈（`latest`）。
///
/// `values` 只装**与基线不同的项**（没动过的不出现）；`curves` 只装动过的通道。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopStackDto {
    pub values: BTreeMap<String, f64>,
    pub curves: BTreeMap<String, Vec<[f32; 2]>>,
    /// 拍摄色温（K）—— 色温拉杆的基线，**跟着 issue 一起存**（见 `store::develop::DevelopStack`）
    #[serde(default)]
    pub as_shot_k: Option<f32>,
}

impl From<DevelopStack> for DevelopStackDto {
    fn from(stack: DevelopStack) -> Self {
        Self {
            values: stack.params,
            curves: stack.curves,
            as_shot_k: stack.as_shot_k,
        }
    }
}

impl DevelopStackDto {
    /// 转成 store 的形态。
    #[must_use]
    pub fn into_stack(self) -> DevelopStack {
        DevelopStack {
            params: self.values,
            curves: self.curves,
            as_shot_k: self.as_shot_k,
        }
    }
}

/// 落库的结果：栈 + 撤销栈快照（界面据此显示「撤销：调整参数」）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopCommitResult {
    pub stack: DevelopStackDto,
    /// 刚记进撤销栈的那一步叫什么（没改动就是 `null`）
    pub undo_label: Option<String>,
    pub can_undo: bool,
}

/// 一个文件属于哪个库的哪个资产（找不到 = 未入库 / 库不在线）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAsset {
    pub repository_id: String,
    pub asset_id: i64,
    /// 库根（在线路径）
    pub root: PathBuf,
    /// 库内相对路径（'/' 分隔）
    pub rel_path: String,
}

/// 按**绝对路径**反查资产。
///
/// 为什么需要它：缩略图与看图那两条命令只有**路径**（前端给的就是绝对路径），
/// 而「这张照片编辑过吗」的答案在 catalog 里。与其让前端到处传
/// `repositoryId + assetId`（缩略图队列是导入/浏览共用的，加参数要动一串），
/// 不如在这一层按路径反查一次：库有几个、路径前缀匹配一次、按折叠路径查一行。
/// **都是内存 + 索引查询，微秒级**（`AGENTS.md` §2.13：本地应用以「看到真相」为先）。
///
/// 找不到（源文件还没入库 / 库离线 / 不在任何库根下）返回 `None` —— 调用方走老路。
pub fn resolve_asset<R: Runtime>(app: &AppHandle<R>, path: &Path) -> Option<ResolvedAsset> {
    let app_db = app.state::<DbState>();
    let candidates = app_db
        .with(app, |db| {
            let rows = db
                .read(repository::list_repositories)
                .map_err(|e| e.to_string())?;
            let mut out: Vec<(String, PathBuf)> = Vec::new();
            for row in rows {
                if let Ok(RepositoryState::Online { root }) =
                    db.read(|conn| repository::resolve_repository(conn, &row.id))
                {
                    out.push((row.id.clone(), root));
                }
            }
            Ok(out)
        })
        .ok()?;

    for (repository_id, root) in candidates {
        let Ok(relative) = path.strip_prefix(&root) else {
            continue;
        };
        let rel_path = relative.to_string_lossy().replace('\\', "/");
        if rel_path.is_empty() {
            continue;
        }
        let rel_for_query = rel_path.clone();
        let browse = app.state::<BrowseState>();
        let found = browse
            .with_catalog(app, &repository_id, move |db| {
                db.read(move |conn| assets::find_by_rel_path(conn, &rel_for_query))
                    .map_err(|e| e.to_string())
            })
            .ok()
            .flatten();
        if let Some(asset_id) = found {
            return Some(ResolvedAsset {
                repository_id,
                asset_id,
                root,
                rel_path,
            });
        }
    }
    None
}

/// 这张照片该显示哪个 issue（连同它的编辑栈）。
///
/// # Errors
/// 库没打开 / 数据库读失败。
pub fn issue_of<R: Runtime>(
    app: &AppHandle<R>,
    asset: &ResolvedAsset,
) -> Result<(IssueChoice, DevelopStack), String> {
    let browse = app.state::<BrowseState>();
    browse.with_catalog(app, &asset.repository_id, |db| {
        let asset_id = asset.asset_id;
        let choice = db
            .read(move |conn| develop::choose_issue(conn, asset_id))
            .map_err(|e| e.to_string())?;
        let stack = db
            .read(move |conn| develop::load(conn, asset_id))
            .map_err(|e| e.to_string())?;
        Ok((choice, stack))
    })
}

/// 编辑器该编辑哪个文件（「编辑落在 RAW 上」）。
///
/// 返回**绝对路径**（前端直接拿去 `editor_set_photo`）；没有可编辑文件时返回 `None`。
///
/// # Errors
/// 库没打开 / 数据库读失败。
#[tauri::command]
pub async fn develop_edit_target<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    asset_id: i64,
) -> Result<Option<String>, String> {
    let handle = app.clone();
    blocking(move || {
        // 先把库根解析出来（**不能**在 `with_catalog` 里做：那把锁正被持着）
        let root = crate::browse::resolve_root(&handle, &repository_id)?;
        let state = handle.state::<BrowseState>();
        state.with_catalog(&handle, &repository_id, |db| {
            let rel = db
                .read(move |conn| develop::edit_target(conn, asset_id))
                .map_err(|e| e.to_string())?;
            let Some(rel) = rel else {
                return Ok(None);
            };
            Ok(Some(
                root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR))
                    .to_string_lossy()
                    .into_owned(),
            ))
        })
    })
    .await
}

/// 读一张照片的编辑栈（没有就是空栈 —— 界面据此显示「未编辑」）。
///
/// # Errors
/// 库没打开 / 数据库读失败。
#[tauri::command]
pub async fn develop_get<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    asset_id: i64,
) -> Result<DevelopStackDto, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        state.with_catalog(&handle, &repository_id, |db| {
            let stack = db
                .read(|conn| develop::load(conn, asset_id))
                .map_err(|e| e.to_string())?;
            Ok(DevelopStackDto::from(stack))
        })
    })
    .await
}

/// **落库**（覆盖式）：把前端那份载荷原样存下来。
///
/// 语义：载荷里没有的项 = 没动过 = 删掉。所以「拖回默认」与「重置这一项」
/// 在数据上是同一件事，不需要额外的接口。
///
/// # Errors
/// 参数 / 曲线不合法（校验在 `store::develop::save` 里，**写之前**就报错）、
/// 或数据库写失败。
#[tauri::command]
pub async fn develop_commit<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    asset_id: i64,
    values: BTreeMap<String, f64>,
    curves: BTreeMap<String, Vec<[f32; 2]>>,
    as_shot_k: Option<f32>,
) -> Result<DevelopCommitResult, String> {
    let handle = app.clone();
    let undo_repository = repository_id.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        let stack = DevelopStack {
            params: values,
            curves,
            as_shot_k,
        };
        let (stored, ops) = state.with_catalog(&handle, &repository_id, move |db| {
            // ① 先读旧值（撤销要它 —— 「读旧值 → 写新值」在同一次调用里完成）
            let before = db
                .read(|conn| develop::load(conn, asset_id))
                .map_err(|e| e.to_string())?;
            // ② 覆盖式写入
            let now = time::now_millis();
            db.write_tx(move |tx| develop::save(tx, asset_id, &stack, now))
                .map_err(|e| e.to_string())?;
            // ③ 回读一遍（写进去的与读出来的必须是同一份 —— 顺手把序列化问题挡在这里）
            let stored = db
                .read(|conn| develop::load(conn, asset_id))
                .map_err(|e| e.to_string())?;
            let ops = diff_stack(asset_id, &before, &stored);
            Ok((stored, ops))
        })?;

        // ④ 大图缓存立刻作废（编辑改了画面，旧的 latest.avif 不许再被读到）
        if let Ok(root) = crate::browse::resolve_root(&handle, &undo_repository)
            && let Ok(cache) = raybend::display::FullCache::open(&root)
        {
            let removed = cache.invalidate(asset_id);
            if removed > 0 {
                eprintln!("[develop] 编辑落库，作废 {removed} 个大图缓存文件");
            }
        }

        // ⑤ 记进**同一个**撤销栈（`browse` 的标记操作与编辑共用一套历史 —— 不另造一份）
        let label = undo_label(&ops);
        let (undo_label, can_undo) = if ops.is_empty() {
            (None, false)
        } else {
            let change = ChangeSet::new(label, ops);
            state
                .with_undo(&undo_repository, |undo| {
                    undo.push(change);
                    (
                        undo.undo_label().map(str::to_string),
                        undo.can_undo(),
                    )
                })?
        };

        Ok(DevelopCommitResult {
            stack: DevelopStackDto::from(stored),
            undo_label,
            can_undo,
        })
    })
    .await
}

/// 新旧两份栈的差量（撤销补丁）。
///
/// 只记**真的变了**的项：拖回原位再松手不该占一步撤销。
fn diff_stack(asset_id: i64, before: &DevelopStack, after: &DevelopStack) -> Vec<Op> {
    let mut ops = Vec::new();

    for (param_id, value) in &after.params {
        let old = before.params.get(param_id).copied();
        if old != Some(*value) {
            ops.push(Op::DevelopParam {
                asset_id,
                param_id: param_id.clone(),
                before: old.map(f64::to_bits),
                after: Some(value.to_bits()),
            });
        }
    }
    for (param_id, value) in &before.params {
        if !after.params.contains_key(param_id) {
            ops.push(Op::DevelopParam {
                asset_id,
                param_id: param_id.clone(),
                before: Some(value.to_bits()),
                after: None,
            });
        }
    }

    for (channel, points) in &after.curves {
        let old = before.curves.get(channel);
        if old != Some(points) {
            ops.push(Op::DevelopCurve {
                asset_id,
                channel: channel.clone(),
                before: old.and_then(|points| serde_json::to_string(points).ok()),
                after: serde_json::to_string(points).ok(),
            });
        }
    }
    for (channel, points) in &before.curves {
        if !after.curves.contains_key(channel) {
            ops.push(Op::DevelopCurve {
                asset_id,
                channel: channel.clone(),
                before: serde_json::to_string(points).ok(),
                after: None,
            });
        }
    }

    ops
}

/// 撤销菜单里那一句（例如「调整参数」/「调整曲线」）。
fn undo_label(ops: &[Op]) -> String {
    let mut params = 0usize;
    let mut curves = 0usize;
    for op in ops {
        match op {
            Op::DevelopParam { .. } => params += 1,
            Op::DevelopCurve { .. } => curves += 1,
            _ => {}
        }
    }
    match (params, curves) {
        (0, 0) => "调整".to_string(),
        (_, 0) => "调整参数".to_string(),
        (0, _) => "调整曲线".to_string(),
        _ => "调整参数与曲线".to_string(),
    }
}

/// **重置全部**：清掉这张照片的编辑栈（回到与 SOOC 一致）。
///
/// 返回值与 `develop_commit` 同一个形状（前端两条路共用一套读数）。
///
/// # Errors
/// 库没打开 / 数据库写失败。
#[tauri::command]
pub async fn develop_reset<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    asset_id: i64,
) -> Result<DevelopCommitResult, String> {
    let handle = app.clone();
    let undo_repository = repository_id.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        let ops = state.with_catalog(&handle, &repository_id, move |db| {
            let before = db
                .read(|conn| develop::load(conn, asset_id))
                .map_err(|e| e.to_string())?;
            db.write_tx(move |tx| develop::clear(tx, asset_id))
                .map_err(|e| e.to_string())?;
            // 「重置全部」也要能撤销：把旧值整份记下来
            let ops = diff_stack(asset_id, &before, &DevelopStack::default());
            Ok(ops)
        })?;
        if let Ok(root) = crate::browse::resolve_root(&handle, &undo_repository)
            && let Ok(cache) = raybend::display::FullCache::open(&root)
        {
            cache.invalidate(asset_id);
        }
        let label = undo_label(&ops);
        let (undo_label, can_undo) = if ops.is_empty() {
            (None, false)
        } else {
            let change = ChangeSet::new(label, ops);
            state.with_undo(&undo_repository, |undo| {
                undo.push(change);
                (undo.undo_label().map(str::to_string), undo.can_undo())
            })?
        };
        Ok(DevelopCommitResult {
            stack: DevelopStackDto::default(),
            undo_label,
            can_undo,
        })
    })
    .await
}
