/**
 * 预览队列的纯逻辑测试（`lib/preview-queue.ts`）。
 *
 * 盯四件事：单飞、最新者优先、落库闸门（失败不渲染）、失败不卡队。
 */

import assert from "node:assert/strict";
import test from "node:test";

import { createPreviewQueue } from "./preview-queue.ts";

function deferred<T = void>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (error: unknown) => void;
} {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/** 让微任务与已排的 promise 回调跑完 */
const tick = (): Promise<void> => new Promise((resolve) => setImmediate(resolve));

test("单飞 + 最新者优先：在飞时排队只留最后一次", async () => {
  const runs: string[] = [];
  const first = deferred();
  const queue = createPreviewQueue({
    run: (path) => {
      runs.push(path);
      return path === "a" ? first.promise : Promise.resolve();
    },
  });
  const a = queue.push("a");
  const b = queue.push("b");
  const c = queue.push("c");
  await tick();
  assert.deepEqual(runs, ["a"], "同一时刻只跑一张");
  first.resolve();
  await Promise.all([a, b, c]);
  assert.deepEqual(runs, ["a", "c"], "中间的 b 被 c 取代，不再单独渲染");
});

test("落库闸门：先落库再渲染；落库失败就不渲染", async () => {
  const runs: string[] = [];
  const queue = createPreviewQueue({
    run: (path) => {
      runs.push(path);
      return Promise.resolve();
    },
  });
  const gate = deferred();
  const first = queue.push("a", gate.promise);
  await tick();
  assert.deepEqual(runs, [], "闸门没开不渲染");
  gate.resolve();
  await first;
  assert.deepEqual(runs, ["a"]);

  const failed = deferred();
  const second = queue.push("b", failed.promise);
  await tick();
  failed.reject(new Error("commit failed"));
  await second;
  assert.deepEqual(runs, ["a"], "落库失败 → 不拿旧栈渲染一张过期预览");
});

test("渲染失败只报错，队列继续跑后面的", async () => {
  const runs: string[] = [];
  const errors: unknown[] = [];
  const queue = createPreviewQueue({
    run: (path) => {
      runs.push(path);
      return path === "a" ? Promise.reject(new Error("boom")) : Promise.resolve();
    },
    onError: (error) => errors.push(error),
  });
  await queue.push("a");
  await queue.push("b");
  assert.deepEqual(runs, ["a", "b"]);
  assert.equal(errors.length, 1);
});

test("dispose 放行等待者并停止后续渲染", async () => {
  const runs: string[] = [];
  const gate = deferred();
  const queue = createPreviewQueue({
    run: (path) => {
      runs.push(path);
      return Promise.resolve();
    },
  });
  const wait = queue.push("a", gate.promise);
  queue.dispose();
  await wait;
  await tick();
  assert.deepEqual(runs, [], "在飞那一次被丢弃，没有新渲染");
});
