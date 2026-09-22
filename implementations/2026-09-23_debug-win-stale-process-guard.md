# debug:win 加残留进程预检（os error 5 排障）

完成时间：2026-09-23 03:42:38 CST

## 起因

崔总跑 `pnpm debug:win`，第 ② 步 cargo 挂在：

```text
error: failed to remove file `C:\rb-target\raybend\debug\raybend-desktop.exe`
Caused by:
   拒绝访问。 (os error 5)
```

原话是「我明明应用关着没开啊」—— **窗口确实关着，进程却活着**。

## 排障证据（全部本机实测）

| 项 | 值 |
| --- | --- |
| PID | 37420 `raybend-desktop.exe` |
| 路径 | `C:\rb-target\raybend\debug\raybend-desktop.exe`（文件 mtime 9/21 00:50） |
| 启动时间 | 2026/9/21 11:14:38 |
| 父进程 | 11480 —— **已不存在**（孤儿进程） |
| 线程 / 句柄 / 工作集 | 5 个线程（全 Wait）/ 252 / 2.4 MB |
| 顶层窗口 | 只有 `Tao Thread Event Target`（tao 的事件循环消息窗，可见但**标题为空**）+ 一个隐藏 `IME` 窗；应用界面窗（main / splash）**一个都不存在** |
| 子进程 | 无 `msedgewebview2`（但 `EmbeddedBrowserWebView.dll` 还加载着） |
| CDP 端口 | 没占 :9333 |
| 文件占用 | 对该 exe 做独占打开 → 拒绝访问（**它就是锁的主人**） |

结论：**进程活着、界面早没了**。cargo 链接前要删旧 exe，删不掉 → os error 5。`taskkill /F /PID 37420` 之后独占打开立刻成功，构建随即通过。

来历推断：9/21 那轮 perf-win 真机采样（`implementations/2026-09-21_perfwin-*.md` 三篇）——
`scripts/perf-win.mjs:launchExe()` 用 `spawn(EXE, { detached: true, stdio: "ignore" })` + `unref()`
拉起 exe，跑完**从不回收**（设计如此：下次「复用现有实例，不重复拉起」），于是它就一直挂在后台。

## 改动

**文件：`scripts/debug-win.mjs`**（唯一改动文件，+约 130 行）

新增步骤 **⓪ 残留进程预检**，排在 `pnpm build` 之前：

1. PowerShell 探针（一次调用问清三件事）：所有 `raybend-desktop.exe` 进程、它们名下的**全部顶层窗口**（类名 / 是否可见 / 标题长度）、目标 exe 是否被独占占用。
   - 走 `-EncodedCommand`（UTF-16LE base64）：绕开 WSL→Windows 的引号/转义地雷（§5.3 第 3 条）；
   - `$ProgressPreference = 'SilentlyContinue'`：否则 stderr 非控制台时 PS 5.1 会把进度记录序列化成 CLIXML 混进输出（实测踩到）；
   - `[Console]::OutputEncoding = UTF8`：否则中文经 WSL 回来是乱码；
   - 输出只认标记 `<<<RB-PROBE>>>` 之后的 JSON，中文一律由 JS 侧打印 —— 不受控制台代码页影响。
2. 判定与动作：
   - 有**可见且类名不是内部消息窗**的顶层窗 → 应用正开着 → **停下**，请人自己关（脚本不替人杀活着的应用）；
   - 启动不到 20 秒 → 可能还在闪屏阶段（主窗口 `visible:false`）→ 也停；
   - 没有界面窗 → 僵尸 → `taskkill /F /T` 清掉，清完**复核**一遍才继续；
   - 探针本身失败 → **不阻塞构建**，只提示（「诊断工具挂了就编不了」比原问题更糟）；
   - `RB_KEEP_STALE=1` 可保留样本（会直接停下，不打这次构建）。

**判据取最保守的那个，是有依据的**（源码级核实，不是猜）：

- 应用真正的窗口类名是 `Tauri Window`（`tauri-runtime-wry-2.11.4/src/lib.rs:856`）；
- `Tao Thread Event Target` 是 tao 0.35.3 `event_loop.rs:627` 给自己注册的消息窗，**进程活着就存在**、`IsWindowVisible` 还返回 true。

所以「有可见窗 + 类名不在这张内部表里」= 界面还开着。**故意不用标题长度或窗口个数当判据**：
判错的两个方向代价不对称 —— 误判成僵尸会**杀掉正开着的应用**，误判成开着只是多停一次。

## 验证方式（Agent 侧，冒烟）

- `node --check scripts/debug-win.mjs` 通过；pi-lens 报 `JavaScript/TypeScript clean`。
- `pnpm debug:win` 连跑 **3 次全部退出码 0**：⓪ 打印「· 没有残留实例，exe 可写」→ ① 前端 build → ② Windows cargo（30–37s）→ ③ `check:win` 产物核对通过（时间戳 + 4 个资源名全命中）。
- 过程中修掉两个自己引入的问题：PS 进度流 CLIXML 噪音、以及**在 `String.raw` 模板里写了反引号**把模板字面量提前截断（后者是 pi-lens 抓出来的，已在模板头注明「这里不能出现反引号」）。

**未验证（按 §2.8 归人类）**：「有窗口 → 停下」与「僵尸 → 清掉」这两条分支没有在本机实测
（要真拉起 GUI）。判据只用源码核实的类名做成，逻辑上不可能把开着窗口的应用认成僵尸；
真机首次命中时以脚本打印的「界面窗 N 个｜可见窗类名」行人为复核即可。

## 遗留问题

**「窗口关掉后进程不退」本身没修** —— 本次只是识别并清掉它，没找到根因。
现象：界面窗全没了、`msedgewebview2.exe` 子进程也没了，宿主进程却停在 tao 事件循环里不退出
（`EmbeddedBrowserWebView.dll` 仍加载 = 疑似卡在 WebView2 销毁握手）。
本仓 2026-09-18 那次「拖动跨屏 → 未响应、松手也不恢复」是同一类现象的另一面。

对症的下一步是**复现**：正常运行 → 关窗 → 看进程还在不在（要 GUI，归人类）。
复现出来才谈得上查（进程 dump / WebView2 teardown 路径）。
在那之前，每次构建少吃一次 os error 5 已经由步骤 ⓪ 兜住。
