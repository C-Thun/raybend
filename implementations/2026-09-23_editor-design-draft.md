# M3-W0：编辑工作区设计稿（editor.pen 第一批）

完成时间：2026-09-23 03:19:22 CST

## 1. 本次范围

按 `prompts/editor.pd`（人类口述、**尚未写完**）把 M3 的编辑工作区界面落成画布：

- 新建 `design/editor.pen`（原本只有一个空 `Frame` + 全套变量）；
- 配同名说明书 `design/editor.md`（§5.1 纪律）；
- 连带把两条**人类明确要求写进 `AGENTS.md`** 的规则落进文档（toolsbar 三段式、取消/确认顺序）；
- `DESIGN.md` 登记两个新令牌 + 编辑视口覆盖线与右栏三页签组两节。

**没有改任何实现代码**（M3 还没开工，本轮纯设计）。

## 2. 产出（`design/editor.pen` 10 个顶层帧）

| Frame | id | 内容 |
| --- | --- | --- |
| `Shell / Editor / Dark-Compact` | `P1JiB` | 主稿：外壳 + 左 LUT / 中 视口+胶片带+statusbar / 右 三页签组 |
| `States / Editor / 裁切` | `aH6HI` | 裁切框（角/边把手 + 主色井字 + 框外压暗）+ 右栏裁切控制块 |
| `States / Editor / 旋转` | `Y9Nn5u` | 斜画布 + 水平内接展示框（辅色井字）+ 拉线校正 + 角度控制块 |
| `States / Editor / 对比` | `gmHtH` | 对分线 + 双侧外粗线把手 + statusbar 的 SOOC/当前结果 |
| `Components / Editor / 调整面板` | `Ltq5G` | 右栏 7 个页签内容样本 |
| `Components / Editor / ToolsBar 三段式` | `FPvEw` | 常态 + 挤压态（渐变盖住中段） |
| `States / Editor / Tab 三档` | `M4eIE` | 三档示意 |
| `Components / Editor / 浮层` | `KZDEZ` | 导入 LUT / 应用中 / 裁切比例下拉 |
| `States / Editor / 空态四态` | `NNwR7` | 无库 / 库无照片 / 目录无照片 / 未选照片 |

布局：全部落在 `y ∈ [0, 1870]`，脚本校验过**顶层帧两两不重叠**。

## 3. 关键决策与理由

1. **跨文件 `Copy` 不可用**（实测 `Copy("PRpZa", …)` 在 `editor.pen` 里报 `Can't find node`）——
   三行外壳只能在 `editor.pen` 内**重搭**。结构是从 `browse.pen` 读出来照抄的（逐属性对齐）。
2. **右栏改成三组页签**（人类 2026-09-23 纠正）：不是「一条总页签 + 子页签」，
   而是 `总览·定稿·信息` / `影调·色彩·清晰度·镜头` / `曲线` 三个**同时在场**的组。
   `AI调节` 按人类口述**不做**，画布上不出现。
3. **色彩页签去掉色轮**（人类 2026-09-23）：只留 色温 / 饱和度 / 自然饱和度 三根拉杆。
4. **旋转的几何被人类纠正过一次**：展示框是**斜画布的内接框**（框 = 成片边界），
   框外压暗；早期画法（斜照片浮在视口里、框再套在内层）是错的，已重画。
5. **新增两个主题无关令牌** `overlay-line` / `overlay-halo`：照片覆盖线必须永远是「浅线 + 深包边」，
   跟着主题翻转会在亮照片上消失（与 `--tile-bar-scrim` 的分工不同：后者是面，前者是线）。
6. **画布上的画稿辅助**：视口里的「照片」是平色占位块；旋转帧里额外画了一条**斜地平线带**
   （`MockContent / 地平线`），否则「图被转了几度」在画布上看不出来 —— 这条是**画稿辅助，不是产品元素**。

## 4. 验证方式（Agent 冒烟）

- 结构：`Get(..., ctx.problems)` 逐帧扫裁剪/溢出；顶层帧重叠用脚本两两校验（0 命中）。
- 视觉：`Export([id],"png","/tmp/pen-shots")` 落盘 + 逐张看图（比 `TakeScreenshot` 可靠且清晰）。
  已复核：主稿、裁切、旋转、对比、7 个页签面板、浮层三张、空态四态。
- **未经人类目视确认**：密度/主题两档的观感、`宽松` 档、浅色主题（本轮只画了深色·紧凑）。

## 5. 踩到的坑（已写进 `design/editor.md` §8）

| 坑 | 现象 | 处置 |
| --- | --- | --- |
| `execute` 全局变量**不跨调用** | 第二次调用里 `editorId is not defined` | 写死 id 或按名字重新找 |
| `Update(id,{fill:undefined})` **清不掉**填充 | 页签改文案后旧的主色底留在别的项上 | 显式 `fill:"$surface-track"`（与凹槽同色） |
| `TakeScreenshot` 同批改动内**常拿到空白** | 明明有内容却全白 | 改成独立调用截图；或直接用 `Export` 落盘看 |
| 节点在 `y ≳ 1900` 后**截图/导出整块空白** | 疑似渲染纹理高度限制 | 画布重排到 `y < 1870` |
| 无 `theme` 的帧渲染主题不确定 | 同一批帧有的深有的浅 | 所有新建顶层帧都显式 `theme: {mode:"dark",density:"compact"}` |

## 6. 遗留问题

1. **文件名**：`PLAN.md` 原写 `design/edit.pen`，实际是 `editor.pen` —— 需人类拍板统一
   （`PLAN.md` 的「设计阶段」表已改成「实际是 editor.pen，待拍板」）。
2. `prompts/editor.pd` **没写完**：issue 定稿/切换的交互、AI 面板、风格 style、
   智能调节拉杆、蒙版等都还没口述；画布上的 issue 面板是**最小可用形态**。
3. `design/editor.md` §7 列了 9 条待决项（右栏宽度、左栏宽度、工具是否互斥、进 editor 时 LUT 默认开关、
   旋转框外是否压暗、禁用态画法、issue 列表信息量、直方图画法、文件名）。
4. 浅色主题与宽松档**没有派生帧**（主稿只有深色·紧凑）—— 等主稿定稿再补。
5. `browse.pen` 的密度控件选中块仍是 `$surface-layer`，而实现（`SegmentedControl`）是 `bg-brand`：
   `editor.pen` 已按实现画成主色，**`browse.pen` 待同步**。

## 7. 改动文件

- 新增：`design/editor.pen`、`design/editor.md`、`implementations/2026-09-23_editor-design-draft.md`（本文件）
- 修改：`AGENTS.md`（§11.1 toolsbar 三段式、§11.5 issue/取消确认顺序、编辑栈口径）、
  `DESIGN.md`（§9.2 两个新令牌、§14.8 视口覆盖线、§14.9 右栏三页签组、§16 变更记录）、
  `PLAN.md`（设计阶段表：edit.pen 行状态改「进行中」并注明实际文件名）
