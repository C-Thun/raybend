# compare 错位拖动（每格 offset + 右键单格拖 + 双击归位）

完成时间：2026-09-28 22:53:25

## 改动范围

`src/components/ui/viewer/CompareView.tsx`（单文件，无接口变化）。

## 需求（崔总 2026-09-28，原文见 `todos/2026-09-28-compare-pan-offset.md`）

- compare 多格视图：**右键拖某一格 = 只挪那一格**（错位）；左键拖 = 所有格同步（保持各自错位一起动）；
- 拖到边界就停；同步拖时拖不过去的格钉在边上、其余继续——错位在过程中被自动修正；
- **双击**（原有 100%↔适配 语义不变）随便点一下就**归位**（清全部错位）；
- 右键拖动期间不出浏览器菜单。

## 实现要点

- `paneOffsets: Map<photoId, {x,y}>`（信号）叠在共享 `pan` 之上；**生效平移 = clamp(共享 pan + 本格错位)**，
  clamp 在**渲染层逐格**做（`effectivePanFor`），不在存储层做。
- 由此左键同步拖动把共享 pan **原始地**动（`panBy` 不再 clamp）：
  拖不过去的格被渲染层 clamp 钉住、其余继续跟——「错位被自动修正」就是这个构造的直接结果，不需要逐帧对账。
- `dragging` 形状扩为 `{x,y,button,paneId}`：button 2 走 `offsetPaneBy`（单格、clamp 到生效层），
  其余走 `panBy`。pointerdown 时从 `focusFromTarget` 解析右键所在格。
- **松手归一化**（`normalizePaneOffsets`）：把 clamp 后的生效值写回错位表
  （`offset = clampedEffective − pan`），否则下一次手势会踩在看不见的越界偏移上（拖了没反应）。
- 归位：`fitTo` / `goToOneToOne` 里清错位——覆盖双击、适配键、`viewer.actual` 命令等所有入口；
  缩放（滚轮/加减速键）**不清**（缩放不该顺带重置错位）。
- 菜单：宿主 `onContextMenu` 统一 `preventDefault`（控件目标除外）+ 右键 pointerdown `preventDefault`。
- 可测性：每格带 `data-compare-offset="x,y"`（零偏移时不出现）。

## 关键决策

- **clamp 从存储层挪到渲染层**：保留旧的共享 clamp 会让「钉住的格继续被拖出界」或「一格到边全体停」，都违背需求；
  渲染层逐格 clamp 是唯一同时满足「同步拖 + 各自到边就停 + 自动修正」的构造。
- 归一化只写 `effectivePanFor` 的结果，不重算几何——与渲染同一条 clamp，不会出现存储/显示两个口径。

## 验证

- `pnpm test`：1083 项全绿（含既有 compare 布局/交互测试）。
- `pnpm lint:colors` / `lint:arch` / `lint:i18n`：✓。
- `npx tsc --noEmit`：本文件 0 错（仓库唯一红是**并行另一路**在 `src/api/types.ts` 加 `RepositoryView.connection`
  未同步 `dto-contract.test.ts` 键表所致，非本改动；运行时契约测试本身是绿的——tsx 不做类型检查）。
- 真机手感（右键拖/到边就停/双击归位）待崔总在 Windows 侧验证。

## 遗留

- 无（双指触摸/触控笔的右键等价物未特殊处理，走浏览器合成事件）。
