/**
 * 编辑调节响应率的偏好测试（设备级 `localStorage` 适配层）。
 *
 * 关注三件事：非法值回落、两档间隔的数字契约、存储不可用时的静默降级。
 */

import assert from "node:assert/strict";
import test from "node:test";

import {
  createEditorResponseStore,
  DEFAULT_EDITOR_RESPONSE_RATE,
  EDITOR_RESPONSE_INTERVAL_MS,
  EDITOR_RESPONSE_STORAGE_KEY,
  normalizeEditorResponseRate,
  readEditorResponseRate,
  writeEditorResponseRate,
  type EditorResponseStorage,
} from "./editor-response.ts";

function memoryStorage(initial?: string) {
  const values = new Map<string, string>();
  if (initial !== undefined) values.set(EDITOR_RESPONSE_STORAGE_KEY, initial);
  const storage: EditorResponseStorage = {
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => {
      values.set(key, value);
    },
  };
  return { storage, values };
}

test("默认档是高（0.1s），两档间隔就是定好的那两个数", () => {
  assert.equal(DEFAULT_EDITOR_RESPONSE_RATE, "high");
  assert.equal(EDITOR_RESPONSE_INTERVAL_MS.high, 100);
  assert.equal(EDITOR_RESPONSE_INTERVAL_MS.low, 500);
});

test("非法 / 缺失值一律回落到默认档（存储里的垃圾不能让编辑器起不来）", () => {
  assert.equal(normalizeEditorResponseRate(null), "high");
  assert.equal(normalizeEditorResponseRate("turbo"), "high");
  assert.equal(normalizeEditorResponseRate(0.1), "high");
  assert.equal(normalizeEditorResponseRate("low"), "low");
  assert.equal(readEditorResponseRate(memoryStorage("high").storage), "high");
  assert.equal(readEditorResponseRate(undefined), "high", "没有存储也要能读");
});

test("设置档位会写回存储，并能被下一次读取", () => {
  const { storage, values } = memoryStorage();
  const store = createEditorResponseStore(storage);
  assert.equal(store.rate(), "high");
  assert.equal(store.intervalMs(), 100);

  store.setRate("low");
  assert.equal(store.rate(), "low");
  assert.equal(store.intervalMs(), 500);
  assert.equal(values.get(EDITOR_RESPONSE_STORAGE_KEY), "low");
  assert.equal(readEditorResponseRate(storage), "low");
});

test("存储抛错不影响本次会话（读写失败都静默降级）", () => {
  const hostile: EditorResponseStorage = {
    getItem: () => {
      throw new Error("存储被禁用了");
    },
    setItem: () => {
      throw new Error("存储被禁用了");
    },
  };
  assert.equal(readEditorResponseRate(hostile), "high");
  writeEditorResponseRate("low", hostile); // 不抛即为通过

  const store = createEditorResponseStore(hostile);
  store.setRate("low");
  assert.equal(store.rate(), "low", "存不下也要在本次会话里生效");
});
