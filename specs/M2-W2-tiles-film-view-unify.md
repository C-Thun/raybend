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
- `.agents/memory/FUNCTION-BROWSE.md` §5.4 与 `.agents/memory/DESIGN.md` 的口径同步更新；
- 一份 `.agents/implementations/` 记录（含「import tiles 未被改变」的证据）。

---

## 5. 完成记录（2026-09-19 14:39 CST）

**已全部做完**（`.agents/implementations/2026-09-19_viewer-three-states-shared.md` 有完整记录与证据）：

1. ✅ 三个纯逻辑模块搬到 `lib/`（`viewer-chrome` / `viewer-compare` / `viewer-keys`），
   引用路径按编译器报错逐条改正；对比数学改成**结构类型 + 泛型**（`ComparablePhoto`），
   顺带修掉 `lib → ui` 的分层违规。
2. ✅ `FilmStrip` / `CompareView` 搬到 `components/ui/viewer/`；
   `FilmStrip` 解开对 browse store 的依赖（`selectedIds` + `onSelect(id: string, mode)`），
   并去掉「id 是数字」这个 browse 专属假设。
3. ✅ browse 的 tiles 未改（本来就共用 `lib/tile-flow.ts` + `Tile`）。
4. ✅ import 中列接上三态 + 胶片带 + 对比 + `Tab` / `Enter` / `Esc`。
5. ✅ 冒烟双侧覆盖：`check:browse` 的三态断言更新为新语义（并且**正是它当场抓到**语义回归）；
   `smoke:ui` 绿；**import 的 tiles 未动**有 `git diff` 为空的证据。

---

## Requirement (verbatim)

> 来源 `specs/M2-W2-tiles-film-view-unify.md`（2026-11-15 迁入；`todos/` 已退役）。**正文逐字保留**，只把标题层级降一级以免破坏本文档结构。

## 需求原文存档：toolsbar 紧缩 + 导出筛选 + export tile 小改（2026-10-11）

> 状态：**未开工**（spec 已写，待崔总审 → 画稿 → 实施）
> 进度事实来源是 pi 的 `todo` 工具，本文件只是需求暂存（`AGENTS.md` §10）。
> 落地规格：`specs/toolsbar-compact-and-export-gallery.md`

### 1. 原文（崔总 2026-10-11，逐字）

> 我尝试在这里简单描述一下，然后你来做下规划。我的思维链是这样：我发现在 export 中事实上需要实现相片筛选功能，
> 然后就想把 browse 的筛选功能搬过去，接着就发现那个条太长了，里面有很多文字混在里面，所以我计划是这样：
>
> 1、先把所有 flow 的 toolsbar 上的按钮都检查一遍，看看有哪些可以精简文字（基本上图标就能看明白的），
> 就把文字精简掉，进行紧缩；
>
> 2、为所有只有图标没有文字的 toolsbar 上的图标，加上鼠标指向文字，之前也有这样功能的，
> 鼠标指向显示按钮文字要统一风格，最好做到统一的按钮组件里；
>
> 3、完成这些后，将 browse toolsbar 上仅用于筛选的（主要筛选相片自身属性的，比如筛选锁不锁的就不需要了）
> 搬到 export 的 toolsbar 上，并且用之前做过的竖线分隔条与原来的 export toolsbar button 隔开（功能逻辑不同）；
>
> 4、export tiles top 这块，把图标最小尺寸缩小一点，大约边长缩个 20%，现在这个宽度其实有点尴尬，
> 刚好会空，没有四列机会；
>
> 5、export tiles top 的每个 tile 的设计优化，改成主图下最多只下挂 2 张 issue，下挂两张 issue 的下面
> 两左右各放一个按钮，一个是展示全部（原来是要 6 个以上才有显示全部的），一个是 raw 按钮，
> 点击 raw 按钮直接把 raw 加入队列，原始 raw 不再在 issue 中直接以图的形式显示，其他逻辑和原来保持一致。你先规划，
> 不是马上干，并且我已经开了 pencil，你到时要先画图再做的，在开工前先规划写 spec，然后我说开工后也是先画好 penc
> il，我审核过后再正式开干，如果 pencil 连接不上停下来跟我说。

### 2. 背景（为什么做）

- 导出需要「按相片自身属性筛出要导的图」，browse 已有整套筛选（评级阈值 / 色标 / 喜欢）与结果 chips；
  这套能力要接到 export toolsbar 上。
- 但 export toolsbar 目前全是文字按钮，再接四组筛选控件会过长 ⇒ 先把四个 flow 的 toolsbar 做一次
  文字精简（`.agents/memory/DESIGN.md` §12.12 的分组与分隔继续适用）。
- 只留图标的按钮必须能说清自己是什么 ⇒ 悬停提示统一到按钮组件里（现在是原生 `title=` 与
  `Tooltip` 混用、两套观感）。
- export 上部网格最小边长 240 时中列（≈920）永不出现四列 ⇒ 缩 20%（240 → 192）。
- export tile 的 issue 条最多三行六张、RAW 也以图的形式占位 ⇒ 改为最多一行两张 +
  「全部定稿 / RAW」两个按钮；RAW 变成动作（直接入队），不再占 issue 图位。

### 3. 落点提示

- 规格：`specs/toolsbar-compact-and-export-gallery.md`（含逐条口径、文件级改动、测试与画稿计划）
- 会碰到的既有文档/规格：
  - `.agents/memory/DESIGN.md` §12.12（toolsbar 分组与分隔、文字按钮一律带图标——本轮要补「可精简为纯图标」的口径）
  - `design/export.md` §2.1 / §4.2（export toolsbar 与 mid top 画廊）
  - `design/browse.md` §2.1、`design/editor.md` §2.1（另两个 flow 的 toolsbar）
  - `specs/export-issue-ordinal.md` §4（RAW 回到 issue 行的旧口径，本轮修订）
- 代码侧不能违背的红线：筛选状态仍只有一份（`browseStore`）、入队实现只有一套
  （`exportStore.enqueue` → `appendSnapshots`）、tile/网格组件不复制、`untrack` 纪律照旧。

### 4. 验收要点（写 spec 时细化）

- 四个 flow 的 toolsbar 在紧凑档下不出现「文字混排过长」；纯图标按钮都有统一悬停提示与无障碍名。
- export toolsbar 出现筛选组，与队列动作之间有 `ToolsSeparator`；筛选条件在画廊生效且 chips 可见可删。
- export 上部四列可达成（当前窗口宽度下 4×最小边长 + 3×3px 间距 ≤ 中列宽）。
- 每张 tile：主图 + ≤2 张 issue（不含 RAW）+「全部定稿」（多于 2 张时）/「RAW」（直接入队）按钮。
- `pnpm typecheck && pnpm test`、颜色/i18n/架构 lint、`pnpm build`、`pnpm check:export` 全绿；
  GUI 观感与真机由崔总验收。
