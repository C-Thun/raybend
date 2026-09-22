# tiles 缩放杆「拖一格就断」——Solid children 被重建的根因与修复

完成时间：2026-09-23 04:06:40 CST

## 1. 起因

人类 2026-09-23 报：**「tiles 一拉缩放杆，移动一格后又丢焦点拖不动」**（「又」= 这个问题以前修过一次，
`AGENTS.md` §11.4 那句「受控滑杆更新配置时不得重建 `TilesControlBar` DOM」就是上一轮留下的）。

## 2. 复现与定位（真机、可复现）

工装：临时探针页（直接挂真实的 `TilesShell + TilesControlBar + Slider`，只把 `tileStep` 换成一根信号）
+ 无头 Chromium 走 CDP 发**真鼠标事件**（`Input.dispatchMouseEvent`），每步比对节点身份。
页面用完即删，未进仓库；留下的**永久回归**在 `scripts/check-browse-boot.mjs`。

实测（修复前）：

```
起点值 4 → 鼠标 +10px → 值 5.54，onTileStepChange 只收到 1 次
每步 +8px 的值序列：[5.23, 5.23, 5.23, ...]   ← 第一格之后再也不动
身份核对：{ barSame: true, sliderRootSame: false, controlSame: false, thumbSame: false }
```

给 `Node.prototype.insertBefore/appendChild/replaceChild` 挂钩子后拿到的调用栈全部落在
`solid-js_web` 的 `insertExpression → reconcileArrays` —— **状态栏的 children 被整排重建了**。

## 3. 根因（不是滑杆的错）

链条三步，缺一不可：

1. `BarFrame`（状态栏外框）写的是 `{props.children}`，Solid 把它编译成
   `insert(el, () => props.children)` —— 那是个 **render effect**，**不是一次性插入**；
   而 children 的 getter 每求值一次，里面的 `createComponent(...)` 就**重跑一遍**。
2. Ark 的 `SliderRoot` 创建时用 `createSplitProps()` **同步**读走 `props.value`
   （`@ark-ui/solid` 的 `slider-root`）—— 那次读被记在了上面那个 render effect 头上。
3. 于是：拖动第一格 → `tileStep` 变 → 那个 effect 重跑 → 整条状态栏（含 Ark Slider 实例）
   被 `createComponent` 重建 → **正在拖的滑块元素没了** → 拖动与焦点一起断。

所以病根不是「滑杆没抓住指针」，而是**包装组件把 children 透传出去时没 `untrack`** ——
任何在「创建 children 期间被读到」的信号，都成了「重建整棵子树」的开关。

## 4. 改动

| 文件 | 改动 |
| --- | --- |
| `src/components/ui/tiles/TilesControlBar.tsx` | `BarFrame` 里 `const children = untrack(() => props.children)`，插一次就完事；带整段「为什么」注释 |
| `src/components/ui/tiles/TilesShell.tsx` | 同一条规则加固外壳：它的 children 是网格/看图件，被重建 = 滚动位置与选择全丢（§11.4 红线） |
| `scripts/check-browse-boot.mjs` | 新增真浏览器回归：按住缩放滑块拖 4 步 → 断言**一路跟手** + 滑块/状态栏/**网格**节点身份不变 + 拖回原位能还原档位 |
| `AGENTS.md` | 新增 **§2.17 硬约束**（包装组件透传 children 必须 untrack）+ 把 §11.4 那条旧记述改成「带根因 + 带回归」的版本 |

判据为什么取**节点身份**：只断言「值变了」的话，「重建 + 拿新值重画一遍」这种假绿照样能过
（实测：修复前第一步的值确实变了，`onTileStepChange` 也真的收到了一次）。顺带把网格也钉住 ——
tile 尺寸一变就重建网格的话，滚动位置与选择都会丢，那是更贵的 bug。

## 5. 验证方式（Agent 侧冒烟）

- **探针页（修复前 / 修复后对比）**：修复后
  `{ sliderRootSame: true, controlSame: true, thumbSame: true }`、每步 +8px 的值
  `[5.23, 6.46, 7.69, 8.92, 10.15, 11.38, 12.62, 13.85, 15.08, 16, 16, 16]`（单调、到顶饱和）。
- **`pnpm check:browse`（真应用、假后端、真鼠标）**：
  * 暂存掉修复（`git stash push src/components/ui/tiles/TilesControlBar.tsx`）→ 新回归**立刻报红**：
    「拖动缩放滑块必须一路跟手（起点 10，每步实测 [10.92,10.92,10.92,10.92]）」
    「拖动过程中滑块节点被重建了」——**反面验证通过**；
  * 恢复修复 → 上述三条**全部消失**（网格身份断言也通过：网格没被重建）。
- `pnpm typecheck` 0 错；`pnpm test` **802 通过 / 0 失败**。

## 6. 遗留

1. **树里还有另一个工作单元的在途改动**（`viewer-chrome.ts` / `ToolsBar.tsx` / `viewing.ts` /
   `plans/M3-W1.md` / `design/editor.pen` 等，2026-09-23 03:57–04:05 之间在改）。
   `check:browse` 里剩下的失败（看图进不去、`photosFromSource is not defined` 等）都出在那批文件上，
   **与本次改动无关**；本次只提交自己的三个文件，`AGENTS.md` 因为夹着那批在途改动**暂不提交**。
2. 同类包装组件还有 `shell/TitleBar.tsx`、`shell/ToolsBar.tsx`、`dev/KitchenSink.tsx` 透传 children，
   尚未加 `untrack`。§2.17 已把规则写死，但那两个 `shell/` 文件正在被另一个工作单元改，
   这次**没有动**（避免同一文件两个写者）；它们目前没有已知的重建症状，属低风险。
3. 上一轮修这个 bug 时（§11.4 旧文）只记了「不要重建 DOM」的结论，没记**机制** ——
   于是这次又从零查了一遍。现在 §2.17 里连**诊断手法**（insert 钩子 + 身份比对）一起写了。
