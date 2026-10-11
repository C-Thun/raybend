//! 不可变定稿 IPC 与独立图像快照。

use crate::browse::BrowseState;
use crate::develop::{self, DevelopStackDto, ResolvedAsset};
use crate::thumbs::SourcesThumbs;
use raybend::display::{CacheClass, FullCache};
use raybend::store::develop::{DevelopStack, EditBase, load as load_stack};
use raybend::store::issues::{self, Issue, Selection};
use raybend::store::time;
use raybend::thumbnail::SizeClass;
use raybend::thumbnail::{cache, render};
use serde::Serialize;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, Runtime};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueDto {
    pub id: i64,
    pub name: String,
    pub profile_hash: String,
    pub source_base: String,
    pub created_at: i64,
    /// 定稿的**成片输出尺寸**（`issues.output_*`；老库未回填 → `null`）。
    /// 显示尺寸的唯一口径见 `specs/display-size-source.md` §5。
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub stack: DevelopStackDto,
}

impl From<Issue> for IssueDto {
    fn from(issue: Issue) -> Self {
        Self {
            id: issue.id,
            name: issue.name,
            profile_hash: issue.profile_hash,
            source_base: issue.source_base.as_str().to_string(),
            created_at: issue.created_at,
            width: issue.width,
            height: issue.height,
            stack: issue.stack.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueLibraryDto {
    pub issues: Vec<IssueDto>,
    pub selection: Selection,
    pub can_finalize: bool,
    pub suggested_name: Option<String>,
    pub snapshot_error: Option<String>,
}

fn library<R: Runtime>(
    app: &AppHandle<R>,
    repo: &str,
    asset_id: i64,
    english: bool,
    current: Option<DevelopStack>,
) -> Result<IssueLibraryDto, String> {
    let browse = app.state::<BrowseState>();
    browse.with_catalog(app, repo, |db| {
        /*
         * 懒回填：老库里尺寸为 NULL 的定稿，能算就算一个补上（只补空值；
         * 失败只记日志，不影响这次读 —— `specs/display-size-source.md` §3.2）。
         */
        let target = asset_id;
        if let Err(error) = db.write(move |conn| issues::backfill_output_sizes(conn, target)) {
            // i18n-exempt: 终端诊断（回填失败不拦读定稿，下次再来）
            eprintln!("[issues] 定稿尺寸回填跳过（asset {asset_id}）：{error}");
        }
        db.read(|conn| {
            let stack = match current {
                Some(stack) => stack,
                None => load_stack(conn, asset_id)?,
            };
            let issues = issues::list(conn, asset_id)?;
            let selection = issues::selection(&stack, &issues)?;
            let can_finalize = issues::can_finalize(&stack, &issues)?;
            let suggested_name = if can_finalize {
                Some(issues::suggested_name(conn, asset_id, &stack, english)?)
            } else {
                None
            };
            Ok(IssueLibraryDto {
                issues: issues.into_iter().map(Into::into).collect(),
                selection,
                can_finalize,
                suggested_name,
                snapshot_error: None,
            })
        })
        .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub async fn issue_library<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    asset_id: i64,
    english: Option<bool>,
    current_stack: Option<DevelopStackDto>,
) -> Result<IssueLibraryDto, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        let current = current_stack.map(DevelopStackDto::into_stack).transpose()?;
        library(
            &handle,
            &repository_id,
            asset_id,
            english.unwrap_or(false),
            current,
        )
    })
    .await
}

fn issue_of<R: Runtime>(
    app: &AppHandle<R>,
    repo: &str,
    asset_id: i64,
    issue_id: i64,
) -> Result<Issue, String> {
    let browse = app.state::<BrowseState>();
    browse
        .with_catalog(app, repo, |db| {
            db.read(|conn| issues::get(conn, asset_id, issue_id))
                .map_err(|error| error.to_string())
        })?
        .ok_or("定稿不存在".into())
}

fn asset_for<R: Runtime>(
    app: &AppHandle<R>,
    repo: &str,
    asset_id: i64,
) -> Result<ResolvedAsset, String> {
    Ok(ResolvedAsset {
        repository_id: repo.to_string(),
        asset_id,
        root: crate::browse::resolve_root(app, repo)?,
        rel_path: String::new(),
    })
}

pub(crate) fn cache_key(repo: &str, asset_id: i64, issue_id: i64) -> Vec<u8> {
    format!("issue:{repo}:{asset_id}:{issue_id}").into_bytes()
}
fn render_signature(issue: &Issue, source: &str) -> String {
    format!(
        "issue-v{}-{}-src{source}",
        render::PIPELINE_VERSION,
        issue.profile_hash
    )
}
fn preview_name(issue: &Issue, source: &str) -> String {
    FullCache::source_name(
        &format!("issue-{}-{}", issue.id, issue.profile_hash),
        source,
    )
}
fn source_signature<R: Runtime>(
    app: &AppHandle<R>,
    repo: &str,
    issue: &Issue,
) -> Result<String, String> {
    let asset = asset_for(app, repo, issue.asset_id)?;
    let source =
        develop::source_path_of(app, &asset, issue.source_base).ok_or("定稿源文件不可用")?;
    raybend::media::source::source_signature(Path::new(&source)).map_err(|e| e.to_string())
}

/// 定稿时等 `latest` 预览出现的上限。
///
/// 定稿那一刻两条后台链在跑：前端的预览队列（`develop_preview_refresh`）与本文件的
/// 快照生成。与其自己再渲一遍同一张画面，不如**等它落地**（`work::WorkQueue::wait`，
/// 条件变量，不轮询、不吃 CPU）。
const LATEST_PREVIEW_WAIT_MS: u64 = 30_000;

/// 同栈复用 `latest` 预览：等到它出现（最多 `wait_ms`）就返回字节。
///
/// `wait_ms = 0` 只查一次（按需路径用 —— 等不到就直接渲染，不能把对比 / 全屏拖住）。
/// 等待走**后台队列的同 key 单飞**（§4.3）：前端预览队列正在渲同一张画面时，
/// 这里等它落地再复制，省掉第二遍整管线；**不轮询**（有结果就醒）。
/// `sessions` 用来在等待期间**感知库退场**：退场了立刻放弃，不把 `TaskPermit` 押满超时。
/// 在队列工作线程上（嵌套场景）不等 —— 见 `work::on_worker_thread`，宁可自己渲。
///
/// 边界：latest 又被编辑后它的预览就不是这张定稿的画面了 —— 每次拿到文件都重读一次库栈，
/// 栈不同就放弃（返回 `None`，调用方走整渲染）。预览始终不出现（没预览任务 / 写失败）
/// 也走整渲染：正确优先，多花一次渲染可接受。
fn reuse_latest_preview<R: Runtime>(
    app: &AppHandle<R>,
    catalog: &raybend::store::db::CatalogDb,
    full: &FullCache,
    issue: &Issue,
    source_signature: &str,
    sessions: &raybend::store::session::CatalogSessions,
    repo: &str,
    wait_ms: u64,
) -> Result<Option<Vec<u8>>, String> {
    let name = FullCache::source_name(
        &crate::develop::latest_variant(&issue.stack)?,
        source_signature,
    );
    let read = |full: &FullCache| -> Result<Option<Vec<u8>>, String> {
        let Some(bytes) = full.read(
            issue.asset_id,
            &name,
            issue.source_base,
            render::PIPELINE_VERSION,
        ) else {
            return Ok(None);
        };
        let latest = catalog
            .read(|conn| load_stack(conn, issue.asset_id))
            .map_err(|error| error.to_string())?;
        Ok((latest == issue.stack).then_some(bytes))
    };
    if let Some(bytes) = read(full)? {
        return Ok(Some(bytes));
    }
    if wait_ms > 0 && !sessions.is_released(repo) && !crate::work::on_worker_thread() {
        let key = crate::develop::latest_preview_key(repo, issue.asset_id, &issue.stack)?;
        let waited = app.state::<crate::work::WorkQueue>().wait(
            &key,
            std::time::Duration::from_millis(wait_ms),
        );
        if waited.is_some() && !sessions.is_released(repo) {
            return read(full);
        }
    }
    Ok(None)
}

/// 生成定稿的三张图（1920 预览 + grid/strip 小图）。
///
/// `wait_for_latest` = 要不要等 latest 预览（见 [`reuse_latest_preview`]）：
/// * 定稿创建的后台任务传 `true` —— 那一刻前端预览队列正在渲染同一张画面，等它比再渲一遍便宜；
/// * 按需路径（对比 / 全屏读预览）传 `false` —— 用户正等着看图，不能干等。
fn ensure_snapshots<R: Runtime>(
    app: &AppHandle<R>,
    repo: &str,
    issue: &Issue,
    wait_for_latest: bool,
) -> Result<(), String> {
    let browse = app.state::<BrowseState>();
    let _permit = browse.sessions.begin_task(repo).map_err(|e| e.to_string())?;
    let catalog = browse.lease(app, repo)?;
    let asset = asset_for(app, repo, issue.asset_id)?;
    if catalog.root()!=asset.root { return Err(raybend::Error::SessionExpired.to_string()); }
    let source = develop::source_path_of(app, &asset, issue.source_base)
        .ok_or_else(|| format!("定稿的 {} 源文件不可用", issue.source_base.as_str()))?;
    if !Path::new(&source).is_file() || EditBase::of_file(&source) != issue.source_base {
        return Err(format!(
            "定稿的 {} 源文件不可用",
            issue.source_base.as_str()
        ));
    }
    let full = FullCache::open(&asset.root).map_err(|error| error.to_string())?;
    let source_signature =
        raybend::media::source::source_signature(Path::new(&source)).map_err(|e| e.to_string())?;
    let name = preview_name(issue, &source_signature);
    let preview = if let Some(bytes) = full.read(
        issue.asset_id,
        &name,
        issue.source_base,
        render::PIPELINE_VERSION,
    ) {
        bytes
    } else if let Some(bytes) = reuse_latest_preview(
        app,
        &catalog,
        &full,
        issue,
        &source_signature,
        &browse.sessions,
        repo,
        if wait_for_latest { LATEST_PREVIEW_WAIT_MS } else { 0 },
    )? {
        /*
         * 同一张图**不再渲第二遍**：定稿那一刻 latest 预览与定稿预览是同一份画面，
         * 前端把 latest 预览排在后台队列里，这里等它落地后直接复制。
         * 高质量降噪（BM3D）在 3840 输入边上一遍就是十几秒 —— 省掉的就是那十几秒。
         */
        catalog.ensure_current().map_err(|e| e.to_string())?;
        full.write(
            issue.asset_id,
            &name,
            issue.source_base,
            render::PIPELINE_VERSION,
            &bytes,
        )
        .map_err(|error| error.to_string())?;
        bytes
    } else {
        let lens = crate::lens::render_correction(
            app,
            repo,
            issue.asset_id,
            issue.stack.lens_profile.as_deref(),
            issue.stack.lens_enabled,
        );
        let lut = issue
            .stack
            .lut_id
            .as_deref()
            .filter(|_| issue.stack.lut_enabled == Some(true))
            .map(|id| crate::lut::resolve(app, id))
            .transpose()?;
        let rendered = render::render_file_with_profiles(
            &source,
            SizeClass::Screen,
            Some(&issue.stack),
            lens.as_ref(),
            lut.as_deref(),
            raybend::thumbnail::render::EncodeFormat::Avif,
            |id, role| crate::color_profiles::resolve(app, id, role),
        )
        .map_err(|error| error.to_string())?
        .ok_or("无法渲染定稿预览")?;
        catalog.ensure_current().map_err(|e|e.to_string())?;
        full.write(
            issue.asset_id,
            &name,
            issue.source_base,
            render::PIPELINE_VERSION,
            &rendered.data,
        )
        .map_err(|error| error.to_string())?;
        rendered.data
    };
    let image = image::load_from_memory_with_format(&preview, image::ImageFormat::Avif)
        .map_err(|error| format!("定稿预览解码失败：{error}"))?;
    let image = render::clamp_display_aspect(image, render::MAX_DISPLAY_ASPECT);
    let db = app
        .state::<SourcesThumbs>()
        .get(&crate::thumbs::sources_cache_dir(app)?, time::now_millis())?;
    let key = cache_key(repo, issue.asset_id, issue.id);
    let signature = render_signature(issue, &source_signature);
    for size in [SizeClass::Grid] {
        let read_key = key.clone();
        let read_sig = signature.clone();
        if db
            .read(move |conn| cache::get(conn, &read_key, size, &read_sig))
            .map_err(|error| error.to_string())?
            .is_some()
        {
            continue;
        }
        let rgb = render::resize_for_thumb(image.clone(), size.long_edge()).to_rgb8();
        let (width, height) = rgb.dimensions();
        let bytes =
            render::encode_avif(rgb.as_raw(), width, height).map_err(|error| error.to_string())?;
        let write_key = key.clone();
        let write_sig = signature.clone();
        db.write(move |conn| {
            cache::put(
                conn,
                &write_key,
                size,
                &write_sig,
                &bytes,
                width,
                height,
                time::now_millis(),
            )
        })
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn issue_create<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    asset_id: i64,
    name: String,
    english: Option<bool>,
) -> Result<IssueLibraryDto, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        let browse = handle.state::<BrowseState>();
        let issue = browse.with_catalog(&handle, &repository_id, |db| {
            db.write_tx(move |conn| {
                let stack: DevelopStack = load_stack(conn, asset_id)?;
                issues::create(conn, asset_id, &name, &stack, time::now_millis())
            })
            .map_err(|error| error.to_string())
        })?;
        // sidecar 镜像：定稿清单变了（`specs/xmp-w1.md` §6.1）
        crate::sidecar::queue_sync(handle.clone(), &repository_id, vec![asset_id]);
        /*
         * 快照生成**离开响应路径**（崔总 2026-10-10）：条目先出现在列表里（占位先行），
         * 1920 预览 + 小图在后台补齐。以前这一整套渲染 + AVIF 编码是同步等的 ——
         * 高质量降噪下近 1 分钟，面板只能干等。
         * 走**后台队列的 P3（批量）**（W2 §4.3）：前台重活可以插队；
         * 同 key（同库同资产同定稿）只排一次。失败只记日志：真正去看这份定稿时
         * （对比 / 全屏）按需路径会再要一次、那时报错。
         */
        let snapshot_app = handle.clone();
        let snapshot_repo = repository_id.clone();
        let key = format!("issue-snapshots:{repository_id}:{asset_id}:{}", issue.id);
        let _ = handle
            .state::<crate::work::WorkQueue>()
            .spawn(crate::work::Priority::Batch, key, move |_token| {
                if let Err(error) = ensure_snapshots(&snapshot_app, &snapshot_repo, &issue, true) {
                    // i18n-exempt: 后台诊断（终端日志，不进界面）
                    eprintln!("[issues] 定稿 #{} 的快照生成失败：{error}", issue.id);
                }
            });
        // `snapshot_error` 不再有同步来源：失败走日志 + 按需路径（见上面的注释）。
        library(
            &handle,
            &repository_id,
            asset_id,
            english.unwrap_or(false),
            None,
        )
    })
    .await
}

#[tauri::command]
pub async fn issue_rename<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    asset_id: i64,
    issue_id: i64,
    name: String,
    english: Option<bool>,
) -> Result<IssueLibraryDto, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        let browse = handle.state::<BrowseState>();
        let renamed = browse.with_catalog(&handle, &repository_id, |db| {
            db.write_tx(move |conn| issues::rename(conn, asset_id, issue_id, &name))
                .map_err(|error| error.to_string())
        })?;
        if !renamed {
            return Err("定稿不存在".into());
        }
        // sidecar 镜像：`rb:profiles` 每条都带 name（`specs/xmp-sidecar.md` §4），
        // 改名一样要把这张照片重新同步出去。
        crate::sidecar::queue_sync(handle.clone(), &repository_id, vec![asset_id]);
        library(
            &handle,
            &repository_id,
            asset_id,
            english.unwrap_or(false),
            None,
        )
    })
    .await
}

#[tauri::command]
pub async fn issue_delete<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    asset_id: i64,
    issue_id: i64,
    english: Option<bool>,
) -> Result<IssueLibraryDto, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        let browse = handle.state::<BrowseState>();
        browse.with_catalog(&handle, &repository_id, |db| {
            db.write_tx(move |conn| issues::delete(conn, asset_id, issue_id))
                .map_err(|error| error.to_string())
        })?;
        // sidecar 镜像：定稿清单变了（`specs/xmp-w1.md` §6.1）
        crate::sidecar::queue_sync(handle.clone(), &repository_id, vec![asset_id]);
        if let Ok(root) = crate::browse::resolve_root(&handle, &repository_id)
            && let Ok(cache) = FullCache::open(&root)
        {
            cache.remove_issue(asset_id, issue_id);
        }
        if let Ok(db) = handle.state::<SourcesThumbs>().get(
            &crate::thumbs::sources_cache_dir(&handle)?,
            time::now_millis(),
        ) {
            let key = cache_key(&repository_id, asset_id, issue_id);
            let _ = db.write(move |conn| cache::remove_key(conn, &key));
        }
        library(
            &handle,
            &repository_id,
            asset_id,
            english.unwrap_or(false),
            None,
        )
    })
    .await
}

/// 返回命名定稿独立的 1920 AVIF；缺缓存时按该定稿的完整 profile 重建。
pub(crate) fn preview_bytes<R: Runtime>(
    app: &AppHandle<R>,
    repo: &str,
    asset_id: i64,
    issue_id: i64,
) -> Result<Vec<u8>, String> {
    let _permit=app.state::<BrowseState>().sessions.begin_task(repo).map_err(|e|e.to_string())?;
    let issue = issue_of(app, repo, asset_id, issue_id)?;
    ensure_snapshots(app, repo, &issue, false)?;
    let root = crate::browse::resolve_root(app, repo)?;
    FullCache::open(&root)
        .map_err(|error| error.to_string())?
        .read(
            asset_id,
            &preview_name(&issue, &source_signature(app, repo, &issue)?),
            issue.source_base,
            render::PIPELINE_VERSION,
        )
        .ok_or_else(|| "定稿预览生成后无法读取".into())
}

#[tauri::command]
pub async fn issue_thumb_get<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    asset_id: i64,
    issue_id: i64,
    size: String,
) -> Result<tauri::ipc::Response, String> {
    let handle = app.clone();
    let bytes = crate::source::blocking(move || {
        let size = SizeClass::parse(&size)
            .filter(|value| matches!(value, SizeClass::Grid))
            .ok_or("定稿缩略图只支持 grid")?;
        thumb_bytes(&handle, &repository_id, asset_id, issue_id, size)
    })
    .await?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// 编辑器和 export 复用既有定稿 thumb，不重复建立 export 缓存。
pub(crate) fn thumb_bytes<R: Runtime>(app: &AppHandle<R>, repo: &str,
    asset_id: i64, issue_id: i64, size: SizeClass) -> Result<Vec<u8>, String> {
        let issue = issue_of(app, repo, asset_id, issue_id)?;
        let db = app.state::<SourcesThumbs>().get(
            &crate::thumbs::sources_cache_dir(app)?,
            time::now_millis(),
        )?;
        let key = cache_key(repo, asset_id, issue_id);
        let sig = render_signature(&issue, &source_signature(app, repo, &issue)?);
        let read_key = key.clone();
        let read_sig = sig.clone();
        if let Some(bytes) = db
            .read(move |conn| cache::get(conn, &read_key, size, &read_sig))
            .map_err(|error| error.to_string())?
        {
            return Ok(bytes);
        }
        let asset = asset_for(app, repo, asset_id)?;
        let source = develop::source_path_of(app, &asset, issue.source_base).ok_or("定稿源文件不可用")?;
        let bytes = crate::thumbs::render_profile_thumb(app, &db, &asset, &source, &issue.stack,
            size, Some(&format!("issue-{}-{}", issue.id, issue.profile_hash)))
            .map_err(|error| error.to_string())?;
        let image = image::load_from_memory(&bytes).map_err(|error| error.to_string())?;
        let (width, height) = (image.width(), image.height());
        let write_key = key.clone(); let write_sig = sig.clone();
        db.write(move |conn| cache::put(conn, &write_key, size, &write_sig, &bytes, width, height, time::now_millis()))
            .map_err(|error| error.to_string())?;
        db.read(move |conn| cache::get(conn, &key, size, &sig))
            .map_err(|error| error.to_string())?
            .ok_or("定稿缩略图生成后无法读取".into())

}

/* ══════════════════════════════════════════════════════════════
 * 定稿图自愈（`specs/editor-session-pipeline.md` §7 / W5）
 * ══════════════════════════════════════════════════════════════ */

/// **定稿两档图的补齐顺序**（§7.2）：先 Screen 再 Grid —— 小图能从 1920 缩出来，
/// 先渲大图 = 每张照片只付一次整管线。
///
/// 192 那一档 2026-10-11 取消（`specs/thumbnail-tiers-and-import-memory.md`）。
const SNAPSHOT_ORDER: [SizeClass; 2] = [SizeClass::Screen, SizeClass::Grid];

/// 两档里**缺哪些**（纯逻辑，可单测）：只报缺的，顺序固定 Screen → Grid。
fn missing_snapshot_sizes(
    wanted: &[SizeClass],
    mut present: impl FnMut(SizeClass) -> bool,
) -> Vec<SizeClass> {
    SNAPSHOT_ORDER
        .iter()
        .copied()
        .filter(|size| wanted.contains(size) && !present(*size))
        .collect()
}

/// 自愈补过的条目（前端据此刷新对应缩略图；旧图留守，不闪）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueSnapshotsFilled {
    pub issue_id: i64,
    /// 这次真补了的档位（`screen` / `grid` / `strip`）。
    pub sizes: Vec<String>,
}

/// 扫一遍这张照片**所有定稿**的指定档位，缺什么补什么（静默、幂等）。
///
/// 单条失败只记日志、不影响其它定稿；全齐时只读缓存与文件状态（毫秒级返回）。
/// **复用而非重写**：判定后的生成、写缓存、命名全部走 `ensure_snapshots` / `thumb_bytes`，
/// 不新造缓存键（§2.12）。
fn ensure_issue_snapshots<R: Runtime>(
    app: &AppHandle<R>,
    repo: &str,
    asset_id: i64,
    sizes: &[SizeClass],
) -> Result<Vec<IssueSnapshotsFilled>, String> {
    let browse = app.state::<BrowseState>();
    let issues = browse.with_catalog(app, repo, |db| {
        db.read(|conn| issues::list(conn, asset_id))
            .map_err(|error| error.to_string())
    })?;
    if issues.is_empty() {
        return Ok(Vec::new());
    }
    let asset = asset_for(app, repo, asset_id)?;
    let full = FullCache::open(&asset.root).map_err(|error| error.to_string())?;
    let db = app.state::<SourcesThumbs>().get(
        &crate::thumbs::sources_cache_dir(app)?,
        time::now_millis(),
    )?;
    let mut filled = Vec::new();
    for issue in issues {
        match fill_issue_snapshots(app, repo, &issue, sizes, &full, &db) {
            Ok(Some(entry)) => filled.push(entry),
            Ok(None) => {}
            // 静默自愈：单条失败不报给界面，下一次进入 / 选中时再试（§7.2-3）
            Err(error) => eprintln!("[issues] 定稿 #{} 的图自愈跳过：{error}", issue.id),
        }
    }
    Ok(filled)
}

/// 一条定稿：判缺 → 按序补齐；全齐返回 `None`。
fn fill_issue_snapshots<R: Runtime>(
    app: &AppHandle<R>,
    repo: &str,
    issue: &Issue,
    sizes: &[SizeClass],
    full: &FullCache,
    db: &raybend::thumbnail::ThumbsDb,
) -> Result<Option<IssueSnapshotsFilled>, String> {
    // 源文件不可用（定稿已删源 / 盘不在线）→ 跳过该条，不报错
    let signature = source_signature(app, repo, issue)?;
    let key = cache_key(repo, issue.asset_id, issue.id);
    let sig = render_signature(issue, &signature);
    let preview = preview_name(issue, &signature);
    let mut present = |size: SizeClass| -> bool {
        match size {
            SizeClass::Screen => full
                .read(
                    issue.asset_id,
                    &preview,
                    issue.source_base,
                    render::PIPELINE_VERSION,
                )
                .is_some(),
            SizeClass::Grid => {
                let key = key.clone();
                let sig = sig.clone();
                db.read(move |conn| cache::get(conn, &key, size, &sig))
                    .ok()
                    .flatten()
                    .is_some()
            }
            // 真身档在同一个 `cache/full` 目录里，只是文件名带 `-full`、扩展名 `.jpg`
            //（2026-10-11）。它**不在** `SNAPSHOT_ORDER` 的自愈范围内（那两档是缩略图
            // 与占位），这里顺手把判定补齐，免得将来有人把它加进 `wanted` 时静默当不存在。
            SizeClass::Full => full
                .read_class(
                    issue.asset_id,
                    &preview,
                    issue.source_base,
                    CacheClass::Full,
                    render::PIPELINE_VERSION,
                )
                .is_some(),
        }
    };
    let missing = missing_snapshot_sizes(sizes, &mut present);
    if missing.is_empty() {
        return Ok(None);
    }
    for size in &missing {
        match size {
            SizeClass::Screen => ensure_snapshots(app, repo, issue, false)?,
            SizeClass::Grid => {
                thumb_bytes(app, repo, issue.asset_id, issue.id, *size)?;
            }
            // 进不来：`missing_snapshot_sizes` 只在 `SNAPSHOT_ORDER`（两档）里挑，
            // 真身档按需生成（`thumbs::render_full_cached`），不在这条自愈链上。
            SizeClass::Full => {}
        }
    }
    Ok(Some(IssueSnapshotsFilled {
        issue_id: issue.id,
        sizes: missing
            .iter()
            .map(|size| size.as_str().to_string())
            .collect(),
    }))
}

/// **定稿图自愈**（§7.3）：进 editor / 选中某个定稿时扫描该照片所有定稿的指定档位，
/// 缺什么补什么（静默、幂等、低优先）。
///
/// * `sizes` 缺省三档（`screen` / `grid` / `strip`）；
/// * 返回**这次真补了**的条目（前端据此刷新缩略图）；
/// * 走后台队列的 **P3 + 同 key 单飞**：同一张照片并发进入只跑一次。
#[tauri::command]
pub async fn issue_snapshots_ensure<R: Runtime>(
    app: AppHandle<R>,
    repository_id: String,
    asset_id: i64,
    sizes: Option<Vec<String>>,
) -> Result<Vec<IssueSnapshotsFilled>, String> {
    let handle = app.clone();
    crate::source::blocking(move || {
        let wanted = match sizes {
            Some(values) => {
                let mut out = Vec::new();
                for value in values {
                    let size = SizeClass::parse(&value)
                        .ok_or_else(|| format!("未知的定稿图档位：{value}"))?;
                    if !matches!(size, SizeClass::Screen | SizeClass::Grid) {
                        return Err(format!("定稿图只支持 screen/grid：{value}"));
                    }
                    out.push(size);
                }
                out
            }
            None => SNAPSHOT_ORDER.to_vec(),
        };
        let key = format!("issue-snapshots:{repository_id}:{asset_id}");
        let filled = Arc::new(Mutex::new(Vec::new()));
        let task_filled = Arc::clone(&filled);
        let task_app = handle.clone();
        let task_repo = repository_id.clone();
        handle
            .state::<crate::work::WorkQueue>()
            .run(crate::work::Priority::Batch, key, move |token| {
                if token.cancelled() {
                    return Ok(());
                }
                let entries = ensure_issue_snapshots(&task_app, &task_repo, asset_id, &wanted)?;
                *task_filled
                    .lock()
                    .map_err(|_| "自愈结果锁中毒".to_string())? = entries;
                Ok(())
            })?;
        let entries = filled
            .lock()
            .map_err(|_| "自愈结果锁中毒".to_string())?
            .clone();
        Ok(entries)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 缺项判定（§7.4）：两档全有 → 空；缺 screen → 只报 screen；缺小图 → 只报小图；
    /// 顺序固定 Screen → Grid。
    #[test]
    fn missing_snapshot_sizes_reports_only_the_missing_ones_in_order() {
        let all = [SizeClass::Screen, SizeClass::Grid];
        assert!(
            missing_snapshot_sizes(&all, |_| true).is_empty(),
            "全齐 → 空"
        );
        assert_eq!(
            missing_snapshot_sizes(&all, |size| size != SizeClass::Screen),
            vec![SizeClass::Screen]
        );
        assert_eq!(
            missing_snapshot_sizes(&all, |size| size == SizeClass::Screen),
            vec![SizeClass::Grid],
            "只缺小图时顺序仍是 Screen → Grid（这里 screen 已有）"
        );
        assert_eq!(
            missing_snapshot_sizes(&[SizeClass::Grid], |_| false),
            vec![SizeClass::Grid],
            "只要一档时只报这一档"
        );
        // 全部缺：顺序固定（先大图再小图）
        assert_eq!(
            missing_snapshot_sizes(&all, |_| false),
            vec![SizeClass::Screen, SizeClass::Grid]
        );
    }
}
