/**
 * `photo-grid` 模块的唯一对外出口（`ARCHITECTURE.md` §2）。
 *
 * 对外只有两样：**网格视图**与**它的 store 工厂**。
 * 行模型、缩略图队列、控制条都是内部实现 —— 工作区不需要知道它们。
 */

export { PhotoGrid } from "./PhotoGrid.tsx";
export type { PhotoGridProps } from "./PhotoGrid.tsx";
export {
  createPhotoGridStore,
  DEFAULT_GAP_MINUTES,
  GRID_SETTING_KEYS,
} from "./store.ts";
export type { PhotoGridApi, PhotoGridDeps, PhotoGridStore } from "./store.ts";
export { itemId } from "./rows.ts";
