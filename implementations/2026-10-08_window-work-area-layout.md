完成时间：2026-10-09 00:03:40 +0800（本批工作自 2026-10-08 晚起，跨零点）

# 窗口「工作区布局」：Shift+点击最大化键

## 需求与依据

崔总 2026-10-08 提出（原文逐字收录在 `specs/window-work-area-layout.md` §1）：取消最大化的同时把窗口
调整到所在屏幕工作区内的留白矩形（默认左右上各 5%、下 10%），绑定到「Shift+左键点击最大化键」。
四项决策同日拍板：**工作区为基准**（排除任务栏）/ 比例**本期硬编码**（命令接口留 insets 参数，不做 DB 设置项）/
命令**默认键留空** / 重复触发**幂等**（不做 toggle）。规格 `specs/window-work-area-layout.md`。

> **后续调优**：2026-10-09 崔总要求默认留白全部减半（5/5/5/10 → 2.5/2.5/2.5/5），见
> `implementations/2026-10-09_window-work-area-insets-tuning.md`。本文所记比例以那份为准。

## 改动范围与文件

| 文件 | 改动 |
| --- | --- |
| `src-tauri/src/window_layout.rs`（新） | 纯函数 `inset_rect`（工作区 + 比例 → 内容区矩形）+ 同步命令 `window_fit_work_area` + 6 项单测 |
| `src-tauri/src/lib.rs` | `mod window_layout;` + `invoke_handler` 注册 |
| `src/api/window.ts` | `WindowHandle.fitWorkArea`、`createWindowChrome.fitWorkArea`、`tauriWindowHandle` 走 `invoke("window_fit_work_area")`、纯函数 `maximizeKeyAction`（Shift 判定唯一处） |
| `src/api/window.test.ts` | 4 项新测试（判定 / 转发 / 失败不崩 / 浏览器降级静默）+ 假句柄扩展 |
| `src/shell/TitleBar.tsx` | 最大化键 `onClick` 判 Shift；`WindowButton` 增 `hint`（拼进原生 `title` 第二段） |
| `src/features/commands/catalog.ts` | 命令 `window.fitWorkArea`（menu = 窗口；**默认键留空并写明理由**） |
| `src/App.tsx` | `withWindow` 类型 + `commandDeps.window` 注入 |
| `src/i18n/zh-CN.ts` / `en-US.ts` | `cmd.window.fitWorkArea`、`titlebar.window.shiftFit` |
| `design/main.md` §2.1 | 「设计思路」新增第 7 条：Shift+点击语义（**画布无变化，`.pen` 未动**） |

## 关键决策与理由

1. **几何全在 Rust**（`AGENTS.md` §6.1 红线 ②、§7.9）：前端只发意图 + 判定 Shift，不碰工作区矩形、
   边框偏移与物理像素换算。
2. **尺寸按内容区、位置按外框再补偿**：Tauri 的 `set_size` 是客户区语义（Windows 上 tao 已补偿
   undecorated+shadow 窗口的不可见边框），而 `set_position` 是外框语义 —— 不补偿会让左右留白
   差 8px 级（肉眼可辨的不对称）。
3. **先恢复、再用 getter 当屏障**：最大化状态下 set_size/set_position 会被系统忽略；tao 的
   `set_maximized(false)` 是投递到事件循环线程的异步动作，而窗口消息同队列 FIFO，
   `is_maximized()` 阻塞读返回时恢复已落地 —— 不需要 sleep 轮询。
4. **读回校验 + 最多两轮修正**：位置偏差 > 1px 就用「当前外框 + 实测偏差」修正（首轮边框偏移
   因跨屏 DPI 失准时也能收敛）；尺寸被 `minWidth/minHeight` 钳制时记一行日志并在报告里标
   `sizeClamped`，**不假装成功**。
5. **命令默认键留空**（§2.15 纪律要求同批给出决定）：原生触发是鼠标修饰手势，键盘上没有自然键位；
   但仍登记成正式命令 —— 命令面板可搜到、可手动绑键、「窗口」菜单里可见。
6. **提示走原生 `title` 第二段（`hint`）**：Shift+点击是隐藏手势，不提示没人会知道；
   不为此新造浮层组件。
7. **XMP 影响：无** —— 纯窗口层，不触碰 sidecar 数据、存储布局与照片身份（§2.19 评估结论）。

## 验证（Agent 冒烟，2026-10-08 23:3x–23:5x 实跑）

- `cargo check -p raybend-desktop --offline`：通过。
- `cargo test -p raybend-desktop --lib window_layout`：**6 passed / 0 failed**（1080p 默认比例、
  负坐标多屏、比例 0、奇数尺寸取整、极小工作区下界、非法比例拒绝）。
- `pnpm typecheck`：通过。另做**探针实验**证明 tsc 覆盖该行：把 i18n 键临时改成不存在的名字会报错、
  还原后通过（`TitleBar.tsx:278`）。
- `pnpm test`：**1228/1228**；`src/api/window.test.ts` 单项 **25/25**（含 4 条新增）。
- `pnpm lint:colors` / `pnpm lint:arch` / `pnpm lint:i18n`：全部通过。
- `pnpm build`：通过；`cargo check --workspace`：通过；`cargo test --workspace`：首行
  `1451 passed / 0 failed / 12 ignored`（⚠️ 该数字**包含并行会话**在 `raybend` crate 的改动，
  不是本任务独有；`window_layout` 的 6 项已由上面的定向命令单独证明）。
- pi-lens 曾报 `TitleBar.tsx:278 titlebar.window.shiftFit 类型错误`：经「探针实验 + `tsc` +
  LSP 主动探测」三重证伪，是**陈旧快照**（键在 `zh-CN.ts:402` / `en-US.ts:399`，
  且 `MessageKey = keyof typeof zhCN`）。

## 遗留与待真机验收（崔总，Windows）

- 真机四组组合：Shift+点击 × 起始状态（最大化 / 还原 / 半屏 snap / 已在该布局）× 屏幕
  （单屏 / 双屏不同 DPI）；核对左右留白对称、无可见闪烁、最大化图标正确翻转、重复触发无变化。
- 「从最大化恢复再摆位」是否有可见跳变，本机无法判定（WSL 配置里三键不渲染、点不到）；
  若真机发现闪烁，退路是用 Win32 `SetWindowPlacement` 一次完成 restore+定位（规格 §3 已记）。
- 工作区里同时有**并行会话**的未提交改动（compare/viewer、presets 修复、clean-target）：
  本次只改本任务相关文件，未触碰他人改动；提交/发版仍由崔总决定。
