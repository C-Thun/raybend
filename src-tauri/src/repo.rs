//! 库（相片仓）相关命令：列表、建库前的探测、建库、重挂载、计数与**重建数据**。
//!
//! 业务规则在 `raybend::store::repository`（`REPOSITORY.md` §2 的库身份/多路径/在线离线），
//! 这里只做三件事：拿 `app.db` / `catalog.db`、把结构转成前端视图、把耗时活儿丢到后台线程。
//!
//! **计数（2026-09-19 的新口径）**：`photos_count`（相片，不含 `_RAW`）与
//! `images_count`（图片，含 `_RAW`）存在 `app.db` 的 `repositories` + `directories` 里，
//! 由「导入时写」「进目录时增量同步」「重建时全量重算」三条路径维护 ——
//! 列表命令因此**不再逐个打开库的 catalog.db**（老实现那样做，慢盘上几秒起步）。

use std::path::{Path, PathBuf};

use raybend::store::db::{CatalogDb, OpenOpts};
use raybend::store::{repository, time};
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

use raybend::import::template;

use crate::db::DbState;
use crate::source::blocking;

/// 一个库在界面上的形状（`design/main.md` §3.3 的库卡片）。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryViewDto {
    pub id: String,
    pub name: String,
    pub import_template: Option<String>,
    pub created_at: i64,
    pub last_opened_at: Option<i64>,
    pub online: bool,
    pub root: Option<String>,
    /// 卡片上显示哪条路径：在线 = 库根，离线 = 上次已知路径。
    pub display_path: String,
    pub paths: Vec<RepositoryPathDto>,
    /// **相片数量**（不含 `_RAW/`）。`None` = 还没数过 —— 界面显示「—」，**不是 0**。
    pub photos_count: Option<i64>,
    /// **图片数量**（含 `_RAW/`）。
    pub images_count: Option<i64>,
    /// 探测过几条路径（离线时给「已试过 N 处」的提示）。
    pub tried_paths: usize,
}

/// 一条登记路径。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryPathDto {
    pub path: String,
    /// `online` / `offline` / `unknown`。
    pub status: String,
    pub last_seen_at: Option<i64>,
}

impl From<repository::RepositoryView> for RepositoryViewDto {
    fn from(view: repository::RepositoryView) -> Self {
        Self {
            id: view.id,
            name: view.name,
            import_template: view.import_template,
            created_at: view.created_at,
            last_opened_at: view.last_opened_at,
            online: view.online,
            root: view.root,
            display_path: view.display_path,
            paths: view
                .paths
                .into_iter()
                .map(|p| RepositoryPathDto {
                    path: p.path,
                    status: p.status,
                    last_seen_at: p.last_seen_at,
                })
                .collect(),
            photos_count: view.photos_count,
            images_count: view.images_count,
            tried_paths: view.tried_paths,
        }
    }
}

/// 建库弹窗在按下确认前看到的东西。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryProbeDto {
    /// `notDirectory` / `empty` / `existing` / `broken`。
    pub kind: String,
    /// 目录里已有库时的库名。
    pub name: Option<String>,
    /// 目录里已有库时的库 ID。
    pub repository_id: Option<String>,
    /// 这个 ID 是否**已经登记过** —— 是的话，这次操作是「给已有的库登记一条新路径」。
    pub registered: bool,
    /// `broken` 时的原因（给用户看）。
    pub message: Option<String>,
}

/// 库列表（含在线状态与两个计数）。
#[tauri::command]
pub async fn repositories_list<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Vec<RepositoryViewDto>, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<DbState>();
        views(&handle, &state)
    })
    .await
}

/// 库列表 + **给还没数过的库补一次盘扫**。
///
/// 为什么要「顺手补」：老库（或刚登记、还没导入过的库）在 `directories` 里没有行，
/// 卡片上就是「—」。第一次列表时扫一遍 `photos/` 把数字建立起来（本机磁盘，毫秒到几十毫秒），
/// 之后就都在 app.db 里了 —— 与「实时性优先于缓存」那条纪律同一个取向。
fn views<R: Runtime>(app: &AppHandle<R>, state: &DbState) -> Result<Vec<RepositoryViewDto>, String> {
    let list = state.with(app, |db| {
        db.read(repository::build_views).map_err(|e| e.to_string())
    })?;
    for view in &list {
        if view.photos_count.is_some() || !view.online {
            continue;
        }
        let (Some(root), Some(folder)) = (view.root.as_deref(), Some(DEFAULT_PHOTOS_DIR)) else {
            continue;
        };
        let (id, root) = (view.id.clone(), PathBuf::from(root));
        // 写闭包要跨线程（`'static`）：把 id / root **move 进去**（外面已经 clone 好了）
        let _ = state.with(app, move |db| {
            db.write(move |conn| {
                repository::count_library_on_disk(
                    conn,
                    &id,
                    &root,
                    folder,
                    raybend::store::time::now_millis(),
                )
                .map(|_| ())
            })
            .map_err(|error| error.to_string())
        });
    }
    // 补完之后重读一次（这次数字都在 app.db 里了）
    let list = if list.iter().any(|view| view.photos_count.is_none() && view.online) {
        state.with(app, |db| {
            db.read(repository::build_views).map_err(|e| e.to_string())
        })?
    } else {
        list
    };
    Ok(list.into_iter().map(RepositoryViewDto::from).collect())
}

/// `photos/` 目录名（库内落地目录；`FUTURE G14` 将来可配）。
const DEFAULT_PHOTOS_DIR: &str = "photos";

/// 探一个目录：里面有没有库？能不能新建？
#[tauri::command]
pub async fn repository_probe<R: Runtime>(
    app: AppHandle<R>,
    path: String,
) -> Result<RepositoryProbeDto, String> {
    let handle = app.clone();
    blocking(move || {
        let probe = repository::probe_root(Path::new(&path));
        let (name, repository_id) = match &probe {
            repository::RootProbe::Existing(meta) => (Some(meta.name.clone()), Some(meta.id.clone())),
            _ => (None, None),
        };
        let message = match &probe {
            repository::RootProbe::Broken(reason) => Some(reason.clone()),
            _ => None,
        };
        let registered = match &repository_id {
            Some(id) => {
                let state = handle.state::<DbState>();
                state.with(&handle, |db| {
                    db.list_repositories()
                        .map(|rows| rows.iter().any(|r| &r.id == id))
                        .map_err(|e| e.to_string())
                })?
            }
            None => false,
        };
        Ok(RepositoryProbeDto {
            kind: probe.code().to_string(),
            name,
            repository_id,
            registered,
            message,
        })
    })
    .await
}

/// 建库，或把一个已有库登记进来。
///
/// * 目录不存在 → **建出来**（用户手打的路径也该能用）；
/// * 目录里没有 `catalog.db` → 新建库（生成新 ID + `photos/`）；
/// * 目录里已有 `catalog.db` → **原样使用它的 ID**（这正是「同库多路径」的实现方式），
///   库名以库内记录为准，弹窗里填的名字不覆盖它；
/// * 有 `catalog.db` 却读不出来 → **拒绝**（绝不覆盖别人可能还在用的库文件）。
#[tauri::command]
pub async fn repository_create<R: Runtime>(
    app: AppHandle<R>,
    path: String,
    name: Option<String>,
) -> Result<RepositoryViewDto, String> {
    let handle = app.clone();
    blocking(move || {
        let root = PathBuf::from(&path);
        let now = time::now_millis();

        std::fs::create_dir_all(&root).map_err(|e| format!("无法创建目录 {path}：{e}"))?;
        if let repository::RootProbe::Broken(reason) = repository::probe_root(&root) {
            return Err(format!(
                "该目录下已经有 catalog.db，但它不是可用的相片库：{reason}"
            ));
        }

        let fallback = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "照片库".to_string());
        let name = name
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or(fallback);

        let backups = crate::db::data_dir(&handle)?.join(raybend::store::db::BACKUPS_DIR);
        let catalog = CatalogDb::create(&root, &name, None, OpenOpts::new(Some(&backups), now))
            .map_err(|e| e.to_string())?;
        let meta = catalog.meta().clone();
        drop(catalog); // 建完就放掉（写线程、读池都不需要一直占着）

        let state = handle.state::<DbState>();
        state.with(&handle, |db| {
            db.register_repository(&meta, &root, now)
                .map_err(|e| e.to_string())
        })?;
        /*
         * 视图在**锁外**再取：`views` 自己还要 `db.read` / `db.write`（补扫盘计数），
         * 而 `state.with` 持着那把互斥锁 —— 在它里面再叫一次就是自己等自己（死锁）。
         */
        views(&handle, &state)?
            .into_iter()
            .find(|v| v.id == meta.id)
            .ok_or_else(|| "库刚登记完却查不到，请重试".to_string())
    })
    .await
}

/// 重新挂载一个离线库：对**所有登记路径**找一遍，找到就转为在线。
///
/// 找不到**不是错误** —— 返回的视图里 `online = false`，界面照常显示离线徽标
/// （`REPOSITORY.md` §2.3：用户插上盘再点一次就行）。
#[tauri::command]
pub async fn repository_remount<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
) -> Result<RepositoryViewDto, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<DbState>();
        let id = repository_id.clone();
        let now = time::now_millis();
        state.with(&handle, |db| {
            db.write_tx(move |tx| repository::refresh_path_status(tx, &id, now))
                .map_err(|e| e.to_string())
        })?;
        // 同上：视图要在锁外取（它会顺手补一次盘扫计数，内部还要拿锁）
        views(&handle, &state)?
            .into_iter()
            .find(|v| v.id == repository_id)
            .ok_or_else(|| format!("没有这个库：{repository_id}"))
    })
    .await
}

/// 一个库现在的两个计数（离线或没数过 → `None`）。
///
/// 返回值是 `[photos, images]` 两个数（相片数量 / 图片数量）——
/// 界面上的库卡片只显示前者，齿轮弹窗里两个都显示。
#[tauri::command]
pub async fn repository_counts<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
) -> Result<Option<[i64; 2]>, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<DbState>();
        state.with(&handle, |db| {
            db.read(|conn| repository::totals(conn, &repository_id))
                .map(|counts| counts.map(|c| [c.photos, c.images]))
                .map_err(|e| e.to_string())
        })
    })
    .await
}

/// **进目录时同步计数**（人类 2026-09-19 的要求）。
///
/// 做什么：把 `scopePath` 这个目录在磁盘上的真实文件数读出来（本目录 + 它自己的 `_RAW`），
/// 与 `app.db` 里那行对比 —— 不一样就写回去，并把差值滚到库级汇总上。
///
/// 为什么每次进目录都做：**本地应用实时性优先**（`AGENTS.md` §2 #13）——
/// 程序外面往目录里加/删文件是常事，一次 `readdir` 是微秒级，没必要为省它去承担「数字对不上」。
///
/// 返回：[目录的计数, 库级汇总]。
#[tauri::command]
pub async fn repository_sync_dir<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    scope_path: Option<String>,
) -> Result<Option<[[i64; 2]; 2]>, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<DbState>();
        let Some(root) = state.with(&handle, |db| {
            db.resolve_repository(&repository_id)
                .map(|resolved| match resolved {
                    repository::RepositoryState::Online { root } => Some(root),
                    repository::RepositoryState::Offline { .. } => None,
                })
                .map_err(|e| e.to_string())
        })?
        else {
            return Ok(None);
        };
        let Some(scope) = scope_path.filter(|path| !path.is_empty()) else {
            return Ok(None);
        };
        let now = time::now_millis();
        let (id, scope_for_task) = (repository_id.clone(), scope.clone());
        state
            .with(&handle, move |db| {
                db.write(move |conn| {
                    let counts = repository::count_dir_on_disk(&root, &scope_for_task);
                    repository::set_directory_counts(conn, &id, &scope_for_task, counts, now)?;
                    let totals = repository::totals(conn, &id)?.unwrap_or_default();
                    Ok([[counts.photos, counts.images], [totals.photos, totals.images]])
                })
                .map_err(|e| e.to_string())
            })
            .map(Some)
    })
    .await
}

/* ══════════════════════════════════════════════════════════════
 * 重建数据（人类 2026-09-19：齿轮弹窗里的那个按钮）
 * ══════════════════════════════════════════════════════════════ */

/// 「重建数据」的结果（给用户看的一句话 + 几个数字）。
#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebuildReportDto {
    /// 扫到多少文件。
    pub scanned: usize,
    /// 新登记进来的文件（库里有记录之外、磁盘上多出来的）。
    pub registered: usize,
    /// 新标记为「磁盘上找不到」的。
    pub missing: usize,
    /// 之前找不到、这次又出现的。
    pub returned: usize,
    /// 路径变了（文件被改名/移动）但认出来的。
    pub renamed: usize,
    /// 补上元数据的资产数（老库那批没有 EXIF 的）。
    pub metadata_filled: usize,
    /// 重建后的**相片数量**。
    pub photos_count: i64,
    /// 重建后的**图片数量**。
    pub images_count: i64,
}

/// 重扫整个库、把 catalog 与 app.db 的计数拉平（耗时，界面上要挡住操作）。
///
/// 四件事，顺序不能反：
///   1. **扫盘**（`photos/` 之下）+ 与 `asset_files` 对比 → 登记新文件、标记缺失、修正路径；
///   2. **重读元数据**（`taken_at` / 宽高 / 朝向为空的老资产）—— 老库那批的根因就是它们空着；
///   3. **重算目录计数**（清空后按磁盘重数一遍）；
///   4. 汇总进 `repositories`（第 3 步里已经顺带做了）。
#[tauri::command]
pub async fn repository_rebuild<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
) -> Result<RebuildReportDto, String> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<DbState>();
        let root = online_root(&handle, &repository_id)?;
        let now = time::now_millis();
        let catalog = CatalogDb::open(&root, OpenOpts::new(backups(&handle).as_deref(), now))
            .map_err(|e| e.to_string())?;

        // ①② 磁盘 ↔ catalog 对齐 + 元数据重读
        let rescan = raybend::store::rebuild::rescan_library(&catalog, &root, DEFAULT_PHOTOS_DIR, now)
            .map_err(|e| e.to_string())?;

        // ③④ 计数：清空重来（这一份在 app.db 里）
        let (id, dir) = (repository_id.clone(), root.clone());
        let totals = state
            .with(&handle, move |db| {
                db.write(move |conn| {
                    repository::clear_directories(conn, &id)?;
                    repository::count_library_on_disk(conn, &id, &dir, DEFAULT_PHOTOS_DIR, now)
                })
                .map_err(|e| e.to_string())
            })
            .map_err(|e| e.to_string())?;

        Ok(RebuildReportDto {
            scanned: rescan.scanned,
            registered: rescan.registered,
            missing: rescan.missing,
            returned: rescan.returned,
            renamed: rescan.renamed,
            metadata_filled: rescan.metadata_filled,
            photos_count: totals.photos,
            images_count: totals.images,
        })
    })
    .await
}

/* ══════════════════════════════════════════════════════════════
 * 库设置：导入模版（M1-6 的「齿轮」）
 * ══════════════════════════════════════════════════════════════ */

/// 库设置里可改的东西（M1 里只有导入模版）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositorySettingsDto {
    /// 库 id。
    pub repository_id: String,
    /// 当前导入模版。
    pub import_template: String,
}

/// 模版预览（**纯函数命令**：不碰库、不碰盘，所以可以在用户打字时随手调）。
///
/// 预览的是「几张示例照片按这个模版会落到哪」—— 包括 `_RAW/` 那条分流规则
/// （它不属于模版本身，但用户看预览时就是想看这个）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplatePreviewDto {
    /// 模版能不能用。
    pub ok: bool,
    /// 不能用时的原因（给用户看的一句话）。
    pub error: Option<String>,
    /// 能用但值得提醒（例如不认识的变量）。
    pub warnings: Vec<String>,
    /// 示例照片的落盘路径（用不了时是空的）。
    pub paths: Vec<String>,
}

/// 一个库当前设置（打不开/离线就报错 —— 设置必须在线改）。
#[tauri::command]
pub async fn repository_settings<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
) -> Result<RepositorySettingsDto, String> {
    let handle = app.clone();
    blocking(move || {
        let root = online_root(&handle, &repository_id)?;
        let now = time::now_millis();
        let catalog = CatalogDb::open(&root, OpenOpts::new(backups(&handle).as_deref(), now))
            .map_err(|e| e.to_string())?;
        Ok(RepositorySettingsDto {
            repository_id: repository_id.clone(),
            import_template: catalog.meta().import_template.clone(),
        })
    })
    .await
}

/// 改一个库的导入模版。
///
/// 两处都要写：库自己的 `catalog.db`（真相源）与 `app.db` 的缓存
/// （离线时界面也要能显示它是什么模版）。
#[tauri::command]
pub async fn repository_set_template<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    template_source: String,
) -> Result<RepositorySettingsDto, String> {
    let handle = app.clone();
    blocking(move || {
        // 先校验：模版坏了就别写进库（否则下次导入才发现）
        template::parse(&template_source).map_err(|e| e.to_string())?;
        let root = online_root(&handle, &repository_id)?;
        let now = time::now_millis();
        let catalog = CatalogDb::open(&root, OpenOpts::new(backups(&handle).as_deref(), now))
            .map_err(|e| e.to_string())?;
        catalog
            .set_import_template(&template_source)
            .map_err(|e| e.to_string())?;
        let mut meta = catalog.meta().clone();
        meta.import_template = template_source.clone();
        drop(catalog);

        let state = handle.state::<DbState>();
        state.with(&handle, |db| {
            db.register_repository(&meta, &root, now)
                .map_err(|e| e.to_string())
        })?;
        Ok(RepositorySettingsDto {
            repository_id,
            import_template: template_source,
        })
    })
    .await
}

/// 模版预览（纯函数：拿几张示例照片渲染一遍）。
#[tauri::command]
#[must_use]
pub fn repository_template_preview(template_source: String) -> TemplatePreviewDto {
    let parsed = match template::parse(&template_source) {
        Ok(parsed) => parsed,
        Err(error) => {
            return TemplatePreviewDto {
                ok: false,
                error: Some(error.to_string()),
                warnings: Vec::new(),
                paths: Vec::new(),
            };
        }
    };
    // 示例照片：同一天的 3 张位图 + 1 张 RAW（后者演示 `_RAW/` 分流）
    let day = time::from_civil(2026, 8, 15, 12, 0, 0).unwrap_or(0);
    let samples = [
        ("P0001", "jpg", 1u64),
        ("P0002", "jpg", 2),
        ("P0003", "JPG", 3),
        ("P0004", "ORF", 4),
    ];
    let mut paths: Vec<String> = Vec::new();
    for (stem, ext, seq) in samples {
        let values = [(3usize, seq)];
        let ctx = raybend::import::template::RenderCtx {
            taken_at: Some(day),
            stem,
            brand: Some("NIKON"),
            model: Some("Z7II"),
            seqs: raybend::import::template::SeqValues::new(&values),
        };
        let rendered = parsed.render(&ctx);
        let (dir, name) = rendered.split_dir_name();
        let dir = dir.map_or(String::from("photos"), |d| format!("photos/{d}"));
        let path = match ext.eq_ignore_ascii_case("ORF") {
            // RAW 有同名位图时进 `_RAW/`（`REPOSITORY.md` §4.1）
            true => format!("{dir}/_RAW/{name}.{ext}"),
            false => format!("{dir}/{name}.{ext}"),
        };
        paths.push(path);
    }
    TemplatePreviewDto {
        ok: true,
        error: None,
        warnings: parsed.warnings().iter().map(ToString::to_string).collect(),
        paths,
    }
}

/// 在线库的根目录（离线给一句人话）。
fn online_root<R: Runtime>(app: &AppHandle<R>, repository_id: &str) -> Result<std::path::PathBuf, String> {
    let state = app.state::<DbState>();
    state.with(app, |db| {
        match db.resolve_repository(repository_id).map_err(|e| e.to_string())? {
            repository::RepositoryState::Online { root } => Ok(root),
            repository::RepositoryState::Offline { tried } => Err(format!(
                "库当前离线：登记过的 {tried} 个路径下都没有找到它"
            )),
        }
    })
}

/// 迁移前快照目录。
fn backups<R: Runtime>(app: &AppHandle<R>) -> Option<std::path::PathBuf> {
    crate::db::data_dir(app).ok().map(|dir| dir.join(raybend::store::db::BACKUPS_DIR))
}
