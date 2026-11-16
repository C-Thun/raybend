# LUT 应用层拖动排序：整行跟手 + 让位动画

2026-10-31 18:59:13 CST 完成。

## 起因

上一轮应用层链只接了 HTML5 DnD（`draggable` / `dragstart` / `drop`）。崔总问「拖动时的动画、
LUT 条粘在光标上的效果有没有做」—— 答案是没有：原生 DnD 的拖影是**浏览器对源元素的截图**，
半透明、不可控、也没有任何让位动画，与本项目其它拖动手势（browse 拖动删除、export 定稿拖到预设）的质感不一致。

## 改动范围

只动 LUT 面板的排序手势，数据与撤销口径不变（松手才落库 = 一个撤销节点）。

- **手势基础**换成共享的 `lib/pointer-drag.ts::trackPointerDrag`：阈值 4px（点一下不会变拖动）、
  `capture: true`（指针捕获）、`cancelOutsideWindow: true`（拖出窗口不留幽灵、越界松手不算投放）。
- **跟手幽灵**：被拖的那一行**本体**在拖动中变成 `position: fixed` 的副本，`left/top/width` 由指针位置每次 move 更新，
  纵向抓取偏移保留（抓在行中间就一直在中间）。幽灵是 `surface-layer` + 主色细描边，`pointer-events-none`，
  走 `<Portal>` 到 body（与 `DragPreview` 同一套「派生出窗口层级」的做法）。
- **让位动画**：源行 `visibility: hidden` 留在流里（布局不塌），其余行按「目标在源之前/之后」平移一格，
  `transform 140ms ease` 过渡；**落点槽位**在列表里绝对定位一块 `state-selected` 占位块（`top` 同样 140ms 过渡）。
- **自动滚**：指针进入滚动容器上下 24px 内时按 10px/事件滚动滚动容器（链与分类同一容器，`data-lut-scroll` 标记）。
- **控件不参与拖动**：`pointerdown` 落在 `button` / `input` / `[role=slider]` / `[data-lut-no-drag]`（强度那一行）时直接放弃。
- 行内视觉抽成 `LayerRowBody`：躺着的那一行和幽灵**共用同一份内容**，避免「拖起来的长得不一样」（`AGENTS.md` §2.12）。

## 涉及文件

- `src/lib/lut-chain-drag.ts`（新增）：纯几何 `lutDropTarget` / `lutLayerShift`。
- `src/lib/lut-chain-drag.test.ts`（新增）：3 条用例（槽位四舍五入与夹取、前后让位、相邻一格）。
- `src/features/editor/lut-panel.tsx`：删除 HTML5 DnD 状态与处理器（`draggable`/`dragOver`/`sliderActive`），
  换成 `dragFrom` / `dragTarget` / `dragGhost` / `dragSlot` / `dragRowHeight` / `dragLayer`；
  新增 `startLayerDrag`、`dragShift`、`LayerRowBody`、幽灵 Portal；滚动容器加 `data-lut-scroll`。
- `design/editor.md` §3.1.3 与 `specs/editor-lut-chain.md`：把「原位留占位块」改成实际实现（**落点槽位**留洞、
  幽灵是整行本体、140ms 让位、边缘自动滚）。

## 关键决策

1. **不用原生 DnD**：拖影不可控、没有让位动画，且与项目其它拖动（指针手势）不同源；
   复用 `trackPointerDrag` 还顺带拿到「出窗即取消」与「窗口外松手不算投放」两个已测行为。
2. **幽灵是行本体而不是截图**：`LayerRowBody` 是同一份 JSX，幽灵与静止行在像素上必然一致；
   幽灵只做视觉，真实顺序仍以 `store.lutChain()` 为准。
3. **几何抽成纯函数**：`lutDropTarget`（指针 → 槽位）与 `lutLayerShift`（其它行让位量）不依赖 DOM 与 Solid，
   面板里只剩测量与事件；这两条正是排序手感最容易错的地方（相邻一格、越过两端夹取），所以给它们单测。
4. **松手才 `moveLutLayer`**：拖动过程只改本地信号，与 §2.21「一次手势 = 一次 confirm」一致。

## 验证

- `pnpm typecheck`：通过。
- `pnpm test`：**1356 passed / 0 failed**（较上轮 +3，新增 `lut-chain-drag.test.ts` 三条）。
- `pnpm lint:colors` / `lint:arch` / `lint:i18n`：三条均通过（幽灵与占位块只用令牌 `surface-layer` / `state-selected` / `brand`）。
- `pnpm build`：通过（仅既有的 chunk > 500 kB 提示）。

**未经人类验证**：手感（跟手偏移、让位过渡的节奏、自动滚速度）与观感需要在 Windows 真机确认；
WSL/lavapipe 下无法判断拖动的跟手程度。

## 遗留

- **自动滚只在 `pointermove` 时发生**：指针停在边缘不动就不会继续滚。链上限 8 行、面板通常够高，
  暂不引入 rAF 自滚循环；真机上若确有需要再补（做法是拖动期间开一个 rAF，按「指针是否在边缘带内」步进滚动）。
- 拖动中的键盘可达性：排序目前只有指针一条路（与 browse/export 的拖动一致）；键盘重排未做。
