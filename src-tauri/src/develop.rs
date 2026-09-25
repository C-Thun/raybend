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

use raybend::develop::denoise::NrMethod;
use raybend::develop::geometry::EditGeometry;
use raybend::store::assets;
use raybend::store::develop::{self, DevelopStack, EditBase, IssueChoice, Setting};
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
    /// latest 的唯一来源；旧 IPC 调用没有此项时沿用编辑器默认 RAW。
    #[serde(default)]
    pub source_base: Option<String>,
    pub curves: BTreeMap<String, Vec<[f32; 2]>>,
    /// 拍摄色温（K）—— 色温拉杆的基线，**跟着 issue 一起存**（见 `store::develop::DevelopStack`）
    #[serde(default)]
    pub as_shot_k: Option<f32>,
    /// 镜头配置文件（`null` = 自动识别；`"none"` = 显式关掉；否则是 `maker|model`）
    #[serde(default)]
    pub lens_profile: Option<String>,
    /// 配置文件那一半的开关（`null` = 默认开）
    #[serde(default)]
    pub lens_enabled: Option<bool>,
    /// 降噪方式（`null` = 快速档；`"high"` = BM3D）
    #[serde(default)]
    pub nr_method: Option<String>,
    /// 无损裁切/旋转。
    #[serde(default)]
    pub geometry: Option<EditGeometry>,
}

impl From<DevelopStack> for DevelopStackDto {
    fn from(stack: DevelopStack) -> Self {
        Self {
            values: stack.params,
            source_base: Some(stack.source_base.as_str().to_string()),
            curves: stack.curves,
            as_shot_k: stack.as_shot_k,
            lens_profile: stack.lens_profile,
            lens_enabled: stack.lens_enabled,
            nr_method: stack.nr_method.map(|method| method.as_str().to_string()),
            geometry: stack.geometry,
        }
    }
}

impl DevelopStackDto {
    /// 转成 store 的形态。
    ///
    /// # Errors
    /// 降噪方式认不出（前端只能发 `"fast"` / `"high"`；发别的就是 bug，**不静默当默认**）。
    pub fn into_stack(self) -> Result<DevelopStack, String> {
        let nr_method = match self.nr_method {
            Some(text) => Some(NrMethod::parse(&text).ok_or_else(|| {
                format!("未知的降噪方式：{text}（只认 fast / high）")
            })?),
            None => None,
        };
        let source_base = self.source_base.as_deref().map_or(Ok(EditBase::Raw), |text| {
            EditBase::parse(text).ok_or_else(|| format!("未知的 issue 源：{text}"))
        })?;
        Ok(DevelopStack {
            source_base,
            params: self.values,
            curves: self.curves,
            as_shot_k: self.as_shot_k,
            lens_profile: self.lens_profile,
            lens_enabled: self.lens_enabled,
            nr_method,
            geometry: self.geometry,
        })
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

/// 这张照片在**指定基准**下的源文件（绝对路径）。
///
/// 与 [`develop_edit_target`] 同一个口径（走 `store::develop::edit_target`），
/// 只是这里给**进程内**调用方用（编辑器的过渡帧计划要拿 SOOC 那一侧的文件），
/// 不经过 IPC 往返。没那一侧的文件（只有 RAW 没有 JPG）就是 `None`。
pub fn source_path_of<R: Runtime>(
    app: &AppHandle<R>,
    asset: &ResolvedAsset,
    base: raybend::store::develop::EditBase,
) -> Option<PathBuf> {
    let browse = app.state::<BrowseState>();
    let asset_id = asset.asset_id;
    let rel = browse
        .with_catalog(app, &asset.repository_id, move |db| {
            db.read(move |conn| develop::edit_target(conn, asset_id, base))
                .map_err(|e| e.to_string())
        })
        .ok()??;
    Some(
        asset
            .root
            .join(rel.replace('/', std::path::MAIN_SEPARATOR_STR)),
    )
}

/// 编辑器该编辑哪个文件（「编辑落在 RAW 上」）。
///
/// 返回**绝对路径**（前端直接拿去 `editor_set_photo`）；没有可编辑文件时返回 `None`。
///
/// **编辑器该编辑哪个文件**（「编辑落在 RAW 上」，`REPOSITORY.md` §4.1）＋
/// 这张照片**能不能切到另一侧**（人类 2026-09-24：总览图下的 SOOC / RAW 切换按钮）。
///
/// `base`：`"sooc"` / `"raw"`（缺省 `"raw"`，人类定的默认值）；
/// 认不出的词**报错**，不静默回退 —— 否则界面上按钮显示的和实际编的不是同一张。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditTargetDto {
    /// 实际会编辑的文件（**绝对路径**，前端直接拿去 `editor_set_photo`）；
    /// `None` = 没有可编辑的文件（资产缺文件 / 库离线）
    pub path: Option<String>,
    /// 有可用的位图（SOOC 那一侧）吗
    pub has_bitmap: bool,
    /// 有可用的 RAW 吗
    pub has_raw: bool,
    /// 实际取到的源（某侧缺失时可能从另一侧回退）。
    pub actual_base: Option<String>,
}

/// 解析编辑基准（缺省 RAW）。
fn parse_edit_base(base: Option<&str>) -> Result<raybend::store::develop::EditBase, String> {
    match base {
        None => Ok(raybend::store::develop::EditBase::Raw),
        Some(text) => raybend::store::develop::EditBase::parse(text)
            .ok_or_else(|| format!("未知的编辑基准：{text}（只认 sooc / raw）")),
    }
}

/// # Errors
/// 库没打开 / 数据库读失败 / 编辑基准认不出。
#[tauri::command]
pub async fn develop_edit_target<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    asset_id: i64,
    base: Option<String>,
) -> Result<EditTargetDto, String> {
    let base = parse_edit_base(base.as_deref())?;
    let handle = app.clone();
    blocking(move || {
        // 先把库根解析出来（**不能**在 `with_catalog` 里做：那把锁正被持着）
        let root = crate::browse::resolve_root(&handle, &repository_id)?;
        let state = handle.state::<BrowseState>();
        state.with_catalog(&handle, &repository_id, |db| {
            let (rel, available) = db
                .read(move |conn| {
                    let rel = develop::edit_target(conn, asset_id, base)?;
                    let available = develop::edit_base_available(conn, asset_id)?;
                    Ok((rel, available))
                })
                .map_err(|e| e.to_string())?;
            let (has_bitmap, has_raw) = available;
            let path = rel.map(|rel| {
                root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR))
                    .to_string_lossy()
                    .into_owned()
            });
            let actual_base = path.as_deref().map(|path| {
                EditBase::of_file(Path::new(path)).as_str().to_string()
            });
            Ok(EditTargetDto {
                path,
                has_bitmap,
                has_raw,
                actual_base,
            })
        })
    })
    .await
}

/// **刷新 preview**（`IMAGING.md` §4）：编辑器**进 / 出**两个节点各调一次。
///
/// preview = `<库根>/cache/full/<asset>/latest-v<pipeline>.avif`（长边 1920，AVIF）——
/// 与 `view_image` 走的是**同一份**（`thumbs::render_latest_cached`：命中只读、未命中才渲染），
/// 所以两边不会各写一份。
///
/// 三条口径（人类 2026-09-24 定，规则本体在 `store::develop::needs_preview`）：
///
/// * **没编辑过就什么都不做** —— SOOC / RAW 的内置位图就代替 preview；
/// * **进编辑**时调一次：缓存多半已被上一次落库作废（`develop_commit` 会删它），
///   这一下把预览重新备好；
/// * **退出编辑**时再调一次：把「最后剩下的状态」落成预览。
///
/// 返回 `false` = 没生成（没编辑过 / 资产找不到），**不是错误**。
///
/// # Errors
/// 渲染失败或库读失败。
#[tauri::command]
pub async fn develop_preview_refresh<R: Runtime>(
    app: AppHandle<R>,
    path: String,
) -> Result<bool, String> {
    let handle = app.clone();
    blocking(move || {
        let Some(asset) = resolve_asset(&handle, Path::new(&path)) else {
            return Ok(false);
        };
        let (choice, stack) = issue_of(&handle, &asset)?;
        if !raybend::store::develop::needs_preview(choice, &stack) {
            return Ok(false);
        }
        crate::thumbs::render_latest_cached(&handle, &asset, &stack)?;
        Ok(true)
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
    stack: DevelopStackDto,
) -> Result<DevelopCommitResult, String> {
    let handle = app.clone();
    let undo_repository = repository_id.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        let stack = stack.into_stack()?;
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

    // 编辑栈设置（镜头配置文件 / 启用开关 / 降噪方式）：三项都是「一个可空字符串」
    for setting in [Setting::LensProfile, Setting::LensEnabled, Setting::NrMethod, Setting::SourceBase, Setting::Geometry] {
        let old = before.setting_value(setting);
        let new = after.setting_value(setting);
        if old != new {
            ops.push(Op::DevelopSetting {
                asset_id,
                key: setting.key().to_string(),
                before: old,
                after: new,
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

#[cfg(test)]
mod geometry_tests {
    use super::*;
    use raybend::develop::geometry::{CropRect, EditGeometry};

    #[test]
    fn geometry_confirmation_and_reset_are_single_reversible_settings() {
        let baseline = DevelopStack::default();
        let edited = DevelopStack {
            geometry: Some(EditGeometry { rotation: 15.0,
                crop: Some(CropRect { x: 0.2, y: 0.2, width: 0.6, height: 0.6 }), crop_ratio: None }),
            ..DevelopStack::default()
        };
        let forward = diff_stack(7, &baseline, &edited);
        assert!(matches!(forward.as_slice(), [Op::DevelopSetting { asset_id: 7, key, before: None, after: Some(_) }]
            if key == "geometry"));
        let backward = diff_stack(7, &edited, &baseline);
        assert!(matches!(backward.as_slice(), [Op::DevelopSetting { asset_id: 7, key, before: Some(_), after: None }]
            if key == "geometry"));
    }
}
