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

test("同路径同内容身份：合并到在飞任务，只执行一次并报告重复", async () => {
  const runs: string[] = [];
  const duplicates: Array<{ path: string; identity: string; where: string }> = [];
  const first = deferred();
  const queue = createPreviewQueue({
    run: (path) => { runs.push(path); return first.promise; },
    onDuplicate: (event) => duplicates.push(event),
  });
  const a = queue.push("a", undefined, "rev:7");
  const same = queue.push("a", undefined, "rev:7");
  await tick();
  assert.deepEqual(runs, ["a"], "重复的同内容请求不另起渲染");
  first.resolve();
  await Promise.all([a, same]);
  assert.deepEqual(runs, ["a"]);
  assert.deepEqual(duplicates, [{ path: "a", identity: "rev:7", where: "active" }]);
});

test("同路径不同内容身份：只保留最新尾随；同身份待跑项也合并", async () => {
  const runs: string[] = [];
  const duplicates: string[] = [];
  const first = deferred();
  const queue = createPreviewQueue({
    run: (path) => {
      runs.push(path);
      return runs.length === 1 ? first.promise : Promise.resolve();
    },
    onDuplicate: ({ identity }) => duplicates.push(identity),
  });
  const a = queue.push("a", undefined, "rev:1");
  const old = queue.push("a", undefined, "rev:2");
  const oldDuplicate = queue.push("a", undefined, "rev:2");
  const latest = queue.push("a", undefined, "rev:3");
  await tick();
  assert.deepEqual(runs, ["a"]);
  first.resolve();
  await Promise.all([a, old, oldDuplicate, latest]);
  assert.equal(runs.length, 2, "旧 rev 不逐个补跑，只跑在飞的 rev:1 与最新 rev:3");
  assert.deepEqual(duplicates, ["rev:2"]);
});

test("不传内容身份时保持旧行为：同路径也允许尾随新内容", async () => {
  const runs: string[] = [];
  const first = deferred();
  const queue = createPreviewQueue({
    run: (path) => { runs.push(path); return runs.length === 1 ? first.promise : Promise.resolve(); },
  });
  const a = queue.push("a");
  const b = queue.push("a");
  await tick();
  first.resolve();
  await Promise.all([a, b]);
  assert.equal(runs.length, 2, "没有身份就不能猜它是重复请求");
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
