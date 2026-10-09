# 规格：窗口「工作区布局」（Shift+点击最大化键）

> 状态：**已拍板待实施 → 实施中**（2026-10-08）。需求原文与决策记录：`todos/2026-10-08-window-work-area-layout.md`。
> 决策（崔总 2026-10-08）：① 基准 = **工作区**（`rcWork`，排除任务栏）；② 比例**本期硬编码**（左 2.5% / 上 2.5% / 右 2.5% / 下 5%），命令接口按可选参数设计、不做 DB 设置项；
> **2026-10-09 崔总调优**：留白全部减半（原 5/5/5/10 → 现 2.5/2.5/2.5/5），其余决策不变。
> ③ 命令**默认键留空**（鼠标修饰手势专属；命令面板仍可搜到并手动绑键）；④ 重复触发**幂等**，不做 toggle。

## 1. 语义

**需求原文（崔总 2026-10-08）**：

> 有没有什么方案，能让窗口取消最大化的同时，尺寸调整到与窗口所在屏幕匹配的一个小区域，
> 比如我可以指定左右各留 5% 空间，上面留 5% 空间，下面留 10% 空间这样。
> 如果可以的话我想做一个按住 shift 键鼠标左键点击最大化窗口时（不管当前是最大化了，还是没最大化）
> 都统一调整到这种尺寸/位置来部署窗口。先研究规划一下

`Shift + 左键点击 titlebar 最大化键` → 窗口统一调整到「所在屏幕工作区内、四边按比例留白」的矩形。
**不管当前是最大化、还原、半屏 snap 还是已经在该布局**，结果都是同一矩形；重复触发不产生变化。

比例基准 = **工作区**（排除任务栏），所以任务栏在底部时「下留 5%」在屏幕上的视觉留白会多出任务栏那一条高——这是有意的，窗口永不压到任务栏。

## 2. 几何规则（物理像素，全在 Rust）

```text
work  = 窗口所在屏的 work_area()（PhysicalRect，物理像素；Linux 走 GDK，Windows 走 GetMonitorInfoW）
left  = round(work.width  × insets.left)     right  = round(work.width  × insets.right)
top   = round(work.height × insets.top)      bottom = round(work.height × insets.bottom)
target.x = work.x + left                     target.y = work.y + top
target.w = work.width  - left - right        target.h = work.height - top - bottom   （下界 1）
```

- **尺寸基准 = 内容区（client）**：Tauri 的 `set_size` 目标是客户区（Windows 上 tao 对 undecorated+shadow 窗口已补偿不可见边框）。
- **位置基准 = 外框（outer）**：`set_position` 设的是外框左上角 → 必须减掉边框偏移 `inner_position - outer_position`，否则左右留白会差一个不可见边框（肉眼可辨的不对称）。
- 参考依据：`tao-0.35.3/src/platform_impl/windows/window.rs:213-313`、`tauri-2.11.5/src/window/mod.rs:58-97`（详见需求文件 §3 的事实表）。

## 3. 时序（Rust 同步命令内）

1. 取 `current_monitor()` → `work_area()`；算出目标矩形；
2. 若 `is_maximized()` → `unmaximize()`，再用 `is_maximized()` 当**屏障**（tao 的恢复是投递到事件循环线程的异步动作；窗口消息同队列 FIFO，getter 返回时恢复已落地，不需要 sleep 轮询）；
3. 量一次边框偏移（必须在恢复之后量）；
4. `set_size(目标内容区尺寸)` + `set_position(目标内容区左上 - 偏移)`；
5. **读回校验**：`inner_position()` 与目标偏差 > 1px 时用「当前 outer + 偏差」修正，最多 2 次（修正量来自实测值，因此跨屏 DPI 变化导致的偏移失准也能收敛）；
6. 读回 `inner_size()`，与目标不一致时记日志并在报告里标 `sizeClamped`（主窗口有 `minWidth:1200 / minHeight:800`，小屏会被系统钳制——**不假装成功**）。

## 4. 接口

### 4.1 Rust 命令 `window_fit_work_area`

```rust
#[tauri::command]  // 同步命令：窗口调用在同一线程同步执行，无竞态
pub fn window_fit_work_area<R: Runtime>(
    window: WebviewWindow<R>,
    insets: Option<WorkAreaInsets>,   // 缺省 = 产品默认比例（DEFAULT）
) -> Result<WindowLayoutReport, String>
```

- `WorkAreaInsets { left, top, right, bottom }`（0–1；各自合法 + 左右/上下之和 < 1，非法即 `Err`）；
- `WindowLayoutReport { target, inner, workArea, restored, sizeClamped }`（物理像素）——前端只用于 `console.debug` 诊断，不做 UI；
- 失败一律 `Err(String)`（前端静默记日志，不弹窗、不带崩界面）。

### 4.2 前端

| 位置 | 改动 |
| --- | --- |
| `src/api/window.ts` | `WindowHandle` 增 `fitWorkArea()`；`tauriWindowHandle()` 里 `invoke("window_fit_work_area")`；`createWindowChrome.fitWorkArea()`（失败返回 false）；**纯函数 `maximizeKeyAction(event) -> "fitWorkArea" \| "toggle"`**（Shift 判定的唯一处，配单测） |
| `src/shell/TitleBar.tsx` | 最大化键 `onClick` 判 `maximizeKeyAction`；`title` 追加 Shift 提示 |
| `src/features/commands/catalog.ts` | 新增 `window.fitWorkArea`（group/menu = `window`，scope = `global`，`enabled: deps.window.available`，**`defaultKey` 不填**） |
| `src/App.tsx` | `commandDeps.window.fitWorkArea` 注入（走既有 `withWindow`） |
| i18n | `cmd.window.fitWorkArea`（命令标题）、`titlebar.window.shiftFit`（提示）中英各一份 |

## 5. 测试与验收

**单元测试（必须齐备）**

- Rust 纯函数 `inset_rect`：正常 1920×1080 → (96, 54, 1728, 918)；负坐标多屏（`work.x = -1920`）；比例 0（整块工作区）；极小工作区（宽/高不足时下界 1）；四舍五入（奇数尺寸）；非法比例（负数 / >1 / 之和 ≥ 1）→ `Err`。
- 前端 `window.test.ts`：浏览器降级不调用 `fitWorkArea`；Tauri 里调用转发；抛错返回 false 不崩；`maximizeKeyAction` 的 Shift/非 Shift 判定。

**Agent 冒烟**：`pnpm typecheck && pnpm test && pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n && pnpm build` + `cargo check --workspace` + `cargo test`。

**真机验收（崔总，Windows）**：Shift+点击 × 起始状态（最大化 / 还原 / 半屏 snap / 已在该布局）× 屏幕（单屏 / 双屏不同 DPI）；
核对：左右留白对称（无 8px 级偏差）、无可见闪烁、最大化图标状态正确翻转、重复触发无变化。**进程/测试通过 ≠ 功能对了。**

## 6. 非目标

- 不做「一键切回最大化」toggle（第 4 项拍板：幂等）；
- 不做比例设置 UI / DB 设置项（本期硬编码；接口已留参数）；
- 不改 `design/*.pen`（视觉零变化）；不动 titlebar 双击（Tauri drag 区行为）；
- **XMP 影响：无**（纯窗口层，不触碰 sidecar 数据与存储布局）。
