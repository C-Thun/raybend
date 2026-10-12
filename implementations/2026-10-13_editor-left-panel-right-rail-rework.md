# 编辑左栏改版：定稿面板 / 底部总览 · 信息 + 右栏只留直方图与可滚动参数区

**完成时间**：2026-10-13 02:47:47 CST（崔总 2026-10-12 晚口述需求；画稿先改、代码后改）
**画稿**：`design/editor.pen`（主稿 + ToolsBar 三段式 + 调整面板 + Tab 三档 + 左栏底部状态帧 + 三张派生状态帧）
**规格**：`design/editor.md` §2.1 / §2.2 / §3.1 / §3.11 + 末尾「2026-10-12 左栏改版」节；`memory/DESIGN.md` §14.9

## 一、需求（崔总原文要点）

1. 把右栏的**定稿**搬到 `toolsbar left`，放在 LUT 图标**左边**（图标用 browse tile 上定稿数量指示的同款）；
   左列从此可以切 LUT / 定稿，反复点开关的逻辑与之前一致（再点同一个 = 关整列）。
2. **图片总览固定到左列底部**：无论上面是 issue 还是 LUT 面板，底部都是总览；
   **整列关掉（toolsbar left 全关 / Tab ③）时总览一起消失**，不常驻。
3. 总览与 browse 的总览**用同一套组件**，缩放时带**准确的显示区域框**。
4. `toolsbar right` 的**定稿按钮**移进左列定稿面板（上部或贴底大按钮，图标不丢）→ right 只剩重置。
5. 左列底部是**页签组**：`总览` / `信息`；切到信息时底部区**升高约 80%**，挤占上方面板；两者都带滚动。
6. 右栏改成：**直方图固定在顶部**（不滚动）+ 下面才是可滚动的「影调系列 + 曲线系列」。

## 二、影响调研（`AGENTS.md` §2.23：先列会碰到的模块）

| 模块 | 碰法 |
| --- | --- |
| `lib/editor-chrome.ts` | 面板意图从 `lutOpen: boolean` 变成 `leftPanel: "lut"｜"issues"｜null`；③ 档的记忆要记住**是哪一个** |
| `lib/editor-prefs.ts` | 设备级偏好升 **v3**（v2 的 `lutOpen` 一次性迁移） |
| `features/editor/store.ts` | 面板读数 / 动作改名（`leftVisible` / `leftPanel` / `toggleLeft` / `setLeftPanel`） |
| `features/editor/toolbar.tsx` | left 段两个开关；right 段只剩重置 |
| `features/commands/catalog.ts` + `App.tsx` | 新增 `editor.issues.toggle` 命令与 `toggleIssues` 依赖；right 段不再传 `canFinalize/onFinalize` |
| `features/editor/panels.tsx` | 第 1 组整块移除；直方图提为固定块、下半个可滚动区 |
| `features/editor/left-panel.tsx`（新） | 左列：上面板（LUT / 定稿）+ 底部页签（总览 / 信息） |
| `features/editor/layout.ts`（新） | 左列宽 280 与底部块高度公式的唯一出处（LUT 面板原来自带 `LUT_PANEL_WIDTH`，现共用） |
| `features/editor/overview-rect.ts`（新） | 视野框：Rust 渲染状态 → `visibleRect()`（与浏览右栏同一变换） |
| `workspaces/editor/EditorWorkspace.tsx` | 左 aside 换成 `EditorLeftPanel`；右 aside 去掉搬走的 props |
| `scripts/ui-smoke.mjs` | ③ 档注释、右栏滚动锚点改为 `[data-editor-param-scroll]` |
| 文档 | `design/editor.md`、`memory/DESIGN.md` §14.9、`AGENTS.md` §12 的 `PreviewFrame` 行 |

## 三、关键决策与理由

1. **定稿主按钮贴底整宽**（需求给了「上部或贴底」两个选项）：列表滚动时按钮始终可见，
   且不与「SOOC / RAW / 最近编辑」三种基准行的点击区混在一起。图标与文案沿用原按钮
   （`IconBookmark` + `editor.issue.toolbar`）。
2. **总览 / 信息是一个页签组、跟着左列显隐**：`leftVisible()` 为假时整个 aside `hidden`，
   底部块自然一起消失 —— 不给它单独的开合状态（避免第二份真相）。
3. **视野框复用浏览右栏的数学**：`visibleRect()`（`components/ui/viewer/store.ts`）是纯函数、
   已有测试；编辑侧只做单位换算（`zoom` 是图像→**物理**像素，洞口/pan 是 CSS/物理，要除 DPR）。
   **旋转草稿（旋转工具开着）不画框**：那时可见区域在图像空间是斜四边形，轴对齐框会撒谎。
4. **底部块高度用 CSS `calc`**（`layout.ts`）：预览高度随栏宽（3:2）与密度档（`--panel-pad`）变化，
   信息态 ×1.8 直接乘在同一个表达式上，不写死像素。
5. **`Tab` ③ 档记住「关掉的是哪一个」**：恢复时回到原面板（LUT 或定稿），不漂成另一个。
   显式开关仍然优先，且 ③ 里点开关会亮「临时通行证」（既有规则，不改）。
6. **工具控制块（裁切 / 旋转）固定在直方图之上**：它们是工具的临时主 UI，不能随滚动跑掉；
   右栏「不替用户重置阅读位置」的旧纪律不变。
7. **命令体系**（`AGENTS.md` §2.15）：新增 `editor.issues.toggle`（`cmd.editor.issues`），
   **无默认热键** —— 与 `editor.lut.toggle` 同口径（面板开关类命令，避免抢编辑键）。
8. **偏好升 v3 而不是复用 v2 字段**：结构变了（布尔 → 三态），按 §2.16 走版本化 key + 显式迁移；
   v1 的 LUT 分类读取路径保持不变。

## 四、验证（命令原文与结果）

| 命令 | 结果 |
| --- | --- |
| `pnpm typecheck` | 通过（无输出） |
| `pnpm test` | **1308 passed / 0 failed**（含新增 `editor-chrome` 左列面板语义 4 条、`editor-prefs` v2→v3 迁移、`overview-rect` 6 条） |
| `pnpm lint:arch` / `lint:colors` / `lint:i18n` | 三条全绿 |
| `pnpm build` | 构建通过（3.23s，仅既有的 chunk 体积提示） |
| `pnpm dev` + `pnpm smoke:ui http://localhost:1420/` | **`problems: []`**；编辑器读数：`workspace=true`、`hole=true`、`leftInViewOnly=0`、`leftAfterToggle=280`（③ 档里点左列开关确实把左列开出来，宽度 = 280）、`advanced={curve:335,preset:335,back:335,treeOverflow:"auto",treeInside:true,colorGap:8,pageOverflow:0}` |
| Pencil | 主稿与派生帧截图逐帧核对（工具条两开关 / 左列底部页签与预览 / 右栏直方图固定 + 滚动区）；`WCT5O` 重画为「左栏底部（总览 · 信息）」 |

## 五、遗留与未验证（归人类 E2E）

* **真机（Windows）目视**：左列底部总览的比例 / 视野框对齐、信息态 ×1.8 的观感、
  密度档（紧凑 / 宽松）下的高度；这些按 `AGENTS.md` §2.8 归人类。
* **真实照片下的视野框**：只在单测里钉了单位换算；缩放 / 拖动 / 适应窗口 / 100% 的实际跟手
  需真机确认（Rust 视口状态为唯一真相，前端只读）。
* 画稿里左列底部预览是示意高度（196），实现按 `calc` 计算，密度档下会有几像素出入（属预期）。
* 后续计划（崔总说以后再做）：左列「修改层级面板」、右栏「蒙版 / 智能区域识别 / 基点控制」——
  本轮不做，未登记进 `memory/FUTURE.md`（等崔总明确后补）。
