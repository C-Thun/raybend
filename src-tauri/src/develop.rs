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
) -> Result<DevelopStackDto, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        let stack = DevelopStack {
            params: values,
            curves,
        };
        state.with_catalog(&handle, &repository_id, move |db| {
            let now = time::now_millis();
            db.write_tx(move |tx| develop::save(tx, asset_id, &stack, now))
                .map_err(|e| e.to_string())?;
            // 回读一遍（写进去的与读出来的必须是同一份 —— 顺手把序列化问题挡在这里）
            let stored = db
                .read(|conn| develop::load(conn, asset_id))
                .map_err(|e| e.to_string())?;
            Ok(DevelopStackDto::from(stored))
        })
    })
    .await
}

/// **重置全部**：清掉这张照片的编辑栈（回到与 SOOC 一致）。
///
/// # Errors
/// 库没打开 / 数据库写失败。
#[tauri::command]
pub async fn develop_reset<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    asset_id: i64,
) -> Result<DevelopStackDto, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        state.with_catalog(&handle, &repository_id, move |db| {
            db.write_tx(move |tx| develop::clear(tx, asset_id))
                .map_err(|e| e.to_string())?;
            Ok(DevelopStackDto::default())
        })
    })
    .await
}
