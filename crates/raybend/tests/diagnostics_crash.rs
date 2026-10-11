//! 诊断层端到端：一次初始化 → 写日志 → 人为 panic → 崩溃记录落盘（`AGENTS.md` §2.8 冒烟）。
//!
//! 这是**本机可跑**的那部分验证；真机崩溃（Windows 上 abort/minidump、助手进程）归人类
//! （`specs/logging-and-crash-report.md` §13 的 E2E 清单）。

use raybend::diagnostics::{self, paths};

#[test]
fn log_line_and_panic_report_land_in_the_temp_root() {
    let dir = tempfile::tempdir().expect("临时目录");
    let root = dir.path();

    // ① 初始化（幂等；本进程只此一份）
    let diagnostics = diagnostics::init(root);
    diagnostics::log::info!(target: "diagnostics.test", "集成测试的一行日志 marker=hello-4421");
    if let Some(sink) = diagnostics.sink() {
        sink.flush_blocking();
    }

    // ② 人为 panic（被 catch_unwind 接住，测试继续）——路径里塞一个「用户目录」验证脱敏
    let caught = std::panic::catch_unwind(|| {
        panic!("集成测试崩溃 at C:\\Users\\bob\\secret\\x.RAF");
    });
    assert!(caught.is_err(), "panic 应当被接住");

    // ③ 日志文件：会话头 + 我们的行 + panic 行（同步写）
    let log_path = paths::logs_dir(root).join(diagnostics::writer::ACTIVE_FILE);
    let log = std::fs::read_to_string(&log_path).expect("日志文件应当存在");
    assert!(log.contains("=== session "), "会话头在文件开头");
    assert!(log.contains("marker=hello-4421"), "日志行落盘：{log}");
    assert!(log.contains("crash"), "panic 行同步写进了日志");

    // ④ 崩溃记录：report.json + report.txt，且**原路径已脱敏**
    let crashes = paths::crashes_dir(root);
    let entries: Vec<_> = std::fs::read_dir(&crashes)
        .expect("崩溃目录应当存在")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("RB-"))
        .collect();
    assert_eq!(entries.len(), 1, "一次 panic 一份记录");
    let report = std::fs::read_to_string(entries[0].path().join("report.json")).expect("读 report.json");
    assert!(report.contains("\"kind\": \"panic\""), "{report}");
    assert!(report.contains("集成测试崩溃"), "{report}");
    assert!(!report.contains("bob"), "panic 消息必须已脱敏：{report}");
    assert!(!report.contains("secret"), "panic 消息必须已脱敏：{report}");
    let human = std::fs::read_to_string(entries[0].path().join("report.txt")).expect("读 report.txt");
    assert!(human.contains("RayBend 崩溃记录"), "{human}");

    // ⑤ 正常退出：删 session.lock
    diagnostics.shutdown();
    assert!(
        diagnostics::crash::read_previous_session(&crashes).is_none(),
        "正常退出后不该留下 session.lock"
    );
}
