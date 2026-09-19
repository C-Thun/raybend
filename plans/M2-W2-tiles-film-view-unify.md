# 工作单元：import 与 browse 统一 tiles / film / view（+ 多选对比 + Tab 三态）

> 人类 2026-09-19 下达。**一次只做这一个单元**（`AGENTS.md` §5.4）。
> 前置：Tab 三态已修正为 `film+左右` → `film only` → `view only`（`2251a64`）。

## 0. 目标与硬约束

**目标**：import 与 browse 的**中列**都能在 `tiles` / `film` / `view` 三态之间切换，
两边都支持**多选对比**与 **Tab 循环**，且**只有一套实现**（`AGENTS.md` §2 #13 复用优先）。

**硬约束（人类原话，不许打折）**：

1. **目前 `tiles` 调得最正确的是 import** —— 统一时**以 import 为准**，
   **不要拿 browse 那份未修复的错误实现去覆盖 import** ✗。
2. 三态就是三态：`film+左右` / `film only` / `view only` —— 没有 `view+左右`（已修正）。
3. browse 里那些**已经做对的东西不能丢**：标记（星级/色标/旗标/锁）、筛选、标签、排序、
   选择逻辑（Shift 区间翻转）、锚点与对比规则。

## 1. 现状盘点（谁有什么）

| 能力 | import | browse | 结论 |
| --- | --- | --- | --- |
| tiles 网格 | `PhotoGrid` + `lib/tile-flow.ts`（行列数学、`aspect` 取法） | `BrowseGrid`（同一套 `Tile`，但标记/取数是 browse 自己的） | **import 的排版口则是基准** |
| film 胶片带 | ✗ 没有 | `features/browse/FilmStrip.tsx`（自绘，与 browse store 耦合） | 要提到共享层 |
| view 看图 | ✗ 没有 | `features/browse/CompareView.tsx` + `components/ui/viewer/*` | 要提到共享层 |
| 三态循环 | ✗ 没有 | `features/browse/chrome.ts`（纯函数，已修正） | 提到 `lib/` |
| 多选对比规则 | ✗ 没有 | `features/browse/compare.ts`（纯函数 + 单测） | 提到 `lib/` |
| 键盘意图 | 部分 | `features/browse/keys.ts`（纯函数 + 单测） | 提到 `lib/` |

## 2. 做法（按顺序，每步可独立验证）

1. **纯逻辑先搬家**（低风险、无 UI 依赖）：
   `chrome.ts` / `compare.ts` / `keys.ts` → `src/lib/`（保留原单测，路径一起改）。
2. **胶片带与对比视图去耦合**：`FilmStrip`、`CompareView` → `components/ui/viewer/`，
   把「从 browse store 取数据」改成 **props + 回调**（选谁、缩放、当前锚点都由调用方给）。
   —— 这是「一套实现两边用」的关键，不改行为。
3. **browse 的 tiles 对齐 import 口径**：`BrowseGrid` 的行列数学与 `aspect` 取法
   **直接用 `lib/tile-flow.ts`**（已如此 ✓），差异只剩「标记插槽」——
   把差异**参数化**（`context="library" | "source"` 已有此类槽位），不要复制第二份。
4. **import 接入 view/film/compare**：中列加三态切换（同一份 `chrome.ts` + 同一个
   `Viewer` / `FilmStrip` / `CompareView`），Tab 走同一个 `nextChrome`。
5. **冒烟两侧都覆盖**：`check:browse` 继续断言 browse 的三态与对比；
   新增/扩展断言**import 的 tiles 行为未变**（回归保护 —— 这条最重要）。

## 3. 不许做的事

- ✗ 不要把 browse 的标记/选择实现搬进 import（import 没有库内标记的概念）。
- ✗ 不要改 import tiles 的排版口径（列数、余量、`aspect` 夹取）—— 那是基准。
- ✗ 不要新增第二份三态/对比/键盘逻辑；发现重复就地收敛。

## 4. 完成定义（DoD）

- import 与 browse 都能：三态切换（Tab）、进对比（多选 ≥2）、Esc 返回；
- 两边的 tiles 排版一致（同一份 `lib/tile-flow.ts` + 同一个 `Tile`）；
- `pnpm test` / `cargo test` / `check:browse` / `smoke:ui` 全绿；
- `BROWSE.md` §5.4 与 `DESIGN.md` 的口径同步更新；
- 一份 `implementations/` 记录（含「import tiles 未被改变」的证据）。
