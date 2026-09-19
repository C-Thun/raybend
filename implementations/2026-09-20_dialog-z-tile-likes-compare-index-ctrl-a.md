# 弹窗层级、赞踩上 tile、对比双击/适配键、格子与右栏尺寸、图标描边、Ctrl+A

完成时间：2026-09-20 04:02:19 CST

## 范围（人类 2026-09-20 第三批 + 之前挂着的两条，原文见 `docs/user-requirements.md`）

1. **弹窗层级**：弹窗必须是全局最高（仅次于 `titlebar` 与右上角 message）。
2. **切语言时横向选择器选中底色**（上一轮挂着的）。
3. **赞 / 踩要显示在图片顶部**的信息条上。
4. **双击放大缩小还是不行**；以及 **compare 右下角「适配」键再点切不到 100%**（单图 view 下正常）。
5. **格子尺寸范围重排**：最小档加大 ~30%。
6. **browse 右栏宽度 −20%**。
7. **顶部信息条的图案也要描边**（跟底部名字条一样）。
8. **tiles 支持 `Ctrl/Cmd + A` 全选**，且要选中「未显示的部分」。

## 涉及文件

- 层级：`src/styles/tokens.css`、`src/index.css`、`src/shell/TitleBar.tsx`、
  `src/features/migration/MigrationGate.tsx`、`src/features/import/ImportProgressDialog.tsx`
- 选择器底色：`src/components/ui/SegmentedControl.tsx`、`src/shell/FlowSwitcher.tsx`
- tile：`src/components/ui/Tile.tsx`、`src/features/photo-grid/PhotoGrid.tsx`、`src/index.css`
- 对比：`src/components/ui/viewer/CompareView.tsx`
- 尺寸：`src/lib/tile-flow.ts`（+ 测试）、`src/lib/layout-prefs.ts`
- 全选：`src/lib/viewer-keys.ts`（+ 测试）、`src/workspaces/browse/BrowseWorkspace.tsx`、
  `src/workspaces/import/ImportWorkspace.tsx`、`src/features/browse/store.ts`（+ 测试）
- 冒烟：`scripts/check-browse-boot.mjs`

## 关键决策与理由

### 1. 弹窗层级：Zag 的 dialog 根本没有 z-index

`index.css` 里那套「按 Ark 的 data 属性赋 `--z-index` 变量」的机制**只对 popper 系生效**
（menu/tooltip/select/combobox/date-picker）；Zag 的 **dialog positioner 不写**
`z-index: var(--z-index)` —— 实测计算值 `auto`、内联里也没有 z-index。
于是弹窗只能靠 DOM 顺序，被工作区里 `z-10` 的看图/胶片覆盖层压住 —— 正是人类报的
「film/view 下点库齿轮，弹窗看不见、只有周围一圈被遮罩压暗」。

修法：

* dialog 的 `backdrop` / `positioner` **直接**写 `z-index: var(--z-scrim|--z-modal) !important`
  （`!important` 与既有那批同一理由：对面将来若写了内联 z-index 也要压得住）；
* 新增 `--z-titlebar: 75`，标题栏 `relative z-(--z-titlebar)` —— **标题栏在弹窗之上**：
  窗口是沉浸式的，关窗键与拖拽区都在标题栏上，弹窗盖住它就没法挪窗口/关软件；
* 顺手把两处硬编码的 `z-50`（迁移闸门、导入取消的二级确认）换成 `z-(--z-modal)` ——
  阶梯纪律要求浮层只认令牌。

### 2. 切语言时选中底色：让「选中项自己」铺主色底

探针结论要先说清楚：**无头环境复现不出那一瞬间**（按帧采样 74 帧，指示块的宽/位与选中项
始终一致；后来发现连「按帧驱动语言切换」都做不到 —— 动态 import 拿到的是**另一个模块实例**，
`setLocale` 改的是它自己的信号，与 App 不互通）。所以这条**没有先复现再修**，而是做成
**构造性不可能出错**：

指示块（`ArkSegmentGroup.Indicator`）的几何是 Ark 量出来的，标签文字一变宽它就得重量 ——
那段窗口里（尤其带着 `transition-all`）选中项看上去就不是主色底了。现在让**选中项自己**
带 `data-[state=checked]:bg-brand`，指示块退化成「只负责滑动动画」：
它量偏/晚一拍时底色仍然是对的，任何布局变化（换语言、字体加载、窗口缩放）都不会豁口。
`SegmentedControl` 与 `FlowSwitcher` 两处同样处理。

**这条需要人类在真机上确认是否真的好了**（Agent 只能保证「底色不再可能由测量决定」）。

### 3. 赞 / 踩上 tile 顶部条

`Tile` 新增 `like?: "like" | "dislike" | null`，顶部条渲染 `IconThumbUpFilled` /
`IconThumbDownFilled`（与工具条同一套三态语义）；网格把 `marks.likeState` 传下来
（认不出的值当没有）。顶部条的顺序：星标 …… 色标、赞踩、旗标。

### 4. 双击 / 适配键：两个真问题

**(a) 真双击根本没到处理器。** 人类两次报「双击不行」，两次都不是「逻辑没写对」：

* 第一次（上一轮）是 effect 把刚设好的缩放按回适配值；
* 这一次是 **DOM 在两次点击之间被重建**：`<For each={canvas().placements}>` 按**引用**认身份，
  而 `compareCanvas()` 每次都返回**新的落点对象**（点击第一下会 `focus()` → store 变化 →
  memo 重算）⇒ 那一格的 DOM 被销毁重建 ⇒ **第二下落在新元素上，浏览器不发 `dblclick`**。
  用「盖戳」探针钉死：真双击之后那一格上的 `data-probe-mark` 没了（`mark: null`）。

修法：`For` → **`Index`**（按下标认身份，DOM 一直活着；顺带不再反复重建 `<img>`）。
冒烟也从合成 `new MouseEvent("dblclick")` 换成 **`Input.dispatchMouseEvent`（clickCount=2）**
—— 合成事件绕过 pointer capture 与命中测试，正是它让上一轮假绿。

**(b) 「适配」键只做了一半。** 单图 view 那颗键本来就是 `toggleFit()`（适配 ↔ 100%），
对比这边只写了 `fitTo()`。现在：

* **双击某格**：已经在 100% ⇒ 按**被双击的那张**算适合窗口；其它状态 ⇒ **直接到 100%**
  （不是「先按这张适配、再点一次才 100%」—— 适配目标不是这张时，第一下看起来什么都没发生）；
* **右下角「适配」键**：按当前那张适配 ↔ 100%（与单图 view 同一条口径），键盘 `0` 同它。

### 5. 格子档位重排、右栏缩窄

* `TILE_SIZE_STEPS`：`96…512` → **`128, 152, 180, 216, 256, 304, 360, 432, 512`**
  （最小档 +33%、仍 9 档、仍到 512 收；中间默认档 208 → 256 ——「一页图太多」也一起缓了）。
* `BROWSE_RIGHT_WIDTH`：375 → **300（−20%）**。

### 6. 顶部条图案的描边

`text-shadow` **对 SVG 完全无效**（只作用于文字字形），所以星标/旗标/赞踩要另走一条：
`.tile-info-icon { stroke: var(--tile-info-outline); stroke-width: 2; paint-order: stroke; }`
（先描边后填充 ⇒ 外描边，颜色复用同一条令牌）；色点是 `span`，用
`.tile-info-dot { box-shadow: 0 0 0 1px var(--tile-info-outline); }`。
只在**强制显示层**加，标准层（有半透底）不加。

### 7. `Ctrl/Cmd + A` 全选（含未渲染的）

* `viewer-keys.ts` 新增 `{ kind: "select-all" }`：这是**唯一**允许带修饰键的一条
  （其余 Ctrl/Cmd/Alt 组合照旧一律不管，留给将来的快捷键体系）；
* **只在 tiles 下接**（看图/对比态不接：全选会把对比集合一起换掉，不是用户要的），
  并且 `preventDefault()` —— 否则浏览器会选中整页文字、蓝一片；
* **「整个范围」怎么做到**：浏览侧的列表是**按页取**的（`PAGE_SIZE = 256`），
  只选 `entries()` 里已加载的那部分就是错的；而 `timeline()` 是这个 scope 的**全量 id 清单**
  （首屏就取了）—— 用它，不必为此再发「把所有页取回来」的请求（十万张要四百次）。
  导入侧的网格本来就握着整目录。

## 验证（Agent 侧，冒烟）

- `pnpm test`：**709 通过 / 0 失败**。本批新增/改写：
  `viewer-keys.test.ts`（Ctrl+A → select-all；不带修饰键 / 带 Alt 都不算）、
  `features/browse/store.test.ts`（**selectAll 覆盖整个 scope**：fixture 跨页，
  只加载第一页时全选仍要有 `total` 个 id）、`tile-flow.test.ts`（新档位：默认档 256、最小档 128）。
- `pnpm typecheck`、`pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n`：通过
- `pnpm build`：通过；`pnpm smoke:ui`：通过（`problems: []`）
- `pnpm check:browse`：通过。新增断言：
  * **弹窗层级**：进看图 → 点库齿轮 → 弹窗中心的**命中测试**必须命中弹窗、
    弹窗 z > 看图 z、遮罩 z > 看图 z、标题栏 z > 弹窗 z 且标题栏那条仍由标题栏接事件、
    toast z > 标题栏 z（这一段放在冒烟最后：模态的收尾在无头环境里可能留痕）；
  * **真双击**（`clickCount=2`）→ 100%（画布按原图像素 4000×3000）→ 再双击回适合窗口；
  * **「适配」键连点两次**：先适合窗口、再 100%；
  * **赞踩**：带标记的那张顶部条里出现 `thumb-up-filled`；
  * **描边**：强制层每个 svg 都带 `.tile-info-icon`，标准层一个都不带；
  * **Ctrl+A**：`preventDefault` 生效、没有文本选择、当前渲染出来的每一格都变选中态。

## 未经 Agent 验证（需人类真机/目视）

- **切语言那一瞬间的选中底色**（见上文 §2：无头环境复现不出来，修法是构造性的）——
  请切几次中英文看 `titlebar` 的紧凑/宽松与 flow 选择器是否一直保持主色底。
- 弹窗层级在 Windows 真机上的观感：film/view 下点库齿轮，弹窗是否正常盖在看图之上、
  标题栏是否仍可拖动/关窗。
- 对比里**真双击**与「适配」键的手感；赞踩在 tile 顶部条上的可见性；
  顶部条图案描边在亮色照片上是否够清楚。
- 新的格子尺寸范围（最小 128 / 默认 256）在一屏里的密度是否符合预期；
  browse 右栏 300 的宽度是否合适。

## 遗留问题

- **切语言时选中底色**那条是「构造性修复」，不是「复现后修复」。若真机上仍能看到问题，
  请告知具体是哪个控件、哪一态，我再上探针（这次要先解决「动态 import 拿不到 App 的模块实例」
  这个测量障碍 —— 可以改成驱动 UI 菜单，或用 `?t=` 后的模块 URL 去 import）。
- 冒烟里那段弹窗层级断言必须留在**最后**：模态开关会（在无头环境里）影响后续的 hover/键盘断言。
- `tileStep` 的**旧值**（`app.db` 的 `grid.tile_step`）同样不再迁移；新档位表下老索引仍落在
  合法范围内，只是含义变了（老 `4` 现在是 256 而不是 208）。
