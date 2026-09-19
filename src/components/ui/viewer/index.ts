/**
 * 看图的共享件（store + 视图）。**2026-09-18 从 `features/photo-grid/viewer/` 搬到这里。**
 *
 * 为什么住在 `components/ui/`：架构规则（`ARCHITECTURE.md` §1、`check-architecture.mjs` 规则 2）
 * **不允许 feature 之间互相 import**，而看图现在有两个消费方 —— 导入工作区的网格（M1）
 * 与浏览工作区（M2-W2）。放进基础组件层，两边都能用（features / workspaces → ui 是允许的方向），
 * 也不必为它单开一层。
 *
 * 代价与纪律：`components/ui/` **不许 import `api`** —— 所以取图这件事由调用方注入
 * （`ViewerStoreDeps.loadScreen` / `loadThumb`），视图只管显示。这样也正好对上
 * M2-W2 §2.1 的「统一取图口」：取图的策略在 Rust 侧，前端只把字节交给它。
 *
 * 看图的状态在 `store.ts`（缩放锚点、夹取、适配、渐进显示），视图在 `Viewer.tsx`。
 * 后续加胶片带 / 对比 / 标记时：**往 store 上加状态**，视图跟着长。
 */

export { Viewer } from "./Viewer.tsx";
export type { ViewerProps } from "./Viewer.tsx";
export {
  clampPan,
  clampFramePan,
  clampZoom,
  computeFitScale,
  createViewerStore,
  EMPTY_VIEWER,
  MAX_ZOOM,
  MIN_ZOOM,
  visibleRect,
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
/** 胶片带与对比视图：**import 与 browse 共用同一份**（人类 2026-09-19） */
export { FilmStrip } from "./FilmStrip.tsx";
export { CompareView } from "./CompareView.tsx";
