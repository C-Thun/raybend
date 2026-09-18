/**
 * 提示队列的测试（视图不管，只钉状态机）。
 *
 * 三件事最容易出错，都在这里钉死：
 * 1. **自动消失**：定时器要真被排上、被取消时不要重复 dismiss；
 * 2. **上限**：同时最多 3 条，新的顶掉最老的（提示不是日志）；
 * 3. **清空**（`clear`）要把挂着的定时器一起收掉 —— 否则组件卸载后定时器还会去改状态。
 */

import assert from "node:assert/strict";
import test from "node:test";
import { createToastStore, TOAST_DURATION_MS, TOAST_MAX } from "./toast.ts";

/** 假定时器：手动决定什么时候到点（不真等 5 秒） */
function fakeClock() {
  const pending = new Map<number, () => void>();
  let next = 1;
  return {
    schedule: (fn: () => void, ms: number) => {
      const handle = next;
      next += 1;
      pending.set(handle, fn);
      void ms;
      return handle;
    },
    cancel: (handle: unknown) => {
      pending.delete(handle as number);
    },
    /** 让所有挂着的定时器到点 */
    fireAll: () => {
      const fns = [...pending.values()];
      pending.clear();
      for (const fn of fns) fn();
    },
    size: () => pending.size,
  };
}

test("show：新的在最上面，并带上自增 id", () => {
  const store = createToastStore({ schedule: () => 0, cancel: () => {} });
  const first = store.show({ tone: "info", message: "一" });
  const second = store.show({ tone: "success", message: "二" });
  assert.deepEqual(
    store.items().map((item) => item.message),
    ["二", "一"],
  );
  assert.notEqual(first, second);
});

test("dismiss：按 id 摘掉一条，别的留着", () => {
  const store = createToastStore({ schedule: () => 0, cancel: () => {} });
  const a = store.show({ tone: "info", message: "一" });
  store.show({ tone: "info", message: "二" });
  store.dismiss(a);
  assert.deepEqual(
    store.items().map((item) => item.message),
    ["二"],
  );
});

test("上限：同时最多 3 条，第 4 条顶掉最老的", () => {
  assert.equal(TOAST_MAX, 3);
  const store = createToastStore({ schedule: () => 0, cancel: () => {} });
  for (const text of ["一", "二", "三", "四"]) {
    store.show({ tone: "info", message: text });
  }
  assert.deepEqual(
    store.items().map((item) => item.message),
    ["四", "三", "二"],
  );
});

test("自动消失：到点自己走；默认停留 5 秒", () => {
  assert.equal(TOAST_DURATION_MS, 5000);
  const clock = fakeClock();
  const store = createToastStore(clock);
  store.show({ tone: "success", message: "好了" });
  assert.equal(store.items().length, 1);
  clock.fireAll();
  assert.equal(store.items().length, 0, "到点之后应当自己消失");
});

test("自定义时长传下去（想多留一会儿的提示）", () => {
  const seen: number[] = [];
  const store = createToastStore({
    schedule: (_fn, ms) => {
      seen.push(ms);
      return 0;
    },
    cancel: () => {},
  });
  store.show({ tone: "info", message: "短", duration: 1000 });
  store.show({ tone: "info", message: "默认" });
  assert.deepEqual(seen, [1000, TOAST_DURATION_MS]);
});

test("手动 dismiss 之后定时器到点不会把它再摘一次（不抛错）", () => {
  const clock = fakeClock();
  const store = createToastStore(clock);
  const id = store.show({ tone: "info", message: "一" });
  store.dismiss(id);
  assert.equal(clock.size(), 0, "dismiss 要把定时器也收掉");
  clock.fireAll(); // 空转，不该炸
  assert.equal(store.items().length, 0);
});

test("clear：全清 + 把挂着的定时器都收掉", () => {
  const clock = fakeClock();
  const store = createToastStore(clock);
  store.show({ tone: "info", message: "一" });
  store.show({ tone: "info", message: "二" });
  assert.equal(clock.size(), 2);
  store.clear();
  assert.equal(store.items().length, 0);
  assert.equal(clock.size(), 0, "clear 之后不该还有定时器在跑");
});

test("上限为 0 或负数时至少留 1 条（不把提示变成黑洞）", () => {
  const store = createToastStore({ schedule: () => 0, cancel: () => {}, max: 0 });
  store.show({ tone: "info", message: "一" });
  assert.equal(store.items().length, 1);
});
