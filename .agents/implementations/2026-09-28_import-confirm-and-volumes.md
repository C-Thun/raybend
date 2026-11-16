# 导入确认弹窗、来源卷轮询、worker 控制台窗口（三件一起）

完成时间：2026-09-28 21:25:00 CST

## ① 导入前确认弹窗（崔总 2026-09-28 口述）

点「导入」不再直接开工，先弹**确认窗**：上面是选了哪些目录（只读），中间一个宽扁的向下箭头
（`IconChevronDown 56`），下面是选中的库卡片；取消在左、「开始导入」在右。确认后关自己，
接上既有的「导入中」进度窗。

- 新组件 `src/workspaces/import/ImportConfirmDialog.tsx`（workspace 层：它要同时组
  `features/selected-dirs` 与 `components/ui/RepositoryCard`，features 之间不许互相 import）。
- **只展示不改清单**：`SelectedDirs` 加 `readOnly`（不给移除与开关，开了子目录的目录把那行字
  写出来——它改变导入范围，不能在确认时隐身）；`RepositoryCard` 加 `interactive={false}`
  （无点击/键盘/悬停、不出齿轮与重找格；选中底色保留）。
- 目录多时弹窗内滚（最多 30vh）。
- i18n：`import.confirm.title/into/start`（中英）。
- **新冒烟脚本 `scripts/check-import-boot.mjs`（`pnpm check:import`）**：此前导入的**开工那条路**
  没有任何脚本走过（`check:browse` 的假后端是浏览侧的、`smoke:ui` 没有假后端）。覆盖：勾选目录 →
  选库 → 禁用态 → 弹窗结构（目录/箭头/库卡/按钮序）→ 取消**不调** `import_start`（连预检都不发）→
  确认后 `import_start` 收到正确的库与源（含 `includeSubdirs`）→ 顶上「导入中」。
  - 踩坑两则记档：①路径带反斜杠**不能用 CSS 属性选择器**（`[aria-label="C:\x"]` 里的 `\"`
    在 CSS 里是转义引号，选择器静默变义）——一律 JS 比字符串；②来源树第一层是盘，**双击展开**
    后才有目录可勾。

## ② import source 卷列表的「实时」刷新（崔总问的确认）

**确认结论（代码层面）**：旧口径**不是实时**——只在「导入工作流挂载」与「窗口重新获得焦点」
（`LeftColumn` 的 `focus` 监听）时重列。崔总看到「插上就出现」，是因为插盘后点了窗口一下。

**补上**：`ImportStore.startSourceWatch()/stopSourceWatch()`——挂载期间每 `SOURCE_POLL_MS`（2 秒）
轮询 `volumes_list`；上一拍还在飞就跳过（不叠请求），失败下一拍自愈；切走工作流即停。
`LeftColumn` 挂载时 start、清理时 stop（与 focus 刷新并存）。

- 为什么轮询不监听 `WM_DEVICECHANGE`：那要自建消息窗 + 设备通知（Windows 专属、本机无法验证）；
  `volumes_list` 在 Windows 只走 `GetLogicalDrives + GetDriveTypeW`（**不取卷标**，`store/volumes.rs`
  的已知取舍），一次几微秒，不构成 §2.13 说的「疯狂扫描」。真要事件驱动，换的是这一处，接口不动。
- 定时器走注入（`deps.timers.repeat`，返回停止函数——句柄类型不外露）；新增单测 1 条
  （挂载期间到点重列 / 在飞不叠 / 停后不拉）。
- 「最近」的挂载标灰**不进**轮询：那要 `is_dir` 一批路径，断线的网络盘会一挂几秒，轮询会被拖死；
  仍按原口径（启动、获焦、选中时）。

## ③ release 产物启动弹终端（崔总报的）

**确诊（真机证据）**：`raybend-desktop.exe`（GUI 子系统，`main.rs` 的 `windows_subsystem` 在）spawn
了同目录的 `raybend-raw-worker.exe`（**控制台子系统**，无该属性），父进程没有控制台 → Windows 给
子进程**新开一个** conhost（实测：worker 的子进程表里有 `conhost.exe 0x4`）。首张 RAW 解码在启动
阶段就来，所以看着像「启动弹终端」。**与发版无关**（安装包只装主程序、走自重启路径，没这个问题；
是构建目录直跑的形态）。

**修法**：`raw/worker.rs::spawn_proc` 按需加 `CREATE_NO_WINDOW`——只在**父进程自己没有控制台**时
（`GetConsoleWindow()==NULL`）：双击/打包运行 → 不弹窗；开发期从终端跑（父进程有控制台）→ 什么都不加，
worker 日志照旧落终端。没有把 worker 改成 GUI 子系统（那它就不能单独跑给人看日志了）。
`windows-sys` 加 `Win32_System_Console` feature（现有依赖，只加 feature）。
附带探针结论：worker 在无控制台环境下启动/退出码 0 正常（不会因 stderr 无效而 panic）。

## 验证

- `pnpm typecheck` / `pnpm test` / 三条 lint：通过；`pnpm check:import`（新）/ `check:browse` /
  `smoke:ui`：通过（均为真实 CDP 操作）。
- Rust 侧 Linux `cargo check --workspace` 通过；Windows 侧编译见 `debug:win`（本记录同时段构建）。

## 遗留

- `WM_DEVICECHANGE` 事件驱动列为将来可选（登记在本记录即可）。
- 轮询间隔 2 秒是「插上就看见 vs 不刷屏」的取舍，崔总觉得不合适随时改 `SOURCE_POLL_MS` 一处。
- `CREATE_NO_WINDOW` 的真机观感（无终端弹出）归崔总在 Windows 上确认。
