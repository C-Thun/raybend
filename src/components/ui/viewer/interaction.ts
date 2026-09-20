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

/** 滚轮累计量 → 缩放倍数（> 1 放大）。指数映射：无论快慢滚，视觉上的缩放速度一致。 */
export const WHEEL_ZOOM_RATE = 0.0015;

/** 滚轮缩放因子（纯函数，好测）。 */
export function wheelZoomFactor(deltaY: number): number {
  return Math.exp(-deltaY * WHEEL_ZOOM_RATE);
}

export interface WheelZoomOptions {
  /** 事件落点 → 缩放锚点（**视口内坐标**）；返回 `null` = 用视口中心 */
  resolveAnchor?: (event: WheelEvent) => ViewerCursor | null;
  /** **一帧一次**地收到倍数与最近一次的锚点（拿不到锚点就是 `null` = 视口中心） */
  apply: (factor: number, anchor: ViewerCursor | null) => void;
  /** 帧调度（单测注入用；默认 `requestAnimationFrame`） */
  schedule?: (callback: () => void) => number;
  /** 取消帧调度（与 `schedule` 配套；默认 `cancelAnimationFrame`） */
  cancel?: (handle: number) => void;
}

/**
 * 滚轮缩放：**按帧合并 + 指数映射**（单张看图与对比共用，行为必须一致）。
 *
 * 为什么要合并：高精度触控板的滚轮事件能到 100+ Hz，每次都改状态 = 每帧重算多次变换；
 * 合并成「一帧一次」手感一样但开销固定。**停止时那一次也要发**（尾样本没丢：
 * 累计量在下一帧照常兑现）。
 */
export function createWheelZoom(options: WheelZoomOptions): {
  onWheel: (event: WheelEvent) => void;
  dispose: () => void;
} {
  const schedule =
    options.schedule ??
    ((callback: () => void): number => requestAnimationFrame(callback));
  const cancel =
    options.cancel ?? ((handle: number): void => cancelAnimationFrame(handle));

  let delta = 0;
  let anchor: ViewerCursor | null = null;
  let frame = 0;

  return {
    onWheel(event: WheelEvent): void {
      event.preventDefault();
      const resolved = options.resolveAnchor?.(event) ?? null;
      // 锚点取**最近一次**能拿到的：一帧内多次滚动时，画面按最后一次落点缩放
      if (resolved !== null) anchor = resolved;
      delta += event.deltaY;
      if (frame !== 0) return;
      frame = schedule(() => {
        frame = 0;
        const total = delta;
        const at = anchor;
        delta = 0;
        if (total === 0) return;
        options.apply(wheelZoomFactor(total), at);
      });
    },
    dispose(): void {
      if (frame !== 0) cancel(frame);
      frame = 0;
      delta = 0;
    },
  };
}

/**
 * 看图面（单张看图 / 对比）**接管键盘焦点**。
 *
 * 为什么必须有这一条（人类 2026-09-20 报的「browse 里回车进得去、退不出来」）：
 * 看图是**覆盖层**，网格不卸载 —— 打开看图时焦点如果还留在底下那张 tile 上，
 * 它会先把自己的 `Enter`（激活 = 重新打开看图）吃掉并 `preventDefault()`，
 * 看图件挂在 `window` 上的「回车退出」永远等不到这个事件。
 * 导入侧当时看着正常，只是因为一次重渲染恰好把 tile 换掉、焦点掉到了 `body` ——
 * 偶然行为不算行为。
 *
 * 看图面一旦拥有焦点，键盘语义就只有一份：
 *   * 单张看图的 `Enter` / `Esc` / `←→` / `+−01` 都是看图件的；
 *   * 对比下的 `Enter` 由命令分发器接（`viewer.compareOnly`）。
 *
 * 关闭后由**网格**把焦点收回（`PhotoGrid` 里的 `focusTiles`）—— 那一份也在网格内部，
 * 两个工作区不需要各接一条线。
 */
export function takeViewerFocus(host: HTMLElement | undefined): void {
  // `preventScroll`：看图面铺满中列，聚焦不该引起任何滚动
  host?.focus({ preventScroll: true });
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
  // SAFETY: 运行时只调 `closest`，所以只断言「可能带 closest 的对象」；
  // 这与 `instanceof Element` 相比更宽（SVG 图标、测试里的假对象都能用），
  // 而 `closest` 的类型检查由下面 `typeof === "function"` 自己兜住。
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
