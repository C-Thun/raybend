/**
 * 预览框几何的单测（`lib/preview-frame.ts`）。
 *
 * 覆盖边界：正方、恰好等于框比例（4:3）、全景、极端纵图、0 与非法值。
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import { PREVIEW_FRAME_ASPECT, fitAxisFor } from "./preview-frame.ts";

test("预览框是 4:3（人类 2026-09-20 定的，别改回 3:2）", () => {
  assert.equal(PREVIEW_FRAME_ASPECT, 4 / 3);
});

test("横图 / 正方 / 纵图各走哪条边", () => {
  // 3:2（主流横构图）比 4:3 宽 ⇒ 铺满宽度
  assert.equal(fitAxisFor(3000, 2000, PREVIEW_FRAME_ASPECT), "width");
  // 1:1 比 4:3 **窄**（更方）⇒ 铺满高度（这一条测试初稿写错过，留着当提醒）
  assert.equal(fitAxisFor(1000, 1000, PREVIEW_FRAME_ASPECT), "height");
  // 恰好 4:3 ⇒ 归 width（两边结果一样）
  assert.equal(fitAxisFor(4000, 3000, PREVIEW_FRAME_ASPECT), "width");
  // 纵图（3:4 与 2:3）比 4:3 窄 ⇒ 铺满高度
  assert.equal(fitAxisFor(3000, 4000, PREVIEW_FRAME_ASPECT), "height");
  assert.equal(fitAxisFor(2000, 3000, PREVIEW_FRAME_ASPECT), "height");
  // 极端比例：全景与细长条
  assert.equal(fitAxisFor(12000, 1000, PREVIEW_FRAME_ASPECT), "width");
  assert.equal(fitAxisFor(500, 10000, PREVIEW_FRAME_ASPECT), "height");
});

test("尺寸未知 / 非法值不制造 NaN，一律退回 width", () => {
  assert.equal(fitAxisFor(0, 0, PREVIEW_FRAME_ASPECT), "width");
  assert.equal(fitAxisFor(3000, 0, PREVIEW_FRAME_ASPECT), "width");
  assert.equal(fitAxisFor(Number.NaN, 2000, PREVIEW_FRAME_ASPECT), "width");
  assert.equal(fitAxisFor(Number.POSITIVE_INFINITY, 2000, PREVIEW_FRAME_ASPECT), "width");
  assert.equal(fitAxisFor(3000, 2000, 0), "width");
  assert.equal(fitAxisFor(3000, 2000, Number.NaN), "width");
  // 负值（后端给了脏数据）
  assert.equal(fitAxisFor(-3000, -2000, PREVIEW_FRAME_ASPECT), "width");
});
