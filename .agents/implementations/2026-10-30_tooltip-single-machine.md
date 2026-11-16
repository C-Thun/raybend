# 提示气泡（tooltip）嵌套修复 —— 一台触发器只允许一台 machine

> 完成时间：2026-10-30 02:42:11

## 症状（崔总报）

- 编辑工作流 LUT 面板右上角的「新建分类 / 导入 LUT」两个图标按钮：悬停一个能弹提示，
  **鼠标直接移到另一个时不弹**，要先移出按钮区再回来才弹第一次指向的那个。
- 提示位置不稳：一会儿在按钮上面，一会儿在下面，有时能看到「先在上面闪一下又跳到下面」。
- 判断（崔总原话）：「一些按钮看起来正常，但我认为实现应该有隐患」。

## 根因

**同一个按钮上挂了两台 Ark tooltip machine。**

- `Button` / `IconButton` / `ToggleBlock` 自带 `ButtonTip`：没有可见文字时用 `aria-label` / `label`
  自动生成提示（默认 `placement="bottom"`），并把触发器挂到包装 `<span>` 上（为了禁用态也能弹）。
- 调用点又写了一遍 `<Tooltip content={…}><Button …/></Tooltip>`（默认 `placement="top"`），
  触发器挂在 `<button>` 上。

于是同一条指针路径上有两台 machine（外层 → button、内层 → span），它们抢 **Zag 的全局 tooltip store**：

- 谁先 `open` 谁把 `store.id` 抢走，另一台收到 `trackStore` 的 `close`；败者被踢掉时
  `hasPointerMoveOpened` **不会被清**（`open` 状态收 `close` 只做 `invokeOnClose`）。
- 下一台按钮的 machine 收 `pointer.move` 时两条 guard 都不成立
  （`noVisibleTooltip` 为假 + `hasPointerMoveOpened` 仍为真）→ **没有任何 transition**，
  只有真正的 `pointerleave` 才把它清掉 —— 这就是「得移出去再回来」。
- 两台的默认方位不同（内层 bottom / 外层 top），谁赢取决于定时器先后 →
  「一会儿上面一会儿下面」「先上后下闪一下」。

探针实测（CDP 真鼠标，`/dev/kitchen-sink` 里并排两枚图标按钮）：

| 组合 | A 悬停 | A→B 直移 | 移出再回来 |
| --- | --- | --- | --- |
| 纯 `span` 触发器 + `<Tooltip>` | ✅ | ✅ | ✅ |
| `IconButton` 自带 tip（单机） | ✅ | ✅ | ✅ |
| 原生 `<button>` + `<Tooltip>` | ✅ | ✅ | ✅ |
| `<Tooltip>` 包 `IconButton`（**嵌套**） | ✅ | ❌ 什么都不弹 | ✅ |

## 改动

### 1. 结构上禁止双提示（`src/components/ui/Tooltip.tsx`）

- 新增 `TooltipOwnerContext`，由 `Tooltip` **只包在触发子树**上（不包 Portal 里的内容）。
- `ButtonTip` 读到这个标记就不再自己套一层 —— 外层 `Tooltip` 是唯一的那台 machine。
- 顺手修掉 `ButtonTip` 里 `{...triggerProps}` 的隐患：那里传进来的是 `propsFn`（函数），
  能工作全靠 Solid `mergeProps` 对函数源做 `createMemo` 的副作用 —— 改成和其它调用点一致的
  `triggerProps()`，类型也从「碰巧」变成「契约」。

### 2. 调用点收敛到单一写法（`AGENTS.md` §2.12：同一能力只允许一套实现）

| 文件 | 改法 |
| --- | --- |
| `src/features/editor/lut-panel.tsx` | 去掉两个外层 `<Tooltip>`，改用按钮自己的 `tip` |
| `src/features/editor/preset-panel.tsx` | 三处同上；**删掉**「应用」按钮外面那层手工 span —— `ButtonTip` 本来就负责禁用态命中 |
| `src/shell/TitleBar.tsx` | 更新指示器 / 主题开关两个 `IconButton` 同上 |

### 3. 不再同时弹两种提示（`Button.tsx` / `ToggleBlock.tsx`）

按钮原语的 `title` 现在只当**文案来源**：一旦有提示可弹（`tip` / `title` / 无文字按钮的
`aria-label`），就不再把它写到 DOM 上 —— 否则我们的气泡之后，浏览器还会补一个系统灰框（两张提示）。
没有提示可弹时 `title` 原样保留。

## 验证

- `pnpm typecheck`：通过。
- `pnpm test`：1336 passed / 0 failed。
- `pnpm lint:colors` / `lint:arch` / `lint:i18n`：通过。
- `pnpm smoke:ui`：**problems: []**（含新加的两组断言）：
  - `[data-tip-switch]`（相邻两枚图标按钮）：A→B、B→A 都必须弹，且全程最多一张气泡；
  - `[data-tip-nested]`（外层 `Tooltip` 包图标按钮）：只弹外层那一张（内层 `ButtonTip` 让位）。
  - 断言用 `Input.dispatchMouseEvent` 发**真鼠标移动**（合成 PointerEvent 测不出来这条 bug）。
- 手工探针（CDP，`/mnt/c/src/tmp/tooltip-verify.mjs`）：修复前 `A→B` 为 `[]`，修复后为 `['新建@bottom']`；
  两个按钮的 `title` 属性均为 `null`（不再有原生双提示）。

## 遗留

- `RemoveButton` 仍用原生 `title`（它没接 `ButtonTip`）。全项目只剩它一处原生提示，
  要么后续统一接到 `ButtonTip`，要么明确「行尾动作按钮用原生」——留给评审拍板。
- 画廊里留了两个冒烟钩子（`data-tip-switch` / `data-tip-nested`），它们同时是这两个 bug 的示例。
