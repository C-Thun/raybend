//! 自动更新：检查 → 下载（可续传）→ **下一次启动**才问安装。
//!
//! 判据（崔总 2026-10-15）：更新永不允许阻塞使用；用户只有两个决策点、各只问一次
//! （下载 / 跳过、安装 / 取消）；界面里不出现密钥与自定义源。规格 `specs/update-flow-v2.md`。
//!
//! 状态落盘在 `<app_local_data>/updates/`，见 [`state`]；下载与校验见 [`download`]。
mod download;
mod state;

use crate::import::ImportBatches;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_updater::{Update, UpdaterExt};

use download::Plan;
use state::{Phase, Stored};

/// 状态变化的广播名（前端据此刷新标题栏指示器与对话框）。
pub const STATE_EVENT: &str = "updates://state";
/// 检查清单的超时（小 JSON，不用等太久）。
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
/// 进度落盘的最小间隔；广播比落盘密一些（只动内存）。
const PERSIST_EVERY: Duration = Duration::from_secs(2);
const EMIT_EVERY: Duration = Duration::from_millis(200);
const DIR_NAME: &str = "updates";

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSource {
    mode: String,
    endpoint: String,
    public_key: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateView {
    version: String,
    notes: Option<String>,
}

/// 给前端的完整更新状态（唯一事实源在 Rust）。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    phase: Phase,
    version: Option<String>,
    done_bytes: u64,
    total_bytes: Option<u64>,
    /// 最近一次失败原因（只在「检查更新」里显示，不弹窗）
    error: Option<String>,
    skipped_version: Option<String>,
    install_prompted_version: Option<String>,
    current_version: String,
    /// 启动时该静默重试一次（上次失败、这个版本还没重试过）
    auto_retry: bool,
    /// 有下载任务在跑（内存态）
    downloading: bool,
}

struct Inner {
    busy: bool,
    downloading: bool,
    loaded: bool,
    dir: PathBuf,
    stored: Stored,
    last_persist: Instant,
    last_emit: Instant,
}

impl Default for Inner {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            busy: false,
            downloading: false,
            loaded: false,
            dir: PathBuf::new(),
            stored: Stored::default(),
            last_persist: now,
            last_emit: now,
        }
    }
}

#[derive(Default)]
pub struct UpdateState {
    inner: Arc<Mutex<Inner>>,
}

/// 互斥的操作闸门：检查 / 开始下载 / 安装 三者互斥（下载任务本身不占）。
struct Operation(Arc<Mutex<Inner>>);

impl Drop for Operation {
    fn drop(&mut self) {
        if let Ok(mut inner) = self.0.lock() {
            inner.busy = false;
        }
    }
}

impl UpdateState {
    fn lock(&self) -> Result<MutexGuard<'_, Inner>, String> {
        self.inner.lock().map_err(|_| "UPDATE_LOCK".to_string())
    }

    /// 首次使用时把状态目录与状态文件准备好（含「装完了」的收摊）。
    fn touch<R: Runtime>(&self, app: &AppHandle<R>) -> Result<(), String> {
        let mut inner = self.lock()?;
        if inner.loaded {
            return Ok(());
        }
        let dir = app
            .path()
            .app_local_data_dir()
            .map_err(|_| "UPDATE_DIR".to_string())?
            .join(DIR_NAME);
        std::fs::create_dir_all(&dir).map_err(|_| "UPDATE_DIR".to_string())?;
        inner.stored = Stored::load(&dir);
        // 装着的那一版已经跑到当前版本上了 ⇒ 状态归零（成品文件随后清掉）
        if let (Phase::Ready, Some(version)) = (inner.stored.phase, inner.stored.version.clone())
            && version == app.package_info().version.to_string() {
                inner.stored.reset();
            }
        let keep = match inner.stored.phase {
            Phase::Idle => None,
            _ => inner.stored.version.clone(),
        };
        Stored::cleanup(&dir, keep.as_deref());
        inner.stored.save(&dir);
        inner.dir = dir;
        inner.loaded = true;
        Ok(())
    }

    fn operation(&self) -> Result<Operation, String> {
        let mut inner = self.lock()?;
        if inner.busy {
            return Err("UPDATE_BUSY".into());
        }
        inner.busy = true;
        Ok(Operation(self.inner.clone()))
    }

    fn status_with(&self, current_version: &str) -> Result<UpdateStatus, String> {
        let inner = self.lock()?;
        Ok(UpdateStatus {
            phase: inner.stored.phase,
            version: inner.stored.version.clone(),
            done_bytes: inner.stored.done_bytes,
            total_bytes: inner.stored.total_bytes,
            error: inner.stored.last_error.clone(),
            skipped_version: inner.stored.skipped_version.clone(),
            install_prompted_version: inner.stored.install_prompted_version.clone(),
            current_version: current_version.to_string(),
            auto_retry: inner.stored.should_auto_retry(),
            downloading: inner.downloading,
        })
    }

    fn status<R: Runtime>(&self, app: &AppHandle<R>) -> Result<UpdateStatus, String> {
        self.status_with(&app.package_info().version.to_string())
    }

    fn auto_suppressed(&self, version: &str) -> Result<bool, String> {
        Ok(self.lock()?.stored.auto_suppressed(version))
    }

    fn is_downloading(&self) -> Result<bool, String> {
        Ok(self.lock()?.downloading)
    }

    fn set_downloading(&self, value: bool) -> Result<(), String> {
        self.lock()?.downloading = value;
        Ok(())
    }

    /// 清旧文件、记下要下的版本与已有进度，返回可续传的字节数。
    fn prepare(&self, version: &str, automatic: bool) -> Result<u64, String> {
        Stored::safe_version(version)?;
        let mut inner = self.lock()?;
        Stored::cleanup(&inner.dir, Some(version));
        let part = Stored::part_path(&inner.dir, version);
        let existing = std::fs::metadata(&part).map(|meta| meta.len()).unwrap_or(0);
        inner
            .stored
            .start_download(version, existing, None, automatic);
        inner.stored.save(&inner.dir);
        Ok(existing)
    }

    fn part_path(&self, version: &str) -> Result<PathBuf, String> {
        Stored::safe_version(version)?;
        Ok(Stored::part_path(&self.lock()?.dir, version))
    }

    fn bin_path(&self, version: &str) -> Result<PathBuf, String> {
        Stored::safe_version(version)?;
        Ok(Stored::bin_path(&self.lock()?.dir, version))
    }

    fn ready_version(&self) -> Result<String, String> {
        let inner = self.lock()?;
        match (inner.stored.phase, inner.stored.version.clone()) {
            (Phase::Ready, Some(version)) => Ok(version),
            _ => Err("UPDATE_NO_DOWNLOAD".into()),
        }
    }

    /// 进度推进 + 节流落盘；返回是否该广播（广播比落盘密）。
    fn advance(&self, done: u64, total: Option<u64>) -> bool {
        let Ok(mut inner) = self.lock() else {
            return false;
        };
        inner.stored.advance(done, total);
        let now = Instant::now();
        let finished = inner.stored.total_bytes == Some(done);
        if !finished && now.duration_since(inner.last_emit) < EMIT_EVERY {
            return false;
        }
        inner.last_emit = now;
        if finished || now.duration_since(inner.last_persist) >= PERSIST_EVERY {
            inner.last_persist = now;
            inner.stored.save(&inner.dir);
        }
        true
    }

    fn complete(&self) -> Result<(), String> {
        let mut inner = self.lock()?;
        inner.stored.complete();
        inner.downloading = false;
        inner.stored.save(&inner.dir);
        Ok(())
    }

    fn fail(&self, error: String) -> Result<(), String> {
        let mut inner = self.lock()?;
        inner.stored.fail(error);
        inner.downloading = false;
        inner.stored.save(&inner.dir);
        Ok(())
    }

    /// 安装已交接：状态归零（成品文件留给下次启动收摊）。
    fn installed(&self) -> Result<(), String> {
        let mut inner = self.lock()?;
        inner.stored.reset();
        inner.stored.save(&inner.dir);
        Ok(())
    }

    fn discard_ready(&self) -> Result<(), String> {
        let mut inner = self.lock()?;
        if inner.stored.phase == Phase::Ready {
            inner.stored.reset();
            inner.stored.save(&inner.dir);
        }
        Ok(())
    }

    /// 线上版本没有了/不再是更新的版本：在办的进度与文件一起作废。
    /// 不这么做，界面会永远停在「下载中」而实际什么都没发生。
    fn discard_in_flight(&self) -> Result<(), String> {
        let mut inner = self.lock()?;
        if inner.stored.phase == Phase::Idle {
            return Ok(());
        }
        inner.stored.reset();
        inner.stored.save(&inner.dir);
        Stored::cleanup(&inner.dir, None);
        Ok(())
    }

    fn skip(&self, version: &str) -> Result<(), String> {
        Stored::safe_version(version)?;
        let mut inner = self.lock()?;
        inner.stored.skip(version);
        inner.stored.save(&inner.dir);
        // 跳过的正好是在办的那个版本：半截文件一起收掉（不留给下次启动）
        if inner.stored.phase == Phase::Idle {
            Stored::cleanup(&inner.dir, None);
        }
        Ok(())
    }

    fn mark_install_prompted(&self) -> Result<(), String> {
        let mut inner = self.lock()?;
        inner.stored.mark_install_prompted();
        inner.stored.save(&inner.dir);
        Ok(())
    }
}

fn https_url(value: &str) -> Result<tauri::Url, String> {
    let url = tauri::Url::parse(value).map_err(|_| "UPDATE_SOURCE_INVALID")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("UPDATE_SOURCE_INVALID".into());
    }
    Ok(url)
}

fn validate(source: &UpdateSource) -> Result<tauri::Url, String> {
    if !matches!(source.mode.as_str(), "stable" | "beta" | "custom")
        || source.public_key.trim().is_empty()
        || source.public_key.len() > 8192
        || source.endpoint.len() > 4096
    {
        return Err("UPDATE_SOURCE_INVALID".into());
    }
    https_url(&source.endpoint)
}

fn allowed() -> Result<(), String> {
    if option_env!("RAYBEND_DISTRIBUTION") == Some("store") {
        Err("UPDATE_STORE_MANAGED".into())
    } else {
        Ok(())
    }
}

fn notes_of(update: &Update) -> Option<String> {
    update
        .body
        .as_ref()
        .map(|body| body.chars().take(4000).collect())
}

fn plan_of(update: &Update, source: &UpdateSource) -> Plan {
    Plan {
        version: update.version.clone(),
        url: update.download_url.clone(),
        signature: update.signature.clone(),
        public_key: source.public_key.clone(),
    }
}

/// 查清单；顺带把「只接受 https 下载地址」「稳定源不接预发布」两条守住。
async fn check_update<R: Runtime>(
    app: &AppHandle<R>,
    source: &UpdateSource,
    url: tauri::Url,
) -> Result<Option<Update>, String> {
    let found = app
        .updater_builder()
        .pubkey(source.public_key.clone())
        .endpoints(vec![url])
        .map_err(|error| error.to_string())?
        .timeout(CHECK_TIMEOUT)
        .build()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?;
    let Some(update) = found else {
        return Ok(None);
    };
    https_url(update.download_url.as_str())?;
    if source.mode == "stable" && update.version.contains('-') {
        return Err("UPDATE_STABLE_PRERELEASE".into());
    }
    Ok(Some(update))
}

fn emit_state<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<UpdateState>();
    if let Ok(status) = state.status(app) {
        let _ = app.emit(STATE_EVENT, status);
    }
}

/// 后台下载任务：跑完（成功或失败）都落到状态里，界面只通过事件看结果。
async fn run_download<R: Runtime>(app: AppHandle<R>, plan: Plan, existing: u64) {
    let state = app.state::<UpdateState>();
    let outcome = async {
        let part = state.part_path(&plan.version)?;
        download::fetch(&plan, &part, existing, |done, total| {
            if state.advance(done, total) {
                emit_state(&app);
            }
        })
        .await?;
        download::verify_file(&part, &plan)?;
        let bin = state.bin_path(&plan.version)?;
        std::fs::rename(&part, &bin).map_err(|error| format!("UPDATE_DOWNLOAD_WRITE:{error}"))
    }
    .await;
    match outcome {
        Ok(()) => {
            let _ = state.complete();
        }
        Err(error) => {
            let _ = state.fail(error);
        }
    }
    let _ = state.set_downloading(false);
    emit_state(&app);
}

#[tauri::command]
pub fn updates_status<R: Runtime>(app: AppHandle<R>) -> Result<UpdateStatus, String> {
    let state = app.state::<UpdateState>();
    state.touch(&app)?;
    state.status(&app)
}

/// 查更新。`manual = false`（启动时自动查）会压住「已跳过 / 已在办」的版本，
/// 手动查永远如实返回 —— 用户主动来问，就必须告诉他真相。
#[tauri::command]
pub async fn updates_check<R: Runtime>(
    app: AppHandle<R>,
    source: UpdateSource,
    manual: bool,
) -> Result<Option<UpdateView>, String> {
    allowed()?;
    let state = app.state::<UpdateState>();
    state.touch(&app)?;
    let _operation = state.operation()?;
    let url = validate(&source)?;
    let Some(update) = check_update(&app, &source, url).await? else {
        return Ok(None);
    };
    let version = update.version.clone();
    let view = UpdateView {
        version: version.clone(),
        notes: notes_of(&update),
    };
    if !manual && state.auto_suppressed(&version)? {
        return Ok(None);
    }
    Ok(Some(view))
}

/// 开始或继续下载（后台跑，立刻返回）。`automatic` 只用于「启动时静默重试一次」记账。
#[tauri::command]
pub async fn updates_begin<R: Runtime>(
    app: AppHandle<R>,
    source: UpdateSource,
    automatic: bool,
) -> Result<UpdateView, String> {
    allowed()?;
    let state = app.state::<UpdateState>();
    state.touch(&app)?;
    if state.is_downloading()? {
        return Err("UPDATE_BUSY".into());
    }
    let _operation = state.operation()?;
    let url = validate(&source)?;
    let Some(update) = check_update(&app, &source, url).await? else {
        // 线上已经没有比当前更新的版本：把在办的作废，否则界面会永远停在「下载中」
        state.discard_in_flight()?;
        emit_state(&app);
        return Err("UPDATE_NOT_AVAILABLE".into());
    };
    let plan = plan_of(&update, &source);
    let view = UpdateView {
        version: plan.version.clone(),
        notes: notes_of(&update),
    };
    let existing = state.prepare(&plan.version, automatic)?;
    state.set_downloading(true)?;
    let handle = app.clone();
    tauri::async_runtime::spawn(async move { run_download(handle, plan, existing).await });
    emit_state(&app);
    Ok(view)
}

/// 记「这个版本跳过了」：只压自动提示；同一版本若在下载中，进度一并作废。
#[tauri::command]
pub fn updates_skip<R: Runtime>(app: AppHandle<R>, version: String) -> Result<(), String> {
    let state = app.state::<UpdateState>();
    state.touch(&app)?;
    let _operation = state.operation()?;
    state.skip(&version)?;
    emit_state(&app);
    Ok(())
}

/// 记「已经问过安装」：问过一次就不再问（取消也不追问）。
#[tauri::command]
pub fn updates_mark_install_prompted<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let state = app.state::<UpdateState>();
    state.touch(&app)?;
    state.mark_install_prompted()?;
    emit_state(&app);
    Ok(())
}

/// 安装已下载好的成品（读盘 → 再验一次 → 交给安装器，进程随后退出）。
#[tauri::command]
pub async fn updates_install<R: Runtime>(
    app: AppHandle<R>,
    source: UpdateSource,
) -> Result<(), String> {
    allowed()?;
    let state = app.state::<UpdateState>();
    state.touch(&app)?;
    let _operation = state.operation()?;
    if app.state::<ImportBatches>().has_unfinished()?
        || app.state::<crate::export::ExportState>().has_unfinished()?
        || app
            .state::<crate::external_editor::ExternalState>()
            .has_unfinished()?
        || app.state::<crate::migration::MigrationState>().is_running()?
    {
        return Err("UPDATE_TASKS_ACTIVE".into());
    }
    let version = state.ready_version()?;
    let url = validate(&source)?;
    let update = check_update(&app, &source, url)
        .await?
        .ok_or_else(|| "UPDATE_VERSION_CHANGED".to_string())?;
    if update.version != version {
        state.discard_ready()?;
        return Err("UPDATE_VERSION_CHANGED".into());
    }
    let plan = plan_of(&update, &source);
    let bin = state.bin_path(&version)?;
    // 落盘后再验一次：本地文件也可能被换过
    download::verify_file(&bin, &plan)?;
    let bytes = std::fs::read(&bin).map_err(|error| format!("UPDATE_DOWNLOAD_READ:{error}"))?;
    match update.install(&bytes) {
        Ok(()) => {
            state.installed()?;
            Ok(())
        }
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("raybend-updates-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建测试目录");
        dir
    }

    /// 不走 AppHandle 的测试入口：直接把状态目录装上（含读盘，跟 `touch()` 一致）。
    fn state_at(dir: PathBuf) -> UpdateState {
        let state = UpdateState::default();
        {
            let mut inner = state.lock().expect("锁");
            inner.stored = Stored::load(&dir);
            inner.dir = dir;
            inner.loaded = true;
        }
        state
    }

    #[test]
    fn source_boundaries() {
        let mut source = UpdateSource {
            mode: "stable".into(),
            endpoint: "https://example.org/update.json".into(),
            public_key: "key".into(),
        };
        assert!(validate(&source).is_ok());
        source.mode = "custom".into();
        assert!(validate(&source).is_ok(), "源码层的自定义源能力保留");
        for endpoint in [
            "http://example.org",
            "file:///x",
            "https://user:pass@example.org/u",
            "https://example.org/u#secret",
            "",
        ] {
            source.endpoint = endpoint.into();
            assert!(validate(&source).is_err(), "{endpoint} 不该通过");
        }
        source.endpoint = "https://example.org".into();
        source.public_key = String::new();
        assert!(validate(&source).is_err());
        source.public_key = "key".into();
        source.mode = "off".into();
        assert!(validate(&source).is_err(), "已删掉的关闭态不再认");
    }

    #[test]
    fn concurrent_operation_rejected_and_released() {
        let state = UpdateState::default();
        let operation = state.operation().expect("第一次能进");
        assert!(state.operation().is_err(), "同时只允许一个操作");
        drop(operation);
        assert!(state.operation().is_ok(), "闸门要能自动松开");
    }

    #[test]
    fn status_reflects_persisted_state() {
        let dir = temp_dir("status");
        let state = state_at(dir.clone());
        state.prepare("0.2.0", false).expect("开始下载");
        let status = state.status_with("0.1.2").expect("状态");
        assert_eq!(status.phase, Phase::Downloading);
        assert_eq!(status.version.as_deref(), Some("0.2.0"));
        assert_eq!(status.current_version, "0.1.2");
        assert!(!status.auto_retry);
        // 落盘了：换一个状态对象读同一目录，能接着续传
        let reopened = state_at(dir.clone());
        assert_eq!(
            reopened.status_with("0.1.2").expect("状态").phase,
            Phase::Downloading
        );
        assert_eq!(
            Stored::part_path(&dir, "0.2.0")
                .extension()
                .and_then(|value| value.to_str()),
            Some("part")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resume_keeps_existing_bytes_and_failure_pauses() {
        let dir = temp_dir("resume");
        let state = state_at(dir.clone());
        std::fs::write(Stored::part_path(&dir, "0.2.0"), vec![0u8; 4096]).expect("造半截文件");
        let existing = state.prepare("0.2.0", false).expect("开始下载");
        assert_eq!(existing, 4096, "断点续传要从已有字节接着下");
        state.fail("UPDATE_DOWNLOAD_NETWORK:boom".into()).expect("记失败");
        let status = state.status_with("0.1.2").expect("状态");
        assert_eq!(status.phase, Phase::Paused);
        assert!(status.auto_retry, "失败后下次启动该静默重试一次");
        assert!(status.error.is_some(), "失败原因要留给人看");
        // 重试一次之后再失败 ⇒ 不再自动重试
        state.prepare("0.2.0", true).expect("自动重试");
        state.fail("boom".into()).expect("再失败");
        assert!(!state.status_with("0.1.2").expect("状态").auto_retry);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn completed_download_becomes_ready_and_survives_restart() {
        let dir = temp_dir("ready");
        let state = state_at(dir.clone());
        state.prepare("0.2.0", false).expect("开始下载");
        state.complete().expect("下载完成");
        let status = state.status_with("0.1.2").expect("状态");
        assert_eq!(status.phase, Phase::Ready);
        assert!(!status.downloading);
        // 重启：状态还在，进入「下一次启动问安装」
        let reopened = state_at(dir.clone());
        assert_eq!(reopened.ready_version().expect("待安装"), "0.2.0");
        reopened.mark_install_prompted().expect("记已问过");
        assert_eq!(
            reopened
                .status_with("0.1.2")
                .expect("状态")
                .install_prompted_version
                .as_deref(),
            Some("0.2.0")
        );
        reopened.installed().expect("安装交接");
        assert_eq!(
            reopened.status_with("0.1.2").expect("状态").phase,
            Phase::Idle
        );
        assert!(
            reopened.ready_version().is_err(),
            "装完就不再是待安装状态"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn skip_suppresses_auto_but_not_manual_and_clears_in_flight() {
        let dir = temp_dir("skip");
        let state = state_at(dir.clone());
        state.prepare("0.2.0", false).expect("开始下载");
        state.skip("0.2.0").expect("跳过");
        assert!(state.auto_suppressed("0.2.0").expect("判定"));
        // 跳过会顺手作废同一版本的半截文件
        assert_eq!(
            state.status_with("0.1.2").expect("状态").phase,
            Phase::Idle
        );
        assert!(
            !Stored::part_path(&dir, "0.2.0").exists(),
            "跳过后半截文件不该留着"
        );
        // 手动检查不看这个记账（由命令层 manual 分支保证），更高版本照常提示
        assert!(!state.auto_suppressed("0.3.0").expect("判定"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
