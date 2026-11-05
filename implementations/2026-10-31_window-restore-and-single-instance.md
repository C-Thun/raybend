# 窗口还原 + 单实例（四角点位比例 / 三种操作 / 最小尺寸保底）

> 2026-10-31 17:19:57

## 改动范围

按崔总 2026-10-31 的十项决定实现「关闭记录 → 启动只判断一次 → 还原 / Shift+最大化回退 / 最大化」：

- **记录**：主窗口正常关闭时把「四角点位占所在屏工作区的比例」+ 屏幕身份 + 正常态内框
  写入独立 `windows-state.json`；最小化关闭清除记录，最大化关闭记录「该屏 + maximized」。
- **启动**：在 `setup` 里、主窗口 `show()` 之前应用一次；只做一次「记录在不在 /
  四角比例是否 0–1 / 原屏是否可识别」判断，然后只执行三种操作之一。
- **最小尺寸保底**：`fitWorkArea` 目标尺寸单边不低于 `minWidth/minHeight`（逻辑像素按目标屏
  scale 换算）；高算 700 → 800，宽算 1300 → 1300。保底后放不下就到此为止，不重试。
- **titlebar 保底**：三个操作末尾各一次，title 不在画面内就把窗口拉到该屏 `work.y`。
- **单实例**：`tauri-plugin-single-instance`，注册在所有插件之前；dev（`tauri::is_dev()`）跳过，
  避免与已安装正式版互相踢；第二个实例退出并聚焦已有主窗口。
- 无前端、设计稿、XMP、catalog、app.db schema 改动。

## 涉及文件

- 新增：`src-tauri/src/window_state.rs`（记录/四角比例/屏幕识别/单次决策/文件/事件缓存/单测）、
  `specs/window-restore.md`。
- 修改：`src-tauri/src/window_layout.rs`（`PixelRect`/`inset_rect` 提为 `pub(crate)`、最小尺寸保底、
  抽出 `set_inner_rect` / `apply_inset_layout`、`titlebar_needs_move` / `ensure_titlebar_visible`）、
  `src-tauri/src/lib.rs`（模块、setup 还原、退出兜底、单实例）、
  `src-tauri/Cargo.toml`（新插件 + `windows-sys` 补 feature）。
- 文档：`specs/window-work-area-layout.md`（最小尺寸保底/共用几何）、`AGENTS.md` §2.28/§11.5/§13、
  `memory/PLAN.md`。
- 生成物：`Cargo.lock`、`public/legal/third-party.json`（`pnpm licenses:generate`）。
- 临时报告 `todos/2026-10-31-window-restore.md` 已转成 `specs/window-restore.md` 后删除。

## 关键决策与理由

- **四角点位比例，不引入「缩放」概念**：崔总口径。`corners = {x1,y1,x2,y2}` 相对记录屏工作区；
  还原时映射到目标屏工作区，分辨率/缩放/任务栏变化都能正确落位。
- **屏幕身份逐级降级**：PnP 设备路径 → GDI 名 + rect → GDI 名 → rect → 主屏。
  `\\.\DISPLAYn` 不保证永久，PnP 路径用 `EnumDisplayDevicesW(EDD_GET_DEVICE_INTERFACE_NAME)`
  取；取不到就留空，不影响其它平台。
- **只判断一次**：`window_state::decide` 是唯一的决策点；之后没有「还不行就换布局」的代码。
  `window_layout::set_inner_rect` 里保留原有的 2 轮读回位置修正（1px 级 DPI/边框偏差），
  它不是可见性判断，也不是回退重试。
- **复用优先**：`window_fit_work_area`（手动 Shift+点击）和启动回退都走
  `window_layout::apply_inset_layout`；最小尺寸只加在目标尺寸计算处，没有第二套几何。
- **最小化/最大化关闭**：最小化读位置会拿到 `-32000` 哨兵，直接清记录；最大化时用事件缓存里的
  最后一次正常态矩形，启动时先摆正常态，**到 `reveal_main()` 的 `show()` 前一刻再 `maximize()`**
  （Windows 的 `maximize()` 会 `ShowWindow`，在闪屏期间对隐藏主窗口调用可能提前显示）；
  pending 状态也计入关闭记录，闪屏期间被关掉不会丢掉「最大化」语义。
- **只在正常关闭写**：不做防抖/周期保存；崩溃/强杀保留上次正常关闭的文件（崔总：这样更稳）。
- **单实例 dev 例外**：插件在 Windows 上按 app identifier 建互斥体，dev 与正式版同 identifier；
  `is_dev()` 时跳过可避免开发时把已安装版本踢掉，`debug:win`/正式版仍生效。

## 验证

- `cargo check --workspace`：通过（仅既有 warning）。
- **Windows 侧 `cargo check -p raybend-desktop --features custom-protocol`**（在 Windows 侧用
  `debug:win` 同款 dav1d 环境、`CARGO_TARGET_DIR=C:\rb-target\raybend`）：通过；
  `#[cfg(windows)]` 的 `EnumDisplayDevicesW` PnP 路径与单实例插件都已真编译。
- `cargo test --workspace`：通过（`raybend-desktop` 188 个 lib 测试 + `window_state` 15 个 +
  main/doctests 全部通过）。
- `cargo clippy -p raybend-desktop --lib`：新增代码无告警；剩余告警均为既有文件。
- `pnpm typecheck`：通过。
- `pnpm test`：1348 tests，0 fail。
- `pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n`：通过。
- `pnpm build`：通过。
- `pnpm licenses:generate`：619 项，已把 `tauri-plugin-single-instance` 纳入许可清单。

## 遗留 / 需人类确认

- 真机验收：首次启动 / 单屏 / 双屏不同 DPI / 改分辨率或缩放 / 拔副屏 / 最大化关闭 /
  最小化关闭 / 任务栏换边 / 负坐标副屏 / 双击启动第二个实例。
- 1080p@150% 与 1366×768 放不下 1200×800 逻辑最小窗，按崔总口径不在支持范围；
  保底后窗口可能超出工作区，属于终止状态，不是 bug。
- `windows-state.json` 与 dev 共用 app 数据目录（本机 dev 在 Linux/WSL，数据目录不同；
  Windows 上若同时跑 `tauri dev` 与正式版，会共用同一份文件——单实例在 dev 下不生效，
  这是为开发便利保留的例外）。
