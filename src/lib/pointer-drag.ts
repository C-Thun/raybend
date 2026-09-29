/** Shared pointer tracking for resize handles and internal drag gestures. */
export interface DragPoint { x: number; y: number }
export interface PointerDragEnv {
  events: EventTarget;
  requestFrame(fn: FrameRequestCallback): number;
  cancelFrame(id: number): void;
  /** 视口尺寸；只有 `cancelOutsideWindow` 用得到。测试注入，浏览器走真实窗口。 */
  viewport?(): { width: number; height: number };
}
export interface PointerDragOptions {
  threshold?: number;
  capture?: boolean;
  /**
   * 指针越出窗口即中止（拖动中把鼠标拖出窗口：不要留一个卡在窗口边缘的幽灵，
   * 也不要把「松手落在窗口外」当成一次投放）。
   *
   * 判据是**指针坐标**而不是 `pointerout`：按住键拖出窗口时浏览器会隐式捕获指针，
   * 越界之后坐标照常上报，而「离开文档」的事件在松开之前不会来。
   */
  cancelOutsideWindow?: boolean;
  start?(point: DragPoint): void;
  move(point: DragPoint): void;
  end(point: DragPoint, cancelled: boolean, started: boolean): void;
}
export function trackPointerDrag(event: PointerEvent, options: PointerDragOptions,
  env: PointerDragEnv = { events: window as EventTarget, requestFrame: (fn: FrameRequestCallback) => requestAnimationFrame(fn), cancelFrame: (id: number) => cancelAnimationFrame(id) }) : () => void {
  const host = event.currentTarget as HTMLElement;
  const origin = { x: event.clientX, y: event.clientY };
  let latest = origin, frame = 0, started = false, ended = false;
  const outsideWindow = (point: DragPoint): boolean => {
    const view = env.viewport?.() ?? { width: window.innerWidth, height: window.innerHeight };
    return point.x < 0 || point.y < 0 || point.x > view.width || point.y > view.height;
  };
  const start = () => {
    started = true;
    if (options.capture) { try { host.setPointerCapture(event.pointerId); } catch { /* synthetic pointer */ } }
    options.start?.(latest);
  };
  const commit = () => { frame = 0; if (!ended && started) options.move(latest); };
  const finish = (cancelled: boolean) => {
    if (ended) return;
    if (frame) { env.cancelFrame(frame); frame = 0; }
    if (started && !cancelled) options.move(latest);
    ended = true;
    env.events.removeEventListener('pointermove', move);
    env.events.removeEventListener('pointerup', up);
    env.events.removeEventListener('pointercancel', cancel);
    env.events.removeEventListener('blur', blur);
    if (options.capture) { try { host.releasePointerCapture(event.pointerId); } catch { /* no capture */ } }
    options.end(latest, cancelled, started);
  };
  const move = (value: Event) => {
    const e = value as PointerEvent;
    if (e.pointerId !== event.pointerId) return;
    latest = { x: e.clientX, y: e.clientY };
    if (options.cancelOutsideWindow && outsideWindow(latest)) { finish(true); return; }
    if (!started && Math.hypot(latest.x-origin.x, latest.y-origin.y) >= (options.threshold ?? 0)) start();
    if (started && !frame) frame = env.requestFrame(commit);
  };
  const up = (value: Event) => {
    const e = value as PointerEvent;
    if (e.pointerId !== event.pointerId) return;
    latest = { x: e.clientX, y: e.clientY };
    // 窗口外松手不算投放（可能没有中间的 move 事件，越界只能在这里补判）
    finish(options.cancelOutsideWindow === true && outsideWindow(latest));
  };
  const cancel = (value: Event) => { if ((value as PointerEvent).pointerId === event.pointerId) finish(true); };
  const blur = () => finish(true);
  env.events.addEventListener('pointermove', move);
  env.events.addEventListener('pointerup', up);
  env.events.addEventListener('pointercancel', cancel);
  env.events.addEventListener('blur', blur);
  if ((options.threshold ?? 0) === 0) start();
  return () => finish(true);
}
