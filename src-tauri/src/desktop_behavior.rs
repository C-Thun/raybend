//! 一处接管所有 Tauri WebView 的桌面行为，含动态创建的全屏窗口。

use serde::Deserialize;
use std::sync::{Arc, OnceLock};
use tauri::{Runtime, Webview, plugin::TauriPlugin};

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Config {
    browser_context_menus: bool,
    browser_accelerator_keys: bool,
}

pub(crate) fn init<R: Runtime>() -> TauriPlugin<R, Config> {
    let config = Arc::new(OnceLock::<Config>::new());
    let setup_config = Arc::clone(&config);
    let ready_config = Arc::clone(&config);
    tauri::plugin::Builder::<R, Config>::new("desktop-behavior")
        .setup(move |_, api| {
            let _ = setup_config.set(*api.config());
            Ok(())
        })
        .on_webview_ready(move |webview| {
            configure_native(&webview, ready_config.get().copied().unwrap_or_default());
        })
        .on_page_load(move |webview, _| {
            if !config
                .get()
                .copied()
                .unwrap_or_default()
                .browser_context_menus
            {
                // DOM 只取消默认菜单；事件仍送到应用自己的右键处理器。
                if let Err(error) = webview.eval(include_str!("desktop_context.js")) {
                    eprintln!(
                        "[raybend] {} 禁用默认右键菜单失败：{error}",
                        webview.label()
                    );
                }
            }
        })
        .build()
}

#[cfg(windows)]
fn configure_native<R: Runtime>(webview: &Webview<R>, config: Config) {
    let label = webview.label().to_string();
    let queued_label = label.clone();
    if let Err(error) = webview.with_webview(move |platform| {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
        use windows_core::Interface;
        let result = (|| -> windows_core::Result<()> {
            // SAFETY: Tauri 在 WebView 创建后于其 UI/COM 线程执行此闭包。
            // 只通过有效控制器获取设置接口，不持有裸指针或跨线程保存接口。
            unsafe {
                let settings = platform.controller().CoreWebView2()?.Settings()?;
                settings.SetAreDefaultContextMenusEnabled(config.browser_context_menus)?;
                settings.SetIsStatusBarEnabled(false)?;
                settings.SetIsZoomControlEnabled(false)?;
                settings
                    .cast::<ICoreWebView2Settings3>()?
                    .SetAreBrowserAcceleratorKeysEnabled(config.browser_accelerator_keys)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("[raybend] {label} 桌面 WebView 设置失败：{error}");
        }
    }) {
        eprintln!("[raybend] {queued_label} 无法接管桌面 WebView：{error}");
    }
}

#[cfg(not(windows))]
fn configure_native<R: Runtime>(_: &Webview<R>, config: Config) {
    // Windows 的浏览器快捷键由 WebView2 原生设置关闭；其它平台的右键复用 DOM 兜底。
    let _ = config.browser_accelerator_keys;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_config_disables_browser_menus_and_accelerators() {
        let app: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let config: Config =
            serde_json::from_value(app["plugins"]["desktop-behavior"].clone()).unwrap();
        assert!(!config.browser_context_menus);
        assert!(!config.browser_accelerator_keys);
    }

    #[test]
    fn missing_settings_default_to_desktop_and_invalid_settings_are_rejected() {
        let config: Config = serde_json::from_str("{}").unwrap();
        assert!(!config.browser_context_menus);
        assert!(!config.browser_accelerator_keys);
        assert!(serde_json::from_str::<Config>(r#"{"browserContextMenus":"false"}"#).is_err());
        assert!(serde_json::from_str::<Config>(r#"{"browserContextMenu":false}"#).is_err());
    }
}
