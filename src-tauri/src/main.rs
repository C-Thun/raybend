// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/*
 * RAW 解码 worker 的**兜底入口**（`crates/raybend/src/bin/raybend-raw-worker.rs` 的文档写着这里）。
 *
 * 为什么主程序要认这个标记：RAW 解码必须在**独立进程**里跑（`AGENTS.md` §6.3 —— 解码器崩了
 * 不能带走主进程）。开发期 `target/debug/` 里有独立的 `raybend-raw-worker[.exe]`，主程序
 * 直接拉它；但**打包后的应用只有一个可执行文件**，那时 `raw::worker::resolve_worker()`
 * 会返回「就是我自己 + 带标记」，于是主程序被自己拉起来当 worker 用。
 *
 * ❗ 2026-09-17 修：这段处理**一直没写**（只有 worker 侧的文档写着「见 main.rs」）。
 * 后果很隐蔽：当同目录没有 worker bin 时（例如只 `cargo build -p raybend-desktop`），
 * 主程序会带上标记重启自己 → 这里忽略标记 → **又开了一个 GUI 实例**，
 * 而解码请求等的是一个说协议的 worker → RAW 缩略图拿不到画面（表现为占位块或空白），
 * 且不会报出真正的原因。
 *
 * ⚠️ 顺序不能反：必须在 `run()` **之前**判断，被拉起来当 worker 时**不能**建窗口、
 * 不能初始化数据库、不能弹闪屏。
 */
fn main() {
    if std::env::args().any(|arg| arg == raybend::raw::worker::WORKER_ARG) {
        // worker 模式：跑协议循环，退出码按协议结果（0 正常 / 非 0 交给调用方判断）
        std::process::exit(raybend::raw::worker::run_worker_main());
    }
    raybend_desktop_lib::run();
}

#[cfg(test)]
mod tests {
    /// 标记常量必须与 worker 侧一致 —— 这里钉一下，免得改名时两边脱节
    /// （脱节的后果：打包后 RAW 缩略图静默失败，且看不出原因）。
    #[test]
    fn worker_arg_matches_the_worker_side_constant() {
        assert_eq!(raybend::raw::worker::WORKER_ARG, "--raybend-raw-worker");
    }

    /// 参数判定：只有**恰好等于标记**的参数才算 worker 调用。
    /// 用「contains 子串」那种写法会把 `--raybend-raw-worker-x` 之类误判进来。
    #[test]
    fn only_the_exact_marker_counts() {
        let is_worker = |args: &[&str]| args.contains(&raybend::raw::worker::WORKER_ARG);
        assert!(is_worker(&["raybend-desktop.exe", "--raybend-raw-worker"]));
        assert!(!is_worker(&["raybend-desktop.exe"]));
        assert!(!is_worker(&["raybend-desktop.exe", "--raybend-raw-worker-x"]));
        assert!(!is_worker(&["--spike=1"]));
    }
}
