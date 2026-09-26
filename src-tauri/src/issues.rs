//! 不可变定稿 IPC 与独立图像快照。

use crate::browse::BrowseState;
use crate::develop::{self, DevelopStackDto, ResolvedAsset};
use crate::thumbs::SourcesThumbs;
use raybend::display::FullCache;
use raybend::store::develop::{DevelopStack, EditBase, load as load_stack};
use raybend::store::issues::{self, Issue, Selection};
use raybend::store::time;
use raybend::thumbnail::SizeClass;
use raybend::thumbnail::{cache, render};
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, Manager, Runtime};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueDto {
    pub id: i64,
    pub name: String,
    pub profile_hash: String,
    pub source_base: String,
    pub created_at: i64,
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

fn ensure_snapshots<R: Runtime>(
    app: &AppHandle<R>,
    repo: &str,
    issue: &Issue,
) -> Result<(), String> {
    let asset = asset_for(app, repo, issue.asset_id)?;
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
        let rendered = render::render_file_with_edit_and_lut(
            &source,
            SizeClass::Screen,
            Some(&issue.stack),
            lens.as_ref(),
            lut.as_deref(),
        )
        .map_err(|error| error.to_string())?
        .ok_or("无法渲染定稿预览")?;
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
    for size in [SizeClass::Grid, SizeClass::Strip] {
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
        let snapshot_error = ensure_snapshots(&handle, &repository_id, &issue).err();
        let mut result = library(
            &handle,
            &repository_id,
            asset_id,
            english.unwrap_or(false),
            None,
        )?;
        result.snapshot_error = snapshot_error;
        Ok(result)
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
    let issue = issue_of(app, repo, asset_id, issue_id)?;
    ensure_snapshots(app, repo, &issue)?;
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
            .filter(|value| matches!(value, SizeClass::Grid | SizeClass::Strip))
            .ok_or("定稿缩略图只支持 grid/strip")?;
        let issue = issue_of(&handle, &repository_id, asset_id, issue_id)?;
        let db = handle.state::<SourcesThumbs>().get(
            &crate::thumbs::sources_cache_dir(&handle)?,
            time::now_millis(),
        )?;
        let key = cache_key(&repository_id, asset_id, issue_id);
        let sig = render_signature(&issue, &source_signature(&handle, &repository_id, &issue)?);
        let read_key = key.clone();
        let read_sig = sig.clone();
        if let Some(bytes) = db
            .read(move |conn| cache::get(conn, &read_key, size, &read_sig))
            .map_err(|error| error.to_string())?
        {
            return Ok(bytes);
        }
        ensure_snapshots(&handle, &repository_id, &issue)?;
        db.read(move |conn| cache::get(conn, &key, size, &sig))
            .map_err(|error| error.to_string())?
            .ok_or("定稿缩略图生成后无法读取".into())
    })
    .await?;
    Ok(tauri::ipc::Response::new(bytes))
}
