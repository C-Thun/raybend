# 诊断层落地（W1 运行日志 + W2 崩溃记录 + W4 独立崩溃助手）

**完成时间**：2026-10-12 02:52:17 CST
**前置**：崔总 2026-10-12 指示——「不要瞎猜这次崩因，关键是后续再崩能找到问题；先把日志这套建起来，包括记录崩溃的独立监控进程；发送/接收留 future」。
**规格**：`specs/logging-and-crash-report.md`（W1/W2/W4 波次；选型一处按崔总当日决定改定）。
**关联**：`AGENTS.md` §5.8（新写的「日志与崩溃记录怎么看」）、`memory/FUTURE.md`（第三层与 W5 仍留该板块）。

---

## 1. 这一轮做了什么（对照规格波次）

| 波次 | 内容 | 状态 |
| --- | --- | --- |
| **W1** | 运行日志：`log` 门面 + **自研 writer**（2 MiB×10 轮转、20 MiB 目录硬上限、有界队列 4096 + `try_send` + 丢弃计数、UTC 定长行、会话头、**出口脱敏**、worker stderr 管道接回） | ✅ |
| **W2** | panic hook 同步写崩溃记录（`crashes/RB-*/report.{json,txt}`：消息 / `file:line:col` / 线程 / backtrace / 日志尾 200 行）+ `session.lock` 未正常退出兜底 | ✅ |
| **W4** | 独立助手 `raybend-crashpad`（crash-handler + minidumper）：主进程崩溃时落 `dump.dmp` + `dump.json`；发版链路接 sidecar | ✅（发版链路**未真机验证**，见 §5） |
| （附加） | 命令 `diagnostics.openLogFolder`（打开诊断目录；热键留空、不进菜单） | ✅ |
| **W3-α** | **崩溃后的报告导出**（崔总 2026-10-12 追加）：崩溃 → 下次启动弹窗问一次 → 「保存报告…」→ 系统另存为 → 在所选目录产 `RayBend-错误报告-RB-*.zip` + 《RayBend-错误报告提交指南.txt》（GitHub issue / 邮件）；「不报告」= 标记后不再问 | ✅ |
| **W3** | 报告对话框（需 `.pen`）/「导出诊断包」命令 / 复制报告摘要 / `mailto:` 预填 | ❌ 未开工（留 `memory/FUTURE.md`） |
| **W5** | WebView2 `ProcessFailed` / 前端上报 / 卡死看门狗 | ❌ 未开工（同上） |
| **S1** | 服务端与一键上传 | ❌ 未开工（崔总明确「以后再说」） |

## 2. 关键决策与理由

| 决策 | 理由 |
| --- | --- |
| **放弃 `flexi_logger`，自研 writer**（崔总 2026-10-12 拍板） | 核实 0.31.10 源码：`WriteMode::Async` 是 `crossbeam_channel::unbounded`（给不了有界队列与丢弃计数），`Cleanup` 只有 `KeepLogFiles`/`KeepForDays`（给不了「目录总容量」这条规格驳回按个数后唯一的判据）。它唯一满足的是按大小轮转——那一段自研 ~250 行、全可单测 |
| **release 保持 `panic = "abort"`**（崔总 2026-10-12 拍板） | 崩溃即退出、行为可预期；既有 `catch_unwind` 自恢复在发布版仍是空操作（dev 有效） |
| **符号策略本次不动**（崔总 2026-10-12 拍板） | 崔总现在测的是 debug 构建（`debug = 1`，backtrace 带行号）；发布版 `strip = true` 的取舍等发版前单独拍，已登记 FUTURE |
| 脱敏放在 **sink**（`LogSink::enqueue_line` / `write_blocking`）而不是门面 | 规格 §8.1「唯一闸门」：门面与崩溃路径都得过它。崩溃记录器再脱敏一次（幂等） |
| 崩溃记录里**保留源码位置**（`crates/.../x.rs:42`），截掉构建机前缀 | 规格 §8.2：那是代码信息不是用户信息；截断同时避免泄漏 `/home/<某人>/…` |
| 助手**独立二进制**、`on_client_disconnected → Exit` | 规格 §5.2：不做 re-exec（社区有启动连崩先例）；主进程走了助手立刻退出，不留孤儿 |
| panic 也请求 dump（空 `CrashContext`） | `panic = "abort"` 在 Windows 上是 `int 0x29`（fastfail），**crash-handler 接不住**；只有 panic hook 那一瞬还活着，所以 dump 请求必须在 hook 里发出（助手在另一进程里对我们 pid 落 dump） |
| 诊断命令只做「打开目录」、热键留空、不进菜单 | 规格 §9；报告包与发送属 W3（未开工） |
| 崩溃后的弹窗：**系统 `Dialog` + `Checkbox` 组合**（不新建对话框范式） | 复用 §12.1 原语；不新增视觉结构 ⇒ 本步不另出 `.pen`（完整报告对话框仍属 W3，开工前按 §5.1 出稿） |
| 「不报告」/ Esc / 点遮罩 → **写 `prompted` 标记**；另存为里取消 → **不标记** | 崔总 2026-10-12：「用户选择不报告，下次启动不能反复弹」；但另存为取消说明用户本想存东西，下次再问一次（一行代码可改成也标记） |
| 报告包里的 **dump 默认不带**，弹窗里勾选 | 规格 §5.2/§8.4：dump 是内存快照，可能含已解码像素片段 |

## 3. 涉及文件

#### 新增

| 文件 | 内容 |
| --- | --- |
| `crates/raybend/src/diagnostics/{mod,logger,writer,redact,crash,paths,timefmt}.rs` | 诊断层全部实现（不依赖 tauri）：init/会话/队列与轮转/脱敏/崩溃记录/时间格式化 |
| `crates/raybend/src/diagnostics/bundle.rs` | **报告包**：找未提示的崩溃 → 打 zip（report/dump/logs/指南）→ 写《提交指南.txt》；`prompted` 标记 |
| `src/api/diagnostics.ts` | 前端封装：`pendingCrash` / `dismissCrash` / `exportBundle` / 打开目录 |
| `crates/raybend-crashpad/{Cargo.toml,src/main.rs}` | 独立崩溃助手（只依赖 crash-handler + minidumper） |
| `crates/raybend/tests/diagnostics_crash.rs` | 端到端：init → 写日志 → 人为 panic → 校验 report.json/txt 与脱敏 → 正常退出删锁 |
| `src-tauri/src/crashpad.rs` | 主进程侧接入：拉起助手、命名管道、崩溃处理器、panic 路径 dump 钩子、退出清理 |
| `src-tauri/src/diagnostics_cmd.rs` | `diagnostics_open_folder` 命令 |
| `src/api/diagnostics.ts` | 前端封装（唯一 invoke 点） |
| `src/api/dialog.ts` | 新增 `pickSaveFile`（系统另存为；取消返回 `null`） |
| `src/App.tsx` | 启动后问一次崩溃弹窗 + 保存流程（另存为 → 导出 → toast） + i18n 文案 |
| `crates/raybend/Cargo.toml` | 新增 `zip` 4.6.1（已在 Cargo.lock；**只开 deflate**，不开加密/lzma/zstd） |

#### 改动

| 文件 | 改动 |
| --- | --- |
| `src-tauri/src/lib.rs` | setup 最前面初始化诊断层 + 接助手；`RunEvent::Exit` 里 flush + 清锁 + 收助手；注册命令 |
| `crates/raybend/src/worker_process.rs` | worker `stderr` 由 `inherit()` 改**管道**，逐行带 `[raw-worker]`/`[ai-worker]` 标签进统一日志（原先双击启动时全丢） |
| `crates/raybend/src/{raw,ai}/worker.rs` | 传 worker 标签 |
| `Cargo.toml` / `crates/raybend/Cargo.toml` / `src-tauri/Cargo.toml` | 新增 `log`（门面）、`crash-handler` 0.8、`minidumper` 0.11；workspace 加 `crates/raybend-crashpad` |
| `scripts/debug-win.mjs` | 构建加 `-p raybend-crashpad`（否则助手不存在，崩溃时没有 dump） |
| `scripts/lib/release-windows.mjs` + `.test.mjs` | 发版：构建助手 → 拷成 `src-tauri/binaries/raybend-crashpad-x86_64-pc-windows-msvc.exe` → 发版配置加 `externalBin` |
| `scripts/check-win-artifact.mjs` | 新增「助手在不在」的核对（缺 → **警告**不阻断，因为日志与 panic 记录仍然可用） |
| `.gitignore` | 忽略 `src-tauri/binaries/`（构建产物） |
| `AGENTS.md` | 新增 §5.8「出问题了去哪看：日志与崩溃记录」（崔总要求） |
| `specs/logging-and-crash-report.md` / `memory/FUTURE.md` | 状态与选型同步（W1/W2/W4 已施工、W3/W5/S1 未开工） |

**关键路径 `println!/eprintln!` → `log`**（W1 的收敛）：editor / sidecar / develop / db / migration / import / thumbs / browse / recycle / thumbnail-render 共转换 **51 处**（`eprintln!` → `log::warn!`；`db::warm_up` 的就绪行 → `log::info!`）。全仓剩余 **83 处**（多为测试、示例与低频模块：`spike_viewport` 8、`lib.rs` 6、`lens.rs` 5、`color/icc` 4、`wavelet` 4 …）——**登记为后续顺手收敛，不阻塞**。

## 4. 验证（Agent 可自测部分）

| 项 | 结果 |
| --- | --- |
| `cargo test -p raybend --lib diagnostics::` | 26 passed（时间格式化边界、脱敏金丝雀、轮转与容量清理、队列丢弃、会话头、panic 去重、未正常退出、路径布局）+ **bundle 4 passed**（未提示筛选/标记、zip 条目与可打开、dump 开关、指南渠道与 ID） |
| `cargo test -p raybend --lib`（全库） | **1540 passed / 0 failed / 11 ignored** |
| `cargo test -p raybend-crashpad` | passed（日期、目录形状、marker、参数） |
| `cargo test -p raybend --test diagnostics_crash` | passed（端到端：日志 + panic 记录 + 脱敏 + 清锁） |
| `cargo check --workspace` | 通过 |
| `pnpm check:diagnostics --reset`（**真机 E2E**，Windows debug 产物） | **七步全绿**（见 §5.1 的两处修复就是在这一步逼出来的） |
| `pnpm typecheck` / `pnpm test`（1296）/ `pnpm lint:i18n` / `pnpm lint:arch` / `pnpm lint:colors` | 全绿 |
| 脱敏金丝雀语料 | 中文路径、带空格路径、UNC、emoji、邮箱、IP、URL、嵌入 panic 消息的路径 → 输出无原串片段；源码路径截段保留 |

## 5. 遗留与未验证（如实登记）

1. **真机（Windows）冒烟已跑**（2026-10-12）：`pnpm check:diagnostics --reset` 七步全绿 ——
   ① 日志有会话头 ② 崩溃助手已就位 ③ 硬杀 → 下次启动落 `unclean-exit` 记录 ④ 导出 zip + 指南 + `prompted` 标记
   ⑤ 同一条崩溃不再重复问 ⑥ 优雅关闭删 `session.lock` ⑦ panic → `report.json`（含 `src-tauri\src\diagnostics_cmd.rs:88:5`）+ **`dump.dmp` 324189 字节** + `dump.json`。
   崔总本人也确认了弹窗出现与「另存为 → 保存成功」（那份包在 `Downloads/`，已核对：report/logs/指南均在）。
2. **发版链路（sidecar / externalBin）未真机验证**：需要一次真实 `pnpm release` 才能确认装出来的包里有
   `raybend-crashpad.exe` 且主程序能找到它。`check:win` 已加核对，发版前请一并确认。
3. **符号策略**：发布版 `strip = true` ⇒ backtrace 只有地址。发版前拍（已登记 FUTURE）。
4. **`RAYBEND_LOG` 只有整档**（`error|warn|info|debug|trace`），规格里的「按 target 细化」未做。
5. **诊断模式开关**（`diagnostics.setVerbose`）与设置页入口属 W3（未开工）。
6. **浏览/日志目录的查看入口**：命令面板（Ctrl+K）一条路 + 弹窗里的保存按钮；菜单与设置页入口留 W3。
7. 助手在 Linux（WSL dev）上也能编译运行，但 `crash-handler` 的平台行为只在 Windows 上验过；非 Windows 不在验收范围（规格 §2）。
8. **W3-α 界面部分已有真机证据**（弹窗 + 保存成功）；「不报告」按钮后的「下次不再弹」是在命令层验的（`prompted` 标记 + `pending_crash` 返回 null），界面按钮本身的点击链未单独验（与 §1 同一批）。
9. 《提交指南.txt》只有中文（它是给用户的文档，不是界面文案）；若要英文版，等 i18n 方案定了一起做。

### 5.1 真机阶段找出来并修掉的两个问题（这就是为何要真机验）

| 现象 | 根因 | 修法 |
| --- | --- | --- |
| `report.json` 完美但 `dump.dmp` **0 字节**，助手日志 `写 dump 失败：参数错误。 (os error 87)` | panic 路径没有 `EXCEPTION_POINTERS`，我最初给了一个**全零** `CrashContext` —— `MiniDumpWriteDump` 拿到 `pid=0/tid=0` 直接拒（规格只说了「异常指针可空」，没说 pid/tid 必须真） | `panic_crash_context()`：空异常指针 + 真实 `process_id` / `GetCurrentThreadId()`（Windows） |
| 指南正文「这份报告记录的是**一起一次**没有正常关闭的退出」 | 模板写的是 `记录的是一起{what}`，而 `what` 自己已带量词 | 改成 `记录的是{what}` |

## 6. 纪律自查

* **§2.8**：单测/集成测试归 Agent（已跑）；真机崩溃、Worker 崩溃、发版包内助手 → 人类（§5 已列清单）。
* **§2.12 复用**：日志落盘/脱敏/崩溃记录各只有一份实现；助手不复用主库（体积与独立性优先，理由写在 crate 头）。
* **§2.15 命令体系**：新增 `diagnostics.openLogFolder`，**热键留空**并在 catalog 注释里写明理由（低频、命令面板可达、弱菜单不进菜单）。
* **§2.16 迁移框架**：无 schema 变化、无 `ALTER`；`session.lock` 是文件不是库。
* **§2.19 XMP**：不碰 sidecar 数据与布局 → 无影响。
* **§2.20 界面文案**：只加了一条带变量的提示（`已打开诊断目录：{path}`）。
* **§5.1 设计稿先行**：本轮**未碰界面结构**（无对话框、无面板；只有一条命令 + toast）⇒ 不需要 `.pen`；
  W3 的报告对话框与提示条开工前必须先出 `design/diagnostics-report.pen` + 同名 `.md`。
* **§5.3 构建红线**：助手进包 + `check:win` 核对；`debug:win` 保持先 `pnpm build` 的顺序不变。
* **§2.9 依赖报备**：`log` / `crash-handler` / `minidumper` 三个新依赖，规格 §11 原表已列（`flexi_logger` 不再引入）。
