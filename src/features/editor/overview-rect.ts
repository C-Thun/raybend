/**
 * 编辑视口的**当前可见区域**（图像像素坐标）—— 左列底部「总览」上的视野框用它。
 *
 * 事实源全在 Rust（`AGENTS.md` §6.1）：渲染状态里给的是
 * `image`（图像像素尺寸）/ `holeCss`（洞口 CSS 矩形）/ `zoom`（图像像素 → **物理**像素）/
 * `panX`·`panY`（物理像素）/ `dpr`。这里只做单位换算，再喂给
 * `components/ui/viewer/store.ts` 的 `visibleRect()` —— **与浏览右栏同一个变换**，
 * 前端不另推一套（推两遍一定会错开，`ViewerReadout` 的注释已经写明这条）。
 *
 * 两条边界：
 *
 * * `zoom` 的基准是**物理像素**（1.0 = 1:1），洞口是 CSS 像素 —— 所以送进
 *   `visibleRect()` 之前要除 DPR（pan 同理）；
 * * **旋转草稿（旋转工具开着）时不画框**：那时可见区域在图像空间里是个斜的四边形，
 *   轴对齐的框会撒谎；宁可不画（工具确认后 `rotation` 归零，框自然回来）。
 */

import { EMPTY_VIEWER, visibleRect } from "../../components/ui/viewer/store.ts";
import type { EditorRenderState } from "../../api/types.ts";

export interface OverviewRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export function overviewVisibleRect(state: EditorRenderState | null): OverviewRect | null {
  if (state === null) return null;
  const image = state.image;
  const hole = state.holeCss;
  if (image === null || hole === null) return null;
  if (!Number.isFinite(state.zoom) || state.zoom <= 0) return null;
  if (!Number.isFinite(state.dpr) || state.dpr <= 0) return null;
  if (state.rotation !== 0) return null;
  return visibleRect({
    ...EMPTY_VIEWER,
    natural: { width: image.width, height: image.height },
    viewport: { width: hole.width, height: hole.height },
    zoom: state.zoom / state.dpr,
    pan: { x: state.panX / state.dpr, y: state.panY / state.dpr },
  });
}
