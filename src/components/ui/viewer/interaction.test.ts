import assert from "node:assert/strict";
import test from "node:test";

import {
  createWheelZoom,
  isViewerControlTarget,
  viewerControlsVisible,
  wheelZoomFactor,
} from "./interaction.ts";

test("isViewerControlTarget：按钮内的图标也算控件，普通画面不算", () => {
  let selector = "";
  const iconInsideButton = {
    closest: (value: string): object | null => {
      selector = value;
      return {};
    },
  } as unknown as EventTarget;
  assert.equal(isViewerControlTarget(iconInsideButton), true);
  assert.match(selector, /button/);

  const canvas = {
    closest: (): null => null,
  } as unknown as EventTarget;
  assert.equal(isViewerControlTarget(canvas), false);
  assert.equal(isViewerControlTarget(null), false);
});

test("viewerControlsVisible：左上与右下触发，中央和离场不触发", () => {
  const viewport = { width: 1200, height: 800 };
  assert.equal(viewerControlsVisible({ x: 80, y: 80 }, viewport), true);
  assert.equal(viewerControlsVisible({ x: 900, y: 750 }, viewport), true);
  assert.equal(viewerControlsVisible({ x: 500, y: 400 }, viewport), false);
  assert.equal(viewerControlsVisible(null, viewport), false);
});

/*
 * 滚轮缩放：单张看图与对比共用一份实现（`createWheelZoom`），所以这两条性质
 * （**一帧只发一次**、**停止那一次也发**）两边都靠它 —— 钉在这里。
 */

function fakeWheelEvent(deltaY: number, target: unknown = null): WheelEvent {
  let prevented = false;
  return {
    deltaY,
    target,
    preventDefault: (): void => {
      prevented = true;
    },
    get defaultPrevented(): boolean {
      return prevented;
    },
  } as unknown as WheelEvent;
}

/** 可手动推进的帧调度器（Node 里没有 requestAnimationFrame） */
function manualFrames(): {
  schedule: (callback: () => void) => number;
  cancel: (handle: number) => void;
  runFrame: () => void;
  pending: () => number;
} {
  const queue = new Map<number, () => void>();
  let next = 1;
  return {
    schedule: (callback) => {
      const handle = next;
      next += 1;
      queue.set(handle, callback);
      return handle;
    },
    cancel: (handle) => {
      queue.delete(handle);
    },
    runFrame: () => {
      const callbacks = [...queue.values()];
      queue.clear();
      for (const callback of callbacks) callback();
    },
    pending: () => queue.size,
  };
}

test("wheelZoomFactor：上滚放大、下滚缩小，对称且不会出 0/负数", () => {
  assert.ok(wheelZoomFactor(-100) > 1);
  assert.ok(wheelZoomFactor(100) < 1);
  assert.ok(Math.abs(wheelZoomFactor(-120) * wheelZoomFactor(120) - 1) < 1e-12);
  assert.ok(wheelZoomFactor(100_000) > 0);
  assert.ok(wheelZoomFactor(-100_000) < Number.POSITIVE_INFINITY);
});

test("createWheelZoom：一帧内多次滚动合并成一次，且拦住浏览器默认行为", () => {
  const frames = manualFrames();
  const calls: { factor: number; anchor: unknown }[] = [];
  const wheel = createWheelZoom({
    resolveAnchor: () => ({ x: 10, y: 20 }),
    apply: (factor, anchor) => calls.push({ factor, anchor }),
    schedule: frames.schedule,
    cancel: frames.cancel,
  });

  const first = fakeWheelEvent(-100);
  wheel.onWheel(first);
  wheel.onWheel(fakeWheelEvent(-100));
  wheel.onWheel(fakeWheelEvent(-100));

  assert.equal(first.defaultPrevented, true, "滚轮不该让页面滚动/缩放");
  assert.equal(calls.length, 0, "还没到帧就不该发");
  assert.equal(frames.pending(), 1, "三次事件只挂一个帧回调");

  frames.runFrame();
  assert.equal(calls.length, 1, "一帧只发一次");
  assert.deepEqual(calls[0]!.anchor, { x: 10, y: 20 });
  // 合并后的倍数 = 三次 -100 的乘积（指数映射求和）
  assert.ok(Math.abs(calls[0]!.factor - Math.exp(0.45)) < 1e-9);

  // 下一帧继续能发（不是发一次就哑了）
  wheel.onWheel(fakeWheelEvent(100));
  frames.runFrame();
  assert.equal(calls.length, 2);
  wheel.dispose();
});

test("createWheelZoom：拿不到锚点就不发送锚点（由调用方回退到视口中心）", () => {
  const frames = manualFrames();
  const calls: { factor: number; anchor: unknown }[] = [];
  const wheel = createWheelZoom({
    resolveAnchor: () => null,
    apply: (factor, anchor) => calls.push({ factor, anchor }),
    schedule: frames.schedule,
    cancel: frames.cancel,
  });
  wheel.onWheel(fakeWheelEvent(-50));
  frames.runFrame();
  assert.deepEqual(calls, [{ factor: wheelZoomFactor(-50), anchor: null }]);
  wheel.dispose();
});

test("createWheelZoom：dispose 会取消在途的那一帧", () => {
  const frames = manualFrames();
  let fired = 0;
  const wheel = createWheelZoom({
    apply: () => {
      fired += 1;
    },
    schedule: frames.schedule,
    cancel: frames.cancel,
  });
  wheel.onWheel(fakeWheelEvent(-100));
  wheel.dispose();
  frames.runFrame();
  assert.equal(fired, 0, "卸载后不该再回调");
});
