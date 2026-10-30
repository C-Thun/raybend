//! **debug 构建自动开 WebView2 调试口（CDP）** —— 让 `pnpm debug:win` 出来的 exe
//! 直接就能被探针连上，不必再记「带参数启动」那一长串（`memory/DEBUGGING.md` §1）。
//!
//! ```text
//! 以前：WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9333' WSLENV=... <exe>
//! 现在：debug 构建的 exe 自己设这个环境变量（发布构建**永远不设**）
//! ```
//!
//! ## 为什么是「自己设环境变量」而不是别的办法
//!
//! * **不能在 `tauri.conf.json` 里写 `additionalBrowserArgs`**：那份配置两个 profile 共用，
//!   写进去等于**发布版也开调试口**（调试口是一个能读 DOM、能调 IPC 的入口，不该跟着发布走）。
//! * **不能等 setup 再设**：窗口（以及 WebView2 环境）是按配置在 `Builder` 里就建好的，
//!   setup 跑起来时环境已经创建完了，再设环境变量没用。
//!   所以入口在 [`install`]，由 `run()` 在 `tauri::Builder` **之前**调用。
//! * WebView2 的 loader 在建环境时读 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`，
//!   而且**它会覆盖** `CoreWebView2EnvironmentOptions::AdditionalBrowserArguments`
//!   （微软 Globals 文档：这几个环境变量会 override 环境选项；WebView2Feedback #5571
//!   就是有人在抱怨这个优先级）。wry 0.55.1 在 `webview2/mod.rs::create_environment` 里
//!   默认塞了三个 `--disable-features`，所以**我们自己设时必须把它们带上** ——
//!   否则开个调试口顺手把 Edge 迷你菜单 / SmartScreen 的默认屏蔽弄丢了，
//!   debug 版的行为就和发布版不一致了。
//!
//! ## 边界
//!
//! * Chromium 把调试口绑在 **Windows 的 `127.0.0.1`**（headed 模式的安全限制），
//!   局域网碰不到；WSL 侧要连仍需那一次性的 `netsh portproxy 9334 → 9333`
//!   （命令见 `scripts/perf-win.mjs` 头部，本机已配好）。
//! * 端口固定（默认 9333，可用 `RAYBEND_CDP_PORT` 覆盖）：同时开两个 debug 实例时
//!   只有一个能绑上，第二个只是没有调试口，应用照常运行。
//! * 已经在环境里显式给了 `--remote-debugging-port` 的（例如 `pnpm perf:win --launch`），
//!   **尊重它**，不再追加第二个。
//! * 关掉：`RAYBEND_CDP=0`（也认 `off` / `false` / `no`）。
//!
//! 不接命令体系：这是开发设施的启动期行为，界面上不该有入口（`AGENTS.md` §2.15 的例外，
//! 理由同 `spike_viewport`）。

use std::sync::OnceLock;

/// WebView2 读的浏览器参数环境变量（loader 在建环境时读一次）。
pub const ENV_ARGS: &str = "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS";
/// 关掉调试口的开关（`0` / `off` / `false` / `no`）。
pub const ENV_SWITCH: &str = "RAYBEND_CDP";
/// 覆盖端口（默认 [`DEFAULT_PORT`]）。
pub const ENV_PORT: &str = "RAYBEND_CDP_PORT";

/// 调试口默认端口 —— 与 `scripts/perf-win.mjs` 的 `PERF_WIN_PORT` 是同一个数
/// （`netsh portproxy` 的 9334 也指向它），改这里要同步那边。
pub const DEFAULT_PORT: &str = "9333";

/// wry 默认的浏览器参数。
///
/// 出处：`wry-0.55.1/src/webview2/mod.rs::create_environment`
/// （`--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection`）。
/// 升级 wry 时对着那一行核一遍；`defaults_match_wry` 这条测试只是钉住「我们这里有一份」，
/// 它盯不住上游改动。
const WRY_DEFAULT_BROWSER_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

/// 本次进程实际开的端口（`None` = 没开）。给 setup 里那行日志用。
static ENABLED_PORT: OnceLock<String> = OnceLock::new();

/// 调试口这次真的开了吗（端口值）。
pub fn enabled_port() -> Option<&'static str> {
    ENABLED_PORT.get().map(String::as_str)
}

/// 开关值是否表示「不要开」。空串不算关（未设置时 `var` 给 `Err`，走不到这里）。
fn is_off(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "off" | "false" | "no"
    )
}

/// 端口取值：只认纯数字；空串 / 非数字 / 越界（> 65535）一律回落到默认端口。
///
/// 不静默接受坏值再让 WebView2 去失败 —— 那种失败只会在 Edge 的日志里，
/// 探针连不上时根本看不出是「端口写错了」。
fn port_of(value: Option<&str>) -> &str {
    let Some(raw) = value.map(str::trim) else {
        return DEFAULT_PORT;
    };
    let valid = !raw.is_empty()
        && raw.chars().all(|c| c.is_ascii_digit())
        && raw.parse::<u32>().is_ok_and(|port| (1..=65535).contains(&port));
    if valid { raw } else { DEFAULT_PORT }
}

/// 算出要设的浏览器参数（`None` = 用户关掉了，不设）。
///
/// 纯函数：环境变量在外面读，这里只做判断，便于单测（改环境变量在测试里是并发不安全的）。
///
/// * `existing`：当前 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`（`None` = 没设）；
/// * `opt_out`：`RAYBEND_CDP` 的值；
/// * `port_env`：`RAYBEND_CDP_PORT` 的值。
pub fn plan(existing: Option<&str>, opt_out: Option<&str>, port_env: Option<&str>) -> Option<String> {
    if opt_out.is_some_and(is_off) {
        return None;
    }
    let port = port_of(port_env);
    match existing.map(str::trim).filter(|value| !value.is_empty()) {
        // 已经有人显式指定了调试口（`perf:win --launch` / 手工起）：不重复加
        Some(value) if value.contains("--remote-debugging-port") => Some(value.to_string()),
        Some(value) => Some(format!("{value} --remote-debugging-port={port}")),
        // 没设过：wry 的默认值会因为我们设了环境变量而失效，所以在这里补上
        None => Some(format!("{WRY_DEFAULT_BROWSER_ARGS} --remote-debugging-port={port}")),
    }
}

/// 在**创建 WebView2 环境之前**装好调试口参数；返回实际开的端口。
///
/// 发布构建直接返回 `None`（`cfg!(debug_assertions)` 为假），一行环境变量都不碰。
pub fn install() -> Option<&'static str> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let existing = std::env::var(ENV_ARGS).ok();
    let opt_out = std::env::var(ENV_SWITCH).ok();
    let port_env = std::env::var(ENV_PORT).ok();
    let args = plan(existing.as_deref(), opt_out.as_deref(), port_env.as_deref())?;
    let port = port_of(port_env.as_deref()).to_string();
    // SAFETY: 调用点是 `run()` 的最开头 —— 那时进程里还没有任何后台线程，
    // Tauri / WebView2 / tokio 都还没起来，不存在「别的线程正在并发读环境变量」
    // （这也是 Rust 2024 把 `set_var` 标成 unsafe 的唯一顾虑）。
    unsafe { std::env::set_var(ENV_ARGS, &args) };
    // 日志层这时还没起（setup 里才 init），先走 stderr；落盘那一行在 setup 里补。
    eprintln!("[raybend] debug 构建：WebView2 调试口 http://127.0.0.1:{port}（RAYBEND_CDP=0 可关）");
    let _ = ENABLED_PORT.set(port);
    enabled_port()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// wry 的默认值必须在我们这条路径上保留（否则 debug 版与发布版行为不一致）。
    #[test]
    fn defaults_match_wry() {
        let args = plan(None, None, None).expect("默认应当开调试口");
        assert!(args.contains(WRY_DEFAULT_BROWSER_ARGS), "{args}");
        assert!(args.contains("--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection"));
    }

    /// 没设过环境变量：默认值 + 默认端口。
    #[test]
    fn fresh_install_uses_default_port() {
        let args = plan(None, None, None).unwrap();
        assert!(args.ends_with("--remote-debugging-port=9333"), "{args}");
        assert!(!args.contains("--remote-debugging-port=9333 --remote-debugging-port"));
    }

    /// 空串按「没设过」处理（Windows 上 `set VAR=` 会留下空值）。
    #[test]
    fn empty_existing_is_treated_as_unset() {
        assert_eq!(plan(Some(""), None, None), plan(None, None, None));
        assert_eq!(plan(Some("   "), None, None), plan(None, None, None));
    }

    /// 已有别的参数：追加，而不是覆盖。
    #[test]
    fn existing_args_are_kept() {
        let args = plan(Some("--foo=1"), None, None).unwrap();
        assert_eq!(args, "--foo=1 --remote-debugging-port=9333");
    }

    /// 已有调试口（`perf:win --launch`）：原样尊重，不追加第二个。
    #[test]
    fn explicit_port_is_respected() {
        let args = plan(Some("--remote-debugging-port=9222"), None, None).unwrap();
        assert_eq!(args, "--remote-debugging-port=9222");
    }

    /// 端口覆盖：纯数字生效。
    #[test]
    fn port_override() {
        let args = plan(None, None, Some("9450")).unwrap();
        assert!(args.ends_with("--remote-debugging-port=9450"), "{args}");
    }

    /// 坏端口回落默认值（空 / 非数字 / 0 / 越界）。
    #[test]
    fn bad_port_falls_back() {
        for bad in ["", "   ", "abc", "9333x", "0", "70000", "-1"] {
            let args = plan(None, None, Some(bad)).unwrap();
            assert!(
                args.ends_with("--remote-debugging-port=9333"),
                "bad={bad:?} -> {args}"
            );
        }
    }

    /// 关掉调试口：`0/off/false/no`（大小写、空白都不敏感）。
    #[test]
    fn opt_out_values() {
        for off in ["0", "off", "false", "no", "OFF", " False "] {
            assert_eq!(plan(None, Some(off), None), None, "{off:?}");
        }
        // `1` / 随便写的值都不算关（默认就是开，不把拼写错误当开关）
        for on in ["1", "on", "yes", "", "0x0"] {
            assert!(plan(None, Some(on), None).is_some(), "{on:?}");
        }
    }

    /// 关掉时即使环境里有值也不动它 —— 用户显式给的参数不该被我们改写。
    #[test]
    fn opt_out_leaves_existing_args_alone() {
        assert_eq!(plan(Some("--foo=1"), Some("0"), Some("9450")), None);
    }
}
