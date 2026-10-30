# debug 构建自动开 WebView2 调试口（CDP）

> 完成时间：2026-10-30 14:00:52

## 起因

2026-10-30 凌晨排查「点定稿确认后卡死」时，现场什么都能拿到（进程、日志、线程栈、截图），
**唯独拿不到 DOM 与 IPC** —— 因为崔总那个 debug 实例是直接跑 exe 起的，没带
`--remote-debugging-port`，WSL 侧连 9333/9334 都是空的（`memory/DEBUGGING.md` §1 的旧流程
要求「带参数启动」，日常根本记不住）。

崔总的问题：**能不能让 `pnpm debug:win` 编出来的版本自己就带上这个效果？**

## 改动范围

debug 构建在**创建 WebView2 环境之前**自己设 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`；
发布构建一行都不碰。

| 文件 | 改动 |
| --- | --- |
| `src-tauri/src/webview_debug.rs` | **新增**：`plan()`（纯函数）+ `install()` + `enabled_port()`，9 条单测 |
| `src-tauri/src/lib.rs` | `mod webview_debug;`；`run()` 最开头调 `install()`；setup 里补一行落盘日志 |
| `memory/DEBUGGING.md` §1 | 「拉起带调试口的 exe」改成「debug 构建自动开」，补端口覆盖 / 关闭开关 / 冲突说明；排障那条同步改 |
| `scripts/perf-win.mjs` | 头注：`--launch` 不再是必需路径 |

## 关键决策与理由

1. **为什么是「程序自己设环境变量」，不是 `tauri.conf.json` 的 `additionalBrowserArgs`**
   —— 那份配置 debug / release 共用，写进去等于**发布版也开调试口**（能读 DOM、能调 IPC 的入口）。
2. **为什么必须在 `tauri::Builder` 之前调**
   —— 窗口（连同 WebView2 环境）是按配置在 builder 里就建好的；setup 跑起来时环境已经创建完，
   那时再设环境变量不会生效。
3. **为什么要把 wry 的默认参数一起带上**
   —— 微软 Globals 文档写明 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` **会覆盖**
   `CoreWebView2EnvironmentOptions::AdditionalBrowserArguments`（WebView2Feedback #5571 就是在抱怨
   这个优先级）；而 wry 0.55.1 `webview2/mod.rs::create_environment` 默认塞了
   `--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection`。
   只设自己的那一项 = 顺手把这三个默认屏蔽弄丢，debug 版行为与发布版不一致。
   该常量在 `webview_debug.rs` 里注明出处，升级 wry 时对着那一行核。
4. **只在 `debug_assertions` 下生效**：发布构建 `install()` 直接返回 `None`。
   接的是 `debug_assertions` 而不是 `tauri::is_dev()` —— `debug:win` 带 `custom-protocol`，
   `is_dev()` 为假，但正是要覆盖的那条路径。
5. **尊重既有值**：环境里已有 `--remote-debugging-port`（`pnpm perf:win --launch` / 手工起）就原样用，
   不追加第二个；已有别的参数则追加而不是覆盖。
6. **端口固定 9333、可 `RAYBEND_CDP_PORT` 覆盖**：`netsh portproxy` 的 9334 指向它。
   两个 debug 实例同时开时只有一个能绑上，另一个只是没有调试口、应用照常跑。
   坏端口（空/非数字/0/越界）回落默认值，不静默交给 WebView2 去失败。
7. **关闭开关 `RAYBEND_CDP=0`**（也认 `off`/`false`/`no`，大小写与空白不敏感）。
8. **不在命令体系里挂入口**：启动期的开发设施，界面上不该有开关（同 `spike_viewport`）。

## 验证

* `cargo test -p raybend-desktop --lib webview_debug` → **9 passed, 0 failed**
  （默认值保留 / 默认端口 / 空串当未设 / 追加不覆盖 / 显式端口不被重复加 / 端口覆盖 /
  坏端口回落 / 关闭开关取值 / 关闭时不改写既有值）。
* `cargo test --workspace` → **1594 + 1 + 7 + 4 + 169 + 2 + 2 passed, 0 failed**（13 ignored）。
  范围与命令原文即此，未跑单包。
* `cargo check --workspace` 通过；`cargo clippy -p raybend-desktop --lib` 对本文件无告警
  （`unsafe { set_var }` 已按 clippy 要求配 `// SAFETY:` 注记，冒号必须是 ASCII）。

## 未经人类验证（E2E）

* **真机跑一次**：`pnpm debug:win` → 直接启动 exe → WSL 侧
  `curl -s http://<默认网关>:9334/json/version` 应返回 WebView2 的 Browser 版本。
  这一步要在 Windows 侧真编译真运行，本会话只验到单测。
* 顺带确认：`RAYBEND_CDP=0` 起了之后 9333 不监听；发布构建（`pnpm release`）不会出现这一行日志。

## 遗留

* 调试口只在**新起的进程**生效，已经在跑的实例（含 2026-10-30 凌晨卡住那个）不受影响。
* 端口写死 9333 是为了配合 `netsh portproxy`；如果哪天要并行开多个可调试实例，需要按实例分配端口
  并改 portproxy（现在没这个需求）。
* 与并行会话共用一个工作树：本次只动了上面四个文件，`src-tauri/src/lib.rs` 里另有他人未提交的
  LUT 取消开关改动（`lut_import_cancel`），两边插入点不重叠。
