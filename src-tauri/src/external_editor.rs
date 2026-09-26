//! Thin async IPC/event glue. TIFF/business/platform rules live in raybend.
use raybend::{
    export::VariantSnapshot,
    external_editor::{Application, Native, Platform},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::{AppHandle, Emitter, Manager, Runtime};
#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskView {
    pub id: u64,
    pub revision: u64,
    pub status: String,
    pub output: Option<String>,
    pub error: Option<String>,
    pub missing_application: Option<String>,
}
impl TaskView {
    fn running(&self) -> bool {
        matches!(
            self.status.as_str(),
            "preparing" | "rendering" | "writing" | "opening"
        )
    }
}
#[derive(Clone, Default)]
pub struct ExternalState {
    inner: Arc<Mutex<(TaskView, Arc<AtomicBool>)>>,
}
impl ExternalState {
    pub(crate) fn has_unfinished(&self) -> Result<bool, String> {
        Ok(self.inner.lock().map_err(|_| "内部锁已损坏")?.0.running())
    }
    fn view(&self) -> TaskView {
        self.inner.lock().unwrap().0.clone()
    }
    fn reserve(&self) -> Result<(u64, Arc<AtomicBool>), String> {
        let mut state = self.inner.lock().map_err(|_| "内部锁已损坏")?;
        if state.0.running() {
            return Err("已有外部编辑导出正在执行".into());
        }
        let id = state.0.id.checked_add(1).ok_or("任务编号超过上限")?;
        let cancel = Arc::new(AtomicBool::new(false));
        *state = (
            TaskView {
                id,
                status: "preparing".into(),
                ..Default::default()
            },
            cancel.clone(),
        );
        Ok((id, cancel))
    }
    fn cancel(&self) -> TaskView {
        let state = self.inner.lock().unwrap();
        state.1.store(true, Ordering::Release);
        state.0.clone()
    }
    fn change(&self, id: u64, update: impl FnOnce(&mut TaskView)) -> Option<TaskView> {
        let mut state = self.inner.lock().unwrap();
        if state.0.id != id {
            return None;
        }
        update(&mut state.0);
        state.0.revision = state.0.revision.saturating_add(1);
        Some(state.0.clone())
    }
    fn update<R: Runtime>(&self, app: &AppHandle<R>, id: u64, update: impl FnOnce(&mut TaskView)) {
        if let Some(view) = self.change(id, update) {
            let _ = app.emit("external-editor://state", view);
        }
    }
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    repository_id: String,
    captured: VariantSnapshot,
    directory: String,
    application: Application,
}
#[tauri::command]
pub async fn external_applications(
    action: String,
    applications: Option<Vec<Application>>,
) -> Result<Vec<Application>, String> {
    crate::source::blocking(move || match action.as_str() {
        "discover" => Ok(raybend::external_editor::discover(&Native)),
        "check" => {
            let apps = applications.unwrap_or_default();
            if apps.len() > 128 {
                return Err("外部应用数量超过上限".into());
            }
            Ok(raybend::external_editor::checked(&Native, &apps))
        }
        _ => Err("无效外部应用操作".into()),
    })
    .await
}
#[tauri::command]
pub async fn external_task<R: Runtime>(
    app: AppHandle<R>,
    action: String,
    request: Option<Request>,
) -> Result<TaskView, String> {
    let state = app.state::<ExternalState>().inner().clone();
    match action.as_str() {
        "status" => Ok(state.view()),
        "cancel" => Ok(state.cancel()),
        "start" => {
            let request = request.ok_or("缺少外部编辑参数")?;
            let (id, cancel) = state.reserve()?;
            let events = app.clone();
            let worker_state = state.clone();
            if let Err(error) = std::thread::Builder::new()
                .name("external-editor-export".into())
                .spawn(move || {
                    let mut output = None;
                    let mut missing = None;
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                        || -> Result<(), String> {
                            if !Native.available(std::path::Path::new(&request.application.path)) {
                                missing = Some(request.application.path.clone());
                                return Err("外部应用已卸载或位置改变".into());
                            }
                            let prepared = crate::export::prepare(
                                &events,
                                &request.repository_id,
                                &request.captured,
                            )?
                            .ok_or("定稿已变更或删除")?;
                            let metadata = crate::export::metadata_for(&events, &prepared)?;
                            let (lens, lut) = crate::export::resources(
                                &events,
                                &request.repository_id,
                                &request.captured,
                            )?;
                            let target = raybend::external_editor::write_tiff(
                                &prepared.source,
                                &request.captured,
                                std::path::Path::new(&request.directory),
                                &metadata,
                                lens.as_ref(),
                                lut.as_deref(),
                                &cancel,
                                |phase| {
                                    worker_state
                                        .update(&events, id, |view| view.status = phase.into())
                                },
                            )
                            .map_err(|error| error.to_string())?;
                            output = Some(target.to_string_lossy().into_owned());
                            if cancel.load(Ordering::Acquire) {
                                return Err("EXTERNAL_CANCELLED".into());
                            }
                            worker_state.update(&events, id, |view| view.status = "opening".into());
                            if let Err(error) = Native
                                .launch(std::path::Path::new(&request.application.path), &target)
                            {
                                if !Native
                                    .available(std::path::Path::new(&request.application.path))
                                {
                                    missing = Some(request.application.path.clone());
                                }
                                return Err(error.to_string());
                            }
                            Ok(())
                        },
                    ))
                    .unwrap_or_else(|_| Err("外部导出后台任务异常终止".into()));
                    worker_state.update(&events, id, |view| {
                        view.status = match &result {
                            Ok(()) => "done",
                            Err(error) if error.contains("EXTERNAL_CANCELLED") => "cancelled",
                            Err(_) => "failed",
                        }
                        .into();
                        view.output = output;
                        view.missing_application = missing;
                        view.error = result
                            .err()
                            .filter(|error| !error.contains("EXTERNAL_CANCELLED"));
                    });
                })
            {
                state.update(&app, id, |view| {
                    view.status = "failed".into();
                    view.error = Some(format!("后台任务无法启动：{error}"));
                });
            }
            Ok(state.view())
        }
        _ => Err("无效外部编辑操作".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_starts_reserve_one_slot_and_cancel_waits_for_worker() {
        let state = ExternalState::default();
        let admitted = std::thread::scope(|scope| {
            let attempts: Vec<_> = (0..16).map(|_| scope.spawn(|| state.reserve())).collect();
            attempts
                .into_iter()
                .filter_map(|attempt| attempt.join().unwrap().ok())
                .collect::<Vec<_>>()
        });
        assert_eq!(admitted.len(), 1);
        let (id, cancel) = &admitted[0];
        assert!(state.has_unfinished().unwrap());
        assert!(state.cancel().running());
        assert!(cancel.load(Ordering::Acquire));
        assert!(state.reserve().is_err());
        state
            .change(*id, |view| view.status = "cancelled".into())
            .unwrap();
        assert!(!state.has_unfinished().unwrap());
        let (next, flag) = state.reserve().unwrap();
        assert_eq!(next, id + 1);
        assert!(!flag.load(Ordering::Acquire));
    }
    #[test]
    fn revisions_isolate_old_worker_and_overflow_leaves_completed_job() {
        let state = ExternalState::default();
        let (first, _) = state.reserve().unwrap();
        assert_eq!(
            state
                .change(first, |view| view.status = "writing".into())
                .unwrap()
                .revision,
            1
        );
        assert_eq!(
            state
                .change(first, |view| {
                    view.status = "done".into();
                    view.output = Some("完成.tiff".into());
                })
                .unwrap()
                .revision,
            2
        );
        let (second, _) = state.reserve().unwrap();
        assert!(
            state
                .change(first, |view| view.status = "failed".into())
                .is_none()
        );
        assert_eq!(state.view().id, second);
        assert!(state.view().output.is_none());
        state.change(second, |view| {
            view.id = u64::MAX;
            view.status = "done".into();
        });
        assert!(state.reserve().is_err());
        assert_eq!(state.view().status, "done");
    }
}
