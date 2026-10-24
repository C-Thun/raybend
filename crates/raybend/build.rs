//! 只做一件事：把发布构建的版本信息变成编译期常量，并让 Cargo 在它们变化时重编。
//!
//! `release.mjs` 已经设置 `RAYBEND_GIT_HASH` / `RAYBEND_BUILD_TIME`；`option_env!` 在
//! `diagnostics::crash::AppInfo` 里读取。没有 build.rs 时 Cargo 不会因环境变量变化而重编，
//! 崩溃报告就可能带着上一版的 commit —— 那比没有更糟。

fn main() {
    for key in ["RAYBEND_GIT_HASH", "RAYBEND_BUILD_TIME"] {
        println!("cargo:rerun-if-env-changed={key}");
    }
}
