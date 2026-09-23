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

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

use raybend::store::develop::{self, DevelopStack};
use raybend::store::marking::{ChangeSet, Op};
use raybend::store::time;

use crate::browse::BrowseState;
use crate::source::blocking;

/// 一张照片的编辑栈（`latest`）。
///
/// `values` 只装**与基线不同的项**（没动过的不出现）；`curves` 只装动过的通道。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopStackDto {
    pub values: BTreeMap<String, f64>,
    pub curves: BTreeMap<String, Vec<[f32; 2]>>,
}

impl From<DevelopStack> for DevelopStackDto {
    fn from(stack: DevelopStack) -> Self {
        Self {
            values: stack.params,
            curves: stack.curves,
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
) -> Result<DevelopCommitResult, String> {
    let handle = app.clone();
    let undo_repository = repository_id.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        let stack = DevelopStack {
            params: values,
            curves,
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

        // ④ 记进**同一个**撤销栈（`browse` 的标记操作与编辑共用一套历史 —— 不另造一份）
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

/// 重置的返回值也是 `DevelopCommitResult`（前端两条路共用一套读数）。

/// **重置全部**：清掉这张照片的编辑栈（回到与 SOOC 一致）。
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
