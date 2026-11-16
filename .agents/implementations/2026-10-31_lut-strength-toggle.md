# LUT 强度拉杆 ⇄ 层开关联动

2026-10-31 18:50:37 CST 完成。

## 需求（崔总 2026-10-31 口述）

> 强制调节杆的行为优化一下，除了之前做的保底逻辑尽管保留外，设计其为当把调节杆拉到 0% 时松手，
> 其效果是同时关闭该 lut 的应用；同样，在 lut 应用开关关闭的状态下，拉动拉杆到大于 0% 的状态松开，
> 自动开启该 lut。
>
> 这套逻辑与顶部 lut 应用总开关不关联，无论上面的总开关是否开启，照常关联运作。
> 只要总开关关着，最终总是不会触发 lut 计算。

## 实现

**语义（只动这一层，不碰总开关）**：

| 手势 | 结果 |
| --- | --- |
| 强度拉杆拖到 **0%** 松手 | 该层 `enabled = false`（强度保留 0，便于再拉回） |
| 该层 `enabled = false`，拖到 **>0%** 松手 | 该层 `enabled = true` |
| 双击拉杆（回到 100%） | 走同一条：`strength = 1` 且该层被打开 |
| 拖动过程中 | **不动**开关，只更新数值（`setLutLayerStrength`） |
| 顶部「应用 LUT」总开关 | 完全不参与这条联动；关着时整条链照旧不参与渲染 |

落点：

- `src/features/editor/store.ts`：新增 `commitLutLayerStrength(index, strength)`
  —— 夹到 0..1，`enabled = strength > 0`，其余不变；无变化时提前返回（不产生空 confirm）。
  拖动中的 `setLutLayerStrength` 保持不变（只改数值）。
- `src/features/editor/lut-panel.tsx`：`Slider.onValueCommit` 改调 `commitLutLayerStrength`；
  双击也走同一条（关着的层会随「回到 100%」一起打开）。

**保留的既有保底**（未被改动）：

- 0% / 关闭的层在渲染侧不参与（`LutChain::new` 丢弃 strength ≤ 0 的层）；
- `DevelopStack::lut_chain_active()` 要求总开关开 + 至少一层 enabled 且 strength > 0；
- 总开关只随「空链 ↔ 有层」与显式换底层变化，调强度/排序/加减层不会擅自打开；
- 强度夹取、上限 8 层、同 id 去重全部照旧。

## 验证

```text
pnpm typecheck   → 通过
pnpm test        → 1353 passed / 0 failed（新增 2 条用例）
pnpm lint:i18n   → 通过
```

新增用例（`src/features/editor/store.test.ts`）：

1. `强度松手：拉到 0% 关掉该层，从关闭状态拉回 >0% 自动开启该层`
   —— 含「拖动中不动开关」「双击还原会开层」。
2. `强度松手与顶部总开关互不影响：总开关关着时该层照常开关，但不参与渲染`
   —— 两个方向都不改动总开关。

**未经人类验证**：真机上的手感（松手瞬间开关跳变是否自然）；面板接线只有类型检查与代码走查覆盖，
渲染层没有组件级 DOM 测试（仓库现有惯例是测 store 逻辑）。
