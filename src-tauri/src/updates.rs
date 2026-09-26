//! 官方 updater 的薄接线；检查与安装分离，源和公钥由同一配置绑定。
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSource {
    mode: String,
    endpoint: String,
    public_key: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateView {
    version: String,
    notes: Option<String>,
}
struct Pending {
    source: UpdateSource,
    update: Update,
    bytes: Option<Vec<u8>>,
}
#[derive(Default)]
struct Inner {
    busy: bool,
    pending: Option<Pending>,
}
#[derive(Default)]
pub struct UpdateState(Arc<Mutex<Inner>>);
struct Operation(Arc<Mutex<Inner>>);
impl Drop for Operation {
    fn drop(&mut self) {
        if let Ok(mut inner) = self.0.lock() {
            inner.busy = false;
        }
    }
}
impl UpdateState {
    fn operation(&self) -> Result<Operation, String> {
        let mut inner = self.0.lock().map_err(|_| "UPDATE_LOCK")?;
        if inner.busy {
            return Err("UPDATE_BUSY".into());
        }
        inner.busy = true;
        Ok(Operation(self.0.clone()))
    }
    fn clear(&self) -> Result<(), String> {
        self.0.lock().map_err(|_| "UPDATE_LOCK")?.pending = None;
        Ok(())
    }
    fn take(&self, source: &UpdateSource) -> Result<Pending, String> {
        let pending = self
            .0
            .lock()
            .map_err(|_| "UPDATE_LOCK")?
            .pending
            .take()
            .ok_or("UPDATE_NOT_CHECKED")?;
        if pending.source != *source {
            return Err("UPDATE_SOURCE_CHANGED".into());
        }
        Ok(pending)
    }
    fn put(&self, pending: Pending) -> Result<(), String> {
        self.0.lock().map_err(|_| "UPDATE_LOCK")?.pending = Some(pending);
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
#[tauri::command]
pub async fn updates_check<R: Runtime>(
    app: AppHandle<R>,
    source: UpdateSource,
) -> Result<Option<UpdateView>, String> {
    allowed()?;
    let state = app.state::<UpdateState>();
    let _operation = state.operation()?;
    state.clear()?;
    let url = validate(&source)?;
    let mut result = app
        .updater_builder()
        .pubkey(source.public_key.clone())
        .endpoints(vec![url])
        .map_err(|e| e.to_string())?
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    if let Some(ref mut update) = result {
        https_url(update.download_url.as_str())?;
        update.timeout = Some(std::time::Duration::from_secs(300));
        if source.mode == "stable" && update.version.contains('-') {
            return Err("UPDATE_STABLE_PRERELEASE".into());
        }
        let view = UpdateView {
            version: update.version.clone(),
            notes: update.body.as_ref().map(|s| s.chars().take(4000).collect()),
        };
        state.put(Pending {
            source,
            update: result.take().unwrap(),
            bytes: None,
        })?;
        Ok(Some(view))
    } else {
        Ok(None)
    }
}
#[tauri::command]
pub async fn updates_download<R: Runtime>(
    app: AppHandle<R>,
    source: UpdateSource,
) -> Result<(), String> {
    allowed()?;
    let state = app.state::<UpdateState>();
    let _operation = state.operation()?;
    let mut pending = state.take(&source)?;
    // 官方插件返回前完成 minisign 校验。失败不会留下可安装的 bytes。
    let bytes = pending
        .update
        .download(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    pending.bytes = Some(bytes);
    state.put(pending)
}
#[tauri::command]
pub fn updates_install<R: Runtime>(app: AppHandle<R>, source: UpdateSource) -> Result<(), String> {
    allowed()?;
    let state = app.state::<UpdateState>();
    let _operation = state.operation()?;
    if app
        .state::<crate::import::ImportBatches>()
        .has_unfinished()?
        || app.state::<crate::export::ExportState>().has_unfinished()?
        || app.state::<crate::external_editor::ExternalState>().has_unfinished()?
    {
        return Err("UPDATE_TASKS_ACTIVE".into());
    }
    let mut pending = state.take(&source)?;
    let bytes = pending.bytes.take().ok_or("UPDATE_NO_DOWNLOAD")?;
    match pending.update.install(&bytes) {
        Ok(()) => Ok(()),
        Err(error) => {
            pending.bytes = Some(bytes);
            state.put(pending)?;
            Err(error.to_string())
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_boundaries() {
        let mut s = UpdateSource {
            mode: "custom".into(),
            endpoint: "https://example.org/update.json".into(),
            public_key: "key".into(),
        };
        assert!(validate(&s).is_ok());
        for endpoint in [
            "http://example.org",
            "file:///x",
            "https://user:pass@example.org/u",
            "https://example.org/u#secret",
            "",
        ] {
            s.endpoint = endpoint.into();
            assert!(validate(&s).is_err());
        }
        s.endpoint = "https://example.org".into();
        s.public_key = "".into();
        assert!(validate(&s).is_err());
        s.public_key = "key".into();
        s.mode = "off".into();
        assert!(validate(&s).is_err());
    }
    #[test]
    fn concurrent_operation_rejected_and_released() {
        let s = UpdateState::default();
        let op = s.operation().unwrap();
        assert!(s.operation().is_err());
        drop(op);
        assert!(s.operation().is_ok());
    }
}
