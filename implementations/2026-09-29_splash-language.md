完成时间：2026-09-29 23:25:08 +08:00

# splash 按软件语言选择图片

## 改动与决策

- `public/splash.html` 移除随机中英文选图。首次没有软件语言设置时显示英文，之后软件设置为 zh-CN 显示中文图，否则显示英文图；读取失败也英文。同步设置 html.lang 和图片 alt，仍在首次绘制前一次性设置图片 src。
- 本次启动语言需要在主界面首次系统探测保存设置前固定：Windows 系统为中文时，首次主界面会保存中文，但这次 splash 仍必须英文。新 `src-tauri/src/splash.rs` 通过 Tauri 既有 `append_invoke_initialization_script` 接入 `public/splash-locale.js`，先于页面脚本捕获保存值；主界面与 splash 使用同一启动标识和派生快照。
- `raybend.locale` 仍是唯一的软件语言设置。新增 `raybend.splash-launch.v1` 只保存 `{launch,locale}` 的本轮派生快照，每次启动换标识并覆盖一条，不积累记录；没有改已有设置键或 app.db schema。本次手动切换语言不改变已选 splash，下次启动重新读取。
- 为不同 WebView 交错读取加了第二次快照检查：若另一文档已固定启动值，优先使用它，避免主界面刚保存的系统语言污染本次 splash。
- 脚本由 Rust 编译期嵌入，不新增 splash 请求、bundle、IPC 或权限，不依赖 Windows 路径。静态浏览器预览没有原生初始化脚本时，直接读同一个软件语言键。
- 沿用现有中英文 WebP、800×600 透明无边框、无按钮、不抢焦点、3 秒最短停留以及现有数据库/首屏闸门。既有设计图不变，只更新 `design/main.md` §2.4 和 `memory/DESIGN.md` 的选图规则；历史随机选图记录不回改。

复用评估：沿用软件语言存储键、Tauri 文档初始化入口与现有 CDP 工装；仓内没有启动前语言快照实现，新增的脚本仅负责这一个启动时序适配，不再造语言设置存储或数据库初始化通道。自动启动行为不接入命令注册表、不设新热键；手动语言切换仍用既有命令。

## 涉及文件

- `public/splash.html`、`public/splash-locale.js`。
- `src-tauri/src/splash.rs`、`src-tauri/src/lib.rs`。
- `src/i18n/splash.test.ts`：真实无 bundle 脚本的 VM 单测，不同上下文共享存储模拟不同 WebView。
- `scripts/check-system-preferences.mjs`：复用 check:startup 验证主界面首启与本次/下次 splash；`scripts/ui-smoke.mjs` 的历史随机断言改为固定中文连续加载；`scripts/check-win-artifact.mjs` 仅同步注释。
- `specs/splash-language.md`、`design/main.md`、`memory/{DESIGN,ARCHITECTURE}.md`。

## 已验证（冒烟）

- `pnpm typecheck`、`pnpm test`：1154 条全部通过（约 1.7 秒）。新增测试覆盖首次英文/下次中文、软件中英文、同次切换保持、多个 WebView 交错读取、空/非法/超长/Unicode 值、坏派生快照、存储禁用、静态预览不写软件语言。
- `pnpm lint:colors` / `lint:arch` / `lint:i18n` 均通过；`pnpm build` 成功（现有 >500 kB 分块提示仍在）。
- `cargo check --workspace --locked --offline` 通过；`cargo test -p raybend-desktop --lib --offline splash`：3 条通过（启动标识 JSON 转义及既有停留/超时边界）。
- `pnpm check:startup http://127.0.0.1:1420/`：9 个初始化/重启场景与首次英文、下次中文、软件英文的 splash 图片加载通过。使用与 Rust 嵌入相同的脚本模拟文档初始化；不是 Windows 原生运行证据。
- `CDP_PORT=9533 pnpm smoke:ui http://127.0.0.1:1420/`：通过，problems 为空；软件保存中文时 splash 连续三次加载同一中文图，图片尺寸、透明背景、无交互元素检查通过。
- `pnpm test:release`：65 条全部通过；源码镜像既有 public/src-tauri 输入包含新增脚本与 include_str 源，不影响既有资源检查。
- 新 Rust 文件 rustfmt 检查、脚本语法检查及 `git diff --check` 通过。

## 遗留与边界

Windows 工具链执行限制沿用前一份实施记录，本轮没有重复尝试或生成 MSI。Windows 原生窗口的初始化注入、多个 WebView 的真实共享存储、首次/重启时序与透明视觉仍需崔总真机确认；macOS/Linux 同样未经真机验收。没有改两张图或窗口时长，没有提交/升版/发布，工作区其它任务变更保留。
