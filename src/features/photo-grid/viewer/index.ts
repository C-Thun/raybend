/**
 * 看图的对外出口。**它住在 `photo-grid` 模块里面**，不是独立模块 ——
 * `ARCHITECTURE.md` §1 不允许 feature 之间直接 import，而看图是「网格里放大一张」，
 * 触发点与显示位置都在网格内。真要做成独立模块（例如 M2 的 wgpu 查看器、胶片带、
 * 对比模式），那时应当在 `workspaces/` 组合层接线，再把这两个文件整体搬出去 ——
 * 内部的 store/视图分层已经准备好，搬的时候只动 import 路径。
 *
 * 看图的状态在这里（`store.ts`），视图在 `Viewer.tsx`。
 * 后续加胶片带 / 对比 / 评级时：**往 store 上加状态**，视图跟着长，
 * 别把新逻辑塞进网格里。
 */

export { Viewer } from "./Viewer.tsx";
export type { ViewerProps } from "./Viewer.tsx";
export {
  clampPan,
  clampZoom,
  computeFitScale,
  createViewerStore,
  EMPTY_VIEWER,
  MAX_ZOOM,
  MIN_ZOOM,
  zoomPanAt,
} from "./store.ts";
export type {
  NaturalSize,
  ViewerPhoto,
  ViewerState,
  ViewerStore,
  ViewerStoreDeps,
  ViewportSize,
} from "./store.ts";
