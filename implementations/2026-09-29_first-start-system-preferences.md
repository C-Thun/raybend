完成时间：2026-09-29 22:57:40 +08:00

# 首次启动的系统外观与语言

## 改动范围与决策

- 复核首次 app.db 建立链：`setup → db::warm_up → DbState::get_or_open → AppDb::open`。数据目录和 backups 由现有实现创建，app.db 经迁移框架从 0 建到当前 schema，再启动读池和写线程；splash 继续由数据库初始化与 ui_ready 闸门控制。本轮不改数据库 schema。
- 主题、密度、语言继续作为设备偏好使用现有 localStorage 键（`raybend.theme` / `raybend.density` / `raybend.locale`）；没有把同一设置再存入 app.db。没有改键形状，因此无需另加存储迁移，已有选择不会被重置。
- 首屏渲染前，为缺失或非法的主题/语言提供系统默认并保存：外观跟系统 light/dark，未知用 dark；系统首选显示语言的 zh 变体（含繁体、脚本、地区）用 zh-CN，其它或未知用 en-US。密度仍 compact。两项分别以保存值为准，两项都已有时跳过系统探测；后续不订阅系统变化。
- `src/api/system-preferences.ts` 的 `SystemPreferencesAdapter` 隔离探测方式。桌面优先原生快照，WebView/browser 补齐未知项；探测各有 800ms 上限、并行执行，失败或迟到不会更改已经返回的默认值。媒体查询和语言失败独立降级。
- Windows/macOS 外观复用 Tauri 窗口 theme API。当前 Tao Linux 在未配置 portal 时固定返回 Light，故 Linux 原生侧返回未知，交给 WebView 媒体查询；保留统一 adapter 供未来平台实现替换。
- Windows 语言使用 `GetUserPreferredUILanguages(MUI_LANGUAGE_NAME)` 读取首选显示语言，而非地区格式，也不因备用语言包含中文就切中文。UTF-16 缓冲大小有上限，错误、无语言及非法编码返回未知。依据 [Microsoft API 文档](https://learn.microsoft.com/en-us/windows/win32/api/winnls/nf-winnls-getuserpreferreduilanguages)。非 Windows 语言目前走 WebView 的 navigator.language，macOS/Linux 真机验证留在平台接入阶段。
- `src/index.tsx` 等首次探测后初始化语言与同一个 appearance store，再渲染主界面/全屏/诊断页。App 接收这份 store，避免再建一份覆盖默认主题。HMR 卸载后不应用迟到的初始化。

复用评估：沿用 appearance、i18n、withTimeout、Tauri 运行环境判定、双侧 DTO 契约和公共 CDP 工装；仓内原来没有系统偏好探测 adapter，新增的模型只做纯转换，平台探测集中一处。Windows target 复用已锁定的 windows-sys 0.61.2，仅增加 Globalization feature；Cargo.lock 只新增桌面包对此既有依赖的关系，无新版本或大型依赖。

自动首次初始化不接入命令注册表、不设置新热键；主题和语言的手动切换继续使用原有命令与快捷键决定。本轮没有界面布局或控件改动，只更新现有设计说明的首次默认规则，不需新 Pencil 画稿。

## 涉及文件

- `src/lib/{appearance,system-preferences}.ts` 及对应单测，`src/i18n/index.ts` / 单测，`src/index.tsx` / `src/App.tsx`。
- `src/api/system-preferences.ts` / 单测、`types.ts`、`dto-contract.json` / 单测。
- `src-tauri/src/system_preferences.rs`、`lib.rs`、`contract.rs`，`src-tauri/Cargo.toml` / `Cargo.lock`。
- `scripts/check-system-preferences.mjs`、package.json 的 `check:startup`；旧 `ui-smoke.mjs` 固定中文/暗色测试场景，避免假定系统首次默认；导入/导出两条旧中文错误断言支持中英。
- `specs/first-start-system-preferences.md`、`memory/{DESIGN,ARCHITECTURE}.md`、`design/main.md`。

## 已验证（冒烟）

- `pnpm typecheck`、`pnpm lint:colors` / `lint:arch` / `lint:i18n`、`pnpm build` 通过。构建仍有既有的 >500 kB 分块提示。
- `pnpm test`：1149 条全部通过（约 2.5 秒）。新增用例覆盖 light/dark/未知、中文变体和大小写、非中文、Unicode、长非法语言、空/非法载荷、保存优先、部分缺失、存储禁用、探测同步/异步异常、超时及迟到响应。
- `pnpm check:startup http://127.0.0.1:1420/`：9 个空配置/重启/部分配置/非法配置/存储禁用的 DOM 冒烟场景通过；首屏挂载时 html 的主题和语言已正确，console.error 与未捕获异常为空。
- `pnpm smoke:ui http://127.0.0.1:1420/`：原有自动化交互/路由冒烟通过，problems 为空。主界面、主题/密度切换及全屏空态没有冒烟回归。
- `cargo check --workspace --locked --offline` 通过。
- `cargo test -p raybend-desktop --lib --offline system_preferences`：4 条通过（原生 UTF-16 解析、序列化、两侧契约）。
- `cargo test -p raybend --lib --offline store::db::tests::opens_and_creates_app_db`：首次建库迁移及目录建立测试通过。现有 core test 的 unused_mut warning 未改。
- 将同一份 Windows FFI 函数源码抽入 `/tmp/raybend-win32-preferences-check`，以已锁定 windows-sys 0.61.2 / Globalization 在 Linux 上 `cargo check --offline` 通过，核对 API 参数与返回值类型；这不等同 Windows 目标编译/链接。
- 新原生文件 `rustfmt --check` 与 `git diff --check` 通过。全仓 `cargo fmt --all --check` 仍被多处既有格式差异阻断，没有批量格式化无关文件。

## 问题、绕法与风险

- pnpm 在受限网络环境启动时卡在包管理器校验；获工具授权后执行同一质量门成功，未改 packageManager 或校验设置。
- 首次 DOM 冒烟的 20 秒等待被 Vite 冷预打包超过；复测通过后，将工装上限改为现有 UI 冒烟同样的 60 秒。系统偏好探测本身仍限 800ms。
- Windows 完整 cargo check 未能实际执行。先遇到 WSL → CMD UNC 引号损坏，切到编码 PowerShell 后发现会话 PATH 不含 Cargo；标准 Cargo 入口为 WSL 符号链接，直接调用已确认存在的 PE rustup.exe 也被此 PowerShell 会话判作文档。无输出且退出码 0 的中间尝试没有可靠 Cargo 成功标记，**不算编译通过**。达到项目的重复尝试上限后停止，采用上述 Win32 绑定类型编译核对继续完成其它验证，没有安装/改动全局工具链。
- 当前未提供 pi todo 工具，没有另造进度事实源。工作区原有其它任务变更保留；未提交、升版、打包、推送或发布。

## 未经崔总验证

Windows 目标完整编译/链接、全新配置首启时真实系统外观与显示语言、GUI 视觉及切换体感仍需真实 Windows 环境确认；macOS/Linux 尚无真机验收。此处 DOM 冒烟、FFI 类型核对和 Linux cargo check 不替代这些验证。新 MSI 流程的实际安装/升级验收保持原发布任务的边界。
