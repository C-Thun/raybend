# 日志、崩溃采集与问题报告

**状态**（2026-10-12 更新）：**W1 / W2 / W4 / W3-α 已施工**（运行日志 + panic/未正常退出记录 + 独立崩溃助手 + **崩溃后弹窗/另存导出的报告包**，崔总当日指示：先把日志这套建起来；发送/接收留以后）：
选型改定：**自研轻量 writer**，不用 `flexi_logger` —— 核实源码后确认它给不了有界队列与按目录总容量清理，见 `implementations/2026-10-12_diagnostics-logs-and-crashpad.md`。
**W3 剩余（报告对话框 / 「导出诊断包」命令 / 复制报告摘要 / `mailto:` 预填）与 W5（WebView2 ProcessFailed / 前端上报 / 卡死看门狗）未开工**；**S1 服务端（T1 一键上传）仍留在 `memory/FUTURE.md`，崔总明确「发送接收以后再说」**。
**关联**：`memory/FUTURE.md`「🧊 发版后（after release）可立即做」（登记条目）与 J 节（原「日志与诊断包」半句的落点即本文件）；
`docs/privacy.md` / `docs/privacy.en.md`（发送功能落地时**同批**更新）；`AGENTS.md` §2.8 §2.12 §2.15 §2.16 §2.19 §2.20 §5.3、§5.8（日志与崩溃记录怎么看）。

---

## 0. 一句话

用户出问题（崩了、卡住、行为不对）时，我们要能拿到一份**已脱敏、可读、能定位到模块与函数行**的近因记录，
并且这份记录由**用户点一下**就能交到崔总手里 —— 全程不自动上传。

---

## 1. 现状（2026-10-10 现场勘查，本单元的前提事实）

| 事实 | 证据 | 后果 |
| --- | --- | --- |
| **没有任何日志设施** | 全仓没有 `log` / `tracing` 依赖；`crates/` + `src-tauri/` 里 120+ 处裸 `println!` / `eprintln!` | Windows GUI 无控制台，这些输出**全部进虚空** |
| **worker 的 stderr 被丢弃** | `crates/raybend/src/worker_process.rs:119` 用 `stderr(Stdio::inherit())`，同文件注释已承认「继承来的 stderr 句柄无效就静默失败」 | RAW/AI 解码进程的 panic 与报错**一条都留不下** |
| **release 是 `panic = "abort"`** | 仓库根 `Cargo.toml` `[profile.release]` | ① `catch_unwind` 全部失效 —— `src-tauri/src/editor.rs`、`crates/raybend/src/render/upload.rs` 等处「接住 panic 并重启线程」在发布版是**空操作**；② panic 直接 abort，异步日志队列来不及落盘 |
| **release 是 `strip = true`** | 同上 | `Backtrace` 抓回来只有地址、没有符号 |
| **隐私承诺已经写在文档里** | `docs/privacy.md` 首段：「应用没有遥测、账户登录、自动上传照片或自动发送诊断的机制」 | 本单元必须与之一致：**上传只能由用户点击触发**，落地时同批更新文案 |

结论：不是「要不要加日志」，是现在**一条有效凭据都没有**。

---

## 2. 目标与非目标

**目标**

1. 崩溃（panic / 硬崩溃 / 前端 / WebView2 / worker）与「卡死」都能在用户机器上留下**本地**记录。
2. 记录**可读**（用户和崔总都能看懂大概发生了什么）且**可定位**（模块 + 函数 + 行号）。
3. 容量有**硬上限**，日志永远不会吃掉用户的磁盘。
4. 用户能把一次问题的凭据**一键交给崔总**（先本地导出，后可选一键上传）。
5. 全链路**不记录用户的实际信息**（照片内容、路径、姓名、库名、标签名、搜索词…）。

**非目标**（明确不做，避免以后重吵）

* 不做遥测 / 统计 / 使用行为上报（无匿名安装 ID、无功能打点、无留存率）。
* 不做自动上传 —— 永不后台悄悄发东西。
* 不接 Sentry 等第三方崩溃平台（默认自动上传 + 数据出境，与隐私立场冲突）。
* 不采集照片内容、EXIF、GPS、相机序列号、文件哈希。
* 不做远程开关 / A-B / 动态配置。
* 不做多平台验收：**Windows 先行**；`crash-handler` 三平台都支持，但 mac/Linux 不在本单元验证范围。

---

## 3. 三层结构（唯一架构，不许长第二套）

```text
第一层  运行日志      常驻 · 轮转 · 20 MiB 硬上限 · 出口脱敏
        %LOCALAPPDATA%\com.cthun.raybend\logs\
第二层  异常采集      panic / 硬崩溃 / 前端 / WebView2 / worker / 卡死 / 未正常退出
        %LOCALAPPDATA%\com.cthun.raybend\crashes\RB-<yymmdd>-<6hex>\
第三层  报告包与提交  zip 打包 + 报告 ID + 用户点击才发送
        %LOCALAPPDATA%\com.cthun.raybend\reports\
```

依赖单向：第二、三层**只读**第一层的产物；第一层不知道第二、三层的存在。
路径全部通过 Tauri path API 解析到 app data 目录（`AGENTS.md` §6.4 的 `%LOCALAPPDATA%\com.cthun.raybend\` 家族）。

---

## 4. 第一层：运行日志

### 4.1 选型（唯一事实源）

| 层 | 选择 | 理由 |
| --- | --- | --- |
| 门面 | `log`（0.4） | 生态标准；替换现有 `println!` 的成本最低；渲染/worker 侧无异步生态负担 |
| 落盘 | **自研轻量 writer**（`crates/raybend/src/diagnostics/writer.rs`） | 崔总 2026-10-12 改定（原选型 `flexi_logger` 经源码核实**满足不了**两条硬需求）：有界队列 4096 + `try_send` + 丢弃计数、**按目录总容量** 20 MiB 清理（它只有按文件个数/天数）。它唯一满足的是按大小轮转 —— 那一段自研仅 ~250 行且有单测 |
| 排除 | `flexi_logger`（MIT/Apache） | 见上：`WriteMode::Async` 是 `crossbeam_channel::unbounded`，`Cleanup` 只有 `KeepLogFiles`/`KeepForDays` |
| 排除 | `tracing` / `tracing-appender` | 按大小轮转至今**未实现**（https://github.com/tokio-rs/tracing/issues/1940 ），满足不了容量硬上限；异步生态对桌面应用是额外负担 |
| 排除 | `tracing` + `file-rotate` | 两条腿拼装，不如 `flexi_logger` 一体；功能重叠 |

### 4.2 落点与文件布局

```text
logs/
├── raybend.log        本次会话（活动文件）
├── raybend.1.log      上一次会话（1 = 最新归档）… 到 raybend.9.log
└── raybend-trace.log  诊断模式专用池（仅诊断模式打开时存在）
```

* **单文件上限 2 MiB，保留当前 + 9 个归档 = 20 MiB 硬上限**（崔总给的 10–20 MiB 区间取宽档）。
* 清理判据是**目录总容量**（超限删最旧），不是文件个数 —— 个数控制挡不住单文件膨胀。
* 轮转时机：启动时、达上限时、会话正常结束时。
* 「上一会话」天然是**一份完整文件**，用户导出时直接取，不用切文件。
* 诊断模式池单独一个文件（单文件 5 MiB、最多 1 个），会话结束自动关闭并回落默认级别。

### 4.3 行格式（先人读，再机读）

```text
2026-10-10T22:41:03.184Z  WARN  editor.render  drain 超时  waited=2500ms frames=812 thread=render
2026-10-10T22:41:04.902Z  ERROR sidecar.sync  写出失败  err=Io(<path:.xmp#3f9a>) attempts=3
```

| 字段 | 规则 |
| --- | --- |
| 时间 | UTC ISO8601，毫秒，定长 24 字符（跨时区排查不用换算） |
| 级别 | `ERROR` / `WARN` / `INFO` / `DEBUG` / `TRACE`，5 字符定宽 |
| 目标 | `<crate>::<module>` 或进程内标签（`editor.render` / `raw-worker` / `webview` / `frontend`） |
| 消息 | 一句人话，**已脱敏**（§8），换行一律转义成 `\n` |
| 字段 | `k=v` 键值对，按 key 排序，值同样过脱敏 |
| 线程 | 只在**非主线程**追加 `thread=<name>`（线程名是我们自己取的，不含用户信息） |

* 一条事件 = 一行；只有 panic 块与报告头使用 `--- BEGIN/END ---` 包裹的多行块（解析器可整块跳过）。
* 不写 JSONL：日志首先给人看（用户会贴、崔总会在手机上扫），JSON 让两边都痛苦；机读靠定宽前缀 + `k=v` 已足够。
* 文件头（每次启动第一行）：`=== session 2026-10-10T22:38:11+08:00 v0.3.2 msi win10.0.26100 cpu=8C/16T gpu=… ram=32GiB dpr=1.375 lang=zh-CN ===`

### 4.4 级别与开关

| 场景 | 级别 |
| --- | --- |
| dev / `cargo tauri dev` | `DEBUG` |
| release 默认 | `INFO` |
| 诊断模式（用户主动打开） | `DEBUG`（可按 target 细化到 `TRACE`） |
| 环境变量覆盖（仅开发用） | `RAYBEND_LOG=<spec>` |

* 诊断模式：命令 + 设置页入口，**持久到 app.db settings KV**（键 `diagnostics.verbose`，默认 `false`，无 schema 变化），
  但**日志级别只在本会话生效**，下次启动自动回落 —— 避免用户忘了关导致长期磁盘写入。
* 渲染线程、音频/GPU 热路径的日志：**禁止写盘**，只允许 `try_send` 入队；队满即丢。

### 4.5 异步与有界（绝不阻塞）

* 单后台写线程 + **有界队列 4096 条**，`try_send` 非阻塞；队满丢弃并**计数**。
* 下一次成功写入时补一行 `dropped=N since=<ts>`，保证「丢过日志」这件事本身可被看见。
* 记录器自身 panic 必须被吞掉（重入保护）：禁止出现「日志写失败 → panic → 崩溃记录 → panic」的递归。

### 4.6 多进程与前端

| 来源 | 做法 |
| --- | --- |
| RAW/AI worker | **worker 不引入日志框架**：父进程把 `stderr` 从 `inherit` 改为**管道捕获**，逐行加 `[raw-worker]` 标签写进同一份日志；worker 侧只保证「stderr 是唯一日志出口」+ panic hook 打单行 |
| 前端 JS | `window.onerror` / `unhandledrejection` / Solid `ErrorBoundary` → 一条 IPC 上报命令；**限流**：≤20 条/分钟，按 (message, 首帧栈) 去重计数，超限只累加计数 |
| WebView console | 默认不进日志；诊断模式下可开（同一限流器） |
| IPC 错误 | 前端已有的错误分支不额外加日志（Rust 侧本来就会记），避免同一条错误双写 |

---

## 5. 第二层：异常采集

| # | 类型 | 采集点 | 关键设计 |
| --- | --- | --- | --- |
| 5.1 | Rust panic | `std::panic::set_hook`（进程级一处） | **同步写**（绕过异步队列）+ `Backtrace::force_capture()` + 内存里最近的日志尾部 200 行 |
| 5.2 | 硬崩溃（段错误/驱动崩/栈溢出/abort） | `crash-handler` + `minidumper` 的**独立助手进程** | 助手是单独二进制 `raybend-crashpad`，**不是 re-exec 主进程** |
| 5.3 | 前端 JS 崩溃 | §4.6 的上报命令，级别 `ERROR` | 限流 + 去重（见 §4.6） |
| 5.4 | WebView2 进程崩溃 | 取 Tauri 的 WebView `controller()` 挂 `ProcessFailed` | 白屏/卡死最常见的真凶；顺手加**自动重载**恢复 |
| 5.5 | worker 子进程崩溃 | 已有 `WorkerError::Crashed(exit)` | 补记退出码 + 该 worker 的 stderr 尾段（§4.6 的管道已有） |
| 5.6 | 卡死（不是崩溃） | UI 心跳 + 渲染线程心跳 | 5 s→`WARN`，15 s→`ERROR` + 主动请求一次 dump；用户也可手动「现在卡住了」 |
| 5.7 | 兜底：未正常退出 | 启动写 `session.lock`，正常退出删除 | 下次启动发现残留 → 判定「上次未正常退出」，取日志尾 200 行生成一条 crash 记录 |

### 5.1 panic 细节

* `panic = "abort"` 下 hook **仍会先跑**，所以同步写是可行的；**必须在 hook 内 `flush` 后返回**再让进程 abort。
* 记录内容：panic payload（字符串化 + 脱敏）、位置（文件:行，文件是**我们自己的源码路径**，可以留）、线程名、backtrace。
* 同一次会话重复 panic 去重（同一位置 + 同一 payload 只记一次，后续只累加 `count`）。

### 5.2 硬崩溃细节

* 助手进程：`crates/raybend-crashpad`（新 bin）或 `crates/raybend/src/bin/raybend-crashpad.rs`；由主进程启动并持有管道；主进程死时它落
  `crashes/RB-…/{report.json, dump.dmp}`。
* **为什么不用 re-exec 自己的做法**：`tauri-plugin-sentry` 的 minidump 特性就是 re-exec 主程序，社区已有「连续三次把启动打死」的记录（https://github.com/readest/readest/commit/c37d7eb2b37184f20e57d202045fb8379e4b2e2a ）——独立小二进制没有这个失败模式。
* dump 档位：**线程栈 + 模块表，不含全量内存**（`MiniDumpNormal` + 必要的间接引用），典型 1–10 MiB，同时把隐私面压到最小。
* 助手二进制必须进安装包：`src-tauri/tauri.conf.json` 的 `bundle`（`externalBin` 或 `resources`），**NSIS 与 MSI 都要带**；
  `pnpm check:win` 要新增一项「包内存在 `raybend-crashpad.exe`」的核对（`AGENTS.md` §5.3 的红线：产物时间与内容核对）。
* 依赖：https://crates.io/crates/crash-handler 、https://crates.io/crates/minidumper （Sentry 出品，MIT/Apache，**不需要服务器**）。

### 5.4 WebView2 细节

* 事件枚举：`BrowserProcessExited` / `RenderProcessExited` / `RenderProcessUnresponsive` / `GpuProcessExited` / `UtilityProcessExited`…
  映射到日志级别：`RenderProcessExited`·`GpuProcessExited` → `ERROR`；`Unresponsive` → `WARN` 并计时（进 §5.6）。
* 参考：https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/process-related-events
* 恢复动作：`RenderProcessExited` 后自动 `reload()` 一次（限一次/会话，防崩溃循环）；失败则显示「界面进程已崩溃，请重启」的兜底提示。
* 实现通道：`src-tauri` 已依赖 `webview2-com = 0.38.2`，走 Tauri 的 `with_webview(|w| …)` 拿 controller，**不新增依赖**。

### 5.6 卡死阈值

| 阈值 | 动作 |
| --- | --- |
| 5 s 无心跳 | `WARN`，记当前模块与最近一次心跳间隔 |
| 15 s 无心跳 | `ERROR` + 请求一次 dump（`minidumper` 的主动 dump 能力）+ 落一条 crash 记录（`kind=hang`） |

---

## 6. 第三层：报告包（提交凭据）

### 6.1 结构

```text
RB-20261010-3F9A21.zip
├── manifest.json        机器读的元数据（见 6.2）
├── summary.md           人读一页：发生了什么、怎么退出的、日志尾部 50 行
├── logs/raybend.log     当前会话（已脱敏）
├── logs/raybend.1.log   上一次会话（已脱敏）
├── logs/build-prev.log  前两次会话各取尾部 200 行（体积保护）
└── crash/report.json    崩溃类型、线程、panic 消息、backtrace（已脱敏）
    crash/dump.dmp       ← 可选，用户勾选才带
```

* 用户可选填**一句话现象描述**（≤500 字符）→ 进 `manifest.json.notes[]`。这是最有价值的字段，但它是用户输入，
  必然可能含个人信息 → **发送前预览里原文展示**，由用户自己决定。
* 报告 ID：`RB-<yymmdd>-<6 位十六进制>`，纯随机、**不关联任何安装身份**（本单元不引入持久安装 ID）。
* 用户侧凭据用法：对话框里显示 + 一键复制；用户贴到论坛/群里即可，崔总按 ID 搜邮箱。

### 6.2 `manifest.json`（schema v1，字段即契约）

```json
{
  "schema": 1,
  "reportId": "RB-20261010-3F9A21",
  "createdAt": "2026-10-10T14:33:02.184Z",
  "kind": "panic|abort|hang|webview|worker|manual",
  "app": { "version": "0.3.2", "channel": "msi", "build": "9f3c1ab", "profile": "release" },
  "os":  { "family": "windows", "version": "10.0.26100", "locale": "zh-CN", "dpr": 1.375 },
  "hw":  { "cpuThreads": 16, "ramGiB": 32, "gpu": "NVIDIA GeForce RTX 3070", "gpuBackend": "dx12" },
  "session": { "id": "s-7c1a", "durationMs": 181234, "crashed": true },
  "includes": { "logs": ["raybend.log", "raybend.1.log"], "dump": "crash/dump.dmp", "dumpBytes": 4312011 },
  "notes": ["打开编辑界面后拖动亮度滑杆时崩了"]
}
```

* **刻意不记**：机器名、用户名、域名、卷序列号、`FileId`、安装路径、库路径、照片名、本地 IP。
* `session.id` 是**会话内随机**，只用于把同一会话的日志/记录串起来。

### 6.3 容量与清理

| 目录 | 上限 | 清理时机 |
| --- | --- | --- |
| `logs/` | 20 MiB | 轮转时 |
| `crashes/` | 最近 5 份 / ≤50 MiB | 启动时 |
| `reports/` | 最近 10 份 / ≤100 MiB | 启动时（另提供「清空报告」命令） |

---

## 7. 提交与发送

### 7.1 通道评估结论（2026-10-10 调研）

| 通道 | 结论 | 依据 |
| --- | --- | --- |
| GitHub 直传附件 | **不可行** | 官方 REST/GraphQL 都没有附件 mutation（https://github.com/orgs/community/discussions/46951 ）；2026-08 有人摸出未公开端点但**必须带 token**（https://island94.org/2026/08/programmatically-upload-attachments-to-github-issues ）。令牌进客户端 = 把仓库写入能力发给全世界 |
| GitHub 预填 Issue | **可做，但只当附加通道** | `issues/new?title=…&body=…` 受 URL 长度限制（日志贴不全）；且**公开 Issue 永久公开**，只放摘要 + 报告 ID |
| 客户端内置 SMTP 直发 | **不做** | 客户端里的凭据等于公开；邮箱风控与退信全是自找的 |
| 自建接收服务 + 服务端发信 | **采用（T1）** | 凭据只活在服务器上；可限流、可配额、可留证据；服务器是崔总的 Azure VM |
| 第三方表单服务（Formspree/Web3Forms） | 不用 | 同样是把 key 公开 + 受第三方隐私政策约束，不比自建 20 行代码省事 |

### 7.2 T0：本地导出（**必做下限**，零基础设施）

* 「导出诊断包」→ 生成 zip 到 `reports/`，弹窗给出路径 + 报告 ID。
* 「打开日志目录」（Explorer 定位）。
* 「复制报告摘要」→ 短文本（版本 + 报告 ID + 现象），适合贴到公开场合：
  `RayBend 0.3.2 · RB-20261010-3F9A21 · 编辑界面拖动亮度滑杆时崩溃`
* `mailto:raybend+andares@outlook.com` 预填主题/正文（**提示用户手动附上 zip** —— `mailto:` 协议不允许带附件）。
* `raybend.exe --export-support-bundle`：**界面根本起不来时**也能出包（直接把 zip 路径写到 stdout + 打开资源管理器）。

### 7.3 T1：一键上传（服务端契约，**本文件为唯一事实源**）

```http
POST /v1/reports
  X-RayBend-Report-Id: RB-20261010-3F9A21        # 幂等键
  X-RayBend-Token:     <可公开的上传令牌>
  X-RayBend-Client:    0.3.2/msi                  # 版本/渠道
  X-RayBend-Kind:      panic|abort|hang|webview|worker|manual
  Content-Type:        application/zip
  Content-Length:      1234567                    # 上限 20 MiB
  body: 报告包 zip 原始字节（不用 multipart —— 省掉一整类解析漏洞）
```

| 响应 | 含义 |
| --- | --- |
| `201 {"id":"RB-…","received":"…Z"}` | 已收妥 |
| `200` | 同 ID 重传（幂等，不重复入库） |
| `400` | 缺头 / 非 zip 魔数 |
| `401` | 令牌无效 |
| `413` | 超过 20 MiB（**先看 Content-Length 直接拒**，不读 body） |
| `429` | 限流，带 `Retry-After` |
| `503` | 容量满 / 维护中 |
| `GET /healthz` | `200`，零信息量 |

服务端判定顺序（任一步不过立刻断开，**不留残片**）：
TLS → 令牌白名单 → `Content-Length` 上限 → 流式落临时文件时**再卡一遍字节数**（防 chunked 与伪造长度）→ zip 魔数 →
`fsync` → 原子改名 → 追加 `index.jsonl` → 异步通知。

### 7.4 客户端上传行为

* 上传在**后台线程**（不阻塞 UI），单次请求超时 60 s。
* 只重试 1 次（指数退避），仍失败**不静默** → 明确提示 + 落到「本地 zip + mailto/打开文件夹」。
* 上传前必须过**预览 + 明确同意**；同意范围仅本次（不提供"以后自动发"开关，除非崔总以后另行拍板）。
* 网络白名单只放一个域名（Tauri http 权限），不带任何自定义 UA 标识以外的东西。
* 记 `status` 与我们自定义的错误码（`retryAfter` 等）；响应体是我们自己的，可以进日志（仍过脱敏）。

### 7.5 待定项（崔总已明确「到时再敲定」，不阻塞本规格）

| 待定 | 影响 |
| --- | --- |
| 域名（建议 `reports.cthun.com`，需 A 记录 + 80/443） | 上传端点与 TLS |
| 是否**另起一台** Azure VM（若与代理同机，IP 可能已被风控 → 国内到达率） | 部署位置 |
| 邮件通道：专用邮箱 SMTP 应用密码 / 事务 API（https://resend.com/ 、https://www.brevo.com/ ）/ 自建 relay | 通知实现 |
| 服务端栈：Rust + axum（推荐，单静态二进制）/ Bun + Elysia / Python FastAPI | 私有仓结构 |
| 报告保留：VM 磁盘 90 天 / 2 GiB（推荐）＋ 是否同步 Azure Blob 归档 | 容量策略 |

---

## 8. 脱敏（本单元最该花力气的地方）

### 8.1 原则

1. **中心化**：脱敏是 sink 层的**唯一一道**闸门（`diagnostics/redact.rs`），所有写入必须先过它 ——
   不指望 120+ 个调用点自觉（这也正是必须中心化的理由）。
2. **确定性**：同一条路径在同一个报告里永远映射到同一个 token（便于对齐日志行）。
3. **不可逆**：token 里的短哈希用**每次生成报告时随机的盐**（盐进 `manifest.json`）——同一用户在两次报告之间不可关联。

### 8.2 映射表

| 原始形态 | 记成 | 说明 |
| --- | --- | --- |
| `C:\Users\某某\Pictures\2024\a.jpg` | `<path:.jpg#3f9a>` | 只留扩展名；`#` 后是本次报告的盐化短哈希 |
| 库根 | `<repo>` | |
| 照片 | `<repo>/<photo:12>` | 会话内序号，可跨日志行对齐 |
| 用户名 / 机器名 / 域名 | `<user>` / `<host>` / `<domain>` | |
| NAS / UNC 路径 | `<unc>` | 主机名也不留 |
| 盘符卷 | `<vol>` | 不记卷序列号 |
| 邮箱 / IP / URL | `<email>` / `<ip>` / `<url>` | URL 连 host 一起替 |
| LUT / 预设 / 标签 / 集合名 | `<name>` | 名字里可能含真名 |
| 搜索词 / 自由文本 | `<text:8>` | 只留长度 |
| EXIF / GPS / 相机与镜头序列号 / 内容哈希 | **不记** | 不是"脱敏"，是根本不采集 |
| 我们自己的源码路径（panic 位置） | 原样保留 | 那是代码信息，不是用户信息 |

### 8.3 三道保险

1. **sink 层自动脱敏**（8.1）—— 兜住所有既有调用点与 panic payload。
2. **发送前预览** —— 对话框里展示将发送的全部文本，用户能自己删/取消。这比任何正则都可靠。
3. **金丝雀单测** —— 一组真实形态的语料（中文路径、UNC、带 emoji 的文件名、`用户名@域名`、嵌入路径的 panic 消息…）
   断言输出中**不出现**任何原串片段。

### 8.4 明确承认的边界

* 用户自己填的「现象描述」是**用户输入**，不脱敏（要脱敏就没法看了）；靠预览 + 用户自己决定。
* 崩溃 dump 是内存快照，理论上可能含已解码像素片段 → 因此 dump **单独勾选**、默认关闭，并在对话框里写清风险。
* 不保证 100% 干净（用户可能把真名写进预设名）→ 所以「不自动上传」这条红线比任何技术手段都重要。

---

## 9. 界面与命令体系（`AGENTS.md` §2.15）

| 命令 id | 标题 | `defaultKey` | 理由 |
| --- | --- | --- | --- |
| `help.reportProblem` | 报告问题… | **留空** | 低频动作，菜单 + 命令面板可达；占全局热键不值当 |
| `diagnostics.exportBundle` | 导出诊断包… | 留空 | 同上 |
| `diagnostics.openLogFolder` | 打开日志目录 | 留空 | 同上 |
| `diagnostics.copyLastReport` | 复制最近报告摘要 | 留空 | 同上 |
| `diagnostics.setVerbose` | 诊断模式（开关） | 留空 | 诊断期间在用，不需要热键 |

* 菜单归属：`help.reportProblem` 进标题栏菜单的「帮助」；`diagnostics.*` 只留命令面板（**弱菜单**原则）。
* 崩溃后提示：**下一次启动**时一条轻提示（不夺焦点、可关闭、同一次崩溃只提示一次），带「查看详情 / 导出诊断包」；
  会话内可恢复的崩溃（前端、渲染线程）就地提示，不重启应用。
* 界面文案：不写固定说明句（§2.20）；只写**带变量**的（版本号、报告 ID、大小、错误原因）。
* 设计稿先行（§5.1）：开工时先出 `design/diagnostics-report.pen` + 同名 `.md`（报告对话框 + 崩溃提示条 + 设置页「诊断」区）。

---

## 10. 服务端（新私有仓；崔总 2026-10-10 定：私有仓是给**服务端程序**用的，不放日志）

### 10.1 归属

| 归属 | 放什么 | 不放什么 |
| --- | --- | --- |
| raybend 公开仓（AGPL） | 客户端上传逻辑、**线协议契约**（§7.3）、脱敏规则、隐私文案 | 任何密钥、任何服务端代码 |
| 新私有仓（崔总） | 接收服务、通知器、保留策略、systemd/Caddy 配置、`config.example` | **真实凭据**、任何用户报告 |
| Azure VM 磁盘 | 报告 zip、`index.jsonl`、服务日志 | 不进任何 git 仓 |

* 凭据红线：放 `/etc/raybend-reports/env`（`0600`）或 systemd `LoadCredential`，**不进 git（私有仓也不例外）** ——
  私有仓会 clone 到第二台机器、将来可能转公开、git 历史擦不干净。
* 报告数据不进仓：二进制 zip 会让仓库随历史无限膨胀，且用户数据不该和代码混住。浏览用 SSH/rsync（后续可选同步 Azure Blob）。
* AGPL 边界：服务端是**独立进程、不链接 raybend 代码**，可以私有；若将来把服务端代码放进公开仓，它自动受 AGPL —— 画面由崔总决定。

### 10.2 组成

```text
Caddy（TLS + 第一层 body 上限/速率）  →  axum 接收服务（单静态二进制，systemd 常驻）
                                          ├── 落盘 /srv/raybend-reports/inbox/YYYY-MM/
                                          ├── 追加 index.jsonl
                                          └── 投递通知任务 → notifier（SMTP 或 API，带退避重试，失败落 pending/）
保留 GC（90 天 / 2 GiB，80% 发告警邮件）
```

* **永不解包、永不经 HTTP 读回**（目录没有静态服务）；客户端文件名一律丢弃，落盘名完全由服务端生成
  `YYYY-MM/<reportId>-<8 位随机>.zip`。
* `index.jsonl` 每行：报告 ID、收到时间、版本、渠道、OS、kind、字节数、**fingerprint**、通知状态 ——
  有了 fingerprint 就能一眼看出「同一个崩溃 37 个人撞上了」，不用逐个下载。

### 10.3 限流与配额

| 维度 | 阈值 | 超出 |
| --- | --- | --- |
| 单请求大小 | 20 MiB | `413` |
| 每 IP | 10 次/时、30 次/日 | `429` + `Retry-After` |
| 全局 | 500 份/日 或 2 GiB/日 | `503`（并给崔总发一封告知邮件） |
| 磁盘 | 90 天 / 2 GiB | 同上 |

令牌在客户端里**必然可被提取**，所以定位是「入场券」（拦随手扫端口的脚本、可随时轮换），
真正的闸门是**限流 + 配额 + 容量 + 通知聚合**。

### 10.4 通知邮件

* 发送账号**专用**（最小权限、只发不收）；收件人是崔总主邮箱；凭据只活在服务器。
* 主题：`[RayBend] RB-20261010-3F9A21 崩溃 v0.3.2`；正文 = manifest 摘要 + 日志尾 50 行（手机可初判）；
  **zip ≤15 MiB 直接附上**，超了只提醒。
* 防炸：单小时 >10 封自动合并成摘要；每天最多 1 封批量汇总；发送失败落 `pending/` 退避重试（**不丢报告**）。
* 事实提醒：Azure VM 出站 **25 端口基本被封** → 只能走 465/587 或服务商 API；自有域名发信建议配 SPF/DKIM，否则进垃圾箱。

### 10.5 部署加固清单

* systemd：`User=raybend-reports`、`NoNewPrivileges`、`ProtectSystem=strict`、`ProtectHome`、`PrivateTmp`、
  `ReadWritePaths=/srv/raybend-reports`、`MemoryMax`、`CPUQuota`。
* 网络安全组：只开 80/443（ACME + 服务）+ SSH；SSH 只密钥；开自动安全更新。
* Caddy 自动 TLS：https://caddyserver.com/
* 部署手册（`install.sh` + Caddyfile + systemd unit + `config.example`）写进私有仓 `docs/deploy.md`。

---

## 11. 工程接入与复用（`AGENTS.md` §2.12）

| 能力 | 唯一实现点 | 禁止 |
| --- | --- | --- |
| 日志初始化 / sink | `crates/raybend/src/diagnostics/log.rs`（不依赖 tauri）+ `src-tauri` 只管路径注入与启动 | 第二套 logger |
| 脱敏 | `crates/raybend/src/diagnostics/redact.rs` | 各调用点自行脱敏 |
| 报告包生成 | `crates/raybend/src/diagnostics/bundle.rs` | CLI 与 GUI 各写一份（`--export-support-bundle` 复用同一个） |
| 上传 | `src-tauri/src/diagnostics/upload.rs` | 前端自己发 HTTP |
| 崩溃记录 | `panic_hook` + `crashpad` 助手，共用一个 `crash_record` 写入函数 | 各处 `catch_unwind` 各写一份 |
| 命令接入 | 既有 `src/features/commands/catalog.ts` | 新开命令通道 |

**新增依赖**（`AGENTS.md` §2.9 先报备）

| 依赖 | 用在哪 | 说明 |
| --- | --- | --- |
| `log` + `flexi_logger` | 客户端日志 | 新增 |
| `zip` | 报告包 | **已在 `Cargo.lock`**（传递依赖），只需显式声明 |
| `reqwest` | 上传 | **已在 `Cargo.lock`（0.13.5）**，只需在 `src-tauri` 显式声明 |
| `crash-handler` + `minidumper` | 硬崩溃采集（第二波） | 新增，MIT/Apache |
| （服务端）`axum` 等 | 私有仓 | **不进 raybend 依赖树** |

**不碰的东西**：编辑器 `confirmEdit` 快照纪律（§2.21）、XMP 写出链路（§2.19）、渲染线程的像素/色彩所有权（§6.1 三条红线）、数据库迁移框架（§2.16：本单元无 schema 变化）。

---

## 12. 波次划分

| 波 | 内容 | 依赖 | 能否独立上线 |
| --- | --- | --- | --- |
| **W1** | 日志底座：`log` + `flexi_logger` + sink 脱敏 + 轮转 + 会话头 + worker stderr 管道接回 + 关键路径 `println!` 替换 | 无 | ✅ |
| **W2** | panic hook + 同步崩溃记录 + `session.lock` 未正常退出兜底 + 崩溃提示条 | W1 | ✅ |
| **W3** | 报告包生成 + 报告对话框（`.pen` 先行）+ 命令接入 + `--export-support-bundle` + T0 导出/邮件/复制 | W1 W2 | ✅ |
| **W4** | 硬崩溃：`raybend-crashpad` 助手 + dump 档位 + 打包接入 + `check:win` 核对 | W2 | ✅ |
| **W5** | WebView2 `ProcessFailed` + 自动重载 + 前端上报限流 + 卡死看门狗 | W1 | ✅ |
| **S1** | 服务端（私有仓）+ 客户端 T1 一键上传 + 隐私文案同批更新 | W3 | ✅（服务端可先上线） |

---

## 13. 验收清单（开工后逐项勾）

**Agent 可自测（冒烟）**

1. 跑一次完整启动/退出 → `logs/` 有本次会话文件 + 会话头 + 至少 N 行可读日志，无 `println!` 残留路径。
2. 目录总容量：灌满归档 → 总和不超 20 MiB，最旧的先被删。
3. worker 崩溃（人为 kill）→ 日志里有 `[raw-worker]` 尾段 + 退出码。
4. 人为 panic（dev 构建）→ crash 记录含位置 + backtrace；日志尾部 200 行被截取。
5. 未正常退出（人为 `kill -9` / 任务管理器结束）→ 下次启动生成「上次未正常退出」记录 + 轻提示一次。
6. 脱敏金丝雀语料 → 全部通过（无原串泄漏）。
7. 报告包生成 → zip 结构、`manifest.json` 字段、`summary.md` 可读、体积符合预期。
8. 上传：本地起 mock 服务端 → 201/409/413/429 各分支行为正确，失败降级到本地导出。
9. `raybend.exe --export-support-bundle` 在界面起不来的情况下也能出包。

**需要崔总真机确认（E2E，`AGENTS.md` §2.8）**

10. 真实崩溃路径：`panic = "abort"` 下点一次真崩溃 → 助手进程落出 dump + 重启后提示条出现。
11. WebView2 渲染进程崩溃（可用任务管理器结束其子进程）→ 记录正确 + 自动重载恢复。
12. 真实上传链路（域名/证书/邮件到达）→ 邮件内容、附件、报告 ID 全链路对得上。
13. 隐私抽查：随机抽 3 份报告，人工确认看不到用户的真实路径/姓名/库名。

**开工前必须先定的两件事**（属决策，不是细节）

* release profile 的 `panic`（改 `unwind` 让既有崩溃自恢复生效，还是接受"panic 即硬崩溃"）与 `strip`/`debug`（留行号表 vs 发版另存 `.pdb`）——
  这两条**改变 5.1/5.2 的实现路径**，必须先拍。
* T1 的 §7.5 五项待定。

---

## 14. 风险与对策

| 风险 | 对策 |
| --- | --- |
| 脱敏漏了某个新调用点 | 中心化 sink（唯一闸门）+ 金丝雀单测 + 发送前预览 |
| 日志本身成为性能瓶颈 | 有界队列 + 非阻塞 + 渲染线程禁写盘 + 丢弃计数可见 |
| 崩溃助手拖慢启动 / 打不开 | 独立小二进制（非 re-exec）+ 启动失败静默降级（没有 dump 也还能跑） |
| 报告里混进用户数据 | 不自动上传 + 预览 + dump 默认不带 + 隐私文案同步 |
| 上传端点被灌 | 令牌可轮换 + 双层限流 + 全局配额 + 通知聚合 + 磁盘上限 |
| 用户忘了关诊断模式 | 级别只在本会话生效，偏好不影响下次启动 |
| 发版包漏了 crashpad | `check:win` 增加「包内存在助手二进制」核对项 |

---

## 15. 纪律自查

* **§2.8 测试分工**：单测（脱敏金丝雀、轮转、幂等、限流）归 Agent；真崩溃、WebView2、真实上传一律崔总真机。
* **§2.12 复用**：所有能力单点实现（§11 表）；CLI 与 GUI 共用同一个打包函数。
* **§2.15 命令体系**：5 条命令**全部留空热键**并写明理由（§9）；接入既有 catalog。
* **§2.16 迁移框架**：无 schema 变化；诊断模式偏好走 `app.db` settings KV（现有 `setting_get/set`）。
* **§2.19 XMP 联动**：本单元不写 XMP、不改存储布局 → **无影响**；报告包不进库根。
* **§2.20 界面文案**：只写带变量的句子（版本/报告 ID/大小/原因），不做说明书。
* **§2.21 编辑器快照**：不涉及（不碰 `confirmEdit`、不在拖动路径上落库）。
* **§5.1 设计稿先行**：报告对话框与提示条开工前出 `.pen` + `.md`。
* **§5.2 实施记录**：开工后每波一份 `implementations/`；本次定档记录见 `implementations/2026-10-10_logging-and-crash-report-spec.md`。
* **§5.3 构建红线**：助手二进制进包 + `check:win` 核对；构建顺序（先 `pnpm build`）不变。

---

## 16. 参考（本轮调研，全部为纯文本 URL）

* GitHub 无官方附件上传接口：https://github.com/orgs/community/discussions/46951
* GitHub 未公开附件端点（需 token）：https://island94.org/2026/08/programmatically-upload-attachments-to-github-issues
* minidump 助手 re-exec 导致启动连崩的实例：https://github.com/readest/readest/commit/c37d7eb2b37184f20e57d202045fb8379e4b2e2a
* `crash-handler`：https://crates.io/crates/crash-handler ；`minidumper`：https://crates.io/crates/minidumper
* `flexi_logger`：https://crates.io/crates/flexi_logger
* `tracing-appender` 按大小轮转未实现：https://github.com/tokio-rs/tracing/issues/1940
* WebView2 进程相关事件：https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/process-related-events
* Caddy：https://caddyserver.com/ ；事务邮件 API：https://resend.com/ 、https://www.brevo.com/
