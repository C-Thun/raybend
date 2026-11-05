# 编辑器：预设面板入左列、工具栏重排、拟合带文字、空格切层、色盘占位

> 完成时间：**2026-11-05 04:20:10 +08:00**
> 需求（崔总 2026-11-05，四项抉择当场拍板）：
> ① 预设面板移到 workspace right、入口按钮放 toolsbar left 的 LUT 右边 → **拍板：改成左列第四个面板（四选一互斥）**；
> ② 眼睛开关移到 mid 的「重做」右边、重置移到「撤销」左边；
> ③ 拟合按钮加文字「拟合」，并确保只有 RAW+JPG 都在时可用 → **拍板：有 RAW+JPG 且基底是 RAW**；
> ④ 空格键在默认层与当前层之间切换 → **拍板：只有默认层时什么都不做**；
> ⑤ 曲线与色彩管理之间加空面板「色盘」。

## 1. 改动一览

| # | 改动 | 落点 |
| --- | --- | --- |
| ① | 预设面板从右栏页签搬到**左列第四个面板**（与 编辑区/定稿/LUT 四选一互斥），入口 `IconBookmark` 在 toolsbar left 的 LUT 右边 | `lib/editor-chrome.ts`（`EditorLeftPanel` 加 `"preset"` + `editorPresetVisible`）、`features/editor/store.ts`（`presetVisible`）、`features/editor/toolbar.tsx`（第四个 `ToggleBlock`）、`features/editor/left-panel.tsx`（渲染 `PresetPanel` + 透传 7 个预设动作）、`features/editor/panels.tsx`（删页签与预设 props） |
| ② | 眼睛与重置搬进 toolsbar mid：顺序 `重置 · 撤销 · 重做 · 眼睛`；toolsbar right 在编辑器里清空（`EditorResetTool` 删除） | `features/editor/toolbar.tsx`、`features/editor/index.ts`、`src/App.tsx` |
| ③ | 「拟合」= 图标 + 文字；判据写死「**基底 RAW + 该照片 RAW/位图文件都在**（+ 机型档案有相机信息）」 | `features/editor/toolbar.tsx`、`workspaces/editor/EditorWorkspace.tsx::canAutoAdjust`（补注释说明 `hasRaw/hasBitmap` 是真实文件）、i18n `editor.autoAdjust` = 拟合 / Fit |
| ④ | `Space` 在默认层与当前层之间来回切（记「刚才那一层」；只有默认层时不发生任何事） | 新命令 `editor.area.toggleFocus`（`catalog.ts`，`defaultKey: "Space"`，`scenes: ["edit"]`）、判定抽成纯函数 `lib/editor-area-focus.ts` + 单测、`EditorWorkspace` 接线、`App.tsx` deps |
| ⑤ | 右栏第 3 组由 `曲线 · 预设 · 色彩管理` 变为 `曲线 · 色盘 · 色彩管理`；色盘是空面板（`StateWatermark` 空态） | `features/editor/panels.tsx`（`CurveTab` + `SwatchPanel`）、`features/editor/store.ts`（`advancedTab` 联合类型）、i18n `editor.group.swatch` / `editor.swatch.empty` |

## 2. 关键决策

1. **右栏页签的「等高留位」一并删除**：那套容器只为「曲线 ↔ 预设」两页签等高而存在；预设搬走后它只会给色盘留下
   一整块空白（正是崔总 2026-10-10 报过的同一个坑）。现在三个页签各自渲染。
2. **`Space` 的判定单独成纯函数**（`nextAreaFocus`）：三个边界（没有层 / 回到刚才那层 / 那层被删）各自有单测，
   组件里只剩「算出的 id 交给 `store.selectArea`」。输入框里按空格仍是打字 —— 分发器统一经 `shouldHandleKey` 跳过输入控件。
3. **`canToggleAreaFocus` 参与命令可用性**：只有默认层时命令 `when` 不成立，`Space` 不会被 `preventDefault`，
   不影响按钮/滚动等浏览器默认行为。
4. **拟合按钮的禁用靠 `canAutoAdjust`**（已在 `disabled` 与命令 `when` 两处生效），本轮只把判据讲清楚并补注释，不新增第二个判据。

## 3. 验证

- `pnpm typecheck` 通过。
- `pnpm test`：**1420 passed / 0 failed**（新增 `lib/editor-area-focus.test.ts` 5 条：没有层 / 在层里 / 回刚才那层 /
  那层被删 / 反复横跳）。
- `pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n`：全部通过。
- `pnpm build`：通过。
- `pnpm smoke:ui`：**problems: []** —— 脚本已按新版更新：右栏校验「色盘页签是空面板且不撑高」，
  预设面板改从 toolsbar left 第 4 个开关打开并校验「预设树有界自滚（不把左列撑长）」。
- **未做**：真机 E2E（工具栏手感、空格、色盘占位观感）由崔总确认。

## 4. 同步的文档

- `design/editor.md`：§2.1 toolsbar 三段表（left 四个开关 / mid 新顺序 / right 空）、§3 第 3 组页签、
  §3.10 预设面板位置、新增 §3.13 色盘。
- `specs/editor-presets.md` §1：预设搬家的变更说明（其余语义不变）。
- `specs/editor-area.md`：命令表补 `Space` 切层。
- `AGENTS.md` §12 与 `memory/COMPONENTS.md` §2：登记 `SectionTitle`（导出那批新增的原语）。

## 5. 遗留

1. `design/editor.pen` 的**画布帧**未随本轮重排（工具栏左右段、左列第四个面板、色盘页签）—— 文字规格已改，
   画布待崔总点头后一次性刷，避免一边改一边返工。
2. toolsbar right 在编辑器里为空后，`ToolsBar` 会用 `hasRightTools` 判空；本轮保持传入 true（空态不绘制），
   以后若要严格化可把 `hasRightTools` 改成按 `flow` 计算。
3. 预设面板在左列时的高度与 LUT 面板共用 `flex-1`，真机上如果觉得列表偏矮，可与 LUT 一起调 `layout.ts`。
