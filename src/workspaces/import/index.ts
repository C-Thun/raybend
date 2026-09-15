/**
 * `import` 工作区的唯一对外出口（`ARCHITECTURE.md` §2）。
 *
 * 组装层（`src/App.tsx`）只需要两样东西：工作区视图与它的 store 工厂。
 * 其余一律不从这扇门出去 —— 公开面越小，能改的地方越多。
 *
 * 注意 `CheckedDir` 这类**跨模块的领域模型**不在 `store.ts` 里，而在
 * `src/lib/checked-dir.ts`：feature 不能 import `workspaces/`（分层规则），
 * 所以大家共用的类型必须住在公共层。
 */

export { ImportWorkspace } from "./ImportWorkspace.tsx";
export { createImportStore } from "./store.ts";
export type { ImportStore } from "./store.ts";
