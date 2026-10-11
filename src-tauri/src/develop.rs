//! 编辑栈的 IPC（M3-W3）：读 / 落库（含两层重置）。
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
use raybend::store::develop::{
    self, AutoAdjustBaseline, DevelopStack, EditBase, IssueChoice, Setting,
};
use raybend::store::marking::{ChangeSet, Op};
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
    #[serde(default)]
    pub color: Option<raybend::color::PhotoColorState>,
    pub values: BTreeMap<String, f64>,
    /// latest 的唯一来源；旧 IPC 调用没有此项时沿用编辑器默认 RAW。
    #[serde(default)]
    pub source_base: Option<String>,
    pub curves: BTreeMap<String, Vec<[f32; 2]>>,
    /// 拍摄色温（K）—— 色温拉杆的基线，**跟着 issue 一起存**（见 `store::develop::DevelopStack`）
    #[serde(default)]
    pub as_shot_k: Option<f32>,
    /// 镜头配置文件（`null` = 未选择；`"none"` = 显式关掉；否则是 `maker|model`）
    #[serde(default)]
    pub lens_profile: Option<String>,
    /// 机型基础曲线：NULL = 未选择；"none" = 明确不用；数字字符串 = 档案 ID。
    #[serde(default)]
    pub base_curve_profile: Option<String>,
    /// 选中时的曲线快照；用于旧照片渲染稳定性与缓存指纹。
    #[serde(default)]
    pub base_curve_points: Option<Vec<[f32; 2]>>,
    #[serde(default)]
    pub lut_id: Option<String>,
    #[serde(default)]
    pub lut_enabled: Option<bool>,
    /// 配置文件那一半的开关（`null` = 默认开）
    #[serde(default)]
    pub lens_enabled: Option<bool>,
    /// 降噪方式（`null` = 快速档；`"high"` = BM3D）
    #[serde(default)]
    pub nr_method: Option<String>,
    /// 无损裁切/旋转。
    #[serde(default)]
    pub geometry: Option<EditGeometry>,
    #[serde(default)]
    pub auto_adjust: Option<AutoAdjustBaseline>,
}

impl From<DevelopStack> for DevelopStackDto {
    fn from(stack: DevelopStack) -> Self {
        Self {
            color: stack.color,
            values: stack.params,
            source_base: Some(stack.source_base.as_str().to_string()),
            curves: stack.curves,
            as_shot_k: stack.as_shot_k,
            lens_profile: stack.lens_profile,
            base_curve_profile: stack.base_curve_profile,
            base_curve_points: stack.base_curve_points,
            lut_id: stack.lut_id,
            lut_enabled: stack.lut_enabled,
            lens_enabled: stack.lens_enabled,
            nr_method: stack.nr_method.map(|method| method.as_str().to_string()),
            geometry: stack.geometry,
            auto_adjust: stack.auto_adjust,
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
            Some(text) => Some(
                NrMethod::parse(&text)
                    .ok_or_else(|| format!("未知的降噪方式：{text}（只认 fast / high）"))?,
            ),
            None => None,
        };
        let source_base = self
            .source_base
            .as_deref()
            .map_or(Ok(EditBase::Raw), |text| {
                EditBase::parse(text).ok_or_else(|| format!("未知的 issue 源：{text}"))
            })?;
        Ok(DevelopStack {
            color: self.color,
            source_base,
            params: self.values,
            curves: self.curves,
            as_shot_k: self.as_shot_k,
            lens_profile: self.lens_profile,
            base_curve_profile: self.base_curve_profile,
            base_curve_points: self.base_curve_points,
            lut_id: self.lut_id,
            lut_enabled: self.lut_enabled,
            lens_enabled: self.lens_enabled,
            nr_method,
            geometry: self.geometry,
            auto_adjust: self.auto_adjust,
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
        .with(app, |db| db.list_repositories().map_err(|e| e.to_string()))
        .ok()?;
    for row in candidates {
        // 只探测绝对路径实际落入的登记位置，避免看一张图扫所有库。
        if !row
            .paths
            .iter()
            .any(|p| path.starts_with(Path::new(&p.path)))
        {
            continue;
        }
        let Ok(catalog) = app.state::<BrowseState>().lease(app, &row.id) else {
            continue;
        };
        let repository_id = row.id;
        let root = catalog.root().to_path_buf();

        let Ok(relative) = path.strip_prefix(&root) else {
            continue;
        };
        let rel_path = relative.to_string_lossy().replace('\\', "/");
        if rel_path.is_empty() {
            continue;
        }
        let found = catalog
            .read(|conn| assets::find_by_rel_path(conn, &rel_path))
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

/// 一个**目录**落在哪个库的根里（与 [`resolve_asset`] 同一套前缀匹配，只是按目录一次查完）。
///
/// 用途：`dir_meta_ensure` 刚读到的展示尺寸要写回库里的文件行（老库懒回填，
/// `specs/display-size-source.md` §3.2）—— 那里只有目录与一批文件名，
/// 逐文件反查会把同一个库查 N 遍。
/// 返回 `(库 ID, 库内相对目录, `'/'` 分隔；库根本身时是空串)`。
/// 不在任何库根下 / 库不在线 → `None`。
pub(crate) fn catalog_for_dir<R: Runtime>(
    app: &AppHandle<R>,
    dir: &Path,
) -> Option<(String, std::sync::Arc<raybend::store::db::CatalogDb>, String)> {
    let app_db = app.state::<DbState>();
    let candidates = app_db
        .with(app, |db| db.list_repositories().map_err(|e| e.to_string()))
        .ok()?;
    for row in candidates {
        if !row
            .paths
            .iter()
            .any(|p| dir.starts_with(Path::new(&p.path)))
        {
            continue;
        }
        let Ok(catalog) = app.state::<BrowseState>().lease(app, &row.id) else {
            continue;
        };
        let root = catalog.root().to_path_buf();
        let Ok(relative) = dir.strip_prefix(&root) else {
            continue;
        };
        let rel = relative.to_string_lossy().replace('\\', "/");
        return Some((row.id, catalog, rel));
    }
    None
}

/// **latest 预览快照的版本名**（`specs/editor-session-pipeline.md` §4.2）：
/// `latest-<profile_hash 前 12 位>`，与 `issue-<id>-<hash>` 同一套哈希
/// （`raybend::store::issues::profile_hash`）。
///
/// 栈一变名字就变 —— 读取方（过渡帧 / 缩略图 / 定稿复用）自然失配，
/// 不再靠「落库删整目录」保证一致性（旧文件由 `FullCache::gc_latest` 回收）。
/// **拼接只此一处**，不在别处再写一遍。
pub(crate) fn latest_variant(stack: &DevelopStack) -> Result<String, String> {
    let hash = raybend::store::issues::profile_hash(stack).map_err(|error| error.to_string())?;
    Ok(format!("latest-{}", &hash[..12]))
}

/// latest 预览的**单飞 key**（同一张照片 + 同一份栈 = 同一个 key）：
/// 前端预览队列与定稿快照等待用同一个，保证同一画面只渲一遍。
pub(crate) fn latest_preview_key(
    repository_id: &str,
    asset_id: i64,
    stack: &DevelopStack,
) -> Result<String, String> {
    Ok(format!(
        "latest-preview:{repository_id}:{asset_id}:{}",
        latest_variant(stack)?
    ))
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

/// **编辑器参照源**（`specs/editor-session-pipeline.md` §5.1）。
///
/// 以前这份事实挂在「过渡帧计划」上 —— 只有换照片那一条任务带，任何不带计划的
/// 缓存重建都会静默丢掉它，对比参照就退化成 RAW（现象 1）。现在由命令层按 asset
/// 解析一次，随换照片任务交给显影线程，显影线程**跨缓存重建保留**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceSources {
    /// SOOC 位图（同名 JPG 那一侧）；`None` = 这张照片没有位图侧。
    pub sooc: Option<PathBuf>,
    /// SOOC 位图可用吗 —— **SOOC 参照能不能用**的唯一判据。
    /// `false` 时不允许拿 RAW 文件冒充 SOOC 参照（编辑基准的兜底语义不受影响）。
    pub bitmap_available: bool,
}

impl ReferenceSources {
    /// 按 asset 解析（不在库里 / 库离线 → `None`：调用方走「没有参照源」那条路）。
    #[must_use]
    pub fn resolve<R: Runtime>(app: &AppHandle<R>, asset: &ResolvedAsset) -> Self {
        let sooc = source_path_of(app, asset, raybend::store::develop::EditBase::Sooc).filter(
            |path| {
                path.is_file()
                    && raybend::store::develop::EditBase::of_file(path)
                        == raybend::store::develop::EditBase::Sooc
            },
        );
        Self {
            bitmap_available: sooc.is_some(),
            sooc,
        }
    }
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
/// **编辑器该编辑哪个文件**（「编辑落在 RAW 上」，`memory/FUNCTION-REPOSITORY.md` §4.1）＋
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
        let state = handle.state::<BrowseState>();
        state.with_catalog(&handle, &repository_id, |db| {
            let root = db.root().to_path_buf();
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
            let actual_base = path
                .as_deref()
                .map(|path| EditBase::of_file(Path::new(path)).as_str().to_string());
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

/// **刷新 preview**（`memory/FUNCTION-IMAGING.md` §4）：编辑器**进 / 出**两个节点各调一次。
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
        /*
         * 走**后台队列的「同 key 单飞」**（§4.3）：同一张照片、同一份栈的预览，
         * 不管几个调用者（前端预览队列 / 定稿复用 / 进编辑器预温）都只渲一遍；
         * 后来的调用者等前一个的结果（条件变量，不轮询）。
         */
        let key = latest_preview_key(&asset.repository_id, asset.asset_id, &stack)?;
        let queue = handle.state::<crate::work::WorkQueue>();
        let task_app = handle.clone();
        queue.run(crate::work::Priority::Warm, key, move |_token| {
            crate::thumbs::render_latest_cached(&task_app, &asset, &stack)?;
            let db = task_app.state::<crate::thumbs::SourcesThumbs>().get(
                &crate::thumbs::sources_cache_dir(&task_app)?, time::now_millis())?;
            let source = source_path_of(&task_app, &asset, stack.source_base)
                .ok_or("最新编辑源不可用")?;
            let variant = latest_variant(&stack)?;
            // 只备缩略图那一档（384）—— 胶片带也吃它（192 那一档 2026-10-11 取消）
            crate::thumbs::render_profile_thumb(
                &task_app, &db, &asset, &source, &stack,
                raybend::thumbnail::SizeClass::Grid, Some(&variant),
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })?;
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
        let mut stack = stack.into_stack()?;
        /*
         * **温度 dirty ⇒ `as_shot_k` 非空**（`specs/editor-session-pipeline.md` §6.3）。
         *
         * 温度是**相对基线**的：基线缺失的栈落库后，编辑器重开只能按静态默认 6250K
         * 解释它（库里 `as_shot_k NULL` + 显式温度那类污染的来源）。
         * 会话里有解码真值就回填；两边都拿不到记诊断 —— **不静默写一个 null 基线**。
         */
        if stack.params.contains_key("temperature") && stack.as_shot_k.is_none() {
            match crate::editor::decoded_as_shot(&handle, &repository_id, asset_id, stack.source_base) {
                Some(shot) => {
                    // i18n-exempt: 落库诊断（终端日志，不进界面）
                    log::warn!("[develop] 落库回填 as-shot 基线 {shot}K（asset {asset_id}）");
                    stack.as_shot_k = Some(shot);
                }
                None => {
                    // i18n-exempt: 落库诊断（终端日志，不进界面）
                    log::warn!(
                        "[develop] 落库诊断：温度是显式值但 as_shot_k 缺失，会话里也没有解码真值\
                         （asset {asset_id}）——渲染会按解码真值算，编辑器下次打开时白平衡基线需重设"
                    );
                }
            }
        }
        let (stored, ops) = state.with_catalog(&handle, &repository_id, move |db| {
            /*
             * 懒回填：这张照片**缺的尺寸记录**顺手补齐（文件行 + latest 输出 + 各定稿输出，
             * `specs/display-size-lifecycle.md` §2）。文件 I/O 在写事务之下；失败只记日志。
             */
            if let Err(error) = raybend::store::sizes::ensure_sizes(db, asset_id) {
                // i18n-exempt: 终端诊断（尺寸补不上只用回退链，不拦提交）
                log::warn!("[develop] 源尺寸懒回填跳过（asset {asset_id}）：{error}");
            }
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

        // ④′ **不再整目录作废**（W2 §4.2）：latest 快照按栈指纹命名，旧文件
        //     自然失配；真正的回收交给 `FullCache::gc_latest`（写新快照之后顺手做）。

        // ⑤ 记进**同一个**撤销栈（`browse` 的标记操作与编辑共用一套历史 —— 不另造一份）
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

        // ⑥ sidecar 镜像（`specs/xmp-w1.md` §6.1：落库后异步写出，不阻塞提交）
        crate::sidecar::queue_sync(handle.clone(), &undo_repository, vec![asset_id]);

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

    // 设置统一以可空序列化值进入撤销；自动来源也走同一管道，但不改变像素。
    for setting in [
        Setting::Color,
        Setting::AutoAdjust,
        Setting::BaseCurveProfile,
        Setting::BaseCurvePoints,
        Setting::LutId,
        Setting::LutEnabled,
        Setting::LensProfile,
        Setting::LensEnabled,
        Setting::NrMethod,
        Setting::SourceBase,
        Setting::Geometry,
    ] {
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

#[cfg(test)]
mod geometry_tests {
    use super::*;
    use raybend::develop::geometry::{CropRect, EditGeometry};

    #[test]
    fn automatic_baseline_and_two_layer_resets_restore_through_shared_undo() {
        use raybend::store::{
            db::{CatalogDb, OpenOpts},
            marking::{self, UndoStack},
        };
        let dir = tempfile::tempdir().unwrap();
        let db = CatalogDb::create(dir.path(), "重置测试", None, OpenOpts::unbacked_up(1)).unwrap();
        db.write(|conn| {
            conn.execute_batch("INSERT INTO assets(id,imported_at,updated_at) VALUES(7,1,1);")
                .unwrap();
            let automatic = DevelopStack {
                params: [("exposure".to_owned(), 0.5)].into(),
                base_curve_profile: Some("3".into()),
                base_curve_points: Some(vec![[0.0, 0.0], [0.5, 0.6], [1.0, 1.0]]),
                auto_adjust: Some(AutoAdjustBaseline {
                    values: [("exposure".to_owned(), 0.5)].into(),
                    ..Default::default()
                }),
                ..Default::default()
            };
            let mut manual = automatic.clone();
            manual.params.insert("exposure".into(), 1.0);
            manual.lut_id = Some("lut".into());
            manual.lut_enabled = Some(true);
            let cleared = DevelopStack::default();
            let mut undo = UndoStack::new(10);
            develop::save(conn, 7, &manual, 1).unwrap();
            for (before, after) in [(&manual, &automatic), (&automatic, &cleared)] {
                let change = ChangeSet::new("重置修改", diff_stack(7, before, after));
                marking::apply(conn, &change).unwrap();
                undo.push(change);
                assert_eq!(develop::load(conn, 7).unwrap(), *after);
            }
            undo.undo(conn).unwrap();
            assert_eq!(develop::load(conn, 7).unwrap(), automatic);
            undo.undo(conn).unwrap();
            assert_eq!(develop::load(conn, 7).unwrap(), manual);
            undo.redo(conn).unwrap();
            assert_eq!(develop::load(conn, 7).unwrap(), automatic);
            undo.redo(conn).unwrap();
            assert_eq!(develop::load(conn, 7).unwrap(), cleared);
            let dto = DevelopStackDto::from(manual.clone());
            assert_eq!(dto.into_stack().unwrap().auto_adjust, manual.auto_adjust);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn geometry_confirmation_and_reset_are_single_reversible_settings() {
        let baseline = DevelopStack::default();
        let edited = DevelopStack {
            geometry: Some(EditGeometry {
                rotation: 15.0,
                crop: Some(CropRect {
                    x: 0.2,
                    y: 0.2,
                    width: 0.6,
                    height: 0.6,
                }),
                crop_ratio: None,
            }),
            ..DevelopStack::default()
        };
        let forward = diff_stack(7, &baseline, &edited);
        assert!(
            matches!(forward.as_slice(), [Op::DevelopSetting { asset_id: 7, key, before: None, after: Some(_) }]
            if key == "geometry")
        );
        let backward = diff_stack(7, &edited, &baseline);
        assert!(
            matches!(backward.as_slice(), [Op::DevelopSetting { asset_id: 7, key, before: Some(_), after: None }]
            if key == "geometry")
        );
    }
}
