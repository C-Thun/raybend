# 修复：LUT 强度拉杆拖一下就断（每拖一次只能动几个百分点）

2026-10-31 19:37:49 CST 完成。

## 现象

拖动应用层的强度拉杆时，数值刚变一点点拖动就失效，必须重新按下才能继续，所以一次只能拖几个百分点。

## 根因（两层，同一个病根：改了强度看起来像「整条链换掉了」）

1. `store.ts::commitChain` 每次都把链里**每一条** layer 浅拷贝一遍
   （`next.slice(0, MAX).map((layer) => ({ ...layer }))`），而强度拖动每个 tick 都会走一次
   `setLutLayerStrength → commitChain`。
2. 面板的层列表用 `<For>` —— Solid 的 `<For>` **按对象引用**比对。引用全变 ⇒ 每一行的 DOM
   连同其上的 Ark `Slider` 组件实例被**销毁重建**；滑块的指针捕获与监听挂在被销毁的 thumb 上，
   于是这一格之后再没有 `pointermove`，拖动当场结束。下一次按下又从新 DOM 开始，再拖一格再断 ——
   与「每次只能拖几个百分点」完全吻合。

顺带一提：用户看到的「数值一变就触发什么」确实是每次 tick 都写 store（为了实时预览，这是设计要的），
问题不在写入本身，而在写入让列表**整片重建**。

## 修复

- `src/features/editor/lut-panel.tsx`：层列表 `<For>` → **`<Index>`**（按**位置**复用 DOM，
  行内容走响应式更新）。排序本来就是按位置算位移（`lib/lut-chain-drag.ts`），语义正好对上；
  源码里留了注释写明「不能换回 `<For>`」及原因。
- `src/features/editor/store.ts`：
  - `commitChain` 只 `slice` 截断、**不再逐层克隆**；
  - 8 个链写入口（换底层 / 圈选加减 / 单层开关 / 强度 / 强度松手 / 排序 / 删除 / `setLut`）
    改为 `[...lutChain()]` 浅拷贝数组、**只新建被改动的那一条**。
- 效果：既使将来某个入口又换了全部对象，`<Index>` 也不会重建 DOM；而身份稳定让下游（预设比较、
  序列化前的浅比较）少做无谓工作。

## 验证

- 新增单测 `改一层强度不换其它层的对象身份`：断言链数组是新数组、被改的那条换新对象、
  **其余层保持同一引用**（单层开关同理）。若有人把 `commitChain` 改回逐层克隆，这条会红。
- `pnpm test`：1359 passed / 0 failed（+3，含上一条与上轮拖动几何用例）。
- `pnpm typecheck`、`pnpm build`、`lint:colors` / `lint:arch` / `lint:i18n`：全绿。

## 口径确认（没有改行为）

- 拖动过程中**只**更新预览参数，不改开关；`0% ⇒ 关层 / >0% ⇒ 开层` 仍只在**松手**（`onValueCommit`）时判定。
- 拖动中每 tick 写 store 是有意的实时预览（渲染走 `developRev` 的节流），与「一次手势一个撤销节点」不冲突：
  撤销条目只在 `onChainChange → confirmEdit` 那一次建立。

## 遗留

- 未经真机验证：拖动手感需崔总在 Windows 上确认（WSL/lavapipe 下只能保证结构正确、单测绿）。
- 相邻风险已排查：`<LutPanel>` 挂在 `<Show when={leftPanel() === "lut"}>` 里，不随 `developRev` 重新挂载，
  所以不存在「面板整体重建」这第二个断点。
