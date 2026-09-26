//! 编译期通道控制发行入口，不能被启动参数或环境变量绕过。
pub fn diagnostics_allowed(channel: &str) -> bool {
    matches!(channel, "dev" | "test")
}
pub fn diagnostics_enabled() -> bool {
    diagnostics_allowed(option_env!("RAYBEND_CHANNEL").unwrap_or("dev"))
}
#[cfg(test)]
mod tests {
    #[test]
    fn channels_fail_closed() {
        for channel in ["dev", "test"] {
            assert!(super::diagnostics_allowed(channel));
        }
        for channel in ["release", "beta", "", "Release", "unknown"] {
            assert!(!super::diagnostics_allowed(channel));
        }
    }
}
