# 三条真机反馈：容差报错、LUT 联动、debug 面板入口

> 完成时间：2026-11-01 22:07:02
> 来源：崔总 2026-11-01 真机反馈（截图：`不支持：控制点坐标必须在 0..=1、容差必须在 100..=1000…`）

## 1. 调容差报错 —— 旧种子的校验死锁（根因）

**现象**：拖控制点的容差手柄，落库时弹 toast「不支持：控制点坐标必须在 0..=1、容差必须在 100..=1000、种子必须是合法有限数」。

**根因**（三个事实叠在一起）：

1. 种子定义 2026-11-01 从线性域 `[luma, c1, c2]` 换成感知域 `[Y′, Cb, Cr]`，新范围是
   `Y′ ∈ 0..=1`、`Cb/Cr ∈ -0.5..=0.5`；
2. 旧种子无法换算，只能重新取样（`EditorWorkspace.migrateLegacySeeds` → `develop_area_seed`）；
3. 而 `develop_area_seed` **第一步就 `stack.validate()`** —— 旧种子按新范围校验必然不合法，
   于是取样失败 → 旧种子永远迁不动 → 之后**任何一次落库**都报这条错。

**修法**：

| 位置 | 改动 |
| --- | --- |
| `crates/raybend/src/store/develop.rs::Area::valid` | 旧种子（`seed_version != SEED_VERSION`）**只要求是有限数**；新种子仍按新范围卡 |
| 同一处错误文案 | 带上实际坐标 / 容差 / 种子 / 版本号，下次真机反馈不必再猜是哪个字段 |
| `crates/raybend/src/develop/areas.rs::feature_vector` | 输出夹回 `Y′ 0..=1`、`Cb/Cr ±0.5`（数学极值正好在边界，f32 会溢到 0.5000001，被 `..=0.5` 误杀） |

新增测试：
* `legacy_seed_outside_the_new_ranges_still_validates_and_bypasses`（旧种子越界仍可校验 / 仍整层旁路 / NaN 照样拦）；
* `feature_vector_always_stays_inside_the_stored_ranges`（七种极值颜色全在范围内）。

## 2. LUT 开关联动「丢了」

**结论**：规则本身还在 `store`，但**判定挂在手势收尾**（窗口 `pointerup` / 键盘 `keyup`）上：
`setLutLayerStrength` 只改数值，`commitLutLayerStrength` 才定开关。只要那一次收尾没来
（指针被别处捕获、程序侧写值、组件重挂、键盘焦点不在行上），就会出现「开关开着 + 强度 0%」，
而且之后的联动也不再发生 —— 就是崔总看到的「整套逻辑全丢了」。

**修法**（把规则放进**写入的同一步**，不再依赖收尾）：

* 抽出 `applyLutStrength(index, strength)`：夹取强度 → **开关恒等于「强度 > 0」** → 一次 `commitChain`；
* `setLutLayerStrength` 与 `commitLutLayerStrength` 都走它（松手只多记一个撤销节点）；
* `setLutLayerEnabled` 保持：0% 的层点开 ⇒ 强度直接给回 100%。

三条行为因此无条件成立：拉到 0 ⇒ 立刻关层；从 0 拉回 >0 ⇒ 立刻开层；0% 点开关 ⇒ 100%。
不变式测试 `任何操作都不会留下「开着但强度 0%」的层` 覆盖各入口。

同步更新：`specs/editor-lut-chain.md`（§2 语义表、§5 交互）、`design/editor.md`（LUT 强度段）。

## 3. debug 面板入口没有

**根因**：入口守卫用的是 `import.meta.env.DEV`，而 `pnpm debug:win` 跑的是**生产前端构建**
（`vite build`）＋ debug Rust —— `DEV` 恒为 `false`，真机调试那份 exe 里永远没有入口。
我上一轮为了「生产包别带调试 chunk」把 `lazy()` 工厂也塞进 `DEV` 分支，等于把入口一起关掉了。

**修法**：

* `scripts/debug-win.mjs` 给前端构建加 `VITE_DEBUG_PANEL=1`（发版构建 / 普通 `pnpm build` 没有）；
* `src/lib/dev-build.ts` 是判定唯一处：`import.meta.env.DEV || import.meta.env.VITE_DEBUG_PANEL === "1"`；
  `mask-debug.ts` 用它（`node --test` 没有 `import.meta.env`，靠 `?.` / `&&` 短路）；
* `TitleBar.tsx` 的 `lazy()` 工厂**写字面量** `import.meta.env.VITE_DEBUG_PANEL === "1"` ——
  实测跨模块常量不参与这里的摇树（用 `DEV_BUILD` 时生产包又冒出 `MaskDebugDialog-*.js`）。

实测两侧都对：

```text
pnpm build                         → dist/assets 无 MaskDebugDialog chunk（摇掉）
VITE_DEBUG_PANEL=1 pnpm build      → 有 chunk，且 bundle 里 DEV_BUILD 折成 !0（true）
```

## 4. 验证

```bash
cargo test --workspace      # 1646 passed / 0 failed（含新增 3 条）
pnpm typecheck && pnpm test # 1382 passed / 0 failed
pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n   # 全过
pnpm debug:win              # 通过；exe 2026-11-01T14:06:18Z，dist 内含 MaskDebugDialog chunk
```

## 5. 遗留 / 需要崔总确认

* 视觉与手感（容差手柄跟手、LUT 拉杆到 0 时开关同步变灰、调试面板入口出现在标题栏 DEV 处）只能在真机确认；
* 旧控制点这次能自动重新取样了，但**旧定稿里的控制点仍按「整层旁路」处理**（不可逆），要新效果需重新定稿；
* 若 LUT 联动在真机上仍不成立，请给一条最简复现（哪一层、从多少拖到多少、开关状态），
  那说明问题在别处（例如面板根本没拿到 `onStrength`），我再按现场定位。

## 6. 追加（22:40）：入口按钮本身还是 `import.meta.env.DEV`

上一轮的修改只换了 `lazy()` 工厂与 `mask-debug.ts` 的判定，**渲染按钮的 `<Show>` 条件仍是
`import.meta.env.DEV`** —— 所以 `debug:win` 的 exe 里依旧看不见入口（崔总 22:37 的截图）。
现已改为 `DEV_BUILD`（工厂仍保留字面量形式，摇树靠它）。

构建产物里可以直接验证两种构建的差别：

```text
pnpm build                  → Show 编译成 when:!1（永不显示），无 MaskDebugDialog chunk
VITE_DEBUG_PANEL=1 pnpm build → Show 编译成 when:!0，有 chunk，mask-debug 的 IS_DEV 也是 !0
```

`/dev/kitchen-sink` 的入口**保持** `import.meta.env.DEV`：它的路由本身只在 DEV 注册
（`src/index.tsx:43`），调试产物里链过去会白屏。
