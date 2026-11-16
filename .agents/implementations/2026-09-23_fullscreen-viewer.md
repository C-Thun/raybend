# 全屏看图：flowbar 磁铁换成全屏按钮，另开无边框窗口沉浸式看图

完成时间：2026-09-23 15:31:20 CST

## 需求（人类 2026-09-23 口述）

- flowbar 右端、图片信息右边的**磁铁（吸附）按钮去掉**（暂时不做吸附）；
- 换成**全屏按钮**，且**跟随 picture info 一起显示**（信息只在选中照片时出现，只有选中的照片才能全屏看——同一件事的两个面）；
- 点击后**不受主窗口形态影响**（实质是另开一个独立窗口），在**主程序窗口所在的那块屏幕**全屏显示；
- 全屏内：默认适应窗口，双击 → 100%，滚轮缩放，**只看单图**（当前选中的），`Esc` / `Enter` 退出，`←`/`→`、`PageUp`/`PageDown` 在同目录图片间切换；
- 是**无 UI 的沉浸式**看图。

## 改动范围

### Rust（`src-tauri/`）

| 文件 | 内容 |
| --- | --- |
| `src/fullscreen.rs`（新） | `FullscreenItem` / `FullscreenPayload`（camelCase）、`FullscreenState`（清单 + 下标，进程级）、`fullscreen_open` / `fullscreen_payload` / `fullscreen_close` 三条命令 + 4 条单测（校验拒绝空清单/越界**不夹取**、序列化键名、状态往返、JSON 往返含中文空格路径） |
| `src/lib.rs` | 模块注册、`manage(FullscreenState)`、三条命令进 handler 表 |
| `src/contract.rs` | `FullscreenItem` / `FullscreenPayload` 进契约登记表 + `fullscreen_payload_matches_contract` 断言 |

开窗要点：

- 幂等：窗口已开着 → `emit` 清单事件 + `set_focus`，**不重建**；
- 建窗：`decorations(false)` + `resizable(false)` + `skip_taskbar(true)` + `visible(false)`，URL 用 `index.html?fullscreen=1`（与 spike 页同一套查询串选页，dev/打包两态都不影响资源解析）；
- **先摆位再全屏**：`set_fullscreen(true)` 是「在窗口当前所在屏幕上全屏」，所以必须先按主窗口所在显示器的**物理像素** `set_position`/`set_size`，再全屏再 `show()`——顺序反了会先在主屏闪一下；
- 清单走 Rust 状态 + `fullscreen://payload` 事件而不是 URL：一个目录几千张时 URL 装不下，且「已开着的窗口换图」也需要事件推给页面；
- `current_monitor()` 属于「拖动期间可能阻塞」的窗口查询（AGENTS.md §7.9），这里只在**点按钮那一刻**问一次（非渲染循环），可接受——注释里写明别搬到每帧跑的地方。

### 前端

| 文件 | 内容 |
| --- | --- |
| `src/api/fullscreen.ts`（新） | `openFullscreen` / `closeFullscreen` / `getFullscreenPayload` / `onFullscreenPayload`（浏览器里全部空操作/null） |
| `src/api/types.ts` | `FullscreenItem` / `FullscreenPayload` 类型 |
| `src/lib/fullscreen-target.ts`（新）+ 测试 | 纯函数 `buildFullscreenTarget(显示序, 锚点)` → `{items, index}` | null（锚点不在清单里宁可不开）；结构类型不反向依赖 api（防环，与 `CommandMarkIntent` 同做法） |
| `src/features/fullscreen/FullscreenViewer.tsx`（新） | 沉浸页：复用 `createViewerStore`（状态/锚点数学/图像缓存）+ `createWheelZoom`（滚轮手感）+ 拖动平移 + 双击 toggleFit；`Esc`/`Enter` 关窗，`←`/`→`/`PgUp`/`PgDn` 走 store 的 `next()`/`prev()`（**到头停住不循环**——主窗口看图的既定口径，不写第二套步进）；挂载取一次 payload + 订阅换图；`data-fullscreen="open"` 供冒烟定位 |
| `src/index.tsx` | `?fullscreen=1` 静态 import 分支（正式功能，不进 DEV 三元） |
| `src/shell/FlowBar.tsx` | 磁铁 ToggleBlock 移除；`onFullscreen?: () => void` 新 prop，按钮只在 `exif != null && onFullscreen != null` 时渲染（**跟随图片信息 + 真能开才出现**，不存在按了没反应的按钮） |
| `src/App.tsx` | `fullscreen()`：当前工作区 `fullscreenTarget()` → 非 null 才给动作；`commandDeps.viewer.fullscreen` + `<FlowBar onFullscreen>`；IPC 失败走 toast（danger） |
| `features/{browse,import,editor}/actions.ts` | 动作槽加 `fullscreenTarget: () => FullscreenTarget | null`（显示序是工作区的知识） |
| 三个工作区 | `createMemo(() => buildFullscreenTarget(photosFromSource(gridSource()), 锚点))`——**与网格同一个函数、同一份数据源**，全屏里的邻居与网格逐张一致（编辑侧用现成的 `photos()`） |
| `features/commands/catalog.ts` | 新命令 `viewer.fullscreen`（group view / scope viewer / 默认键 `F`，未占用），`when` = 「真的能开」——与按钮同一个读数，不会按钮在、键不灵 |
| `src/i18n/{zh-CN,en-US}.ts` | `flow.tool.fullscreen`（替换 `flow.tool.snap`）、`fullscreen.empty` / `fullscreen.failed`、`cmd.viewer.fullscreen` |
| `src/dev/KitchenSink.tsx` | 陈列室的 ToggleBlock 示例换 `flow.tool.fullscreen` + `IconMaximize`（原 snap 键已删） |
| `src/api/dto-contract.json` + `dto-contract.test.ts` | 两个新 DTO 的键表 + 前端覆盖检查 |

### 文档 / 冒烟

| 文件 | 内容 |
| --- | --- |
| `DESIGN.md` | 词汇表 `flow.tool.snap` → `flow.tool.fullscreen`；变更记录加一行（磁铁撤销、全屏按钮口径） |
| `design/main.md` | flowbar 右端表格行 + 「关于最右端」改写（磁铁撤销、全屏按钮口径；ToggleBlock 形态说明保留作组件参考） |
| `BROWSE.md` / `design/browse.md` | 「形态与『吸附』一致」的举例改为「按时间」（吸附已不在 flowbar） |
| `scripts/ui-smoke.mjs` | ① 外壳断言：`吸附`/`Snap` 按钮必须为 0；没选中照片时全屏按钮必须不出现。② 末段新增 `?fullscreen=1` 页检查：`data-fullscreen="open"` 挂上、拿不到清单时空态提示在、应用外壳没混进来 |

## 关键决策

1. **另开窗口而不是主窗口全屏**：人类口径「不受主窗口形态影响」+ Esc 只该关看图这扇窗；
2. **100% 的含义 = 所加载渲染图的 1:1**：取图与主窗口看图同一个 purpose（`screen`，长边 1920），payload 不带 `natural`，`onLoad` 用真实字节尺寸——大图在 4K 上不会再取更清的档（那需要新 purpose，见遗留）；
3. **`F` 当默认键**：未占用（现单键 `0/1/-/=/P/U/X/i`），非危险命令、非保留键；
4. **步进不循环**：与 `store.goTo` 的注释口径一致（循环会让人以为还有更多照片），因此删掉了最初写的 `stepFullscreenIndex`（避免第二套步进实现）；
5. **按钮可用性上移到 App**：`fullscreenTarget() === null` 就不传 `onFullscreen` → 按钮整个不渲染；命令 `when` 用同一读数。

## 验证（Agent 冒烟，全绿）

- `npx tsc --noEmit`：0 错误；`pnpm test`：860 通过（含 fullscreen-target 3 条、dto-contract 更新后 3 条）；
- `pnpm lint:colors` / `lint:arch` / `lint:i18n`：✓；`pnpm build`：✓（`data-fullscreen` 已进产物 chunk）；
- `cargo test -p raybend-desktop --lib`：62 通过（新增 fullscreen.rs 4 条 + 契约断言 1 条）；`cargo clippy -p raybend-desktop`：干净；
- `pnpm smoke:ui`：`problems: []`（新增 flowbar 2 条 + fullscreenPage 3 条断言全过）。

## 归人类真机验收（Windows）

- 选中照片 → flowbar 全屏按钮出现；没选中时不出现（不是灰的）；
- 点按钮：**无边框窗口出现在主窗口所在那块屏幕**、铺满全屏、不占任务栏；多屏时换主窗口到另一块屏再试；
- 默认适应窗口；双击 ↔ 100%；滚轮以鼠标为锚缩放；拖动平移；
- `Esc` / `Enter` 退出；`←`/`→`/`PgUp`/`PgDn` 切图（到头停）；切图后回到适应窗口；
- 窗口开着时回主窗口再点另一张的全屏：**原窗口换图并置前**（不闪新窗）；
- 编辑工作流里：`flowinfo` 空态所以按钮不出现，按 `F` 应能全屏（这是遗留第 3 条的既知不一致，不是 bug）；

## 遗留 / 已知偏差

1. **`design/main.pen` 未同步**：磁铁画稿还在 `.pen` 里（本次只改了配套 .md），下次开 Pencil 时把 flowbar 右端改成全屏按钮；
2. **`ImagePurpose` 的 TS 类型与 Rust 不一致（既有问题，本次撞上）**：`src/api/db.ts` 的联合类型含 `"original"`，但 Rust `ImagePurpose::parse` 只认 `grid/strip/screen`——传 `original` 会被拒。本次按 `screen` 实现；要「真 1:1 原图像素」需在 Rust 加 `original` 档并两端同步；
3. **编辑工作流的 flowinfo 恒空 → 按钮不出现、但 `F` 键可用**：`App.tsx` 的 `flowInfo` 在 edit/export 分支返回 null，FlowBar 的按钮显隐跟着 `exif` 走，所以编辑里选中照片后**按钮不出现**；而命令 `viewer.fullscreen` 的 `when` 走 `fullscreenTarget()`（编辑侧真实提供清单），按 `F` 能全屏。若要编辑里也能点按钮，把 `flowInfo` 的 edit 分支补上编辑锚点的 EXIF 即可（一行改动）；
4. 全屏页未做「闲置隐藏鼠标光标」——沉浸式可再加，本次未做（保持最小）。

## 复查修复（同日，人类要求「检查一下实现有没有错误，有的话修掉」）

复查结论：**三处真问题**（都不是编译期能抓到的），已修并补测试：

| # | 问题 | 症状 | 修法 |
| --- | --- | --- | --- |
| 1 | **清单竞态** | 页面挂载时的 `getFullscreenPayload()` 与后来的 `fullscreen://payload` 事件**可能乱序到达** —— 初始读取若晚于事件返回，会拿旧清单覆盖新清单（换图后显示的还是上一张） | `FullscreenPayload` 加**单调递增 `revision`**（由 `FullscreenState::store` 递增，调用方填 0 即可），页面只应用版本更大的包（`appliedRevision` 守卫）；契约两侧同步该字段；新增 `revision_increases_on_every_store` 单测 |
| 2 | **关窗是异步的** | `close()` 走「请求关闭」，标签从窗口表里消失是异步的 —— 「按 `Esc` 后马上回主窗口再点全屏」可能撞上正在死掉的窗口，于是走复用分支**什么都不发生**（要点第二下） | 关窗改用 `destroy()`（立即销毁、当场释放标签），并在前端 `.catch(() => {})` —— `destroy` 之后 IPC 响应很可能回不来，不接就是一条「看着像功能坏了」的未处理拒绝 |
| 3 | **连点两下会弹假错误** | 第一下正在建窗、第二下也走到建窗 → 标签已被占 → 返回 `Err` → 前端弹一条 toast（用户什么都没做错） | 建窗失败时**先查窗口是否已在**：在就退化成「复用 + 换图」并返回 `Ok`，确实不在才把真错报出去 |

复查中**核实无误**的部分（不必改）：监视器定位用物理像素且「先摆位再全屏」（顺序反了会闪主屏）；`current_monitor()` 只在点击那一刻问一次（非渲染循环）；步进沿用 store 的 `next`/`prev`（到头停）；按钮显隐 = `exif != null && onFullscreen != null`；命令 `when` 与按钮同一个读数；浏览器里全部空操作。

验证（本轮全部重跑）：`npx tsc --noEmit` 0 错误；`pnpm test` 860 通过；`cargo test -p raybend-desktop --lib` **63** 通过（含新单测与契约断言）；`cargo clippy -p raybend-desktop` 干净；`pnpm smoke:ui` `problems: []`。
