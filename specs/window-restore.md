# 窗口还原（启动恢复上次位置与尺寸）

> 2026-10-31 ｜ 状态：**已实施** ｜ 依据崔总 2026-10-31 的十项决定
> 关联：`specs/window-work-area-layout.md`（Shift+点击最大化键的留白布局，本次直接复用）
> 实施记录：`implementations/2026-10-31_window-restore-and-single-instance.md`

## 1. 目标

1. 正常关闭时记住主窗口的**四角点位比例**与所在屏幕；下次启动还原。
2. 启动只判断一次；任何情况下都不能把窗口还原到屏幕外。
3. 判断失败/无记录时，用既有的 `fitWorkArea` 留白布局（左右上 2.5%、下 5%）。
4. 最小化关闭 = 清除记忆；最大化关闭 = 下次定在该屏并自动最大化。
5. 应用禁止同时启动两个实例。
6. 不动设计稿、XMP、catalog、app.db schema、前端命令。

## 2. 记录（`windows-state.json`）

位置：`%LOCALAPPDATA%\com.cthun.raybend\windows-state.json`（`db::data_dir` + 文件名）。
删除文件即可重置窗口记忆。

```json
{
  "version": 1,
  "main": {
    "inner": { "x": 200, "y": 100, "width": 1600, "height": 1000 },
    "corners": { "x1": 0.104, "y1": 0.096, "x2": 0.937, "y2": 1.0 },
    "maximized": false,
    "monitor": {
      "name": "\\\\.\\DISPLAY2",
      "pnpId": "\\\\?\\DISPLAY#...",
      "rect": { "x": 1920, "y": 0, "width": 2560, "height": 1440 },
      "work": { "x": 1920, "y": 0, "width": 2560, "height": 1400 },
      "scale": 1.0
    },
    "topology": [ "…保存时所有显示器快照，仅诊断…" ],
    "savedAt": 1790000000000
  }
}
```

- **四角是相对「记录时所在屏的工作区」的比例**（工作区 = 排除任务栏），
  与 `fitWorkArea` 同一基准；不是「把窗口按比例缩放」。
- `inner` 是内框绝对矩形，保留给诊断、屏幕匹配、最大化记录的「正常态」。
- 屏幕身份逐级匹配：PnP 设备路径 → GDI 名 + rect → GDI 名 → rect → 不可识别。
  Windows 上 PnP 路径由 `EnumDisplayDevicesW` + `EDD_GET_DEVICE_INTERFACE_NAME` 取得；
  Tauri 的 `\\.\DISPLAYn` 是 GDI 设备名，不保证永久。
- `topology` 只进日志，不参与决策。

## 3. 保存行为

只在**正常关闭**时写一次（`CloseRequested`；`RunEvent::Exit` 用内存快照兜底一次，
同属正常退出）。崩溃、强杀、断电不写，保留上一次正常关闭的文件。

| 关闭时状态 | 动作 |
| --- | --- |
| 普通 | 写当前内框四角 + 当前屏 + `maximized:false` |
| 最大化 | 写**正常态**四角/内框 + 最大化时所在屏 + `maximized:true` |
| 最小化 | 清除 `main` 记录（不读 `-32000` 哨兵位置） |

- 正常态矩形来自运行期内存缓存（`Moved/Resized` 在非最大化、非最小化时更新）；
  最大化关闭时用它，保证下次取消最大化有合理落点。
- 原子写：先写 `windows-state.json.tmp`，再 rename 覆盖；失败只记日志，不阻止退出。
- 版本不匹配 / JSON 坏 / 字段非法 ⇒ 按「无记录」处理（不迁移、不猜）。

## 4. 启动决策（只判断一次）

在 `setup` 里、主窗口 `show()` 之前执行；用 `started` 标志保证只执行一次。

```text
state = load("windows-state.json")
if 缺失 / 坏 / 版本不符:              → Fit(primary)                 (A)
target = identify(state.monitor)
if target == null:                    → Fit(primary)                 (C2)
if state.maximized:                   → Maximize(target)             (M)

if corners_valid(state.corners):      → Restore(target, 四角)        (B)
else:                                 → Fit(target)                  (C1)
```

`corners_valid` = 四角有限、`0 ≤ x1 < x2 ≤ 1`、`0 ≤ y1 < y2 ≤ 1`（1e-6 容差）。
窗口在关闭时被拖出屏幕、或横跨两块屏导致比例越界，都算无效 → C1。
判断之后不再重判、不换布局重试。

| 情况 | 条件 | 操作 |
| --- | --- | --- |
| A | 无记录 / 文件坏 / 版本未知 | 主屏 `fitWorkArea` |
| B | 有记录、四角有效、原屏可识别 | 移到原屏，按四角比例还原 |
| C1 | 有记录、原屏可识别、四角无效 | 移到原屏，`fitWorkArea` |
| C2 | 原屏不可识别 | 主屏 `fitWorkArea` |
| M | 记录为最大化、原屏可识别 | 移到原屏并最大化 |

## 5. 三种操作

### 5.1 还原（四角映射）

```text
x1 = work.x + round(corners.x1 × work.width)
y1 = work.y + round(corners.y1 × work.height)
x2 = work.x + round(corners.x2 × work.width)
y2 = work.y + round(corners.y2 × work.height)
w = max(x2 - x1, min_w)     // 单边保底
h = max(y2 - y1, min_h)
set_inner_rect(x1, y1, w, h)
ensure_titlebar_visible(work)
```

### 5.2 Shift+最大化回退

直接复用 `window_layout::apply_inset_layout`（现有 `window_fit_work_area` 的几何），
只把目标尺寸按单边抬到最小尺寸。手动 Shift+点击与启动回退走同一份实现。

### 5.3 最大化

先摆正常态内框（记录的有效四角，或留白矩形），**不立刻 `maximize()`**：
Windows 的 `maximize()` 会 `ShowWindow`，闪屏期间对隐藏主窗口调用可能让它提前露出来。
改为记一个 `pending_maximize`，到 `reveal_main()` 的 `show()` 前一刻再真正最大化
（若它自己显示，刚好就是 reveal 的那一刻）；之后照常 `show()`。
如果工作区本身小于最小窗口尺寸，最大化后仍小于，就到此为止，不重试。

**正常态还必须同步进 Windows 的 restore 矩形**（2026-10-31 真机修复）：
`tao` 的 `set_size` / `set_position` 走异步 `SetWindowPos`，紧接着 maximize 时，
Windows 可能仍保留窗口创建阶段的默认 `rcNormalPosition`（曾实测 763×507，而记录为
1482×990）。因此：

- 启动分支缓存的是**经过校验的目标正常态**，不缓存异步读回的旧值；
- `reveal_main()` 的 `show()` 后调用 `sync_restore_after_show()`，用
  `SetWindowPlacement.rcNormalPosition` 写入这份正常态（不改变 showCmd）；
- 标题栏与命令面板的 toggle 走 Rust `window_toggle_maximize`：反最大化前优先取
  内存正常态，先写 restore，再 unmaximize，并复核实际尺寸；系统 restore 只在缓存缺失时兜底。

### 5.4 最小尺寸保底

- 单边：高算 700 → 800；宽算 1300 → 1300 原样（100% 缩放示例）。
- `min_w = round(1200 × target.scale)`、`min_h = round(800 × target.scale)`
  （`minWidth/minHeight` 是逻辑像素，目标是物理像素）。
- 用于还原、Shift+最大化回退，以及**反最大化按钮**：恢复窗口前如果系统或缓存的
  正常态宽/高小于最小值，各自抬到最小值，再执行还原并复核实际内框。
- 保底后超出工作区（屏幕放不下最小窗口）——接受，不缩小、不换布局、不重试。

### 5.5 titlebar 保底

三个操作末尾各做一次：取内框顶部 y，若不在目标屏工作区内，把窗口 y 设为
`work.y`（该屏最高处），保证 title 可见。只做一次，不重新判断、不循环。

### 5.6 不循环

- 启动决策只发生一次；之后三种操作各自跑完。
- `set_inner_rect` 里保留现有的 2 轮**位置读回修正**（1px 级偏差，跨 DPI/边框偏移），
  它不是可见性判断，也不是回退重试。
- 没有任何「还不行就再换一种布局」的代码路径。

## 6. 单实例

- `tauri-plugin-single-instance`，注册在所有插件之前。
- 第二个实例：插件直接退出；回调对已有 `main` 做 `unminimize()` + `set_focus()`。
- `tauri::is_dev()` 为真（`pnpm tauri dev`）时不注册，避免 dev 与已安装正式版互相踢；
  `pnpm debug:win` 带 `custom-protocol`，`is_dev()` 为假，仍生效。
- 同时保证 `windows-state.json` 只有一个写者。

## 7. 影响面

| 文件 | 改动 |
| --- | --- |
| `src-tauri/src/window_layout.rs` | `PixelRect`/`inset_rect` 提为 `pub(crate)`；最小尺寸保底；抽出 `set_inner_rect` / `apply_inset_layout`；titlebar 保底；Win32 restore 矩形读写与内外框换算 |
| `src-tauri/src/window_state.rs`（新） | 记录、四角比例、屏幕识别、单次决策、三种操作、事件缓存、文件读写、`window_toggle_maximize`、show 后 restore 同步、单测 |
| `src-tauri/src/lib.rs` | 模块注册；`setup` 里装事件并在 show 前还原；`reveal_main` 前应用 pending maximize、show 后同步 restore；`RunEvent::Exit` 兜底；单实例插件 |
| `src/api/window.ts` | toggle 改发 Rust `window_toggle_maximize`；其它窗口 API 不变 |
| `src-tauri/Cargo.toml` | `tauri-plugin-single-instance`；`windows-sys` 补 `Win32_UI_WindowsAndMessaging` |
| `THIRD-PARTY-NOTICES.md` | 重新生成第三方声明 |
| 前端 / 设计稿 / XMP / 数据库 schema | **无改动** |

## 8. 测试

### 单元测试

- 四角比例往返：同工作区、不同分辨率、负坐标；
- 比例合法性：0–1、越界、倒序、NaN、零宽/零高；
- 最小尺寸保底：单边 700→800 / 1300→1300；150% 下 1200×800 逻辑 → 1800×1200 物理；
- 屏幕识别优先级：PnP → GDI + rect → GDI → rect → 无；
- 决策表：A / B / C1 / C2 / M；
- 最大化/最小化关闭的写入与清除；
- `windows-state.json` 往返、坏 JSON、未知版本、字段非法、文件缺失；
- titlebar 保底：上方 / 下方 / 正常 / 负坐标；
- restore 内外框换算往返、负坐标、异常小矩形不产生 0 尺寸；反最大化宽高独立保底。

### 真机验收（崔总）

- 首次启动（无文件）→ 主屏留白布局；
- 单屏移动/缩放后正常关闭 → 重启回原位、原尺寸（±2px）；
- 双屏不同 DPI：在主屏/副屏分别关闭 → 重启各自回原屏；
- 同一块屏改分辨率/缩放 → 重启按四角比例还原（仍在屏内）；
- 记录后拔掉副屏 → 重启：原屏不可识别 → 主屏留白布局，任何部分不超出；
- 最大化关闭 → 重启在原屏最大化；最小化关闭 → 清记录，重启主屏留白布局；
- **最大化关闭 → 重开 → 反最大化**：恢复上次正常态，不回到 Windows 默认小矩形；
  已实测正常态 2432×1332 可精确还原，最小尺寸场景为 1500×1000（125%）；
- 记录被改坏/删除 → 重启回退，不崩；
- 125% 缩放：最小尺寸保底生效；
- 任务栏换边/自动隐藏、副屏在左侧（负坐标）、竖屏；
- 双击图标启动第二个实例：第二个退出，已有窗口被聚焦。

> 1080p@150% / 1366×768 这类放不下 1200×800 逻辑最小窗的配置不在支持范围
> （崔总 2026-10-31 明确）；这些屏幕上窗口可能超出工作区，属于「撞上最小尺寸」的
> 终止状态，不当作 bug、不触发回退循环。

## 9. 不做

- 不做「缩放」概念、不做按窗口宽高比统一缩放；
- 不做小屏兼容降级/自适应最小尺寸；
- 不做按显示拓扑（dock/家里）分别记忆；
- 不做手动「窗口归位」命令；
- 不做周期性/防抖保存；
- splash 不跟随主窗口；
- 不改设计稿、XMP、catalog、app.db schema、前端命令。

## 10. 验收清单

- [x] `cargo test --workspace`
- [x] `cargo check --workspace`
- [x] Windows 侧 `cargo check -p raybend-desktop --features custom-protocol`（含 `#[cfg(windows)]` PnP / 单实例）
- [x] `pnpm typecheck && pnpm test && pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n && pnpm build`
- [ ] 真机：首次 / 单屏 / 双屏 / 改分辨率 / 拔屏 / 最大化 / 最小化 / 任务栏 / 单实例
