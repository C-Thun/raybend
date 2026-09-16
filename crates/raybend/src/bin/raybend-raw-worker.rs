//! RAW 解码 worker 的独立可执行文件（`AGENTS.md` §6.3 的进程隔离）。
//!
//! **不要直接运行它** —— 它按长度前缀协议从 stdin 读请求、往 stdout 写响应，
//! 由 `raybend::raw::worker::RawWorker` 拉起。手动调试时用
//! `RAYBEND_RAW_WORKER=target/debug/raybend-raw-worker` 让主程序指向某个特定构建。
//!
//! 打包后的应用里没有这个文件 —— 那种情况下主程序会用
//! `--raybend-raw-worker` 标记重启自己（见 `src-tauri/src/main.rs`）。

fn main() {
    std::process::exit(raybend::raw::worker::run_worker_main());
}
