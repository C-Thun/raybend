/**
 * `withTimeout` 的单测：三条出路（正常返回 / 正常报错 / 超时）都要钉住，
 * 因为它的用途是**把「卡死」变成「报错」**，错一条就白写。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { TimeoutError, withTimeout } from "./timeout.ts";

const sleep = (ms: number): Promise<void> =>
  new Promise((resolve) => setTimeout(resolve, ms));

test("时限之内正常返回：结果原样透传", async () => {
  const value = await withTimeout(Promise.resolve(42), 1000, "测试动作");
  assert.equal(value, 42);
});

test("时限之内报错：原错误照常抛出（不能变成超时）", async () => {
  const boom = new Error("后端说不行");
  await assert.rejects(
    withTimeout(Promise.reject(boom), 1000, "测试动作"),
    (error: unknown) => error === boom,
  );
});

test("永远不 settle 的任务：到点抛 TimeoutError，消息**原样透传**（由应用层拼好）", async () => {
  const never = new Promise<never>(() => {});
  const message = "启动导入没有在 15 秒内回应（后端可能已经挂了）";
  await assert.rejects(withTimeout(never, 20, message), (error: unknown) => {
    assert.ok(error instanceof TimeoutError);
    assert.equal(error.message, message);
    assert.equal(error.timeoutMs, 20);
    return true;
  });
});

test("超时之后才失败的任务：不会冒出 unhandled rejection", async () => {
  let rejectLater: (error: Error) => void = () => {};
  const late = new Promise<never>((_, reject) => {
    rejectLater = reject;
  });
  await assert.rejects(withTimeout(late, 10, "慢动作"), TimeoutError);
  // 超时之后它才失败 —— 这里不应把测试进程搞崩
  rejectLater(new Error("我迟到了"));
  await sleep(20);
});

test("慢但没超时：等它回来（时限不是「快慢」的判断）", async () => {
  const slow = sleep(30).then(() => "到了");
  assert.equal(await withTimeout(slow, 200, "慢动作"), "到了");
});

test("时限非正数 = 不设限", async () => {
  const slow = sleep(20).then(() => "无限制");
  assert.equal(await withTimeout(slow, 0, "不设限"), "无限制");
});
