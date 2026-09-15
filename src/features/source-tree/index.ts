/**
 * `source-tree` 模块的唯一对外出口（`ARCHITECTURE.md` §2）。
 *
 * 视图、它自己的内部 store（展开/懒加载）、以及行模型都从这里出。
 * 「选中」「勾选」不在这个模块里 —— 它们是工作区的共享状态，
 * 以 props 传进来（这也是「选中不触发展开」的保证方式）。
 */

export { SourceTree, volumeKindLabel } from "./SourceTree.tsx";
export type { SourceTreeProps } from "./SourceTree.tsx";
export { createSourceTreeStore } from "./store.ts";
export type { SourceTreeDeps, SourceTreeStore } from "./store.ts";
export { buildTreeRows, volumeDisplayName } from "./rows.ts";
export type { TreeRow, TreeRowInput } from "./rows.ts";
