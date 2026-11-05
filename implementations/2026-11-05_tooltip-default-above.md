# 提示气泡默认朝上（上方放不下才翻到下方）

> 完成时间：2026-11-05 11:12:13

## 需求（崔总）

按钮的悬停提示浮条原来默认在按钮**下方**，位置正好被鼠标光标挡住 ——
要看清得把鼠标挪到按钮上缘，非常不合理。改为：

- 默认位置在按钮**上方**；
- **只有确定上方已经超出应用窗口边界**（典型：`titlebar` 那一排按钮）时才显示在下方。

## 改动范围

| 文件 | 改法 |
| --- | --- |
| `src/components/ui/Tooltip.tsx` | `ButtonTip` 的默认 `placement` 由 `"bottom"` 改为 `"top"`；文件头第 3 条口径重写（为什么朝上、翻下去的判据、翻转边界为什么是窗口） |
| `src/features/browse/TagFilterMenu.tsx` | 去掉调用点显式的 `placement="bottom"`，跟随新默认（该处没有「必须在下方」的理由，留着一个例外只会变成第二套口径） |
| `src/features/browse/MarkControls.tsx` | 注释里的「默认朝下」改为「默认朝上」 |
| `memory/DESIGN.md` §10.1 | 「纯图标与悬停提示」那条改为「**默认朝上**（只有窗口上缘放不下才翻到下方）」 |
| `memory/COMPONENTS.md` | 组件表里 `Tooltip` / `ButtonTip` 一行补上默认方位 |
| `design/browse.md` / `design/export.md` | 提示口径里的「默认朝下」同步（这两处是文字说明，画布上没有提示气泡的帧，`.pen` 无需改） |
| `specs/toolsbar-compact-and-export-gallery.md` | 「统一口径 · 1. 位置」那条更新为 `placement="top"` 并注明改动日期与理由（原文是历史方案，留着会与代码打架） |
| `scripts/ui-smoke.mjs` | 新增**方位**断言（见下） |

## 关键决策与理由

1. **翻转交给 Zag 定位器自带的 `flip`，不自己算。**
   气泡 Portal 挂在 `body` 下，而 `html/body` 都是 `overflow: hidden` 且 `height: 100%` ——
   浮动的「裁切祖先」就是视口，也就是**应用窗口客户区**，与「在哪个面板里、面板滚没滚动」无关。
   这正是需求要的判据：只看窗口边界。`overflowPadding` 默认 8px，即真的贴到边才翻。
2. **翻转只会翻到 `bottom`，不会横跳到左右。**
   floating-ui 在没指定 `fallbackPlacements` 时，对基础方位 `top` 的回退表就是 `[bottom]`
   （源码 `getFallbackPlacements`：`isBasePlacement` → `[getOppositePlacement(initial)]`）。
   所以「朝上」与「朝下」是仅有的两种结果，不会出现侧边气泡。
3. **不动 `Tooltip` 本体的默认值**（本来就是 `top`）——改完之后两个入口终于一致，
   面板里的既有用法（`PathText` / `EasyCopy` / `TilesControlBar` 都显式写了 `top`）不受影响。
4. **调用点不再保留「显式朝下」的例外。** 全项目只剩 `TagFilterMenu` 一处，且没有理由，
   留着就等于两套默认。

## 验证

- `pnpm typecheck`：通过。
- `pnpm test`：**1421 passed / 0 failed**。
- `pnpm lint:colors` / `lint:arch` / `lint:i18n`：通过。
- `pnpm build`：通过（只有既有的 chunk 体积提示）。
- `pnpm smoke:ui`：**problems: []**。新增两条**几何**断言（不是读 Zag 的 `data-side`，
  而是量气泡与按钮的真实矩形）：
  - 页面中部的图标按钮（`[data-tip-switch="a"]`）：气泡 `bottom <= 按钮 top`（在上方）；
  - `titlebar` 上最后一个带 `aria-label` 的图标按钮（主题开关）：气泡 `top >= 按钮 bottom`（翻到下方）。
- **反向对照（确认断言不是空的）**：临时把默认值改回 `"bottom"` 重跑，冒烟报出
  `页面中部的提示气泡没在上方（side=bottom）：气泡 bottom=144，按钮 top=92`；改回 `"top"` 后恢复 `problems: []`。

## 遗留

- 视觉观感（气泡与按钮的 8px 间距、贴窗口上缘时翻下去是否好读）仍需崔总在真机目视确认 ——
  按 `AGENTS.md` §2.8，这属于 E2E，Agent 只验到单元测试与冒烟。
- `design/*.pen` 没有提示气泡的帧，本轮未动画布；若将来设计稿要画提示态，默认方位以本记录为准。
