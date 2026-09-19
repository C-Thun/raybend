//! 导入的外壳侧接线：**命令 + 进度事件**。
//!
//! 这里不做业务（业务在 `raybend::import` 里），只负责四件事：
//!
//! 1. 用 Tauri 的 path/state API 拿到库根与 `app.db`（`AGENTS.md` §6.2：不硬编码路径）；
//! 2. 把执行器要的注入点接上：`CatalogSink`（catalog.db）、`RepoFs`（真文件系统）、
//!    `FsScanner`、以及「把缩略图任务排进 app.db 的 jobs 表」这个回调
//!    （核心 crate 不认识 `app.db` —— 它的单写者归外壳持有）；
//! 3. 起一条**导入线程**（不是 `spawn_blocking`：一批导入要活几分钟）；
//! 4. 把进度快照发成 `import://progress` 事件，并把批次留在内存表里供
//!    `import_status` / 暂停 / 取消 / 导出错误清单查询。
//!
//! 批次 id 是**进程内**的（重启后没有活线程，自然不该有批次）——
//! 重启后要提示「上次导入被中断」靠的是库里的 `import_runs.state='running'`
//! （见 [`import_interrupted`]）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use raybend::import::fsops::{FsScanner, RepoFs};
use raybend::import::progress::{BatchHandle, BatchProgress, ImportState};
use raybend::import::runner::{run_batch, Control, Deps, RunRequest};
use raybend::import::sink::CatalogSink;
use raybend::import::space;
use raybend::import::template;
use raybend::media::scan::Cancel;
use raybend::store::db::{CatalogDb, OpenOpts, BACKUPS_DIR};
use raybend::store::repository::{self, RepositoryState};
use raybend::store::time;
use raybend::thumbnail::worker::{enqueue_many, ThumbJob};
use raybend::thumbnail::SizeClass;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::db::{data_dir, DbState};

/// 进度事件名（前端 `listen` 的就是它）。
pub const PROGRESS_EVENT: &str = "import://progress";

/// 内存里保留多少个已结束的批次（够弹窗读完成摘要就行）。
const KEEP_FINISHED: usize = 3;

/// 进度事件的节流间隔（执行器内部也按这个节流）。
const THROTTLE: Duration = Duration::from_millis(100);

/// 正在跑的批次表（进程内）。
#[derive(Default)]
pub struct ImportBatches {
    inner: Mutex<HashMap<String, Batch>>,
}

struct Batch {
    handle: BatchHandle,
    control: Control,
}

impl ImportBatches {
    fn insert(&self, batch_id: String, batch: Batch) {
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        guard.insert(batch_id, batch);
        // 只留最近几个已结束的批次（不然长会话里会一直涨）
        if guard.len() > KEEP_FINISHED {
            let mut finished: Vec<(String, i64)> = guard
                .iter()
                .filter_map(|(id, batch)| {
                    batch
                        .handle
                        .snapshot()
                        .finished_at
                        .map(|at| (id.clone(), at))
                })
                .collect();
            finished.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
            for (id, _) in finished.into_iter().skip(KEEP_FINISHED) {
                guard.remove(&id);
            }
        }
    }

    fn snapshot(&self, batch_id: &str) -> Result<BatchProgress, String> {
        self.inner
            .lock()
            .map_err(|_| "内部锁已损坏".to_string())?
            .get(batch_id)
            .map(|batch| batch.handle.snapshot())
            .ok_or_else(|| format!("没有这个导入批次：{batch_id}"))
    }

    fn control<T>(&self, batch_id: &str, f: impl FnOnce(&Control) -> T) -> Result<T, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| "内部锁已损坏".to_string())?;
        let batch = guard
            .get(batch_id)
            .ok_or_else(|| format!("没有这个导入批次：{batch_id}"))?;
        Ok(f(&batch.control))
    }
}

/// `import_precheck` 的结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPrecheckDto {
    /// 源里一共多少字节（只 stat，不读内容）。
    pub total_bytes: u64,
    /// 目标卷可用空间（拿不到就是 `None`）。
    pub free_bytes: Option<u64>,
    /// 够不够（含 5% 余量）。
    pub tight: bool,
    /// 判断里用的「需要多少」（含余量）。
    pub needed_bytes: u64,
}

/// 一个待导入的源目录（「包含子目录」是**每个目录各一份**的开关，
/// 界面上的来源树就是这么给的 —— 不能拿一个布尔值糊弄整批）。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSourceArg {
    /// 源根目录。
    pub path: String,
    /// 是否包含子目录。
    pub include_subdirs: bool,
}

/// `import_start` 的结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportStartDto {
    /// 进程内的批次 id。
    pub batch_id: String,
    /// 这批要跑哪些源目录（`runId` 在该目录真的开始时才分配，事件里带着）。
    pub runs: Vec<PlannedRunDto>,
}

/// 一个待跑的源目录。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedRunDto {
    /// 在这批里的序号（0 起）。
    pub index: usize,
    /// 源根目录。
    pub source_root: String,
}

/// 上次被中断的 run（应用重启后提示用）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InterruptedRunDto {
    /// `import_runs.id`。
    pub run_id: i64,
    /// 源根目录。
    pub source_root: String,
    /// 当时用的模版。
    pub template: String,
    /// 开始时间（Unix 毫秒）。
    pub started_at: i64,
    /// 已经导进来多少。
    pub imported: i64,
    /// 跳过多少。
    pub skipped: i64,
    /// 失败多少。
    pub failed: i64,
}

/// 开工前估一下：这批多少字节、目标卷够不够。
#[tauri::command]
pub async fn import_precheck<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    sources: Vec<ImportSourceArg>,
) -> Result<ImportPrecheckDto, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = resolve_root(&handle, &repository_id)?;
        let scanner = FsScanner;
        // 一个目录一个开关，所以逐个估（源的规模远比库小，重复走一遍不肉疼）
        let mut total_bytes = 0u64;
        for source in &sources {
            total_bytes = total_bytes.saturating_add(
                space::estimate_bytes(
                    &scanner,
                    &[PathBuf::from(&source.path)],
                    source.include_subdirs,
                    &Cancel::new(),
                )
                .map_err(|e| e.to_string())?,
            );
        }
        let free_bytes = raybend::import::fsops::free_bytes_of(&root);
        let verdict = space::check_space(total_bytes, free_bytes);
        let needed_bytes = space::needed_with_margin(total_bytes);
        let tight = verdict.is_tight();
        Ok(ImportPrecheckDto {
            total_bytes,
            free_bytes,
            tight,
            needed_bytes,
        })
    })
    .await
    .map_err(|e| format!("预检失败：{e}"))?
}

/// 开始导入：每个源目录一个 run，整批一个 `batchId`。
///
/// `excluded` = 用户在网格里**排除掉的文件**（绝对路径，跨目录、跨源共用一份）。
/// 它们不会进规划、不会计入任何计数 —— 与「尝试过但跳过」不是一回事（见 runner）。
#[tauri::command]
pub async fn import_start<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    sources: Vec<ImportSourceArg>,
    avoid_duplicates: bool,
    excluded: Option<Vec<String>>,
) -> Result<ImportStartDto, String> {
    if sources.is_empty() {
        return Err("没有选中任何源目录".to_string());
    }

    let handle = app.clone();
    let repository_id_for_lookup = repository_id.clone();
    let (root, template_source) = tauri::async_runtime::spawn_blocking(move || {
        let root = resolve_root(&handle, &repository_id_for_lookup)?;
        let now = time::now_millis();
        let catalog = CatalogDb::open(&root, OpenOpts::new(backups_dir(&handle).as_deref(), now))
            .map_err(|e| e.to_string())?;
        // 模版从库里**现读**（刚在库设置里改过时也能拿到最新的）
        let template = catalog.meta().import_template.clone();
        Ok::<_, String>((root, template))
    })
    .await
    .map_err(|e| format!("读取库信息失败：{e}"))??;

    let parsed = template::parse(&template_source)
        .map_err(|e| format!("这个库的导入模版有问题：{e}"))?;

    let batch_id = format!("b{}", time::now_millis());
    let progress = BatchProgress::new(batch_id.clone(), time::now_millis());
    let batch_handle = BatchHandle::new(progress);
    let control = Control::new();

    let excluded: std::sync::Arc<std::collections::HashSet<String>> =
        std::sync::Arc::new(excluded.unwrap_or_default().into_iter().collect());

    let jobs: Vec<RunRequest> = sources
        .iter()
        .enumerate()
        .map(|(index, source)| RunRequest {
            index,
            source_root: PathBuf::from(&source.path),
            photos_dir: repository::DEFAULT_PHOTOS_DIR.to_string(),
            template: parsed.clone(),
            template_source: template_source.clone(),
            include_subdirs: source.include_subdirs,
            avoid_duplicates,
            excluded: excluded.clone(),
        })
        .collect();

    let planned: Vec<PlannedRunDto> = jobs
        .iter()
        .map(|job| PlannedRunDto {
            index: job.index,
            source_root: job.source_root.display().to_string(),
        })
        .collect();

    // 登记到内存表（暂停/取消/状态查询都靠它）
    {
        let batches = app.state::<ImportBatches>();
        batches.insert(
            batch_id.clone(),
            Batch {
                handle: batch_handle.clone(),
                control: control.clone(),
            },
        );
    }

    let app_for_thread = app.clone();
    let repository_id_for_thread = repository_id.clone();
    std::thread::spawn(move || {
        run_import_thread(
            app_for_thread,
            root,
            repository_id_for_thread,
            jobs,
            batch_handle,
            control,
        );
    });

    Ok(ImportStartDto {
        batch_id,
        runs: planned,
    })
}

/// 暂停整批（生效点在文件之间）。
#[tauri::command]
pub fn import_pause<R: Runtime>(
    app: AppHandle<R>,
    batch_id: String,
) -> Result<BatchProgress, String> {
    app.state::<ImportBatches>()
        .control(&batch_id, Control::pause)?;
    app.state::<ImportBatches>().snapshot(&batch_id)
}

/// 继续。
#[tauri::command]
pub fn import_resume<R: Runtime>(
    app: AppHandle<R>,
    batch_id: String,
) -> Result<BatchProgress, String> {
    app.state::<ImportBatches>()
        .control(&batch_id, Control::resume)?;
    app.state::<ImportBatches>().snapshot(&batch_id)
}

/// 取消（**已导入的部分保留**，`REPOSITORY.md` §4.5）。
#[tauri::command]
pub fn import_cancel<R: Runtime>(
    app: AppHandle<R>,
    batch_id: String,
) -> Result<BatchProgress, String> {
    app.state::<ImportBatches>()
        .control(&batch_id, Control::cancel)?;
    app.state::<ImportBatches>().snapshot(&batch_id)
}

/// 当前快照（弹窗重开、刷新后恢复现场用）。
#[tauri::command]
pub fn import_status<R: Runtime>(
    app: AppHandle<R>,
    batch_id: String,
) -> Result<BatchProgress, String> {
    app.state::<ImportBatches>().snapshot(&batch_id)
}

/// 把这一批的错误清单写成 JSON（路径由前端用保存对话框挑）。
#[tauri::command]
pub fn import_errors_export<R: Runtime>(
    app: AppHandle<R>,
    batch_id: String,
    path: String,
) -> Result<usize, String> {
    let snapshot = app.state::<ImportBatches>().snapshot(&batch_id)?;
    let payload = serde_json::json!({
        "batchId": snapshot.batch_id,
        "exportedAt": time::now_millis(),
        "summary": {
            "total": snapshot.total,
            "imported": snapshot.imported,
            "skipped": snapshot.skipped,
            "failed": snapshot.failed,
            "errorsTotal": snapshot.errors_total,
        },
        "runs": snapshot.runs.iter().map(|run| serde_json::json!({
            "runId": run.run_id,
            "sourceRoot": run.source_root,
            "state": run.state,
            "imported": run.imported,
            "skipped": run.skipped,
            "failed": run.failed,
        })).collect::<Vec<_>>(),
        "errors": snapshot.errors,
    });
    let text = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("写不了 {path}：{e}"))?;
    Ok(snapshot.errors.len())
}

/// 上次**被中断**的导入（应用重启后提示「可继续」用）。
#[tauri::command]
pub async fn import_interrupted<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
) -> Result<Vec<InterruptedRunDto>, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = resolve_root(&handle, &repository_id)?;
        let now = time::now_millis();
        let catalog = CatalogDb::open(&root, OpenOpts::new(backups_dir(&handle).as_deref(), now))
            .map_err(|e| e.to_string())?;
        catalog
            .read(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, source_root, template, started_at, imported, skipped, failed
                       FROM import_runs
                      WHERE state = 'running'
                      ORDER BY started_at DESC",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(InterruptedRunDto {
                        run_id: row.get(0)?,
                        source_root: row.get(1)?,
                        template: row.get(2)?,
                        started_at: row.get(3)?,
                        imported: row.get(4)?,
                        skipped: row.get(5)?,
                        failed: row.get(6)?,
                    })
                })?;
                Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
            })
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("查询被中断的导入失败：{e}"))?
}

/* ══════════════════════════════════════════════════════════════
 * 内部
 * ══════════════════════════════════════════════════════════════ */

/// 导入线程：自己开一个 `catalog.db`，把注入点接好后跑完整批。
///
/// **不能复用命令期的 `CatalogDb`**：那一份属于某次命令调用，
/// 命令返回后就该释放；导入要活几分钟，得自己持有一份。
fn run_import_thread<R: Runtime>(
    app: AppHandle<R>,
    root: PathBuf,
    repository_id: String,
    jobs: Vec<RunRequest>,
    handle: BatchHandle,
    control: Control,
) {
    let now = time::now_millis();
    let catalog = match CatalogDb::open(&root, OpenOpts::new(backups_dir(&app).as_deref(), now)) {
        Ok(catalog) => catalog,
        Err(error) => {
            // 库打不开：整批判失败，并把原因写进快照（弹窗要能看到）
            handle.with(|progress| {
                progress.state = ImportState::Failed;
                progress.finished_at = Some(time::now_millis());
                progress.push_error(raybend::import::progress::ImportError {
                    source: root.display().to_string(),
                    target: None,
                    reason: format!("打不开库：{error}"),
                    status: "failed".to_string(),
                });
            });
            let _ = app.emit(PROGRESS_EVENT, &handle.snapshot());
            return;
        }
    };

    let mut sink = CatalogSink::new(&catalog, now);
    let ops = RepoFs::new(&root);
    let scanner = FsScanner;

    let mut on_progress = |progress: &BatchProgress| {
        // 事件发不出去（窗口关了）不该影响导入
        let _ = app.emit(PROGRESS_EVENT, progress);
    };

    let mut enqueue_thumbs = |paths: &[String]| -> raybend::Result<usize> {
        let state = app.state::<DbState>();
        let jobs: Vec<ThumbJob> = paths
            .iter()
            .map(|rel| ThumbJob::new(&repository_id, rel, SizeClass::Grid))
            .collect();
        state
            .with(&app, |db| {
                db.write(move |conn| enqueue_many(conn, &jobs, now))
                    .map_err(|e| e.to_string())
            })
            .map_err(raybend::Error::Unsupported)
    };

    let deps = Deps {
        sink: &mut sink,
        ops: &ops,
        scanner: &scanner,
        control: &control,
        handle: &handle,
        on_progress: &mut on_progress,
        enqueue_thumbs: &mut enqueue_thumbs,
        now_ms: now,
        throttle: THROTTLE,
    };
    run_batch(deps, &jobs);

    /*
     * 导入之后的**目录计数**（人类 2026-09-19 的数量体系）：
     * 「导入时，是先统计导入目录……随后将这个库下的所有 directories 的这两个 count 加起来，
     * 更新进 repository」。
     *
     * 实现上不去追模版算出来的目标路径，而是**回看库里现在有哪些目录**（`asset_files` 的
     * 父目录去重）——模版是可变的、还可能带子目录透传，从结果反推永远比从参数正推准。
     * 每个目录一次 `readdir`（本目录 + `_RAW`），本地盘上是微秒级。
     */
    refresh_directory_counts(&app, &repository_id, &root);

    // 终态一定再发一条（节流不该让「已完成」这件事丢在路上）
    let _ = app.emit(PROGRESS_EVENT, &handle.snapshot());
}

/// 把库里的**目录计数**重算一遍（导入结束时调；`重建数据`也走同一条路）。
///
/// 做法：把 `photos/` 之下**按目录**数一遍（本目录 + 它自己的 `_RAW`），逐目录写进
/// `app.db` 的 `directories`，并在同一个事务里把库级汇总求和写回 `repositories` ——
/// 正是人类 2026-09-19 说的「将这个库下的所有 directories 的这两个 count 加起来」。
///
/// 为什么整库重数而不是只数这次导入落地的目录：模版可变、子目录透传也不定，
/// 从结果反推（库里现在有哪些目录）比从参数正推稳；而且导入本来就是重活，
/// 多走一遍 `readdir` 树（本地盘、毫秒级）不值得为它省。
///
/// 失败**不打扰用户**：导入本身已经成功了，计数只是展示数字；下次进目录的增量同步会补上。
fn refresh_directory_counts<R: Runtime>(
    app: &AppHandle<R>,
    repository_id: &str,
    root: &std::path::Path,
) {
    let now = time::now_millis();
    let (id, root) = (repository_id.to_string(), root.to_path_buf());
    let state = app.state::<DbState>();
    let result = state.with(app, move |db| {
        db.write(move |conn| {
            repository::count_library_on_disk(conn, &id, &root, PHOTOS_DIR, now).map(|_| ())
        })
        .map_err(|error| error.to_string())
    });
    if let Err(error) = result {
        eprintln!("[raybend] 写目录计数失败：{error}");
    }
}

/// 库内落地目录名（默认 `photos`；`FUTURE G14` 将来可配）。
const PHOTOS_DIR: &str = "photos";

/// 库根：离线库直接给一句人话。
fn resolve_root<R: Runtime>(app: &AppHandle<R>, repository_id: &str) -> Result<PathBuf, String> {
    let state = app.state::<DbState>();
    state.with(app, |db| {
        let resolved = db
            .resolve_repository(repository_id)
            .map_err(|e| e.to_string())?;
        match resolved {
            RepositoryState::Online { root } => Ok(root),
            RepositoryState::Offline { tried } => {
                // 离线时把库名带上（`RepositoryState` 只给了「试过几处」）
                let name = db
                    .list_repositories()
                    .ok()
                    .and_then(|rows| {
                        rows.into_iter()
                            .find(|row| row.id == repository_id)
                            .map(|row| row.name)
                    })
                    .unwrap_or_else(|| repository_id.to_string());
                Err(format!(
                    "库「{name}」当前离线：登记过的 {tried} 个路径下都没有找到它"
                ))
            }
        }
    })
}

/// 迁移前快照的目录（`AGENTS.md` §6.4）。
fn backups_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    data_dir(app).ok().map(|dir| dir.join(BACKUPS_DIR))
}
