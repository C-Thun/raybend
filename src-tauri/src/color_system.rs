//! Windows 显示色彩事实 adapter。核心只见 `ColorSystemAdapter`，Win32 句柄止于此文件。
//! 系统事件与窗口事实触发后台准备；渲染线程不查窗口、不读 ICC。

#[cfg(windows)]
pub use windows::WindowsColorSystemAdapter;

use raybend::color::system::{
    ColorEnvironment, ColorSystemAdapter, PhysicalWindowRect, VersionedDisplaySnapshot,
};
use std::sync::{Arc, OnceLock, Mutex};

fn watchers() -> &'static Mutex<Vec<raybend::color::system::DisplayWatcher>> {
    static WATCHERS: OnceLock<Mutex<Vec<raybend::color::system::DisplayWatcher>>> = OnceLock::new();
    WATCHERS.get_or_init(Default::default)
}

pub(crate) fn watch_window<R: tauri::Runtime>(
    window: &tauri::WebviewWindow<R>,
    publish: impl Fn(Result<raybend::color::display::PreparedDisplay, raybend::color::system::ColorSystemError>) + Send + 'static,
) -> Result<raybend::color::system::DisplayWatcher, String> {
    let position = window.outer_position().map_err(|e| e.to_string())?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let watcher = raybend::color::system::DisplayWatcher::new(Arc::clone(environment()), format!("native:{}", window.label()),
        PhysicalWindowRect { x: position.x, y: position.y, width: size.width, height: size.height }, publish).map_err(|e| e.to_string())?;
    let mut registry = watchers().lock().unwrap_or_else(|e| e.into_inner());
    registry.retain(|watcher| !watcher.is_closed());
    registry.push(watcher.clone());
    Ok(watcher)
}

#[cfg(windows)]
#[path = "color_system_events.rs"]
mod events;

#[cfg(not(windows))]
pub(crate) fn install<R: tauri::Runtime>(_app: &tauri::AppHandle<R>) {}

#[cfg(windows)]
pub(crate) fn install<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::{Emitter, Manager};
    let handle = app.clone();
    match environment().subscribe_changes(Arc::new(move || {
        environment().invalidate_all();
        let mut registry = watchers().lock().unwrap_or_else(|e| e.into_inner());
        registry.retain(|watcher| !watcher.is_closed());
        for watcher in registry.iter() { watcher.refresh(true); }
        let _ = handle.emit("color://environment-changed", ());
    })) {
        Ok(subscription) => { app.manage(std::sync::Mutex::new(subscription)); }
        Err(error) => eprintln!("[color] system event subscription unavailable: {error}"),
    }
}

fn environment() -> &'static Arc<ColorEnvironment> {
    static ENVIRONMENT: OnceLock<Arc<ColorEnvironment>> = OnceLock::new();
    ENVIRONMENT.get_or_init(|| {
        #[cfg(windows)]
        let adapter: Arc<dyn ColorSystemAdapter> = Arc::new(WindowsColorSystemAdapter);
        #[cfg(not(windows))]
        let adapter: Arc<dyn ColorSystemAdapter> =
            Arc::new(raybend::color::system::UnsupportedColorSystemAdapter);
        Arc::new(ColorEnvironment::new(adapter))
    })
}

#[tauri::command]
pub fn color_display_snapshot<R: tauri::Runtime>(
    window: tauri::Window<R>,
) -> Result<VersionedDisplaySnapshot, String> {
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let rect = PhysicalWindowRect {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };
    // Explicit re-detection also refreshes native preparation, including profile
    // bytes changed on disk. This is event driven, never part of the frame loop.
    for watcher in watchers().lock().unwrap_or_else(|e| e.into_inner()).iter() {
        if !watcher.is_closed() { watcher.refresh(true); }
    }
    environment()
        .refresh(window.label(), rect)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "显示器状态在读取期间已变化，请重试".into())
}

#[tauri::command]
pub fn color_open_system_settings() -> Result<(), String> {
    environment()
        .open_color_settings()
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
mod windows {
    use std::ffi::OsString;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;
    use std::ptr;

    use raybend::color::system::{
        ColorSystemAdapter, ColorSystemError, DisplayColorState, DisplaySnapshot,
        PhysicalWindowRect, SystemOutputSpace,
    };
    use windows_sys::Win32::Devices::Display::{
        DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
        DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
        DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
        DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO,
        DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SDR_WHITE_LEVEL,
        DISPLAYCONFIG_SOURCE_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
    };
    use windows_sys::Win32::Foundation::{
        ERROR_INSUFFICIENT_BUFFER, ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER,
        ERROR_NOT_SUPPORTED, RECT,
    };
    use windows_sys::Win32::Graphics::Gdi::{
        CreateDCW, DeleteDC, GetMonitorInfoW, MonitorFromRect, HDC, MONITORINFOEXW,
        MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::UI::ColorSystem::GetICMProfileW;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    pub struct WindowsColorSystemAdapter;

    impl ColorSystemAdapter for WindowsColorSystemAdapter {
        fn read_display_profile(&self, path: &std::path::Path) -> Result<raybend::color::icc::RgbIcc, ColorSystemError> {
            raybend::color::assets::read_profile(path, raybend::color::icc::IccRole::Display)
                .map_err(|e| ColorSystemError::Unavailable(e.to_string()))
        }
        fn subscribe_changes(&self, invalidate: std::sync::Arc<dyn Fn() + Send + Sync>) -> Result<Box<dyn raybend::color::system::ColorSystemSubscription>, ColorSystemError> {
            Ok(Box::new(super::events::subscribe(invalidate)?))
        }
        fn snapshot(
            &self,
            window: PhysicalWindowRect,
        ) -> Result<DisplaySnapshot, ColorSystemError> {
            let right = window
                .x
                .checked_add(
                    i32::try_from(window.width).map_err(|_| ColorSystemError::InvalidWindowRect)?,
                )
                .ok_or(ColorSystemError::InvalidWindowRect)?;
            let bottom = window
                .y
                .checked_add(
                    i32::try_from(window.height)
                        .map_err(|_| ColorSystemError::InvalidWindowRect)?,
                )
                .ok_or(ColorSystemError::InvalidWindowRect)?;
            let rect = RECT {
                left: window.x,
                top: window.y,
                right,
                bottom,
            };
            // MonitorFromRect 选视口占比最大的显示器，横跨屏幕时只声明这块屏幕。
            let monitor = unsafe { MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST) };
            if monitor.is_null() {
                return Err(ColorSystemError::Unavailable(
                    "MonitorFromRect failed".into(),
                ));
            }
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
            if unsafe { GetMonitorInfoW(monitor, &mut info.monitorInfo) } == 0 {
                return Err(ColorSystemError::Unavailable(
                    "GetMonitorInfoW failed".into(),
                ));
            }
            let device = wide_string(&info.szDevice);
            if device.is_empty() {
                return Err(ColorSystemError::Unavailable(
                    "monitor has no GDI device name".into(),
                ));
            }
            let targets = active_targets(&device)?;
            if targets.len() != 1 {
                return Ok(DisplaySnapshot {
                    display_id: Some(device),
                    state: DisplayColorState::Unavailable {
                        reason: format!(
                            "display target mapping is ambiguous: {} matches",
                            targets.len()
                        ),
                    },
                });
            }
            let target = targets[0];
            let mut advanced = DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO::default();
            advanced.header = target_header(
                DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
                size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>(),
                target,
            );
            let status = unsafe { DisplayConfigGetDeviceInfo(&mut advanced.header) };
            if status != 0 {
                // 老版 Windows/驱动可能不提供 Advanced Color 查询。只有明确的“不支持”
                // 才回传统 ICC；其它错误不能猜当前是否正由系统管理色彩。
                if matches!(
                    status as u32,
                    ERROR_INVALID_FUNCTION | ERROR_INVALID_PARAMETER | ERROR_NOT_SUPPORTED
                ) {
                    return Ok(DisplaySnapshot {
                        display_id: Some(device),
                        state: legacy_state(&info.szDevice),
                    });
                }
                return Ok(DisplaySnapshot {
                    display_id: Some(device),
                    state: DisplayColorState::Unavailable {
                        reason: format!("Advanced Color status unavailable ({status})"),
                    },
                });
            }
            // Microsoft 的 Win32 bitfield：bit 1 = advancedColorEnabled。它包含 ACM/WCG/HDR，
            // 不能当成“HDR 开关”；激活时也不能再应用传统显示器 ICC。
            let advanced_enabled = unsafe { advanced.Anonymous.value } & 0x2 != 0;
            let state = if advanced_enabled {
                let mut white = DISPLAYCONFIG_SDR_WHITE_LEVEL::default();
                white.header = target_header(
                    DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
                    size_of::<DISPLAYCONFIG_SDR_WHITE_LEVEL>(),
                    target,
                );
                let white_nits = if unsafe { DisplayConfigGetDeviceInfo(&mut white.header) } == 0 {
                    let nits = white.SDRWhiteLevel as f32 * 80.0 / 1000.0;
                    (nits.is_finite() && nits > 0.0 && nits <= 10_000.0).then_some(nits)
                } else {
                    None
                };
                DisplayColorState::SystemManaged {
                    output_space: SystemOutputSpace::Srgb,
                    sdr_white_nits: white_nits,
                }
            } else {
                legacy_state(&info.szDevice)
            };
            Ok(DisplaySnapshot {
                display_id: Some(device),
                state,
            })
        }

        fn open_color_settings(&self) -> Result<(), ColorSystemError> {
            // 官方公开的跨 Win10/11 入口；各版本没有稳定的独立色彩管理页 URI。
            let uri: Vec<u16> = "ms-settings:display\0".encode_utf16().collect();
            let result = unsafe {
                ShellExecuteW(
                    ptr::null_mut(),
                    ptr::null(),
                    uri.as_ptr(),
                    ptr::null(),
                    ptr::null(),
                    SW_SHOWNORMAL,
                )
            };
            if (result as isize) <= 32 {
                return Err(ColorSystemError::Unavailable(
                    "cannot open Windows display settings".into(),
                ));
            }
            Ok(())
        }
    }

    type Target = (windows_sys::Win32::Foundation::LUID, u32);

    fn target_header(kind: i32, size: usize, target: Target) -> DISPLAYCONFIG_DEVICE_INFO_HEADER {
        DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: kind,
            size: size as u32,
            adapterId: target.0,
            id: target.1,
        }
    }

    fn active_targets(device: &str) -> Result<Vec<Target>, ColorSystemError> {
        for _ in 0..3 {
            let mut path_count = 0;
            let mut mode_count = 0;
            let status = unsafe {
                GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
            };
            if status != 0 {
                return Err(ColorSystemError::Unavailable(format!(
                    "display topology unavailable ({status})"
                )));
            }
            if path_count > 256 || mode_count > 512 {
                return Err(ColorSystemError::Unavailable(
                    "display topology is unexpectedly large".into(),
                ));
            }
            let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
            let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
            let status = unsafe {
                QueryDisplayConfig(
                    QDC_ONLY_ACTIVE_PATHS,
                    &mut path_count,
                    paths.as_mut_ptr(),
                    &mut mode_count,
                    modes.as_mut_ptr(),
                    ptr::null_mut(),
                )
            };
            if status == ERROR_INSUFFICIENT_BUFFER {
                continue;
            }
            if status != 0 {
                return Err(ColorSystemError::Unavailable(format!(
                    "display topology changed ({status})"
                )));
            }
            let mut targets: Vec<Target> = Vec::new();
            for path in paths.iter().take(path_count as usize) {
                let mut name = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
                name.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                    size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                    adapterId: path.sourceInfo.adapterId,
                    id: path.sourceInfo.id,
                };
                if unsafe { DisplayConfigGetDeviceInfo(&mut name.header) } == 0
                    && wide_string(&name.viewGdiDeviceName).eq_ignore_ascii_case(device)
                {
                    let target = (path.targetInfo.adapterId, path.targetInfo.id);
                    if !targets.iter().any(|(adapter, id)| {
                        adapter.LowPart == target.0.LowPart
                            && adapter.HighPart == target.0.HighPart
                            && *id == target.1
                    }) {
                        targets.push(target);
                    }
                }
            }
            return Ok(targets);
        }
        Err(ColorSystemError::Unavailable(
            "display topology changed repeatedly".into(),
        ))
    }

    struct DisplayDc(HDC);
    impl Drop for DisplayDc {
        fn drop(&mut self) {
            unsafe {
                DeleteDC(self.0);
            }
        }
    }

    fn legacy_profile(device: &[u16; 32]) -> Result<Option<PathBuf>, String> {
        let dc = unsafe { CreateDCW(ptr::null(), device.as_ptr(), ptr::null(), ptr::null()) };
        if dc.is_null() {
            return Err("cannot open display device context".into());
        }
        let _dc = DisplayDc(dc);
        let mut len = 32_768_u32;
        let mut path = vec![0_u16; len as usize];
        if unsafe { GetICMProfileW(dc, &mut len, path.as_mut_ptr()) } == 0 {
            return Ok(None);
        }
        let actual = path
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(len as usize);
        path.truncate(actual);
        if path.is_empty() {
            return Ok(None);
        }
        Ok(Some(PathBuf::from(OsString::from_wide(&path))))
    }

    fn legacy_state(device: &[u16; 32]) -> DisplayColorState {
        match legacy_profile(device) {
            Ok(Some(profile_path)) => DisplayColorState::Icc { profile_path },
            Ok(None) => DisplayColorState::SrgbFallback {
                reason: "Windows did not return a default display ICC".into(),
            },
            Err(reason) => DisplayColorState::Unavailable { reason },
        }
    }

    fn wide_string(data: &[u16]) -> String {
        let end = data
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(data.len());
        String::from_utf16_lossy(&data[..end])
    }
}
