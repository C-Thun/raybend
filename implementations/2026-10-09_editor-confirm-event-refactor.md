完成时间：2026-10-09 14:58:30 +0800

# 编辑器 confirm 事件重构：快照只在一个入口建立 + 全量审计 + 诊断

## 需求（崔总 2026-10-09）

> 撤销重做有 bug……给我的感觉是，这套保存快照的逻辑似乎也是基于限流实现的，而不是鼠标放开时实现的，
> 我认为对于用户来说，只有鼠标放开才能保存快照（包括选 lut，调曲线鼠标放开），包括所有的双击归位
> （包括拉杆和曲线清点，曲线重置，开关 lut），你要一个一个面板 run 过去，找到所有能确定用户某一个操作
> 节点结束的标志位，在这个标志位里定义一个类似 `confirm` 的事件，所有的快照建立都基于这个事件来跑，
> 以后每加一个编辑功能，都要确定这个功能有哪些可以视为 confirm 的节点，加这个事件（记进 agents.md），
> 从而确保快照的建立始终不出问题。
>
> 这个问题是可能比较难查，所以我才让你用我的事件思路把功能统一起来做次重构，这样即使后续出问题，也容易排查。

## 一、概念落地

* `EditorWorkspace.confirmEdit`（原 `commitDevelop`）＝ **唯一的快照建立入口**；
  `EditorActions.commitDevelop` 同步改名 `confirmEdit`；面板回调 `onCommit` → `onConfirm`（5 个文件）。
* `confirmEdit` 的文档注释里带一张**节点清单表**（逐面板核对，2026-10-09）：
  参数拉杆（松手 / 双击归位）、降噪方式（选择）、镜头（选档 / 开关）、曲线（松手 / 双击删点 / 重置）、
  LUT（选中 / 开关）、预设（应用）、色彩输入（三个按钮；选 profile 只是预览）、工具栏（自动调整 / 重置 / 编辑源）、
  定稿（切换 / 创建）、画布工具（裁切·旋转的「确认」）。
  另注明**不是**节点的两类：旋转角度拉杆（工具草稿，落库在工具确认）、载入时的自动纠正（`setEditBase(target.actualBase)`）。

## 二、全量审计（崔总给的思路，已执行）

**方法**：看 profile 结构 → 找出「会改 profile 的 store 方法」→ 找它们的 UI 调用点 → 逐个对照 confirm。

* 从 `store.ts` 自动提取所有含 `bumpDevelop()` 的成员（含经内部函数间接标脏的）：
  **18 个**（`setParam` / `resetParam` / `setCurvePoints` / `resetCurve` / `setLut` / `setLutEnabled` /
  `setLensProfile` / `setLensEnabled` / `setBaseCurve` / `setEditBase` / `setGeometry` / `setNrMethod` /
  `setColorState` / `applyDevelop` / `applyPresetSnapshot` / `applyAutoAdjust` / `resetDevelop` / `resetParams`）。
* 每个方法的 UI 调用点全部核对：**没有漏接 confirm 的节点**（4 处存疑点复核：镜头选择 701 行有、
  LUT 开关走 `onToggle`、换照片时的 `setEditBase` 是自动纠正、`resetParams` 无 UI 调用点）。
* 结论：**缺口不在「漏接线」**；崔总遇到的现象更可能出在**时序/多余事件**上（见 §四的怀疑）。

## 三、诊断能力（崔总「容易排查」的抓手）

* `EditorStore.confirmTick` / `confirmNote` / `noteConfirm()`：confirm 计数 + 最近一次的摘要
  （`rev N · X 参数 · Y 曲线`）。快照少了先看它涨没涨：**涨了 = confirm 发生了（问题在快照内容或被合并）；
  没涨 = 某个节点没接到 confirm**。
* `console.debug("[editor] confirm", { rev, values, curves, lutId, lutEnabled, sourceBase })`：
  **每次都打，不进 DEV 守卫**（真机排查要用）—— 打开控制台就能看见每次 confirm 带的是什么。

## 四、对崔总现象的分析（未定论，留给真机日志）

崔总操作 A（高光加大 → 双击归零 → 黑区加大 → Ctrl+Z 回到「像步骤 1」）最可能的一条解释：

**双击时第二下若落在轨道上而不是 Thumb 上**，Zag 会把值跳到点击位置（一次真实的 `onValueChange` +
`onValueChangeEnd` → 一次 confirm），而 `dblclick` 只挂在 Thumb 上 → **归零没有发生**。
于是「撤销一次」回到的是「黑区已撤、高光仍是跳变值」的状态 —— 与崔总看到的「跳到了 1」吻合。

验证方法（真机一条日志即可定论）：双击拉杆后看 `[editor] confirm` 日志条数 ——
**1 条**（只有跳变）⇒ 归零没发生；**2 条**（跳变 + 归零）⇒ 归零发生了。
**本次未擅自改 Zag 的交互**（未证实的猜测，改了可能更糟）；若日志证实，再按「扩大 Thumb 命中区」修。

## 五、纪律（已写进 `AGENTS.md` §2.21）

新加编辑功能必须：列操作节点 → 逐个接 `onConfirm` → 在 `confirmEdit` 的清单表补一行；
工具草稿类与自动纠正类要写明「不是节点」。反查法（崔总提供）也记了进去。

## 验证

* `pnpm typecheck` ✓、`pnpm test` **1239/1239** ✓、`lint:arch` / `lint:colors` / `lint:i18n` ✓
  （`noteConfirm` 的中文摘要按惯例加 `// i18n-exempt: 控制台诊断`）。
* Rust 侧未动；`cargo test --workspace` 见当轮汇总。
