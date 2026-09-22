# 拉杆未填充段加 50% 浓度（`--slider-track` 半透）

完成时间：2026-09-23 03:57:30 CST

## 1. 本次范围

人类 2026-09-23 追加的设计细节（口述）：

> 「pencil 中设计的拉杆的底纹是 dark 用深浅色、light 用浅深色，这两个色再给个 **50% 浓度**控制，
> 做出半透效果，现在确实颜色反差有点太重了，甚至有点挡拉杆有效区域的主色线条。」

落实为三件事：**画布变量 → 令牌定义 → 设计规范**，三处同改（实现侧暂时还没有拉杆组件，M3-W1 才写）。

| # | 位置 | 改动 |
| --- | --- | --- |
| 1 | `design/editor.pen` 变量 `slider-track` | `#EDF0E9` / `#2A2D33` → **`#EDF0E980` / `#2A2D3380`**（尾部 `80` = alpha 50%）。31 根拉杆引用的是变量，所以一次改完全部生效 |
| 2 | `DESIGN.md` §9.2 令牌表 + 说明段 | 令牌值写明 **50% 半透**；补理由（实心时未填充段**压过主色填充线**，主次颠倒）与实现纪律（**值里已带 alpha，用它的地方不要再乘一次**） |
| 3 | `DESIGN.md` §14.10 拉杆规范 | 「未填充段用 `--slider-track`」那一条补上「+ 50% 半透」，并写明半透是**必须的** |
| 4 | `src/styles/tokens.css` | 新令牌 `--slider-track` 落到 dark / light 两个主题块（`#edf0e980` / `#2a2d3380`）+ Tailwind 桥 `--color-slider-track` → `bg-slider-track` |

## 2. 关键决策与理由

1. **浓度做进令牌值，不在组件里乘**。理由是「一处定义」：若令牌是实心值、组件里再写 `opacity-50`，
   同一个浓度会被写进每个用到它的组件（画布、实现、以后可能的第二处），改一次漏一次。
   现在令牌自己就是半透的，引用方无脑用即可。
2. **仍然是「反色方向」的中性色**（dark 取浅、light 取深），只是叠了 50% 浓度 ——
   语义没变，变的是浓度。所以 `DESIGN.md` §9.2 里那条「实心凹槽在两种主题下都难看清」的解释保留，
   只把「实心」的说法改成「半透」。
3. **画布变量的改法用 8 位 hex**（`#RRGGBBAA`）：`editor.pen` 里已有先例
   （`state-hover` = `#F0B03329`、`state-selected` = `#52C6AB38`），Pencil 直接吃这个写法，
   不需要另建一个「浓度」变量。

## 3. 验证方式（Agent 冒烟）

- `python3 -c "json.load(open('design/editor.pen'))"` —— 文件仍是合法 JSON，变量值为 `#EDF0E980` / `#2A2D3380`；
- `pnpm test` → **792 通过 / 0 失败**（含 `styles/tokens.test.ts` 的「两主题定义同一批令牌」结构性检查）；
- `pnpm lint:colors` → 无硬编码色值（新色值只落在 `tokens.css`）。

## 4. 遗留

- 画布上 31 根拉杆的**视觉复核**（半透后压在 `surface-layer` / `surface-main` 上的实际观感）
  需要人类看一眼 —— Agent 只能确认值改对了。
- 实现侧（`SliderRow`，M3-W1）落地时按 `DESIGN.md` §14.10：轨道用 `bg-slider-track`、填充用 `bg-brand`，
  **不再额外加透明度**。

## 5. 改动文件

- 修改：`design/editor.pen`、`DESIGN.md`（§9.2 / §14.10）、`src/styles/tokens.css`
- 新增：`implementations/2026-09-23_slider-track-half-opacity.md`（本文件）
