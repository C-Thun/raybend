# 修复：0% 关掉 LUT 层后点开关，强度被抬回 100%

2026-11-01 00:38:54 CST 完成。

## 现象

把某层的强度拉到 0% 松手 → 该层自动关闭；再点该层的开关想把它打开，结果强度直接回到 100%，
而不是「开着但 0%」。

## 根因（两个叠在一起）

1. **`onValueChangeEnd` 不是「松手」**。Zag 的滑杆机器在**外部 `value` 回灌**时也派发
   `invokeOnChangeEnd`（`@zag-js/slider/dist/slider.machine.mjs` 的 `SET_VALUE` 分支）。
   我们的滑杆是受控的：`onValueChange` → 写 store → 新值回灌 → 又触发一次「change end」
   → 我们把每一格都当成一次「松手提交」（`commitLutLayerStrength`）。
   于是「松手才判开关」实际变成「每拖一格都判」，而层被重新打开后任何一次回灌都会按旧值改写开关。
2. **强度写入的兜底会「猜」成 100%**：`Number.isFinite(strength) ? strength : 1`。
   任何非有限值（NaN / undefined，回灌与重新挂载都可能带进来）都会被当成满强度，
   表现就是「点一下开关，效果突然拉满」。

## 修复

- **新增 `src/lib/slider-gesture.ts`**（有单测 5 条）：`createGestureEnd(done)`
  只认真实手势 —— 指针在控件上按下 → **窗口任意位置**松开（`pointerup` / `pointercancel`，
  捕获阶段挂在 window 上，因为松手常落在控件外）；键盘用 `SLIDER_KEYS` 的 `keyup` 收尾；
  `dispose()` 在组件卸载时摘监听，不留「松手就提交」的孤儿。
- **`lut-panel.tsx` 的强度行**：去掉 `Slider` 的 `onValueCommit`（它就是 Zag 的
  `onValueChangeEnd`），改为手势结束 → `onStrengthCommit()`（无参数，取 store 当前值），
  双击 → `onStrengthReset()`（显式 100%）。行内注释写明「不要改回 `onValueCommit`」。
- **`store.ts`**：`setLutLayerStrength` / `commitLutLayerStrength` 对非有限值**直接忽略**
  （不再猜 1.0）；`NaN` 连对象都不换，不产生无谓的 `developRev`。
- 层**开关**本来就只改 `enabled`、不动强度 —— 现在没有任何回灌路径能改写它了。
- 顺手修：整行拖动的守卫补上 `label` —— Ark 的 `Switch` 根节点是 `<label>`
  （Control / Thumb 是 `span`），漏掉它时「点开关」会先起一次整行拖动。

## 涉及文件

- `src/lib/slider-gesture.ts` + `.test.ts`（新增）
- `src/features/editor/lut-panel.tsx`（手势接线、拖动守卫、`onStrengthReset`）
- `src/features/editor/store.ts`（非有限值忽略）
- `src/features/editor/store.test.ts`（+2 条回归用例）
- `specs/editor-lut-chain.md` §2 / `design/editor.md` §3.1.3（把「松手」的定义写死）

## 验证

- `pnpm test`：**1377 passed / 0 failed**（+18：新增手势单测 5 条与两条回归用例，
  其余为同轮历史用例）。两条关键回归：
  - 「0% 关掉之后点开关：强度直接给回 100%」（⚠️ 这条断言当时写反了：最初按「开着但 0%」
    是期望行为来写，崔总 2026-11-01 澄清规则相反 —— **不允许「开着但 0%」的层**，
    开关打开时强度要给回 100%。已在 `2026-11-01_lut-enable-at-zero-snaps-full.md` 改正）；
  - 「非有限的强度一律忽略，不会被猜成 100%」。
- `pnpm typecheck`、`pnpm build`、`lint:colors` / `lint:arch` / `lint:i18n`：全绿。

**未经人类验证**：真机手感（拉杆到 0% 松手 → 层关闭；点开关 → 开着仍 0%；再拖回去 → 自动开）。

## 遗留

- `Slider`（共享组件）的 `onValueCommit` 语义**没有改**：其它面板（曝光等）的 store 值不回灌
  成不同数字，问题不显形；但同一类「回灌即提交」在多写回灌时会复现。若以后要给其它面板写
  「松手才算」的逻辑，一律用 `lib/slider-gesture.ts`，不要再依赖 Zag 的 `onValueChangeEnd`。
- 真机上若仍见到强度跳变，先看控制台 `[editor] confirm`（会打 `lutChain` 的 id 列表），
  再确认构建是否包含本次改动。
