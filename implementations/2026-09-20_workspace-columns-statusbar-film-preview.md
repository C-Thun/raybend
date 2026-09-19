# workspace 结构红线：跨列状态栏 → 中列最底那一条（+ 胶片带居中 + 预览框 4:3）

完成时间：2026-09-20 04:35:46 CST

## 范围

人类 2026-09-20 连着给的三条（前两条是结构/视觉，第三条是尺寸口径）：

1. 「在 browse 下进 view/compare/film 时，底部的当前图片信息条**不是跨列的**，这个结构必须改掉……
   workspace 只有纵向分列，没有跨列行，每一列到底。这个位置的替代就是 tiles 下的状态栏」
   —— 并要求把这条规则**写进 `AGENTS.md` / design 记忆文本**；
2. 「film 的胶片带要**居中显示**：宽度超出显示范围时与原来一样（从左排、横向滚动），
   小于显示范围时居中」；
3. 「browse view 下右栏的**预览全图的框，比例改成 4:3**，不要 3:2，这样对纵图支持更好」。

## 结构（第 1 条）——改了什么

**以前**：`<main>` 的三列外面还有一条 `<Show when={viewer.active}><ViewerStatusBar/></Show>` ——
它是**三列下面一条全宽的兄弟行**（`BrowseWorkspace` 的注释当时还写着「与画布一致：它在整个工作区下面」）。
后果有两层：① 左右两列的底边被这条行截断，看着像断层；② 它给「当前照片」造了**第二份实现**
（`features/browse/ViewerStatusBar.tsx`），与 tiles 的状态栏各写一遍标记显示规则。

**现在**：

* 跨列那一行**删掉**；`ViewerStatusBar.tsx` **删除**（连带 `features/browse/index.ts` 的导出）。
* 看图时中列的最底那一格就是 **tiles 那条状态栏**：`components/ui/tiles/TilesControlBar.tsx` 里
  新增 `PhotoStatusBar`（与 `TilesControlBar` **共用同一个 `BarFrame`**：高度 / 面色 / 上边线 /
  `data-tiles-control-bar` 标记都在那一处）—— 左 = 文件名 + 紧挨着的锁徽标，右 = 这一张的标记。
* 中列从下往上是：**状态栏 → 胶片带 → 照片区**（`BrowseWorkspace` 里 `<PhotoStatusBar>` 排在
  `<FilmStrip>` 之后，是 `<main>` 的最后一个孩子）。
* 标记显示规则**收敛成一份**：新建 `components/ui/PhotoMarks.tsx`（从 `Tile.tsx` 的 `TileMarks` 抽出，
  加了 `spread` 开关）——`Tile` 顶部条（强制层 + 标准层）与看图状态栏三处共用它。
  抽它的直接理由：状态栏要显示的正是同一组标记，再写一遍必然漂移（`AGENTS.md` §2.12）。

## 胶片带居中（第 2 条）

`FilmStrip` 的滚动容器改成「外面滚动、里面一行 `w-max mx-auto`」：
装得下时 auto 外边距把整行居中；装不下时 auto 外边距归零、与原来一样从左排起、横向滚动。
**不用 `justify-center`** —— flex 的居中对溢出会两头都截，起始那几张滚不回来。
像素锚定（`film-strip-anchor.ts`）不受影响：`offsetLeft` 的公共原点在差值里抵消。

## 预览框 4:3（第 3 条）

* 右栏预览的外框从「写死 `h-[180px]`」改成**固定 `aspect-ratio: 4/3`**（面板宽度定高）；
* 照片盒仍按**原图比例**，但改成**铺满一条边**：横图（含 1:1 之外的所有 ≥4:3）铺宽、
  纵图铺高 —— 框固定之后这条边必定装得下，不需要再夹取；视野框的百分比定位因此仍然成立。
* 常量与纯函数放进 `lib/preview-frame.ts`（`PREVIEW_FRAME_ASPECT` / `fitAxisFor`）+ `preview-frame.test.ts`
  （横 / 竖 / 1:1 / 恰好 4:3 / 全景 / 0 与 NaN）。

## 文档（人类明确要求「录入 design/agents 等记忆文本」）

* `AGENTS.md` §11.1 —— 新增「`workspace` 的结构红线」小节（口述原文 + 允许/禁止 + 三列各自的底条）；
* `DESIGN.md` §8.7 —— 新增视觉条款（含对/错两张 ASCII 图）；§13.2 补预览框 4:3；
* `BROWSE.md` §5.5（胶片带居中）、§5.8（状态栏：看图与 tiles **同一条**、在中列最底）、
  §6 右栏（预览框 4:3）、§12 帧清单第 3 行；
* `design/browse.md` §2.5 —— 看图/对比的布局口径 + **画布待同步告警**（`.pen` 里
  `Shell / Browse / View` 与 `Compare` 把 `ViewerStatusBar` 画成了 `Workspace / View` 下面的全宽兄弟行，
  正是被废弃的结构；需 Pencil 才能改，当前 Pencil 未连接）；
* `design/main.md` §3 —— 工作区三列处加同一条红线（所有工作流适用）。

## 验证

* `pnpm typecheck` 0 ｜ `pnpm test` **712**（新增 3 条预览框几何）｜
  `lint:colors` / `lint:arch` / `lint:i18n` ✓ ｜ `pnpm build` ✓ ｜ `pnpm smoke:ui` → `problems: []`；
* `pnpm check:browse` ✓，其中**新增断言**：
  * 看图状态栏必须 `closest("main")`、左右边不越出 `main`、底边与 `main` 齐平、
    且排在 `[data-filmstrip="open"]` **下面**（`belowStrip`）；
  * 胶片带装得下时内层行左间距 > 4px（居中）；
  * 右栏预览框宽高比 = 4:3（容差 3%）。
* 旧断言 `[data-viewer-status="open"]` 的三处已改成
  `main [data-tiles-control-bar][data-tiles-bar-mode="view"]`。

## 遗留（需要人类）

1. **真机目视**：中列底部那条（tiles / 看图两态）、胶片带居中、4:3 预览框；
2. **画布同步**（需 Pencil，当前未连接）：见 `design/browse.md` §2.5 的告警；
3. Windows 产物已按新前端重建（`pnpm check:win`），exe 时间戳晚于 `dist/`。
