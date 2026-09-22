# M3-W1：编辑工作区骨架与洞口契约

完成时间：2026-09-23 05:12:40 CST

## 1. 本次范围

按 `plans/M3-W1.md` 把「编辑」从一个空壳（切过去落回导入工作区）做成**一块真的工作区**，
并把 W2 的 GPU 视口要用的**洞口契约**先落地。**视口里还没有照片像素、调整还没有真实效果**
（这两件事分别是 W2 / W3，界面上与记录里都写明了）。

同时按人类 2026-09-23 的追加口述改了一处设计：拉杆未填充段 `--slider-track` **加 50% 浓度**
（另见 `implementations/2026-09-23_slider-track-half-opacity.md`）。

## 2. 关键决策与理由

1. **M3 波次重切（`PLAN.md` §M3）**：W0 设计 → **W1 界面骨架与洞口契约** → W2 GPU 视口 →
   W3 编辑栈与常规调整 → W4 清晰度/镜头/LUT/issue → W5 三工具 → W6 收口。
   原则是「每一波结束人都能看到/用到东西」，并把「人看了才知道对不对的（布局/面板/开关/命令）」
   与「做出来才知道能不能用的（GPU/管线/算法）」分开推进，两者互不阻塞。
2. **`toolsbar` 三段式改的是现有组件**（`src/shell/ToolsBar.tsx`），没有新造一份：
   左/右段贴边、中段用**绝对定位铺满整条**（这样它在**整条的正中央**，与左右段多宽无关），
   挤压时左右段各挂一条 `pointer-events-none` 的渐变把中段柔化吃掉（不是硬边 + 阴影）。
3. **`Tab` 档位改成「带 flow 参数的纯函数」**（`src/lib/viewer-chrome.ts`）：
   一个档位 = 一组布尔（左/右/胶片带），三张循环表 —— browse 四档（**逐档照抄，回归不变**）、
   import 三档（2026-09-23 改口径：中间那档先丢左右两列）、editor 三档（先藏胶片带）。
   `PhotoViewingController` 增加 `chromeMode` 依赖与 `showsLeft/showsRight/filmVisible` 三个读数，
   工作区不再自己拼 `chromeShowsX(chrome())`。
4. **editor 的「仅 view」与 LUT 面板之间的交互做成纯状态机**（`src/lib/editor-chrome.ts`）：
   只有切到 ③ 才动面板（开着就关掉并记住「是我关的」，本来关着则无动作），离开时按记忆恢复；
   用户显式点开关**永远优先**（并清掉那条记忆）。持久化的是**用户意图**而非「这一刻可不可见」。
5. **洞口契约：前端只报原始事实**（`src/lib/editor-viewport.ts` + `src/api/editor.ts` +
   `src-tauri/src/editor.rs`）：CSS 矩形 + 运行时 DPR + CSS 视口尺寸三样，换算（×DPR 出物理矩形）
   只在 Rust 做；回读接口把 CSS 与物理两份一起带回来，一眼就能看出「DPR 有没有乘对」——
   这正是 §7.9 那四个「假通过」的破法。
6. **节流用「帧级合并 + 只发最新值」而不是「N 毫秒丢弃」**：后者会把尾样本丢掉，
   而 §7.9 明确要求停手那一次也要发。相同值不重发（`ResizeObserver` 会为空挂载回调）。
7. **复用优先（这一波踩实了几处）**：
   * 胶片带 / 照片状态栏 / 三点缩放把手直接用现成组件；
   * 右栏页签条直接用 `SegmentedControl`（凹槽 + 主色胶囊，与设计稿同形）；
   * 裁切比例下拉与镜头配置文件下拉用现成的 `Menu`（带选中对勾），**没有新写 Select**；
   * `photosFromSource` / `viewingInfoOf`（tiles → 看图件）从 `PhotoGrid` 与 `BrowseWorkspace`
     里**抽成一份**放进 `components/ui/viewer/photos.ts`，编辑工作区直接调它；
   * 直方图的取数 + 缓存 + 画图抽成 `components/ui/HistogramPanel.tsx`（浏览右栏与编辑右栏共用一份），
     取数由调用方注入（`components/ui` 不许 import `api`）；
   * 胶片带尺寸偏好扩成三个作用域（`lib/film-strip-prefs.ts`），**同一份 store**，
     只是 editor 一行自己的存储键 —— 顺手把 `FilmStripViewer` 收窄成「胶片带真正用到的三个读数」，
     于是编辑侧不用为了凑类型造第二个 `ViewerStore`。
8. **当前在编哪张 = browse store 的锚点**（`AGENTS.md` §11.4 红线）：编辑不另造选择模型，
   胶片带点选写回 `browse.select`，所以「编辑里换一张、回浏览还是它」自然成立。

## 3. 本波明确不做（留在界面与计划里，不是遗漏）

| 不做的事 | 谁做 | 界面上怎么说的 |
| --- | --- | --- |
| 视口里的照片像素 | W2 | 水印：「编辑视口（GPU 直绘）在 M3-W2 接入 —— 洞口已经为它留好」 |
| 参数拉杆改变画面 / 落库 | W3 | `PendingNote`：「现在拖动能改数值，但还不会改变画面」 |
| 曲线编辑（现在是恒等曲线的静态示意） | W3 | `PendingNote`：「曲线编辑在 M3-W3 接入」 |
| issue 与编辑栈落库 | W3 | 定稿页签：SOOC + 「编辑中」两条 + 禁用的「定稿为新 Issue」 |
| LUT 导入与应用 | W4 | 「导入 LUT」按钮**禁用 + Tooltip 写明 W4 接入**（不假装能点） |
| 裁切/旋转/对比的画布框线 | W5 | 三个工具能切模式、右栏控制块（比例/角度/取消/确认）已就位 |

## 4. 验证方式（Agent 冒烟）

| 项 | 结果 |
| --- | --- |
| `npx tsc --noEmit`（**权威**） | 通过（exit 0）。另用 `MessageKey[]` 临时证明文件确认 `editor.*` 全部键合法后删除 |
| `pnpm test` | **835 通过 / 0 失败**（新增 5 个测试文件：档位循环、LUT 面板状态机、LUT 库、编辑偏好、洞口上报、参数与空态与胶片带适配） |
| `pnpm lint:colors` | ✓ 无硬编码色值 |
| `pnpm lint:arch` | ✓ 分层依赖合规 |
| `pnpm lint:i18n` | ✓ 无硬编码中文文案 |
| `pnpm build` | ✓ 构建通过（3.2s，无警告） |
| `cargo test -p raybend-desktop --lib` | **50 通过 / 0 失败**（含 `editor::tests` 6 条：DPR 换算、非法值拒绝、0 尺寸合法、快照计数、camelCase 键名、`every_state_type_is_managed`） |
| `pnpm smoke:ui http://localhost:1420/`（CDP，补跑） | 外壳**真的画出来了**（`hasMain` / `hasSearch` / `browseTools` / `restoredImport` 全真），**控制台无 error / warning**；`problems` 只剩「画廊里没有 X 演示」这类 **dev 陈列室** 断言（打应用外壳这条路径本来就不挂那些演示）——与本波无关 |

**没有验证的（归人类，`AGENTS.md` §2.8）**：界面的实际观感（三列比例、右栏 255px 够不够塞三组页签、
渐变挤压好不好看）、真机 DPI 下洞口上报的实际数值、编辑流程的手感。

## 5. 遗留与待人类拍板

1. `design/editor.md` §7 的待决项（右栏是否给 editor 单独一档宽度、左栏宽度数值、旋转时框外是否压暗、
   禁用态画法、issue 列表信息量）——本波按通用令牌实现，改宽度只需改令牌；
2. 仓库根目录有上一轮遗留的 `probe-slider.html` / `probe-slider.tsx`（临时排障页，**未提交**，
   本次 commit 已排除），需要时人工删除；
3. CDP 冒烟已补跑（见上表）；它验的是「页面画得出来 + 控制台干净」，
   **不含**编辑工作区的交互（切到编辑、点工具、拖拉杆）——那些归 W2 起的人工真机验收。

## 6. 改动文件

**新增（前端）**：`src/lib/editor-chrome.ts`（+test）、`src/lib/editor-prefs.ts`（+test）、
`src/lib/lut-library.ts`（+test）、`src/lib/editor-viewport.ts`（+test）、
`src/components/ui/HistogramPanel.tsx`、`src/components/ui/viewer/photos.ts`、
`src/api/editor.ts`、`src/features/editor/{index.ts,store.ts,params.ts,SliderRow.tsx,parts.tsx,panels.tsx,lut-panel.tsx,viewport.tsx,toolbar.tsx,source.ts,model.test.ts}`、
`src/workspaces/editor/EditorWorkspace.tsx`

**新增（Rust）**：`src-tauri/src/editor.rs`（含 6 条单测）

**修改**：`src/shell/ToolsBar.tsx`（三段式）、`src/shell/flow.ts`、`src/lib/viewer-chrome.ts`（重写）、
`src/features/photo-grid/viewing.ts`、`src/workspaces/{browse/BrowseWorkspace.tsx,import/ImportWorkspace.tsx}`、
`src/features/photo-grid/PhotoGrid.tsx`（→ 共用转换）、`src/features/browse/ViewerReadout.tsx`（→ HistogramPanel）、
`src/components/ui/viewer/{FilmStrip.tsx,index.ts}`、`src/features/exif-strip/index.ts`（导出格式化函数）、
`src/features/commands/catalog.ts`（编辑命令）、`src/api/db.ts`（一个设置键）、`src/lib/film-strip-prefs.ts`（三个作用域）、
`src/i18n/{zh-CN.ts,en-US.ts}`（约 87 条编辑文案）、`src/App.tsx`（路由与三段式接线）、
`src/styles/tokens.css`（`--slider-track`）、`DESIGN.md`、`PLAN.md`、`AGENTS.md`（术语）

**计划文件**：`plans/M3-W1.md`
