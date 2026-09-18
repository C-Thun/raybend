import assert from "node:assert/strict";
import { test } from "node:test";
import { latestSample } from "./latest-sample.ts";

function fixture() {
  const callbacks = new Set<() => void>();
  const sent: number[] = [];
  const sampler = latestSample<number>((x) => sent.push(x), (cb) => {
    callbacks.add(cb);
    return () => { callbacks.delete(cb); };
  });
  return { sampler, sent, callbacks, tick() { for (const cb of [...callbacks]) cb(); } };
}

test("停手后仍发送最后一个样本，不把光标永久留在限流窗口之前", () => {
  const f = fixture();
  f.sampler.push(10);
  f.tick();
  f.sampler.push(30);
  f.sampler.push(60);
  f.sampler.push(100);
  assert.equal(f.callbacks.size, 1);
  f.tick(); // 后续没有 pointermove，仍必须上报 100
  assert.deepEqual(f.sent, [10, 100]);
  f.tick();
  assert.deepEqual(f.sent, [10, 100]);
});

test("滚轮/松手前可立即刷新，空刷新不重复发送", () => {
  const f = fixture();
  f.sampler.flush();
  f.sampler.push(0);
  f.sampler.flush();
  f.tick();
  assert.deepEqual(f.sent, [0]);
  assert.equal(f.callbacks.size, 0);
});

test("卸载取消尾样本且不再接受事件", () => {
  const f = fixture();
  f.sampler.push(1);
  f.sampler.dispose();
  f.sampler.push(2);
  f.tick();
  f.sampler.flush();
  assert.deepEqual(f.sent, []);
  assert.equal(f.callbacks.size, 0);
});
