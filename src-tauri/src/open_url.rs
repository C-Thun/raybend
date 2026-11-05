//! 用**系统默认浏览器**打开外链（右栏 GPS 行的「在地图中打开」）。
//!
//! 三条底线：
//!
//! 1. **只放行 `https`**：这是从界面传进来的字符串，能传 `file:` / `javascript:` /
//!    自定义协议就等于给了界面一条越过本应用的执行通道；
//! 2. **必须有主机名**：`https://` 后面空着没有意义，也说明调用方拼错了；
//! 3. 不在 WebView 里导航：`window.open` 会把目标塞进应用自己的窗口栈，
//!    而用户要的是「交给他的浏览器」。
//!
//! 打开失败只回错误、不做重试：这是辅助入口，失败不该影响看片。
use tauri::Url;

/// 校验一个外链：只接受 `https://<host>`。
///
/// 公开成纯函数是为了单测能直接钉住「哪些协议一律拒绝」。
pub fn validate(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw.trim()).map_err(|error| format!("链接格式不对：{error}"))?;
    if url.scheme() != "https" {
        return Err("只允许打开 https 链接".into());
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err("链接缺少主机名".into());
    }
    Ok(url)
}

/// 交给系统默认处理器（Windows 的 `FileProtocolHandler` 等价于双击一个 .url 文件）。
fn launch(url: &Url) -> Result<(), String> {
    #[cfg(windows)]
    let mut command = {
        let mut command = std::process::Command::new("rundll32.exe");
        command.args(["url.dll,FileProtocolHandler", url.as_str()]);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(url.as_str());
        command
    };
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(url.as_str());
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("打不开浏览器：{error}"))
}

#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), String> {
    let parsed = validate(&url)?;
    tauri::async_runtime::spawn_blocking(move || launch(&parsed))
        .await
        .map_err(|error| format!("打开链接失败：{error}"))?
}

#[cfg(test)]
mod tests {
    use super::validate;

    #[test]
    fn only_https_with_a_host_is_allowed() {
        let url = validate("https://www.openstreetmap.org/?mlat=30.25&mlon=120.15").unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("www.openstreetmap.org"));
        // 前后空白也是容忍的（界面里拼出来的串偶尔会带）
        assert!(validate("  https://example.com  ").is_ok());
    }

    #[test]
    fn other_schemes_and_empty_hosts_are_rejected() {
        for raw in [
            "http://example.com",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "raybend://open",
            "https://",
            "not a url",
            "",
        ] {
            assert!(validate(raw).is_err(), "{raw} 不该被放行");
        }
    }
}
