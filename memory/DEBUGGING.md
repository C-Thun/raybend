# 排查工具（memory/DEBUGGING.md）

> 项目提供两件代码之外的工具，本文件写清它们是什么、需要时怎么用、边界在哪：
>
> - **真机直连（CDP）**：在正在运行的 Windows 应用里看 DOM、拦 IPC、驱动界面；针对特定难题的深入排查，不是常态。
> - **日志与崩溃记录**：日志的位置与判读、崩溃（panic / 硬崩溃）的辨认、错误报告导出。
>
> 工具补充的是代码与需求之外的事实：运行期行为、日志、崩溃现场。验收标准见 AGENTS.md §2.8（单元测试）。
>
> 2026-10-12 自 AGENTS.md §5.7 / §5.8 外迁；功能细节与排障内容保持。

## 1. 真机直连调试（CDP）

用途：在正在运行的 Windows 应用里看 DOM、拦 IPC、驱动界面，供特定难题的深入排查。
**不许自己搭隧道**——仓库里已有授权好的直连通道（2026-10-11 教训：绕了一圈反向隧道，其实早有现成的；`rbrelay` 那套已删）。

1. **方案与公共工装**：`scripts/perf-win.mjs` 头部「连接方式」一节、`scripts/lib/cdp.mjs`。
2. **拉起带调试口的 exe**：`pnpm perf:win --launch` 代劳了 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333`
   经 `WSLENV` 透传与直连候选轮询。手工起也一样：
   `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9333' WSLENV=WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS <exe>`。
3. **一次性授权**：本机已做过 `netsh portproxy` 9334 → 127.0.0.1:9333，防火墙只放行 WSL 子网 172.16.0.0/12（原文命令在 `perf-win.mjs` 头部）。授权还在就直接用；WSL 侧连 `http://<默认网关>:9334/json/list`。
4. **拦 IPC 的窍门**：Tauri v2 的 IPC 是 `fetch("http://ipc.localhost/<cmd>")`，不是 `chrome.webview.postMessage`；`window.__TAURI_INTERNALS__.invoke` 是 `{writable:false, configurable:false}`，patch 不了。要计时就拦 `window.fetch`，记 url / 耗时 / 成败 / 回包字节。
5. **纪律**：诊断只看不改；拉起的实例会锁住 exe，挡住下一次 `debug:win`，收工时记得收；别拿探针的「空闲 + 热缓存」数字当用户体感——真机首进目录是冷读，且同时在跑别的命令，场景不同结论就不同。
6. **排障**：`netstat -ano | findstr 9333` 没有输出说明调试口没开，多半已有别的实例占着 WebView2 进程，关掉占用实例再拉；连不上时确认 exe 是带参数起的、`netsh portproxy show v4tov4` 里 9334 那一行还在。

## 2. 日志与崩溃记录

出问题、需要证据时看这两个目录。Windows 下 `<数据目录>` = `%LOCALAPPDATA%\com.cthun.raybend`：

```text
<数据目录>\logs\raybend.log        本次会话（上一会话是 raybend.1.log，直到 .9；2 MiB/个，目录总上限 20 MiB）
<数据目录>\crashes\RB-<日期>-<短码>\  一次崩溃一个目录
    report.json / report.txt        崩溃记录：类型 / panic 消息与位置 / 线程 / backtrace / 日志尾 200 行
    dump.dmp / dump.json            硬崩溃的 minidump（助手进程写；report 里的 panic 那份没有 dump 说明助手不在）
```

**怎么读**：

* 第一行是**会话头**：`=== session <本地时间+偏移> <版本> <dev|release> <os-arch> cpu=<N>t pid=<pid> id=<会话 id> ===`。
  日志行一律 **UTC 毫秒**（`2026-10-12T01:39:51.123Z`），对本地时间要 **+8 小时**，会话头与它一对照就能定位。
* 行格式：`<UTC 时间>  <级别>  <目标>  <消息> [ thread=<线程> ]`；`目标` 是模块或进程标签
  （`editor`、`sidecar`、`db`、`raw-worker`/`ai-worker` 为子进程 stderr、`crashpad` 为崩溃助手）。
* **dev 构建默认 DEBUG、发布版 INFO**；临时要更细：`RAYBEND_LOG=debug` 或 `trace` 启动。
* **日志已脱敏**：路径是 `<path:.jpg#3f9a>`、UNC 是 `<unc>`、邮箱/IP/URL 也是占位符；
  代码位置（`crates/.../x.rs:42`）**保留原样**，所以日志可以直接贴出来。
* **崩溃了怎么判**：
  1. `crashes/RB-*` 里有 `report.json` → 是 **panic**（看 `panic.location` 与 `logTail` 的最后几行）；
  2. 有 `dump.dmp` 但没有 report → **硬崩溃**（访问违例/栈溢出等，进程来不及写字）；
  3. 什么都没有，但上次启动时日志末尾有一行 `上次会话未正常退出` → 进程被外部杀掉（任务管理器/断电）
     或崩得太早（助手与 hook 都还没装上，例如启动第一秒）。
* **worker 崩了**：主日志里搜 `[raw-worker]` / `[ai-worker]`（worker 的 stderr 按行转进同一份日志，含它的 panic）。
* **崩溃后第二次启动会弹一次窗**（崔总 2026-10-12）：点「保存报告…」经系统另存为产出
  `RayBend-错误报告-RB-*.zip` + 《RayBend-错误报告提交指南.txt》，指南里有 GitHub issue 与邮件 `raybend+andares@outlook.com`。
  点「不报告」/ Esc / 点遮罩 → 该崩溃记录被标记，下次启动不再问同一条，只有新的崩溃才会再问；
  在系统另存为里取消 → 不标记，下次启动还会问一次，因为用户本意是要存的。
  报告包里的 `report.json` / 日志都已脱敏；**内存转储默认不带**，需要时在弹窗里勾选，可能含图像数据片段。
* 日志/崩溃目录满了会自己清，只留最近的。出问题时把整个 `RB-*` 目录或整份 `raybend.log` 发给定位的人即可，里面没有路径、名字与文件内容。

**没装成的东西**：「一键发送（T1 自建接收服务）」「复制报告摘要」「`mailto:` 预填」与报告对话框的完整版
属第三层（W3/S1，登记在 `memory/FUTURE.md`）。现在只能自己存包、自己发。规格总表：`specs/logging-and-crash-report.md`。

**要一次验证整条链**：`pnpm debug:win && pnpm check:diagnostics --reset`。脚本会自己启动/硬杀/重启，
验七件事：会话头 → 助手就位 → 硬杀后能被检测到 → 导出 zip+指南 → 不再重复问 → 优雅关闭删锁 → panic 落 report+dump。
