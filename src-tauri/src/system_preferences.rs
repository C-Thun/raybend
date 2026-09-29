//! 启动系统偏好 adapter 的原生侧：只读系统事实，不写数据库或应用偏好。
//! Windows/macOS 外观复用 Tauri，Windows 显示语言用 Win32；其它项由 WebView adapter 补齐。

use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SystemTheme {
    Dark,
    Light,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPreferencesSnapshot {
    pub theme: Option<SystemTheme>,
    pub language: Option<String>,
}

/// tauri.conf 未强制窗口主题，故此处返回系统应用外观。只读取启动时的快照。
#[tauri::command]
pub fn system_preferences<R: tauri::Runtime>(
    window: tauri::Window<R>,
) -> SystemPreferencesSnapshot {
    SystemPreferencesSnapshot {
        theme: platform_theme(&window),
        language: platform_language(),
    }
}

fn platform_theme<R: tauri::Runtime>(window: &tauri::Window<R>) -> Option<SystemTheme> {
    #[cfg(target_os = "linux")]
    {
        // 当前 Tao Linux 在未配置 portal 时固定返回 Light，不能当成成功探测。
        // 返回未知，让现有 WebView 媒体查询 adapter 提供实际值，仍未知则 dark。
        let _ = window;
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        window.theme().ok().and_then(|theme| match theme {
            tauri::Theme::Dark => Some(SystemTheme::Dark),
            tauri::Theme::Light => Some(SystemTheme::Light),
            _ => None,
        })
    }
}

#[cfg(windows)]
fn platform_language() -> Option<String> {
    use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
    let mut count = 0;
    let mut length = 0;
    // SAFETY: 首次只请求 UTF-16 缓冲区大小；两个输出指针均有效。
    let success = unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            std::ptr::null_mut(),
            &mut length,
        )
    };
    if success == 0 || !(2..=32_768).contains(&length) {
        return None;
    }
    let mut buffer = vec![0_u16; length as usize];
    // SAFETY: 缓冲区含 length 个 UTF-16 单元，API 写入上限由 length 指定。
    let success = unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            buffer.as_mut_ptr(),
            &mut length,
        )
    };
    if success == 0 || count == 0 || length as usize > buffer.len() {
        return None;
    }
    first_preferred_language(&buffer[..length as usize])
}

#[cfg(not(windows))]
fn platform_language() -> Option<String> {
    // macOS/Linux 的 WebView navigator.language 是当前适配；以后可在此换原生实现。
    None
}

#[cfg(any(windows, test))]
fn first_preferred_language(buffer: &[u16]) -> Option<String> {
    let end = buffer.iter().position(|unit| *unit == 0)?;
    if end == 0 {
        return None;
    }
    String::from_utf16(&buffer[..end]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_language_wins_including_non_chinese_before_chinese() {
        for (list, expected) in [
            ("en-US\0zh-CN\0\0", "en-US"),
            ("zh-Hant-TW\0en-US\0\0", "zh-Hant-TW"),
        ] {
            assert_eq!(
                first_preferred_language(&list.encode_utf16().collect::<Vec<_>>()).as_deref(),
                Some(expected)
            );
        }
    }

    #[test]
    fn empty_unterminated_or_invalid_utf16_is_unknown() {
        for buffer in [&[][..], &[0, 0], &[122, 104], &[0xD800, 0]] {
            assert_eq!(first_preferred_language(buffer), None);
        }
        assert_eq!(
            first_preferred_language(&"中文\0\0".encode_utf16().collect::<Vec<_>>()).as_deref(),
            Some("中文")
        );
    }

    #[test]
    fn themes_serialize_to_the_frontend_values() {
        assert_eq!(serde_json::to_value(SystemTheme::Dark).unwrap(), "dark");
        assert_eq!(serde_json::to_value(SystemTheme::Light).unwrap(), "light");
        assert_eq!(
            serde_json::to_value(SystemPreferencesSnapshot::default()).unwrap(),
            serde_json::json!({"theme": null, "language": null})
        );
    }
}
