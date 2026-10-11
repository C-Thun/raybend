/**
 * 「100%」的口径：**1 图像像素 = 1 设备像素**（与编辑器原生视口同一坐标契约）。
 *
 * 这条错过的症状（崔总 2026-10-11 真机）：DPR=2 的屏幕上双击 100% 显示成 200%，
 * 而且全尺寸图被浏览器放大 2 倍 ⇒ 100% 看着发虚。
 */

import assert from "node:assert/strict";
import test from "node:test";
import { devicePixelRatioNow, isOneToOne, oneToOneZoom, zoomPercent } from "./zoom.ts";

test("100% 在 DPR=1 就是 CSS 1:1", () => {
  assert.equal(oneToOneZoom(1), 1);
  assert.equal(zoomPercent(1, 1), 100);
  assert.ok(isOneToOne(1, 1));
});

test("100% 在 DPR=2 是 CSS 0.5（1 图像像素 = 1 设备像素）", () => {
  assert.equal(oneToOneZoom(2), 0.5);
  assert.equal(zoomPercent(0.5, 2), 100, "读数仍然是 100%");
  assert.ok(isOneToOne(0.5, 2));
  assert.ok(!isOneToOne(1, 2), "CSS 1:1 在 2x 屏上是 200%，不是 100%");
});

test("DPR 取运行时值，非法值当 1", () => {
  assert.equal(oneToOneZoom(0), 1);
  assert.equal(oneToOneZoom(Number.NaN), 1);
  // 裸 node（没有 DOM）：globalThis.devicePixelRatio 读不到 → 1，不抛
  assert.ok(Number.isFinite(devicePixelRatioNow()));
});
