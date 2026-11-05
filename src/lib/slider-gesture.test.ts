import assert from "node:assert/strict";
import test from "node:test";

import { SLIDER_KEYS, createGestureEnd, type GesturePointerEnv } from "./slider-gesture.ts";

type Listener = { type: string; listener: (event: Event) => void; capture: boolean };

function fakeEnv(): GesturePointerEnv & { listeners: Listener[]; fire(type: string, pointerId: number): void } {
  const listeners: Listener[] = [];
  return {
    listeners,
    addEventListener(type, listener, capture = false) {
      listeners.push({ type, listener, capture });
    },
    removeEventListener(type, listener) {
      const index = listeners.findIndex((entry) => entry.type === type && entry.listener === listener);
      if (index >= 0) listeners.splice(index, 1);
    },
    fire(type, pointerId) {
      for (const entry of [...listeners]) {
        if (entry.type === type) entry.listener({ pointerId } as PointerEvent);
      }
    },
  };
}

test("按下后窗口内松手：恰好收尾一次，且监听被摘掉", () => {
  const env = fakeEnv();
  let done = 0;
  const gesture = createGestureEnd(() => { done += 1; }, env);
  gesture.start({ pointerId: 1 });
  assert.equal(env.listeners.length, 2, "pointerup / pointercancel 各一条（捕获阶段）");
  env.fire("pointerup", 1);
  assert.equal(done, 1);
  assert.equal(env.listeners.length, 0, "收尾后不留监听");
  env.fire("pointerup", 1);
  assert.equal(done, 1, "同一次手势不会重复提交");
});

test("别的指针 / pointercancel 的语义", () => {
  const env = fakeEnv();
  let done = 0;
  const gesture = createGestureEnd(() => { done += 1; }, env);
  gesture.start({ pointerId: 7 });
  env.fire("pointerup", 8);
  assert.equal(done, 0, "别的指针不算这次手势结束");
  env.fire("pointercancel", 7);
  assert.equal(done, 1, "取消也是一次收尾（值已经变了，开关规则照样要落定）");
});

test("手势进行中再按下：不会挂第二份监听（也不会提交两次）", () => {
  const env = fakeEnv();
  let done = 0;
  const gesture = createGestureEnd(() => { done += 1; }, env);
  gesture.start({ pointerId: 1 });
  gesture.start({ pointerId: 1 });
  assert.equal(env.listeners.length, 2);
  env.fire("pointerup", 1);
  assert.equal(done, 1);
});

test("dispose 摘掉监听：卸载后松手不再提交", () => {
  const env = fakeEnv();
  let done = 0;
  const gesture = createGestureEnd(() => { done += 1; }, env);
  gesture.start({ pointerId: 3 });
  gesture.dispose();
  assert.equal(env.listeners.length, 0);
  env.fire("pointerup", 3);
  assert.equal(done, 0);
});

test("SLIDER_KEYS 覆盖方向键与 Home/End/Page", () => {
  for (const key of ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End", "PageUp", "PageDown"]) {
    assert.equal(SLIDER_KEYS.has(key), true, key);
  }
  assert.equal(SLIDER_KEYS.has("Enter"), false);
  assert.equal(SLIDER_KEYS.has("Tab"), false);
});
