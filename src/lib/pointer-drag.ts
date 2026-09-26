/** Shared pointer tracking for resize handles and internal drag gestures. */
export interface DragPoint { x: number; y: number }
export interface PointerDragOptions {
  threshold?: number;
  capture?: boolean;
  start?(point: DragPoint): void;
  move(point: DragPoint): void;
  end(point: DragPoint, cancelled: boolean, started: boolean): void;
}
export function trackPointerDrag(event: PointerEvent, options: PointerDragOptions,
  env = { events: window as EventTarget, requestFrame: (fn: FrameRequestCallback) => requestAnimationFrame(fn), cancelFrame: (id: number) => cancelAnimationFrame(id) }) : () => void {
  const host = event.currentTarget as HTMLElement;
  const origin = { x: event.clientX, y: event.clientY };
  let latest = origin, frame = 0, started = false, ended = false;
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
    if (!started && Math.hypot(latest.x-origin.x, latest.y-origin.y) >= (options.threshold ?? 0)) start();
    if (started && !frame) frame = env.requestFrame(commit);
  };
  const up = (value: Event) => {
    const e = value as PointerEvent;
    if (e.pointerId !== event.pointerId) return;
    latest = { x: e.clientX, y: e.clientY }; finish(false);
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
