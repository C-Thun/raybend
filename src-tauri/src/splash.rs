//! splash 语言在应用文档加载前固定，不等待 IPC、app.db 或系统语言探测。

const LOCALE_SCRIPT: &str = include_str!("../../public/splash-locale.js");

pub fn initialization_script() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    script_for_launch(&format!("{}-{nanos}", std::process::id()))
}

fn script_for_launch(launch: &str) -> String {
    let launch_json = serde_json::to_string(launch).expect("启动标识必须能序列化");
    format!("window.__RAYBEND_LAUNCH_ID__={launch_json};\n{LOCALE_SCRIPT}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_is_json_escaped_before_shared_script() {
        let launch = "启动\"\\\n";
        let script = script_for_launch(launch);
        let (assignment, body) = script.split_once(';').unwrap();
        let encoded = assignment
            .strip_prefix("window.__RAYBEND_LAUNCH_ID__=")
            .unwrap();
        assert_eq!(serde_json::from_str::<String>(encoded).unwrap(), launch);
        assert_eq!(body.trim_start(), LOCALE_SCRIPT);
    }
}
