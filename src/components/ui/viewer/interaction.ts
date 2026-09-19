/**
 * 看图 / 对比共用的指针判据。
 *
 * 这两条若在两个视图里各写一份，很容易只修好其中一个：
 * - 控件命中时不能启动画面拖动，否则 pointer capture 会抢走 click；
 * - 左上返回与右下缩放只在光标靠近对应角落时浮出。
 */

const CONTROL_SELECTOR = "button, a, input, [role='button']";

export interface ViewerCursor {
  x: number;
  y: number;
}

export interface ViewerViewport {
  width: number;
  height: number;
}

/** 控件触发区：左上返回 120×120；右下覆盖整条缩放条及其周边。 */
export const VIEWER_BACK_CORNER = 120;
export const VIEWER_ZOOM_CORNER_X = 420;
export const VIEWER_ZOOM_CORNER_Y = 96;

/**
 * 事件是否落在交互控件内。
 *
 * 用鸭子类型而不是 `instanceof Element`：Node 单测没有 DOM 全局，且 SVG 图标的事件目标
 * 仍然会提供 `closest()`，可以与浏览器保持同一条判据。
 */
export function isViewerControlTarget(target: EventTarget | null): boolean {
  const candidate = target as unknown as
    | { closest?: (selector: string) => unknown }
    | null;
  return (
    typeof candidate?.closest === "function" &&
    candidate.closest(CONTROL_SELECTOR) !== null
  );
}

/** 光标是否足够靠近左上返回或右下缩放控件。 */
export function viewerControlsVisible(
  cursor: ViewerCursor | null,
  viewport: ViewerViewport,
): boolean {
  if (cursor === null) return false;
  const nearBack =
    cursor.x >= 0 &&
    cursor.y >= 0 &&
    cursor.x <= VIEWER_BACK_CORNER &&
    cursor.y <= VIEWER_BACK_CORNER;
  const nearZoom =
    cursor.x <= viewport.width &&
    cursor.y <= viewport.height &&
    viewport.width - cursor.x <= VIEWER_ZOOM_CORNER_X &&
    viewport.height - cursor.y <= VIEWER_ZOOM_CORNER_Y;
  return nearBack || nearZoom;
}
