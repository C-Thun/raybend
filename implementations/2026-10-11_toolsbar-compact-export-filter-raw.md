# toolsbar 紧缩 · 悬停提示 · 导出筛选组 · 画廊小片与 RAW

> 完成时间：**2026-10-11 02:28**（spec 与画稿同日先行；本记录对应代码实施部分）
> 工作单元：崔总 2026-10-11 口述五条（+ 当轮两条修正：分隔线只在大类之间、导出筛选组内容）
> 规格：`specs/toolsbar-compact-and-export-gallery.md`（含 §2 已定案）
> 需求原文：`todos/2026-10-11-toolsbar-compact-export-filter-raw-tile.md`
> 画稿：`design/export.pen`（ToolsBar 三段式与三态、画廊小片与 RAW、上部网格最小边长 + 主稿/派生帧）、
> `design/browse.pen`（主帧 ToolsBar）、`design/editor.pen`（ToolsBar 三段式 + 主帧）

---

## 1. 改了什么

### 1.1 按钮组件的内置悬停提示（唯一实现）

| 文件 | 改动 |
| --- | --- |
| `src/components/ui/Tooltip.tsx` | 新增 `ButtonTip`：有提示才包（无提示零额外 DOM）；提示态下在触发层包一层 `inline-flex` 的 `span` 承接命中，**禁用按钮也能弹**（禁用的 `<button>` 不派发指针事件）；默认 `placement="bottom"`（工具条贴窗口顶部；Zag 定位器自带 flip） |
| `src/components/ui/Button.tsx` | 新增 `tip`；`tips` 规则＝`tip ?? （无可见文字时）aria-label`；`IconButton` 用 `label`（原来是原生 `title`） |
| `src/components/ui/ToggleBlock.tsx` | 新增 `tip`；纯图标时退回 `label`；移除原生 `title` |
| `src/components/ui/Menu.tsx` | 新增 `closeOnSelect`（标签筛选菜单要多选，点一条不关） |

工具条上的原生 `title=` 全部删除；非按钮元素的 `title`（tile 文件名、错误原因等）保持不动。

### 1.2 四个 flow 的 toolsbar 按「大类」收敛

| flow | 结果 | 分隔线 |
| --- | --- | --- |
| browse | `[垃圾桶] │ [撤销][重做] │ [漏斗][旗标][清空旗标][★★★★★][●×6][赞][踩][标签][相片桶][锁]`；右段「导出至外部编辑器」也去文字 | **2** |
| export | `[送入队列][移出队列][取消选中][重置队列] │ [旗标][标签][★★★★★][●×6][赞][踩]` | **1** |
| editor | `[SOOC/RAW]（右侧留白）[撤销][重做] │ [自动调整][裁切][旋转][对比]` | **1** |
| import | 不变（`[批量排除]` 保留文字） | 0 |

* 保留文字：展示范围、锁（文字＝档位）、SOOC/RAW、定稿、批量排除、回收站两键、从相片桶移出；
* 星 / 色标 / 赞踩 / 旗标筛选**抽成共享控件排** `src/features/browse/MarkControls.tsx`
  （`RatingRow` / `ColorRow` / `LikeRow` / `FlagFilterPair` / `MixedMark`）—— 浏览与导出同一份实现，
  差别只在调用方传的取值回调（标记态 / 筛选态）；
* 星标的可达名两种模式都保持「N 星」，**提示**才按模式变（筛选态「≥N 星」）—— 冒烟与读屏都稳。

### 1.3 导出筛选组（无漏斗、恒筛选）

* 导出**没有筛选开关**：这批控件点击即改筛选条件；实现走 `applyMarkIntent(..., { forceFilter: true })`
  （`features/browse/mark-actions.ts` 新增显式选项，同一个函数里分支），条件写入后 `filterMode` 置真 ——
  只为让 browse 的漏斗与 chips 行跟查询保持同一份可解释的状态；
* **新增**：旗标筛选（有 / 无）、**标签筛选**（`features/browse/TagFilterMenu.tsx`：标签菜单，点一条＝加/去一条条件，
  组内或；**只筛不设置、不识别**）。后端早支持（`Filter.tags`），前端补 chips 分支；
* `filter.ts`：`FilterChip` 加 `tag`，`filterChips` / `removeChip` / `clearMarkFilters` 同步；
  `FilterBar` 用标签词典显示名字（拉不到退 `#id`）；
* chips 行挂在**上部画廊之内**（`ExportWorkspace` 的 gallery 区改成 `flex-col` + `<FilterBar>`），只作用于画廊；
* 命令体系：`view.filter.toggle` / `view.filter.clear` / `mark.rating.*` / `mark.color.*` / `mark.like` / `mark.dislike`
  扩到 export 场景（`scenes: ["browse","export"]`、`when: filterScene`），`enabled` 与工具条同口径
  （筛选态不需要先选照片）；`App.tsx` 的 `mark` 接线按 flow 传 `forceFilter`（场景口径 §11.5）。

### 1.4 导出上部最小边长 240 → 192

* `lib/export-prefs.ts`：`EXPORT_TOP_SIZE_BOUNDS.min = 192`（档位 192…320 共 10 档）；
  中列 920 时 `4×226 + 3×3 = 913 ≤ 920` 四列可达（旧下限 `4×240 + 9 = 969 > 920` 永远差一列）；
* 偏好 key `v3 → v4`，**按尺寸回填**（旧下标 → 旧 bounds 尺寸 → 新 bounds 位置）：
  老用户的上部尺寸不被静默顶小（240 → 240），想要四列自己往左拖一档（240 → 226）。

### 1.5 画廊 tile：主图 + ≤2 片 + 「全部定稿 / RAW」

* `lib/export-model.ts` 新增纯函数 `issueStrip(smalls)`：`tiles`（去 RAW、截断到 2）、`more`（多于两片）、
  `raw`（非主行的 RAW 变体）。**排序仍归 `orderedIssues`**（单一事实来源）；
* `ExportWorkspace.IssueStrip` 重做：一行最多两片 + 按钮行（`全部定稿` 只在多于两片时出现；`RAW` 在右侧，
  零小片时只出它）；`全部定稿` 弹窗同样过滤 RAW（两块同一条规则）；
* **RAW 直接入队**：`ExportStore` 新增 `rawFor` / `canEnqueueRaw` / `enqueueRaw`；
  `enqueue` 的「显式 references」路径不再要求先有选择（候选从变体表取，拿到真实 profileHash），
  快照 / 去重 / 错误处理仍是一条路径（§2.12）；
* `source.ts::issueExtraHeight({tiles,hasRaw}, cellSize)` 新公式（一行 + 按钮行，零小片无 RAW 则不占高）；
* i18n 新增 `export.rawQueue`（中/英）；RAW 按钮带 `data-export-raw-enqueue` 锚。

---

## 2. 涉及文件

**新增**：`src/features/browse/MarkControls.tsx`、`src/features/browse/TagFilterMenu.tsx`
**改动**：`src/components/ui/{Tooltip,Button,ToggleBlock,Menu}.tsx`、
`src/features/browse/{BrowseToolbar,FilterBar,filter,mark-actions}.tsx|ts`、
`src/features/editor/toolbar.tsx`、`src/features/commands/catalog.ts`、
`src/workspaces/export/{Toolbar,ExportWorkspace,source,store}.tsx|ts`、
`src/lib/{export-model,export-prefs}.ts`、`src/{App.tsx,i18n/zh-CN.ts,i18n/en-US.ts}`、
`scripts/{check-export-boot.mjs,check-browse-boot.mjs,ui-smoke.mjs}`、
文档：`memory/DESIGN.md` §12.12、`design/{export,browse,editor}.md`、`specs/export-issue-ordinal.md` §4

---

## 3. 关键决策与理由

1. **提示收进按钮组件**（而不是每个工具条自己写 `Tooltip`）：禁用态是最需要解释的场景，
   而只有组件自己知道「现在禁用、提示挂在包装层」；顺带把三处原生 `title` 统一掉。
2. **星/色/赞踩/旗标抽成共享控件排**：浏览与导出的**只是取值语义**不同（标记 vs 筛选），
   渲染不该有两份（§2.12）。
3. **导出没有漏斗**：导出只有筛选模式，加一个开关等于给用户一个「设置模式」的假象；
   条件仍与 browse 共用一份（跨 flow 一致），chips 行负责「看得见」。
4. **标签筛选只筛不设置**：打标签是有依赖的编辑动作（弹窗、批量规则），导出工作流不承担它；
   菜单只是条件入口。
5. **`enqueueRaw` 走 `enqueue` 的显式引用路径**：不复制快照/去重/错误处理；
   选择不被触碰（用户点 RAW 不该改变「当前选了哪些定稿」）。
6. **偏好按尺寸迁移**：不静默改用户的观感；四列是「可达成」而不是「强制」。

---

## 4. 验证（Agent，冒烟级别）

| 命令 | 结果 |
| --- | --- |
| `pnpm typecheck` | ✅ |
| `pnpm test` | ✅ 1267/1267（新增：`issueStrip` 边界、`issueExtraHeight` 新公式、prefs v3→v4 迁移、`enqueueRaw` 不改选择/不重复入队） |
| `pnpm lint:colors` / `lint:arch` / `lint:i18n` | ✅ |
| `pnpm build` | ✅ |
| `pnpm smoke:ui` | ✅ `problems: []` |
| `pnpm check:export` | ✅（断言已按新设计重写：≤2 小片、无 RAW 小片、RAW 按钮、四列下限 192、Ctrl+A 可见片数 3、弹窗 10 项） |
| `pnpm check:browse` | ⚠️ 剩 1 项**与本轮无关**：`Ctrl+,` 现在打开「统一设置」而不是旧的快捷键弹窗（`catalog.ts` 里 `help.shortcuts` 的 `defaultKey` 早已移除、`settings.open` 才是 `Mod+,`），脚本断言是旧设计的 |

**本轮顺带修掉的冒烟夹具漂移**（都是既有问题，不是本轮引入）：
* `check-browse-boot.mjs`：`browse_delete` 夹具字段 `deleted` → `moved`（回收站改造后字段名变了）；
  缺 `browse_recycle_summary` 夹句 → 左栏 `recycleSummary().count` 在渲染里炸出未处理异常；
* `check-export-boot.mjs`：缺 `browse_recycle_summary` 夹具（同上）；
* 两处脚本里「按按钮文字找控件」的查询改成无障碍名 / `data-*` 锚（图标化后文字没了）：
  `[data-export-enqueue]` / `[data-export-remove]` / `[data-export-clear]` / `[data-export-reset]` / `[data-export-stop]`
  是本轮新加的稳定锚；`ui-smoke.mjs` 的整理三入口查询限定到 `[data-toolsbar-left]`
  （中段的「标签」现在也带无障碍名，不加作用域会点错）。

**未经人类验证**（照例单列）：观感与交互手感、悬停提示的实际出现位置、四列在你屏幕上的效果、
标签筛选菜单在真实标签词典下的表现、真机 DPI 与密度两档。**「进程起来了」≠「功能对了」。**

---

## 5. 遗留问题

1. `Ctrl+,` 的口径待崔总拍板：现在开的是**统一设置**（可能停在别的页），脚本期待的是快捷键弹窗。
   要么脚本改成「设置 → 快捷键页」，要么 `Mod+,` 直接落在快捷键页。
2. Pencil 画稿只落到了画布：`design/*.pen` **需要崔总在 Pencil 里手动保存**（磁盘时间戳当时未更新）。
3. 标签筛选目前只在导出出现（浏览侧维持原样：标签按钮开弹窗）。若要在浏览侧也加筛选入口，另开单元。
