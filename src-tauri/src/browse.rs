//! 浏览工作流的命令：查询、标记、撤销、旗标、删除。
//!
//! 业务规则全在 `raybend::store`（`query` / `marking` / `flags` / `delete`），
//! 这里只做四件事：
//!
//! 1. 把前端传来的筛选/排序**翻译**成 store 的类型；
//! 2. 管好「当前打开的库」——`CatalogDb` 自带读池与写者线程，**不能每个请求重开一次**；
//! 3. 撤销栈与旗标是会话态，挂在 [`BrowseState`] 上（见各自模块的说明）；
//! 4. 把结果转成前端视图（camelCase，进 `src/api/dto-contract.json` 的键名清单）。
//!
//! ⚠️ 这里的命令**都要走后台线程**（`source::blocking`）：10 万条的查询是毫秒级，
//! 但开库、迁移检查、迁移快照都可能碰磁盘 —— 在 UI 线程上做这些会让界面卡住。

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Mutex;

use raybend::store::db::{CatalogDb, OpenOpts};
use raybend::store::delete::DeleteReport;
use raybend::store::flags::{Flag, FlagKey, FlagSet};
use raybend::store::marking::{self, UndoStack};
use raybend::store::query::{
    self, AssetRow, Combinator, Filter, FlagFilter, Query, Scope, Sort, SortKey,
};
use raybend::store::repository;
use raybend::store::tags;
use raybend::store::time;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

use crate::db::DbState;
use crate::source::blocking;

/// 浏览相关的会话态：撤销栈（每库一份）+ 旗标（跨库一份）。
#[derive(Default)]
pub struct BrowseState {
    undo: Mutex<HashMap<String, UndoStack>>,
    flags: Mutex<FlagSet>,
    /// 当前打开的库（`CatalogDb` 自带连接池与写者线程，重开一次不便宜）。
    open: Mutex<Option<OpenCatalog>>,
    pub(crate) disk_sync: Mutex<()>,
    pub(crate) live_watch: Mutex<Option<(String, PathBuf, raybend::media::watch::CatalogWatcher)>>,
}

struct OpenCatalog {
    repository_id: String,
    root: PathBuf,
    db: CatalogDb,
}

impl BrowseState {
    /// 借出当前库；`repository_id` 变了就换一个开。
    ///
    /// **为什么要缓存**：`CatalogDb::open` 要做迁移检查 + 开读池 + 起写者线程，
    /// 一次十几毫秒。网格滚动时每个窗口都是一次查询 —— 每次重开一遍是白白浪费，
    /// 而且会把写者线程翻来覆去地起停。
    ///
    /// 锁序（避免死锁的最重要一条）：**任何地方都先拿 `open`、再拿 `undo`**，
    /// 绝不反过来。
    pub(crate) fn with_catalog<R: Runtime, T>(
        &self,
        app: &AppHandle<R>,
        repository_id: &str,
        f: impl FnOnce(&CatalogDb) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut guard = self.open.lock().map_err(|_| "内部锁已损坏".to_string())?;
        let root = resolve_root(app, repository_id)?;
        let stale = guard.as_ref().is_none_or(|open| {
            open.repository_id != repository_id || open.root != root || !open.root.is_dir()
        });
        if stale {
            // 先放掉旧库（让它的写者线程收摊），再开新的
            *guard = None;
            *self.live_watch.lock().map_err(|e| e.to_string())? = None;
            let backups = crate::db::data_dir(app)?.join(raybend::store::db::BACKUPS_DIR);
            let db = CatalogDb::open(&root, OpenOpts::new(Some(&backups), time::now_millis()))
                .map_err(|e| e.to_string())?;
            *guard = Some(OpenCatalog {
                repository_id: repository_id.to_string(),
                root,
                db,
            });
        }
        let open = guard.as_ref().ok_or_else(|| "库未打开".to_string())?;
        f(&open.db)
    }

    /// 撤销栈的入口（`open` 之后才会拿它）。
    pub(crate) fn with_undo<T>(
        &self,
        repository_id: &str,
        f: impl FnOnce(&mut UndoStack) -> T,
    ) -> Result<T, String> {
        let mut guard = self.undo.lock().map_err(|_| "内部锁已损坏".to_string())?;
        let stack = guard.entry(repository_id.to_string()).or_default();
        Ok(f(stack))
    }
}

/// 打开库需要的 root：先从 `app.db` 解析出在线路径。
///
/// `pub(crate)`：编辑器要拿库根拼绝对路径（`develop::develop_edit_target`），
/// 而它**不能**在 `with_catalog` 里调（那把 `open` 锁正被持着 —— 会死锁）。
pub(crate) fn resolve_root<R: Runtime>(
    app: &AppHandle<R>,
    repository_id: &str,
) -> Result<PathBuf, String> {
    let state = app.state::<DbState>();
    state.with(app, |db| {
        db.resolve_repository(repository_id)
            .map_err(|e| e.to_string())
            .and_then(|resolved| match resolved {
                repository::RepositoryState::Online { root } => Ok(root),
                repository::RepositoryState::Offline { .. } => {
                    Err(format!("库「{repository_id}」当前离线"))
                }
            })
    })
}

// ─────────────────────────── 输入 DTO ───────────────────────────

/// 筛选条件（前端传过来；字段都可缺省 = 不限）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterDto {
    /// 星级**阈值**：`Some(n)` = 「n 星及以上」（人类 2026-09-19）。
    /// 旧的 `ratings: Vec<i64>`（精确匹配）已废弃，前端也不再发。
    #[serde(default)]
    pub min_rating: Option<i64>,
    /// 旗标筛选：`{ mode, ids }`（旗标只在内存里，id 列表由外壳给 —— 见 `store::flags`）。
    #[serde(default)]
    pub flag: Option<FlagFilterDto>,
    #[serde(default)]
    pub colors: Vec<String>,
    #[serde(default)]
    pub likes: Vec<String>,
    #[serde(default)]
    pub locks: Vec<i64>,
    #[serde(default)]
    pub taken_from: Option<i64>,
    #[serde(default)]
    pub taken_to: Option<i64>,
    #[serde(default)]
    pub cameras: Vec<String>,
    #[serde(default)]
    pub lenses: Vec<String>,
    #[serde(default)]
    pub iso_from: Option<i64>,
    #[serde(default)]
    pub iso_to: Option<i64>,
    #[serde(default)]
    pub focal_from: Option<f64>,
    #[serde(default)]
    pub focal_to: Option<f64>,
    #[serde(default)]
    pub tags: Vec<i64>,
    #[serde(default)]
    pub text: Option<String>,
    /// `"and"` / `"or"`（缺省 = `or`，见 `specs/M2.md` §3 第 1 条）。
    #[serde(default)]
    pub combinator: Option<String>,
}

/// 旗标筛选的输入形态（与前端 `BrowseFilter.flag` 一一对应）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlagFilterDto {
    /// `pick` / `reject` / `none`
    pub mode: String,
    /// 相关的 id（`none` 时是「要排除的」）
    #[serde(default)]
    pub ids: Vec<i64>,
}

impl FilterDto {
    fn into_filter(self) -> Filter {
        Filter {
            min_rating: self.min_rating.and_then(|r| u8::try_from(r).ok()),
            flag: self.flag.map(|flag| FlagFilter {
                mode: flag.mode,
                ids: flag.ids,
            }),
            colors: self.colors,
            likes: self.likes,
            locks: self
                .locks
                .into_iter()
                .filter_map(|l| u8::try_from(l).ok())
                .collect(),
            taken_from: self.taken_from,
            taken_to: self.taken_to,
            cameras: self.cameras,
            lenses: self.lenses,
            iso_from: self.iso_from,
            iso_to: self.iso_to,
            focal_from: self.focal_from,
            focal_to: self.focal_to,
            tags: self.tags,
            text: self.text,
            combinator: match self.combinator.as_deref() {
                Some("and") => Combinator::And,
                _ => Combinator::Or,
            },
        }
    }
}

/// 排序（缺省 = 拍摄时间降序）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortDto {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub desc: Option<bool>,
}

/// 一次浏览查询：库 + 范围 + 筛选 + 排序。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseQueryDto {
    pub repository_id: String,
    /// 库内相对路径（`null` / 缺省 = 整个库）。
    #[serde(default)]
    pub scope_path: Option<String>,
    #[serde(default)]
    pub filter: FilterDto,
    #[serde(default)]
    pub sort: SortDto,
}

impl BrowseQueryDto {
    /// 按当前的 DTO 造一个 store 层的查询。
    ///
    /// 名字不用 `into_*`：那个约定是「消耗 self 的转换」，这里是**借用着造一份新的**。
    fn to_query(&self) -> Query {
        let scope = match self.scope_path.as_deref().map(str::trim) {
            Some(path) if !path.is_empty() => Scope::subtree(path),
            _ => Scope::Repository,
        };
        let key = match self.sort.key.as_deref() {
            Some("importedAt") => SortKey::ImportedAt,
            Some("fileName") => SortKey::FileName,
            Some("rating") => SortKey::Rating,
            Some("camera") => SortKey::Camera,
            _ => SortKey::TakenAt,
        };
        let desc = self.sort.desc.unwrap_or_else(|| Sort::default_desc(key));
        Query {
            scope,
            filter: self.filter.clone().into_filter(),
            sort: Sort::new(key, desc),
        }
    }
}

/// 标记动作（前端点哪个按钮就是哪个 variant）。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MarkActionDto {
    /// 设评分（0–5）。
    Rating { value: i64 },
    /// 设色标（`null` = 无色）。
    Color { value: Option<String> },
    /// 设喜欢（`null` = 取消）。
    Like { value: Option<String> },
    /// 设锁（0–2）。
    Lock { value: i64 },
    /// 挂标签（批量只能加）。
    AttachTags { tag_ids: Vec<i64> },
    /// 摘标签（单张编辑标签弹窗）。
    DetachTags { tag_ids: Vec<i64> },
    /// 改一个可编辑的文字字段（右栏：作者 / 描述 / 国家 / 省州 / 城市 / 具体地点）。
    /// `field` 只认 `TextField::parse` 认的那几个名字，其余报错（**不做静默忽略**）。
    SetText {
        field: String,
        value: Option<String>,
    },
}

// ─────────────────────────── 输出 DTO ───────────────────────────

/// 网格里的一张照片（一次查询把界面要用的字段全拿到）。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetItem {
    pub id: i64,
    pub rel_path: String,
    pub file_name: String,
    pub ext: String,
    pub is_raw: bool,
    /// 这个资产还有没有 RAW（三种 tile 形态靠它与 `is_raw` 一起判，见 `AssetRow.has_raw`）
    pub has_raw: bool,
    /// 右栏「文件基础信息 / 地理信息」（人写的那几项 + EXIF 读的 GPS）
    pub author: Option<String>,
    pub description: Option<String>,
    pub gps_lat: Option<f64>,
    pub gps_lon: Option<f64>,
    pub country: Option<String>,
    pub province_state: Option<String>,
    pub city: Option<String>,
    pub sublocation: Option<String>,
    /// 创建日期（毫秒；取不到出生时间时是修改时间）
    pub created_ms: Option<i64>,
    pub taken_at: Option<i64>,
    pub taken_at_offset_min: Option<i64>,
    pub rating: i64,
    pub color_label: Option<String>,
    pub like_state: Option<String>,
    pub lock_level: i64,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub focal_mm: Option<f64>,
    pub f_number: Option<f64>,
    pub exposure_ms: Option<f64>,
    pub iso: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub orientation: Option<i64>,
    pub size_bytes: Option<i64>,
    pub missing: bool,
}

impl From<AssetRow> for AssetItem {
    fn from(row: AssetRow) -> Self {
        // catalog 存的是文件头里的原始像素轴；Browse DTO 对外给展示尺寸，必须应用方向。
        // 单图 view / film 直接走解码后的图所以一直正常，只有 tiles / compare 读了这两个
        // 原始数值，才会把 orientation=6/8 的竖图压成横图。
        let (width, height) = oriented_dimensions(row.width, row.height, row.orientation);
        Self {
            id: row.id,
            file_name: query::file_name_of(&row.rel_path).to_string(),
            rel_path: row.rel_path,
            ext: row.ext,
            is_raw: row.is_raw,
            has_raw: row.has_raw,
            author: row.author,
            description: row.description,
            gps_lat: row.gps_lat,
            gps_lon: row.gps_lon,
            country: row.country,
            province_state: row.province_state,
            city: row.city,
            sublocation: row.sublocation,
            created_ms: row.created_ms,
            taken_at: row.taken_at,
            taken_at_offset_min: row.taken_at_offset_min,
            rating: i64::from(row.rating),
            color_label: row.color_label,
            like_state: row.like_state,
            lock_level: i64::from(row.lock_level),
            camera_make: row.camera_make,
            camera_model: row.camera_model,
            lens: row.lens,
            focal_mm: row.focal_mm,
            f_number: row.f_number,
            exposure_ms: row.exposure_ms,
            iso: row.iso,
            width,
            height,
            orientation: row.orientation,
            size_bytes: row.size_bytes,
            missing: row.missing,
        }
    }
}

fn oriented_dimensions(
    width: Option<i64>,
    height: Option<i64>,
    orientation: Option<i64>,
) -> (Option<i64>, Option<i64>) {
    if matches!(orientation, Some(5..=8)) {
        (height, width)
    } else {
        (width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::{edited_asset_ids, oriented_dimensions};
    use raybend::store::marking::Op;

    #[test]
    fn only_develop_undo_ops_invalidate_latest_previews() {
        let ops = vec![
            Op::Rating {
                asset_id: 1,
                before: 0,
                after: 1,
            },
            Op::DevelopParam {
                asset_id: 2,
                param_id: "exposure".into(),
                before: None,
                after: Some(0),
            },
            Op::DevelopSetting {
                asset_id: 2,
                key: "sourceBase".into(),
                before: Some("raw".into()),
                after: Some("sooc".into()),
            },
            Op::DevelopCurve {
                asset_id: 3,
                channel: "rgb".into(),
                before: None,
                after: Some("[]".into()),
            },
        ];
        assert_eq!(
            edited_asset_ids(&ops).into_iter().collect::<Vec<_>>(),
            [2, 3]
        );
        assert!(edited_asset_ids(&[]).is_empty());
    }

    #[test]
    fn browse_dimensions_apply_exif_orientation_once() {
        assert_eq!(
            oriented_dimensions(Some(6000), Some(4000), Some(6)),
            (Some(4000), Some(6000)),
            "竖拍 6/8 必须以竖向尺寸交给 tiles 与 compare",
        );
        assert_eq!(
            oriented_dimensions(Some(6000), Some(4000), Some(1)),
            (Some(6000), Some(4000)),
        );
        assert_eq!(
            oriented_dimensions(None, Some(4000), Some(8)),
            (Some(4000), None)
        );
        assert_eq!(
            oriented_dimensions(Some(6000), Some(4000), Some(99)),
            (Some(6000), Some(4000)),
            "非法方向按不旋转处理",
        );
    }
}

/// 一页结果。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseWindow {
    /// 筛选后的总数（虚拟网格用它算滚动高度）。
    pub total: i64,
    pub offset: i64,
    pub items: Vec<AssetItem>,
}

/// 时间线上的一项。
///
/// `relPath` 是**给片内排序用的**：同一时间段内要按文件名自然序排（这样 JPG 与 RAW
/// 会按名字接着），而自然序只能在前端算（见 `raybend::store::query::TimelineRow`）。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEntry {
    pub id: i64,
    pub taken_at: Option<i64>,
    pub rel_path: String,
}

/// 时间线（用于分组与键盘导航的确定顺序）。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseTimeline {
    pub total: i64,
    pub entries: Vec<TimelineEntry>,
}

/// 一个取值有几张。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FacetCount {
    /// 评分 / 锁：数字；色标 / 喜欢：字符串；`null` = 无值。
    pub value: Option<String>,
    pub count: i64,
}

/// 筛选面板要的分布。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseFacets {
    pub ratings: Vec<FacetCount>,
    pub colors: Vec<FacetCount>,
    pub likes: Vec<FacetCount>,
    pub locks: Vec<FacetCount>,
}

/// 一组照片当前的标记（三态控件显示谁的值）。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkingItem {
    pub id: i64,
    pub rating: i64,
    pub color_label: Option<String>,
    pub like_state: Option<String>,
    pub lock_level: i64,
    /// 这张照片身上的**标签 id**（升序）。
    ///
    /// 只给 id、不给名字：名字在**全局词典**（`app.db`）里，而这条记录来自某个库的
    /// catalog；前端手上本来就有一份词典（标签弹窗要用），拿 id 去查名字即可 ——
    /// 免得每个库的标记接口都要跨界读一次 app.db（见 `tags.rs` 的文件头）。
    pub tag_ids: Vec<i64>,
}

/// 改完之后的状态（前端据此更新按钮与撤销提示）。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkResult {
    pub changed: i64,
    pub skipped_locked: Vec<i64>,
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
    pub can_undo: bool,
    pub can_redo: bool,
}

/// 删除失败的一个文件。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteFailure {
    pub path: String,
    pub reason: String,
}

/// 删除结果（界面上要说清「删了几张、几张被锁挡住、几个文件没移走」）。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    pub deleted: i64,
    pub blocked_locked: Vec<i64>,
    pub already_gone: i64,
    pub failed: Vec<DeleteFailure>,
}

impl From<DeleteReport> for DeleteResult {
    fn from(report: DeleteReport) -> Self {
        Self {
            deleted: i64::try_from(report.deleted).unwrap_or(i64::MAX),
            blocked_locked: report.blocked_locked,
            already_gone: i64::try_from(report.already_gone).unwrap_or(i64::MAX),
            failed: report
                .failed
                .into_iter()
                .map(|(path, reason)| DeleteFailure { path, reason })
                .collect(),
        }
    }
}

/// 旗标快照（当前库的）。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlagsView {
    pub total: i64,
    pub picks: Vec<i64>,
    pub rejects: Vec<i64>,
}

// ─────────────────────────── 命令 ───────────────────────────

/// 取一页照片（虚拟网格的可视窗口）。
#[tauri::command]
pub async fn browse_page<R: Runtime>(
    app: AppHandle<R>,
    query: BrowseQueryDto,
    offset: i64,
    limit: i64,
) -> Result<BrowseWindow, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        let q = query.to_query();
        state.with_catalog(&handle, &query.repository_id, move |db| {
            let total = db
                .read(|conn| query::count(conn, &q))
                .map_err(|e| e.to_string())?;
            let offset = usize::try_from(offset.max(0)).unwrap_or(0);
            let limit = usize::try_from(limit.clamp(0, 5_000)).unwrap_or(0);
            let rows = db
                .read(|conn| query::page(conn, &q, offset, limit))
                .map_err(|e| e.to_string())?;
            Ok(BrowseWindow {
                total,
                offset: i64::try_from(offset).unwrap_or(0),
                items: rows.into_iter().map(AssetItem::from).collect(),
            })
        })
    })
    .await
}

/// 取时间线（顺序 + 拍摄时间；分组与键盘导航要用）。
#[tauri::command]
pub async fn browse_timeline<R: Runtime>(
    app: AppHandle<R>,
    query: BrowseQueryDto,
    limit: i64,
) -> Result<BrowseTimeline, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        let q = query.to_query();
        state.with_catalog(&handle, &query.repository_id, move |db| {
            let total = db
                .read(|conn| query::count(conn, &q))
                .map_err(|e| e.to_string())?;
            let limit = usize::try_from(limit.max(0)).unwrap_or(0);
            let rows = db
                .read(|conn| query::timeline(conn, &q, limit))
                .map_err(|e| e.to_string())?;
            Ok(BrowseTimeline {
                total,
                entries: rows
                    .into_iter()
                    .map(|row| TimelineEntry {
                        id: row.id,
                        taken_at: row.taken_at,
                        rel_path: row.rel_path,
                    })
                    .collect(),
            })
        })
    })
    .await
}

/// 取各取值的分布（筛选面板）。
#[tauri::command]
pub async fn browse_facets<R: Runtime>(
    app: AppHandle<R>,
    query: BrowseQueryDto,
) -> Result<BrowseFacets, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        let q = query.to_query();
        state.with_catalog(&handle, &query.repository_id, move |db| {
            let facets = db
                .read(|conn| query::facets(conn, &q))
                .map_err(|e| e.to_string())?;
            let to_counts = |items: Vec<(Option<String>, i64)>| -> Vec<FacetCount> {
                items
                    .into_iter()
                    .map(|(value, count)| FacetCount { value, count })
                    .collect()
            };
            Ok(BrowseFacets {
                ratings: facets
                    .ratings
                    .into_iter()
                    .map(|(value, count)| FacetCount {
                        value: Some(value.to_string()),
                        count,
                    })
                    .collect(),
                colors: to_counts(facets.colors),
                likes: to_counts(facets.likes),
                locks: facets
                    .locks
                    .into_iter()
                    .map(|(value, count)| FacetCount {
                        value: Some(value.to_string()),
                        count,
                    })
                    .collect(),
            })
        })
    })
    .await
}

/// 读一组照片当前的标记（三态控件的输入）。
#[tauri::command]
pub async fn browse_markings<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    ids: Vec<i64>,
) -> Result<Vec<MarkingItem>, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        state.with_catalog(&handle, &repository_id, move |db| {
            let markings = db
                .read(|conn| {
                    let rows = marking::read_markings(conn, &ids)?;
                    let mut out = Vec::with_capacity(rows.len());
                    for (id, m) in rows {
                        out.push(MarkingItem {
                            id,
                            rating: i64::from(m.rating),
                            color_label: m.color_label,
                            like_state: m.like_state,
                            lock_level: i64::from(m.lock_level),
                            // 一次动作最多是「一屏里选中的那些」，逐张读关联足够快
                            tag_ids: tags::tags_of_asset(conn, id)?,
                        });
                    }
                    Ok(out)
                })
                .map_err(|e| e.to_string())?;
            Ok(markings)
        })
    })
    .await
}

/// 标签关联改动之后，把差量同步到**全局词典**的使用次数（`app.db` 侧）。
///
/// 为什么在外壳层做：词典在 `app.db`、关联在各库的 `catalog.db` —— 一次动作要同时碰两个库，
/// 而 `store` 层是按库分开的（`marking` 只认 catalog 的连接）。
///
/// **失败不影响已经落库的关联**：`use_count` 只用来给标签排序（常用的排前面），
/// 为它让整个标记动作失败是不划算的 —— 所以这里吞掉错误、只在控制台留一行。
fn sync_tag_counts<R: Runtime>(app: &AppHandle<R>, ops: &[marking::Op]) {
    let mut delta = tags::TagDelta::default();
    for op in ops {
        match op {
            marking::Op::TagAttach { tag_id, .. } => delta.added.push(*tag_id),
            marking::Op::TagDetach { tag_id, .. } => delta.removed.push(*tag_id),
            _ => {}
        }
    }
    if delta.is_empty() {
        return;
    }
    let state = app.state::<DbState>();
    let result = state.with(app, |db| {
        db.write(move |conn| tags::apply_delta(conn, &delta))
            .map_err(|e| e.to_string())
    });
    if let Err(message) = result {
        eprintln!("[raybend] ⚠️ 标签使用次数同步失败（不影响这次改动）：{message}");
    }
}

/// 打标记 / 改标签（**会进撤销栈**）。
#[tauri::command]
pub async fn browse_mark<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    ids: Vec<i64>,
    action: MarkActionDto,
) -> Result<MarkResult, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        let id = repository_id.clone();
        /*
         * 「读旧值 → 写新值」必须在**同一个写事务**里：分成两次调用的话，
         * 两次之间别人改了这张照片，撤销栈里记的就是错的旧值。
         */
        let (applied, change) = state.with_catalog(&handle, &repository_id, move |db| {
            // 写事务的闭包要求 `Send + 'static` —— 参数要先克隆一份进去
            let action = action.clone();
            let ids = ids.clone();
            db.write_tx(move |conn| {
                let change = match &action {
                    MarkActionDto::Rating { value } => {
                        let rating = marking::check_rating(*value)?;
                        marking::set_rating(conn, &ids, rating, &format!("标 {rating} 星"))?
                    }
                    MarkActionDto::Color { value } => {
                        let color = marking::check_color(value.as_deref())?;
                        let label = match &color {
                            Some(c) => format!("标{}色", color_name(c)),
                            None => "去掉颜色标记".to_string(),
                        };
                        marking::set_color(conn, &ids, color, &label)?
                    }
                    MarkActionDto::Like { value } => {
                        let like = marking::check_like(value.as_deref())?;
                        let label = match like.as_deref() {
                            Some("like") => "标记喜欢",
                            Some("dislike") => "标记不喜欢",
                            _ => "取消喜欢",
                        };
                        marking::set_like(conn, &ids, like, label)?
                    }
                    MarkActionDto::Lock { value } => {
                        let level = marking::check_lock(*value)?;
                        let label = match level {
                            0 => "解锁",
                            1 => "加一级锁（不可删）",
                            _ => "加二级锁（不可编辑）",
                        };
                        marking::set_lock(conn, &ids, level, label)?
                    }
                    MarkActionDto::AttachTags { tag_ids } => {
                        marking::attach_tags(conn, &ids, tag_ids, "加标签")?
                    }
                    MarkActionDto::DetachTags { tag_ids } => {
                        marking::detach_tags(conn, &ids, tag_ids, "摘标签")?
                    }
                    MarkActionDto::SetText { field, value } => {
                        let parsed = marking::TextField::parse(field).ok_or_else(|| {
                            raybend::Error::Unsupported(format!("右栏没有这个可编辑字段：{field}"))
                        })?;
                        if let Some(text) = value
                            && text.chars().count() > marking::TEXT_FIELD_MAX_CHARS
                        {
                            return Err(raybend::Error::Unsupported(format!(
                                "文字太长（上限 {} 字）",
                                marking::TEXT_FIELD_MAX_CHARS
                            )));
                        }
                        marking::set_text(conn, &ids, parsed, value.as_deref())?
                    }
                };
                let applied = marking::apply(conn, &change)?;
                Ok((applied, change))
            })
            .map_err(|e| e.to_string())
        })?;

        // 标签的使用次数记在全局词典里，这里按本次差量同步（失败不影响落库的关联）
        sync_tag_counts(&handle, &change.ops);

        // 记进撤销栈（只记真的改到了东西的动作 —— 空补丁不该占一步撤销）
        let (undo_label, redo_label, can_undo, can_redo) = state.with_undo(&id, |stack| {
            stack.push(change);
            stack_snapshot(stack)
        })?;
        Ok(MarkResult {
            changed: i64::try_from(applied.changed).unwrap_or(i64::MAX),
            skipped_locked: applied.skipped_locked,
            undo_label,
            redo_label,
            can_undo,
            can_redo,
        })
    })
    .await
}

/// 撤销一步。
///
/// 三步走（见 `UndoStack::take_undo` 的说明）：**取补丁 → 在写事务里执行 → 归位**。
#[tauri::command]
pub async fn browse_undo<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
) -> Result<MarkResult, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        let id = repository_id.clone();
        let Some((patch, change)) = state.with_undo(&id, |stack| stack.take_undo())? else {
            return state.with_undo(&id, |stack| stack_snapshot(stack)).map(
                |(undo_label, redo_label, can_undo, can_redo)| MarkResult {
                    changed: 0,
                    skipped_locked: Vec::new(),
                    undo_label,
                    redo_label,
                    can_undo,
                    can_redo,
                },
            );
        };

        // 补丁要进闭包（会被移动），但同步标签计数还要用它 —— 先留一份
        let applied_ops = patch.ops.clone();
        let outcome = state.with_catalog(&handle, &repository_id, move |db| {
            db.write_tx(move |conn| marking::apply(conn, &patch).map(|_| ()))
                .map_err(|e| e.to_string())
        });
        match outcome {
            Ok(()) => {
                sync_tag_counts(&handle, &applied_ops);
                invalidate_edited_previews(&handle, &repository_id, &applied_ops);
                state.with_undo(&id, |stack| stack.commit_undo(change))?;
            }
            Err(e) => {
                state.with_undo(&id, |stack| stack.give_back_undo(change))?;
                return Err(e);
            }
        }
        state.with_undo(&id, |stack| stack_snapshot(stack)).map(
            |(undo_label, redo_label, can_undo, can_redo)| MarkResult {
                changed: 1,
                skipped_locked: Vec::new(),
                undo_label,
                redo_label,
                can_undo,
                can_redo,
            },
        )
    })
    .await
}

/// 重做一步。
#[tauri::command]
pub async fn browse_redo<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
) -> Result<MarkResult, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        let id = repository_id.clone();
        let Some((patch, change)) = state.with_undo(&id, |stack| stack.take_redo())? else {
            return state.with_undo(&id, |stack| stack_snapshot(stack)).map(
                |(undo_label, redo_label, can_undo, can_redo)| MarkResult {
                    changed: 0,
                    skipped_locked: Vec::new(),
                    undo_label,
                    redo_label,
                    can_undo,
                    can_redo,
                },
            );
        };

        // 补丁要进闭包（会被移动），但同步标签计数还要用它 —— 先留一份
        let applied_ops = patch.ops.clone();
        let outcome = state.with_catalog(&handle, &repository_id, move |db| {
            db.write_tx(move |conn| marking::apply(conn, &patch).map(|_| ()))
                .map_err(|e| e.to_string())
        });
        match outcome {
            Ok(()) => {
                sync_tag_counts(&handle, &applied_ops);
                invalidate_edited_previews(&handle, &repository_id, &applied_ops);
                state.with_undo(&id, |stack| stack.commit_redo(change))?;
            }
            Err(e) => {
                state.with_undo(&id, |stack| stack.give_back_redo(change))?;
                return Err(e);
            }
        }
        state.with_undo(&id, |stack| stack_snapshot(stack)).map(
            |(undo_label, redo_label, can_undo, can_redo)| MarkResult {
                changed: 1,
                skipped_locked: Vec::new(),
                undo_label,
                redo_label,
                can_undo,
                can_redo,
            },
        )
    })
    .await
}

/// 编辑撤销/重做会改变 latest：作废受影响照片的大图缓存。
/// 标记操作仍只更新标记，不让整个库的预览跟着重算。
fn edited_asset_ids(ops: &[marking::Op]) -> BTreeSet<i64> {
    ops.iter()
        .filter_map(|op| match op {
            marking::Op::DevelopParam { asset_id, .. }
            | marking::Op::DevelopCurve { asset_id, .. }
            | marking::Op::DevelopSetting { asset_id, .. } => Some(*asset_id),
            _ => None,
        })
        .collect()
}

fn invalidate_edited_previews<R: Runtime>(
    app: &AppHandle<R>,
    repository_id: &str,
    ops: &[marking::Op],
) {
    let ids = edited_asset_ids(ops);
    if ids.is_empty() {
        return;
    }
    if let Ok(root) = resolve_root(app, repository_id)
        && let Ok(cache) = raybend::display::FullCache::open(&root)
    {
        for asset_id in ids {
            cache.invalidate(asset_id);
        }
    }
}

/// 中文色名（撤销标签用人话）。
fn color_name(color: &str) -> &str {
    match color {
        "red" => "红",
        "yellow" => "黄",
        "green" => "绿",
        "cyan" => "青",
        "blue" => "蓝",
        "purple" => "紫",
        other => other,
    }
}

/// 撤销栈的对外快照（标签 + 能不能撤/重做）。
type StackSnapshot = (Option<String>, Option<String>, bool, bool);

fn stack_snapshot(stack: &UndoStack) -> StackSnapshot {
    (
        stack.undo_label().map(str::to_string),
        stack.redo_label().map(str::to_string),
        stack.can_undo(),
        stack.can_redo(),
    )
}

/// 删除照片（进系统回收站；一级锁挡住）。
#[tauri::command]
pub async fn browse_delete<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    ids: Vec<i64>,
) -> Result<DeleteResult, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        let report = state.with_catalog(&handle, &repository_id, move |db| {
            let root = db.root().to_path_buf();
            db.write_tx(move |conn| raybend::store::delete::delete_assets(conn, &root, &ids))
                .map_err(|e| e.to_string())
        })?;
        Ok(DeleteResult::from(report))
    })
    .await
}

/// 当前库的旗标快照。
#[tauri::command]
pub async fn flags_get<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
) -> Result<FlagsView, String> {
    let state = app.state::<BrowseState>();
    let guard = state.flags.lock().map_err(|_| "内部锁已损坏".to_string())?;
    Ok(FlagsView {
        total: i64::try_from(guard.len()).unwrap_or(i64::MAX),
        picks: guard.filter_ids(&repository_id, Flag::Pick),
        rejects: guard.filter_ids(&repository_id, Flag::Reject),
    })
}

/// 打/清旗标（`flag = null` 表示清掉这些照片的旗标）。
#[tauri::command]
pub async fn flags_set<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    ids: Vec<i64>,
    flag: Option<String>,
) -> Result<FlagsView, String> {
    let parsed = match flag.as_deref() {
        Some("pick") => Some(Flag::Pick),
        Some("reject") => Some(Flag::Reject),
        Some(other) => return Err(format!("不认识的旗标：{other}（只能是 pick / reject）")),
        None => None,
    };
    let state = app.state::<BrowseState>();
    let mut guard = state.flags.lock().map_err(|_| "内部锁已损坏".to_string())?;
    for id in ids {
        guard.set(FlagKey::new(repository_id.clone(), id), parsed);
    }
    Ok(FlagsView {
        total: i64::try_from(guard.len()).unwrap_or(i64::MAX),
        picks: guard.filter_ids(&repository_id, Flag::Pick),
        rejects: guard.filter_ids(&repository_id, Flag::Reject),
    })
}

/// 清空**所有**旗标（跨库；UI 上要先确认）。
#[tauri::command]
pub async fn flags_clear<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
) -> Result<FlagsView, String> {
    let state = app.state::<BrowseState>();
    let mut guard = state.flags.lock().map_err(|_| "内部锁已损坏".to_string())?;
    guard.clear();
    Ok(FlagsView {
        total: 0,
        picks: Vec::new(),
        rejects: guard.filter_ids(&repository_id, Flag::Reject),
    })
}
