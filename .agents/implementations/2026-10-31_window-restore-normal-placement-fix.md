# 最大化还原小窗口：同步 Windows restore 正常态

> 2026-10-31 20:10:09

## 改动范围

修复「拖到某屏 → 最大化 → 关闭 → 重开正确最大化 → 点还原却变成未知小窗口」：

- 启动最大化后，在 `show()` 后把上次正常态明确写入 Win32
  `WINDOWPLACEMENT.rcNormalPosition`，不再让 Windows 使用窗口创建阶段的默认 restore 矩形。
- 启动各分支缓存**经过校验的目标正常态**，不缓存异步 `SetWindowPos` 尚未落地时读回的旧值。
- 标题栏与命令面板的「最大化 / 还原」改走 Rust `window_toggle_maximize`：
  反最大化前优先使用内存正常态，先写 restore，再 unmaximize，最后复核实际内框；
  缓存缺失时才读系统 restore。
- 反最大化和窗口还原一样，宽、高分别执行最小尺寸保底。

## 涉及文件

- `src-tauri/src/window_layout.rs`
  - 新增 `PixelRect::with_min_size`；
  - 新增内框/外框换算与 Win32 `GetWindowPlacement` / `SetWindowPlacement` 适配；
  - 新增 `read_restore_inner_rect` / `set_restore_inner_rect`；
  - 新增换算与单边保底单测。
- `src-tauri/src/window_state.rs`
  - 新增 `window_toggle_maximize` 与 `sync_restore_after_show`；
  - 启动 Fit / Restore / Maximize 三分支改缓存目标正常态；
  - 反最大化后同样缓存目标，不用异步读回旧值污染。
- `src-tauri/src/lib.rs`：注册 toggle 命令；主窗口 show 后同步 restore。
- `src/api/window.ts`：toggle 转发到 Rust 命令。
- `specs/window-restore.md`、本记录。

## 根因与证据

不是四角比例记录错误，而是 Windows 的 restore 状态被启动流程污染：

1. 真机原记录正常态为 **1482×990**，但重开后的 `GetWindowPlacement().rcNormalPosition`
   是 **763×507**。点还原时 Windows 用的是后者，所以出现从未调整过的小窗口。
2. 旧进程再次关闭时，内存正常态又被异步读回旧值污染成 **936×624** 并写回文件，
   证明不能只把 `set_inner_rect()` 的读回值当成已知正常态。
3. `tao` 的 `set_size` / `set_position` 最终走异步 `SetWindowPos`；
   启动后立刻 maximize，甚至 maximize 后立即写 restore，都可能被后排队的几何消息覆盖。
   因此权威正常态应是「校验后的目标」，并在 `show()` 后同步一次 restore。

## 修复后的真机验证

使用 `pnpm debug:win` 构建，CDP 调用真实 `window_toggle_maximize` /
`window_fit_work_area`，PowerShell 只读 `GetWindowPlacement` / `GetClientRect`：

- 最小尺寸场景（125%）：反最大化目标与实际均 **1500×1000 物理像素**
  （即 1200×800 逻辑像素），不再出现 763×507。
- 大正常态闭环：Shift+最大化形成 **2432×1332** → 最大化 → 正常关闭 →
  状态文件 `maximized:true, inner=2432×1332` → 重开正确最大化 →
  反最大化目标与实际均精确为 **2432×1332**。
- 启动后、尚未点击时，Windows restore 也已回到正常态，不再是默认小矩形。

## 自动化验证

- `cargo test -p raybend-desktop window`：31 项通过。
- `cargo test -p raybend-desktop`：190 项 lib 测试 + 2 项 main 测试通过。
- `cargo test --workspace`：最终复跑通过（此前另一组编辑区改动的 Area 序列化编译错误已由对应改动方修复，本轮未改该范围外代码）。
- Windows 侧 `cargo check -p raybend-desktop --features custom-protocol`：通过。
- `cargo clippy -p raybend-desktop --lib`：窗口相关文件无新增告警。
- `pnpm typecheck`：通过。
- `pnpm test`：1359 项通过。
- `pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n`：通过。
- `pnpm build`：通过。
- `pnpm debug:win`：构建与 `check:win` 通过。
- 本轮相关文件 `git diff --check`：通过。

## 遗留

- 多屏热插拔、任务栏换边、系统原生双击标题栏还原仍属真机验收项；
  本轮已覆盖标题栏/命令面板共用的 Rust toggle 路径。
- 当前 PowerShell 是 DPI 感知不同的外部进程，屏幕读数会按 125% 折算；
  尺寸结论以 Rust 日志的物理像素目标/实际值为准。
