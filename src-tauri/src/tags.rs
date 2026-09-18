//! 标签命令：**全局词典**（`app.db`）侧的查询与创建。
//!
//! 标签分两处存（`AGENTS.md` §6.4）：
//!
//! * **词典**在 `app.db` 的 `tags` 表 —— 名字、折叠名、使用次数。**跨库共用**；
//! * **关联**（哪张照片身上有哪些标签）在各库的 `catalog.db` 的 `asset_tags` 里。
//!
//! 所以这个文件只做词典侧的两件事（[`tag_list`] / [`tag_ensure`]）；
//! 「给照片挂/摘标签」走 `browse_mark` 的 `attachTags` / `detachTags`
//! （它在写事务里改关联，并不会动词典 —— 新标签要先经 `tag_ensure` 建出来）。
//!
//! 使用次数（`use_count`）的同步也在这里的调用方那侧：词典是全局的、关联是分库的，
//! 单库重算必然算少，所以按「本次新增/摘掉」做加减（见 `browse.rs` 的 `sync_tag_counts`）。

use raybend::store::db::AppDb;
use raybend::store::tags;
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

use crate::db::DbState;
use crate::source::blocking;

/// 词典里的一条标签（前端画标签 chips 用）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagDto {
    pub id: i64,
    /// 展示名（保留用户输入的大小写与写法）
    pub name: String,
    pub use_count: i64,
}

impl From<tags::Tag> for TagDto {
    fn from(tag: tags::Tag) -> Self {
        Self {
            id: tag.id,
            name: tag.name,
            use_count: tag.use_count,
        }
    }
}

/// 搜索标签（标签弹窗的「输入即搜」）。
///
/// * `query` 空/缺省 → 按使用次数列出一批常用的（弹窗一打开就有东西可点）；
/// * `limit` 缺省 50、夹在 1..=500（别让前端一次把词典倒出来）。
#[tauri::command]
pub async fn tag_list<R: Runtime>(
    app: AppHandle<R>,
    query: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<TagDto>, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<DbState>();
        let text = query.unwrap_or_default();
        let take = limit.unwrap_or(50).clamp(1, 500);
        state.with(&handle, |db: &AppDb| {
            db.read(|conn| tags::search_tags(conn, &text, take))
                .map(|list| list.into_iter().map(TagDto::from).collect())
                .map_err(|e| e.to_string())
        })
    })
    .await
}

/// 建标签（已有同名就返回已有的那条 —— 幂等）。
///
/// 名字的规范化与校验在 `store::tags::clean_name` 里（去首尾空白、空名与超长报错），
/// 去重用**折叠名**（NFC + 小写）。
#[tauri::command]
pub async fn tag_ensure<R: Runtime>(app: AppHandle<R>, name: String) -> Result<TagDto, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<DbState>();
        state.with(&handle, |db: &AppDb| {
            // 先校验（空名/超长在这里就挡掉，不必进写事务）
            let clean = tags::clean_name(&name).map_err(|e| e.to_string())?;
            db.write(move |conn| {
                let id = tags::ensure_tag(conn, &clean, raybend::store::time::now_millis())?;
                // 建完立刻回读：返回给前端的必须是**库里那条**（名字可能被去重命中）
                tags::tag_by_id(conn, id)?
                    .ok_or_else(|| raybend::Error::Unsupported("标签刚建好却查不到".into()))
            })
            .map(TagDto::from)
            .map_err(|e| e.to_string())
        })
    })
    .await
}
