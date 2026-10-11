//! 浏览工作流的命令：查询、标记、撤销、旗标、回收站。
//!
//! 业务规则全在 `raybend::store`（`query` / `marking` / `flags` / `recycle`），
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
use std::sync::{Arc, Mutex};

use raybend::store::availability::{self, Availability, ConnectionStatus, ProbeBudget, Snapshots};
use raybend::store::db::{CatalogDb, OpenOpts};
use raybend::store::recycle::{PurgeReport, RecycleReport, RestoreReport};
use raybend::store::flags::{Flag, FlagKey, FlagSet};
use raybend::store::marking::{self, UndoStack};
use raybend::store::query::{
    self, AssetRow, Combinator, Filter, FlagFilter, Query, Scope, Sort, SortKey, Visibility,
};
use raybend::store::repository;
use raybend::store::organization;
use raybend::store::tags;
use raybend::store::time;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::db::DbState;
use crate::source::blocking;

/// 浏览相关的会话态：撤销栈（每库一份）+ 旗标（跨库一份）。
#[derive(Default)]
pub struct BrowseState {
    undo: Mutex<HashMap<String, UndoStack>>,
    flags: Mutex<FlagSet>,
    /// 当前打开的库（`CatalogDb` 自带连接池与写者线程，重开一次不便宜）。
    pub(crate) sessions: raybend::store::session::CatalogSessions,
    pub(crate) connections: Snapshots,
    pub(crate) probes: ProbeBudget,
    disk_sync: Mutex<HashMap<String, std::sync::Weak<Mutex<()>>>>,
    pub(crate) live_watch:
        Mutex<Option<(String, PathBuf, u64, raybend::media::watch::CatalogWatcher)>>,
}

impl BrowseState {
    /// 同库所有工作流共享租约，固定根目录；I/O 与退场不持 app.db/全局锁。
    pub(crate) fn lease<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        id: &str,
    ) -> Result<Arc<CatalogDb>, String> {
        if self.sessions.is_released(id) { return Err(raybend::Error::RepositoryReleased.to_string()); }
        let before = self.connections.get(id).revision;
        let result = self.acquire_session(app, id);
        match result {
            Ok(db) => {
                let current = self.connections.get(id);
                if current.state != Availability::Online
                    || current.generation != db.generation().to_string()
                {
                    let mut status = ConnectionStatus::unknown(id);
                    status.state = Availability::Online;
                    status.root = Some(db.root().to_string_lossy().into());
                    status.generation = db.generation().to_string();
                    self.publish(app, status);
                }
                Ok(db)
            }
            Err(error) => {
                if self
                    .publish_if(app, ConnectionStatus::failed(id, &error), Some(&before))
                    .is_some()
                {
                    self.clear_watch(id);
                }
                Err(error.to_string())
            }
        }
    }
    pub(crate) fn acquire_session<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        id: &str,
    ) -> raybend::Result<Arc<CatalogDb>> {
        let backups = crate::db::data_dir(app)
            .map_err(raybend::Error::Unsupported)?
            .join(raybend::store::db::BACKUPS_DIR);
        self.sessions.acquire(
            id,
            || {
                let paths = app
                    .state::<DbState>()
                    .with(app, |db| {
                        db.read(|conn| repository::registered_paths(conn, id))
                            .map_err(|e| e.to_string())
                    })
                    .map_err(raybend::Error::Unsupported)?;
                let preferred = app.state::<DbState>().with(app, |db| db.get_setting_json::<availability::PreferredLocation>(&availability::preference_key(id)).map_err(|e|e.to_string())).map_err(raybend::Error::Unsupported)?;
                availability::resolve_preferred(id, &paths, preferred.as_ref())
            },
            OpenOpts::new(Some(&backups), time::now_millis()),
        )
    }
    pub(crate) fn publish<R: Runtime>(&self, app: &AppHandle<R>, status: ConnectionStatus) {
        self.publish_if(app, status, None);
    }
    pub(crate) fn publish_if<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        status: ConnectionStatus,
        revision: Option<&str>,
    ) -> Option<ConnectionStatus> {
        match self.connections.publish_if(status, revision) {
            Ok(Some(status)) => {
                if let Err(e) = app.emit("repository://connection", &status) {
                    log::warn!("[repository] 状态事件失败：{e}");
                }
                Some(status)
            }
            Ok(None) => None,
            Err(e) => {
                log::warn!("[repository] 状态代次失败：{e}");
                None
            }
        }
    }
    pub(crate) fn disk_gate(&self, id: &str) -> Arc<Mutex<()>> {
        let mut gates = self
            .disk_sync
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        gates.retain(|_, gate| gate.strong_count() > 0);
        if let Some(gate) = gates.get(id).and_then(std::sync::Weak::upgrade) {
            return gate;
        }
        let gate = Arc::new(Mutex::new(()));
        gates.insert(id.into(), Arc::downgrade(&gate));
        gate
    }
    pub(crate) fn clear_watch(&self, id: &str) {
        let retired = {
            let mut watcher = self
                .live_watch
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if watcher.as_ref().is_some_and(|(repo, _, _, _)| repo == id) {
                watcher.take()
            } else {
                None
            }
        };
        drop(retired);
    }
    pub(crate) fn observe_session<R: Runtime>(&self, app: &AppHandle<R>, db: &CatalogDb) {
        if self.sessions.is_released(&db.meta().id) { return; }
        if let Err(error) = db.ensure_current() {
            let id = &db.meta().id;
            // 旧任务迟到失败不能使新连接再次离线。
            let observed = self.connections.get(id);
            if observed.generation == db.generation().to_string() {
                let mut status = db
                    .connection_failure()
                    .unwrap_or_else(|| ConnectionStatus::failed(id, &error));
                status.generation = db.generation().to_string();
                if self
                    .publish_if(app, status, Some(&observed.revision))
                    .is_some()
                {
                    self.clear_watch(id);
                }
            }
        }
    }
    /// 按登记位置识别读图任务，释放后不可把库照片退回普通来源绕过屏障。
    pub(crate) fn path_task<R: Runtime>(&self, app: &AppHandle<R>, path: &std::path::Path) -> Result<Option<raybend::store::session::TaskPermit>, String> {
        use raybend::store::path_semantics::PathForms;
        let wanted=PathForms::new(path.to_string_lossy().as_ref());
        let rows=app.state::<DbState>().with(app,|db|db.list_repositories().map_err(|e|e.to_string()))?;
        for row in rows {
            if row.paths.iter().any(|position| { let root=PathForms::new(&position.path); wanted.folded()==root.folded() || wanted.folded().starts_with(&format!("{}/",root.folded().trim_end_matches('/'))) }) {
                let permit=self.sessions.begin_task(&row.id).map_err(|e|e.to_string())?;
                self.lease(app,&row.id)?;
                return Ok(Some(permit));
            }
        }
        Ok(None)
    }
    pub(crate) fn with_catalog<R: Runtime, T>(
        &self,
        app: &AppHandle<R>,
        id: &str,
        f: impl FnOnce(&CatalogDb) -> Result<T, String>,
    ) -> Result<T, String> {
        let _permit = self.sessions.begin_task(id).map_err(|e|e.to_string())?;
        let db = self.lease(app, id)?;
        let result = f(&db);
        self.observe_session(app, &db);
        result
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

/// 通过已验证会话取得库根。一次操作同时使用 DB 与根时，应从同一租约取值。
pub(crate) fn resolve_root<R: Runtime>(
    app: &AppHandle<R>,
    repository_id: &str,
) -> Result<PathBuf, String> {
    let db = app.state::<BrowseState>().lease(app, repository_id)?;
    Ok(db.root().to_path_buf())
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
    pub(crate) fn into_filter(self) -> Filter {
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
            tag_keys: Vec::new(),
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
    /// 查回收站（`true` = 只见回收站资产；`scope_path` 已带 `recycle/` 前缀，见 `specs/library-recycle-bin.md`）。
    #[serde(default)]
    pub recycle: bool,
}

impl BrowseQueryDto {
    /// 按当前的 DTO 造一个 store 层的查询。
    ///
    /// 名字不用 `into_*`：那个约定是「消耗 self 的转换」，这里是**借用着造一份新的**。
    fn to_query(&self) -> Query {
        /*
         * **文件夹语义**（崔总 2026-10-11）：点什么目录看什么目录**直接**的内容，
         * 不递归子目录 —— 点 A 不该把 A/B、A/C 里的照片一起铺出来。
         * `Scope::Directory` 就是这一条（`_RAW/` 视为透明原片目录，与位图同层）。
         */
        let scope = match self.scope_path.as_deref().map(str::trim) {
            Some(path) if !path.is_empty() => Scope::directory(path),
            _ => Scope::Repository,
        };
        let key = match self.sort.key.as_deref() {
            Some("importedAt") => SortKey::ImportedAt,
            Some("fileName") => SortKey::FileName,
            Some("rating") => SortKey::Rating,
            Some("camera") => SortKey::Camera,
            Some("deletedAt") => SortKey::DeletedAt,
            _ => SortKey::TakenAt,
        };
        let desc = self.sort.desc.unwrap_or_else(|| Sort::default_desc(key));
        Query {
            scope,
            filter: self.filter.clone().into_filter(),
            sort: Sort::new(key, desc),
            visibility: if self.recycle {
                Visibility::Recycled
            } else {
                Visibility::Live
            },
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
    PhotoTags { edits:Vec<raybend::store::photo_tags::Edit> },
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
    /// RAW 文件的**展示尺寸**（无 RAW → `null`）—— “这张的 RAW 多大”，毫秒级可得。
    pub raw_width: Option<i64>,
    pub raw_height: Option<i64>,
    /// latest 的**输出尺寸**（无编辑栈 → `null`）—— tiles 比例与看图初值用它。
    ///
    /// 老库的栈列还没回填时这里给的是**现算值**（`sizes::latest_size`，零 I/O）——
    /// 绝不退回基准尺寸：那是另一个口径（未裁切的画面），比例可以完全不同。
    pub latest_width: Option<i64>,
    pub latest_height: Option<i64>,
    /// latest 的**来源基底**（`"raw"` / `"sooc"`；无编辑栈 → `null`）。
    ///
    /// flowinfo 的「文件类型」随显示变体走（崔总 2026-10-11 定）：latest 看它，
    /// 定稿看 `IssueDto.sourceBase`。前端 `lib/display-size.ts::sourceBaseOf` 是唯一读口。
    pub latest_source_base: Option<String>,
    pub orientation: Option<i64>,
    pub size_bytes: Option<i64>,
    pub missing: bool,
    /// 命名定稿数（0–100；SOOC / RAW / latest 不计入）—— tile 左下角定稿图标旁的数字。
    pub issue_count: u16,
    /// 编辑过（编辑栈与原始源不同）—— 没有定稿时 tile 左下角显示编辑图标的判据。
    pub edited: bool,
    /// 回收站里的原位置（`recycled_from`；不在回收站时为 `null`）—— 右列「原位置」。
    pub recycled_from: Option<String>,
}

/// `AssetRow` → 对外 DTO（尺寸的口径全在 `store` 层：latest 的读侧回退在
/// `store::query::page` 里做过了，这里只搬字段）。
impl From<AssetRow> for AssetItem {
    fn from(row: AssetRow) -> Self {
        // catalog 存的是文件头里的原始像素轴；Browse DTO 对外给展示尺寸，必须应用方向。
        // 单图 view / film 直接走解码后的图所以一直正常，只有 tiles / compare 读了这两个
        // 原始数值，才会把 orientation=6/8 的竖图压成横图。
        //
        // v19 起优先读 `asset_files.display_*`（导入时就摆正过的存档值，
        // `specs/display-size-source.md` §5）；老库那批 NULL 才退回旧算式。
        let stored = display_dimensions(&row);
        let (width, height) = if stored.0.is_some() && stored.1.is_some() {
            stored
        } else {
            oriented_dimensions(row.width, row.height, row.orientation)
        };
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
            raw_width: row.raw_width,
            raw_height: row.raw_height,
            latest_width: row.latest_width,
            latest_height: row.latest_height,
            latest_source_base: row.latest_source_base,
            orientation: row.orientation,
            size_bytes: row.size_bytes,
            missing: row.missing,
            issue_count: row.issue_count,
            edited: row.edited,
            recycled_from: row.recycled_from,
        }
    }
}

/// 库里存好的展示尺寸（`asset_files.display_*`，两列都在且为正才算数）。
fn display_dimensions(row: &AssetRow) -> (Option<i64>, Option<i64>) {
    match (row.display_width, row.display_height) {
        (Some(width), Some(height)) if width > 0 && height > 0 => (Some(width), Some(height)),
        _ => (None, None),
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
    use super::{AssetItem, edited_asset_ids, oriented_dimensions};
    use raybend::store::marking::Op;
    use raybend::store::{migration, query};
    use rusqlite::Connection;

    /// 一个跑过全部迁移的内存 catalog。
    fn catalog() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        raybend::store::pragma::apply(&conn, false).unwrap();
        migration::apply(
            &mut conn,
            migration::DbKind::Catalog,
            migration::Backups::none(),
            0,
        )
        .unwrap();
        conn
    }

    /// 一张资产 + 一个文件行（`display_*` 可选）。
    fn asset(conn: &Connection, id: i64, role: &str, display: Option<(i64, i64, i64)>) {
        conn.execute(
            "INSERT OR IGNORE INTO assets(id, taken_at, imported_at, updated_at) VALUES(?1, 1, 1, 1)",
            [id],
        )
        .unwrap();
        let rel = format!("photos/{id}-{role}.bin");
        conn.execute(
            "INSERT INTO asset_files
                (asset_id, role, rel_path, rel_path_folded, ext, size_bytes, mtime_ms,
                 display_width, display_height, orientation, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3, 'bin', 1, 1, ?4, ?5, ?6, 1, 1)",
            rusqlite::params![
                id,
                role,
                rel,
                display.map(|d| d.0),
                display.map(|d| d.1),
                display.map(|d| d.2),
            ],
        )
        .unwrap();
    }

    fn item(conn: &Connection, id: i64) -> AssetItem {
        let rows = query::page(conn, &query::Query::new(query::Scope::Repository), 0, 10).unwrap();
        rows.into_iter()
            .find(|row| row.id == id)
            .map(AssetItem::from)
            .expect("资产在")
    }

    #[test]
    fn asset_item_reads_the_stored_display_sizes_and_falls_back_for_legacy_rows() {
        // `specs/display-size-source.md` §5：三个口径各读自己那一列；
        // 老库（列为 NULL）退回旧算式（assets 原始轴 + 方向）。
        let conn = catalog();
        // 编辑过的一张：位图 4000×3000、RAW 5208×3904、latest 裁成 2000×1500
        asset(&conn, 1, "bitmap", Some((4000, 3000, 1)));
        asset(&conn, 1, "raw", Some((5208, 3904, 1)));
        conn.execute(
            "UPDATE assets SET width = 6000, height = 4000, orientation = 6 WHERE id = 1",
            [],
        )
        .unwrap();
        // 这张合成资产位图行与 RAW 行都有 ⇒ RAW 基底（`default()`）能拿到自己的尺寸
        let mut stack = raybend::store::develop::DevelopStack::default();
        stack.params.insert("exposure".into(), 0.5);
        stack.geometry = Some(raybend::develop::geometry::EditGeometry {
            rotation: 0.0,
            crop: Some(raybend::develop::geometry::CropRect {
                x: 0.0,
                y: 0.0,
                width: 0.5,
                height: 0.5,
            }),
            crop_ratio: None,
        });
        raybend::store::develop::save(&conn, 1, &stack, 1).unwrap();

        let edited = item(&conn, 1);
        assert_eq!((edited.width, edited.height), (Some(4000), Some(3000)), "基准 = 位图的展示尺寸（方向已应用过）");
        assert_eq!((edited.raw_width, edited.raw_height), (Some(5208), Some(3904)), "RAW 各写自己的");
        assert_eq!((edited.latest_width, edited.latest_height), (Some(2604), Some(1952)), "latest = 栈的输出尺寸");
        assert_eq!(edited.latest_source_base.as_deref(), Some("raw"), "latest 基底的唯一出处");

        // 老库：文件行 NULL、assets 只有原始轴 + 方向 ⇒ 退回旧算式；无栈 ⇒ latest 为 NULL
        asset(&conn, 2, "bitmap", None);
        conn.execute(
            "UPDATE assets SET width = 6000, height = 4000, orientation = 8 WHERE id = 2",
            [],
        )
        .unwrap();
        let legacy = item(&conn, 2);
        assert_eq!((legacy.width, legacy.height), (Some(4000), Some(6000)), "方向 8 ⇒ 交换宽高");
        assert_eq!((legacy.raw_width, legacy.raw_height), (None, None), "没有 RAW 行就是 null");
        assert_eq!(
            (legacy.latest_width, legacy.latest_height),
            (None, None),
            "没栈行 ⇒ 读侧不填（latest = 基准是消费方的回退）",
        );
        assert_eq!(legacy.latest_source_base, None, "没栈行就没有基底");
    }

    #[test]
    fn asset_item_computes_latest_size_when_the_stack_row_predates_v19() {
        // 崔总 2026-10-11 真机：老库的编辑栈列还没回填（output_* 为 NULL）
        // ⇒ DTO 必须**按栈现算**，不能退回基准尺寸（那是另一个口径，比例不同）。
        let conn = catalog();
        asset(&conn, 1, "bitmap", None);
        conn.execute(
            "UPDATE assets SET width = 4000, height = 3000, orientation = 1 WHERE id = 1",
            [],
        )
        .unwrap();
        // 基底写死 Sooc：合成资产只有位图行，而尺寸只认各自那一行（2026-10-11）
        let mut stack = raybend::store::develop::DevelopStack {
            source_base: raybend::store::develop::EditBase::Sooc,
            ..raybend::store::develop::DevelopStack::default()
        };
        stack.params.insert("exposure".into(), 0.5);
        stack.geometry = Some(raybend::develop::geometry::EditGeometry {
            rotation: 0.0,
            crop: Some(raybend::develop::geometry::CropRect {
                x: 0.0,
                y: 0.0,
                width: 0.5,
                height: 0.5,
            }),
            crop_ratio: None,
        });
        raybend::store::develop::save(&conn, 1, &stack, 1).unwrap();
        // 模拟 v19 之前建的行：栈在、尺寸列还在 NULL
        conn.execute(
            "UPDATE develop_stacks SET output_width = NULL, output_height = NULL WHERE asset_id = 1",
            [],
        )
        .unwrap();

        let item = item(&conn, 1);
        assert_eq!(
            (item.latest_width, item.latest_height),
            (Some(2000), Some(1500)),
            "回填之前也要给出裁切后的尺寸（不是基准的 4000×3000）",
        );
        assert_eq!(item.latest_source_base.as_deref(), Some("sooc"));
    }

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

/// 移入回收站的结果（界面要说清「移了几张、几张被锁挡住、几个文件没动」）。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleResult {
    pub moved: i64,
    pub blocked_locked: Vec<i64>,
    pub already_gone: i64,
    pub failed: Vec<DeleteFailure>,
}

impl From<RecycleReport> for RecycleResult {
    fn from(report: RecycleReport) -> Self {
        Self {
            moved: i64::try_from(report.moved).unwrap_or(i64::MAX),
            blocked_locked: report.blocked_locked,
            already_gone: i64::try_from(report.already_gone).unwrap_or(i64::MAX),
            failed: failures(report.failed),
        }
    }
}

/// 还原结果（`renamed` = 因原位被占而改名的张数，界面据此提示）。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub restored: i64,
    pub renamed: i64,
    pub failed: Vec<DeleteFailure>,
}

impl From<RestoreReport> for RestoreResult {
    fn from(report: RestoreReport) -> Self {
        Self {
            restored: i64::try_from(report.restored).unwrap_or(i64::MAX),
            renamed: i64::try_from(report.renamed).unwrap_or(i64::MAX),
            failed: failures(report.failed),
        }
    }
}

/// 真删 / 清空的结果。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurgeResult {
    pub purged: i64,
    pub failed: Vec<DeleteFailure>,
}

impl From<PurgeReport> for PurgeResult {
    fn from(report: PurgeReport) -> Self {
        Self {
            purged: i64::try_from(report.purged).unwrap_or(i64::MAX),
            failed: failures(report.failed),
        }
    }
}

/// 回收站概况（入口行「有内容才出现」+ 数量/占用）。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleSummaryView {
    pub count: i64,
    pub bytes: i64,
}

fn failures(list: Vec<(String, String)>) -> Vec<DeleteFailure> {
    list.into_iter()
        .map(|(path, reason)| DeleteFailure { path, reason })
        .collect()
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
                            tag_ids: raybend::store::photo_tags::effective_ids(conn, id)?,
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
fn sync_tag_counts<R: Runtime>(app: &AppHandle<R>, delta: &tags::TagDelta) {
    if delta.is_empty() { return; }
    let delta = delta.clone();
    let result = app.state::<DbState>().with(app, |db| db.write(move |conn| tags::apply_delta(conn, &delta)).map_err(|e| e.to_string()));
    if let Err(message) = result { eprintln!("[raybend] 标签使用次数同步失败：{message}"); }
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
        let attached_terms = match &action {
            MarkActionDto::AttachTags { tag_ids } => Some(
                handle.state::<DbState>().with(&handle, |db| {
                    db.read(|conn| tags::tags_by_ids(conn, tag_ids)).map_err(|e| e.to_string())
                })?
            ),
            MarkActionDto::PhotoTags { edits } => { let edits=edits.clone(); Some(handle.state::<DbState>().with(&handle,|db| {
                db.write(move |conn| {let mut dictionary=Vec::new();for e in edits {let id=tags::ensure_tag(conn,&e.name,raybend::store::time::now_millis())?;dictionary.push(tags::tag_by_id(conn,id)?.ok_or_else(||raybend::Error::Unsupported("标签词典创建失败".into()))?);}Ok(dictionary)}).map_err(|e|e.to_string())
            })?) },
            _ => None,
        };
        /*
         * 「读旧值 → 写新值」必须在**同一个写事务**里：分成两次调用的话，
         * 两次之间别人改了这张照片，撤销栈里记的就是错的旧值。
         */
        let (applied, change) = state.with_catalog(&handle, &repository_id, move |db| {
            if let Some(dictionary) = attached_terms {
                db.write(move |conn| organization::sync_legacy_terms(conn, &dictionary))
                    .map_err(|e| e.to_string())?;
            }
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
                    MarkActionDto::PhotoTags {edits} => raybend::store::photo_tags::edits(conn,&ids,edits,"修改标签")?,
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
                let mut change = change;
                // 被锁跳过的操作不能留下可在解锁后撤销的幽灵改动。
                change.ops.retain(|op| !op.respects_edit_lock() || !applied.skipped_locked.contains(&op.asset_id()));
                Ok((applied, change))
            })
            .map_err(|e| e.to_string())
        })?;

        // 标签的使用次数记在全局词典里，这里按本次差量同步（失败不影响落库的关联）
        sync_tag_counts(&handle, &applied.tag_delta);
        // sidecar 镜像：评级 / 色标 / 标签 / 文字变了（喜欢与锁不进 sidecar，见 marked_assets）
        crate::sidecar::queue_sync(handle.clone(), &id, crate::sidecar::marked_assets(&change.ops));

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
            db.write_tx(move |conn| marking::apply(conn, &patch))
                .map_err(|e| e.to_string())
        });
        match outcome {
            Ok(applied) => {
                sync_tag_counts(&handle, &applied.tag_delta);
                invalidate_edited_previews(&handle, &repository_id, &applied_ops);
                crate::sidecar::queue_sync(
                    handle.clone(),
                    &id,
                    crate::sidecar::marked_assets(&applied_ops),
                );
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
            db.write_tx(move |conn| marking::apply(conn, &patch))
                .map_err(|e| e.to_string())
        });
        match outcome {
            Ok(applied) => {
                sync_tag_counts(&handle, &applied.tag_delta);
                invalidate_edited_previews(&handle, &repository_id, &applied_ops);
                crate::sidecar::queue_sync(
                    handle.clone(),
                    &id,
                    crate::sidecar::marked_assets(&applied_ops),
                );
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

/// 删除照片 = 移入库内回收站（一级锁挡住；`specs/library-recycle-bin.md`）。
#[tauri::command]
pub async fn browse_delete<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    ids: Vec<i64>,
) -> Result<RecycleResult, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        let catalog = state.lease(&handle, &repository_id)?;
        let root = catalog.root().to_path_buf();
        let guard = Arc::clone(&catalog);
        let now = time::now_millis();
        let label = repository_id.clone();
        let report = catalog.write_tx(move |conn| {
            // 诊断（生产也打）：删除链路的**后端唯一抓手** —— 前端「点了没反应」先看这一行
            eprintln!("[recycle] browse_delete repo={label} ids={ids:?}");
            let result = raybend::store::recycle::move_to_recycle_checked(
                conn,
                &root,
                &ids,
                now,
                || guard.ensure_current(),
            );
            eprintln!("[recycle] browse_delete -> {result:?}");
            result
        });
        state.observe_session(&handle, &catalog);
        report.map(RecycleResult::from).map_err(|e| e.to_string())
    })
    .await
}

/// 还原回收站里选中的照片（回原位；同名自动加 `_01` 后缀，`renamed` 报张数）。
#[tauri::command]
pub async fn browse_recycle_restore<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    ids: Vec<i64>,
) -> Result<RestoreResult, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        let catalog = state.lease(&handle, &repository_id)?;
        let root = catalog.root().to_path_buf();
        let guard = Arc::clone(&catalog);
        let report = catalog.write_tx(move |conn| {
            raybend::store::recycle::restore_assets_checked(conn, &root, &ids, || {
                guard.ensure_current()
            })
        });
        state.observe_session(&handle, &catalog);
        report.map(RestoreResult::from).map_err(|e| e.to_string())
    })
    .await
}

/// 真删回收站里选中的照片（**永久**；界面上只有拖动一条路，见规格 §1）。
#[tauri::command]
pub async fn browse_recycle_purge<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    ids: Vec<i64>,
) -> Result<PurgeResult, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        let catalog = state.lease(&handle, &repository_id)?;
        let root = catalog.root().to_path_buf();
        let guard = Arc::clone(&catalog);
        let report = catalog.write_tx(move |conn| {
            raybend::store::recycle::purge_assets_checked(conn, &root, &ids, || {
                guard.ensure_current()
            })
        });
        state.observe_session(&handle, &catalog);
        report.map(PurgeResult::from).map_err(|e| e.to_string())
    })
    .await
}

/// 清空回收站（**永久**删除全部；强确认在界面侧）。
#[tauri::command]
pub async fn browse_recycle_empty<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
) -> Result<PurgeResult, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        let catalog = state.lease(&handle, &repository_id)?;
        let root = catalog.root().to_path_buf();
        let guard = Arc::clone(&catalog);
        let report = catalog.write_tx(move |conn| {
            raybend::store::recycle::empty_recycle_checked(conn, &root, || guard.ensure_current())
        });
        state.observe_session(&handle, &catalog);
        report.map(PurgeResult::from).map_err(|e| e.to_string())
    })
    .await
}

/// 回收站概况（入口行的出现条件与数量/占用）。
#[tauri::command]
pub async fn browse_recycle_summary<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
) -> Result<RecycleSummaryView, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<BrowseState>();
        let catalog = state.lease(&handle, &repository_id)?;
        let summary = catalog.read(raybend::store::recycle::summary);
        state.observe_session(&handle, &catalog);
        summary
            .map(|s| RecycleSummaryView {
                count: s.count,
                bytes: s.bytes,
            })
            .map_err(|e| e.to_string())
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
