import { test } from "node:test";
import assert from "node:assert/strict";
import {
  PREVIEW_FRAME_FALLBACK_ASPECT,
  PREVIEW_FRAME_MAX_ASPECT,
  PREVIEW_FRAME_MIN_ASPECT,
  fitPreviewSize,
  previewFrameAspect,
} from "./preview-frame.ts";

test("总览外框随照片比例变化，限定在 3:1 到 3:4", () => {
  assert.equal(PREVIEW_FRAME_MAX_ASPECT, 3);
  assert.equal(PREVIEW_FRAME_MIN_ASPECT, 3 / 4);
  assert.equal(previewFrameAspect(3000, 1000), 3);
  assert.equal(previewFrameAspect(4000, 1000), 3);
  assert.equal(previewFrameAspect(3000, 2000), 1.5);
  assert.equal(previewFrameAspect(1000, 1000), 1);
  assert.equal(previewFrameAspect(3000, 4000), 3 / 4);
  assert.equal(previewFrameAspect(1000, 4000), 3 / 4);
});

test("未知和非法尺寸采用稳定占位比例", () => {
  for (const pair of [[undefined, undefined], [0, 400], [-1, 400], [100, 0], [Infinity, 10], [NaN, 10]] as const) {
    assert.equal(previewFrameAspect(pair[0], pair[1]), PREVIEW_FRAME_FALLBACK_ASPECT);
  }
});

test("全图在实测内容盒内等比缩放，极宽极长都完整显示", () => {
  assert.deepEqual(fitPreviewSize(4000, 1000, 240, 80), { width: 240, height: 60 });
  assert.deepEqual(fitPreviewSize(1000, 4000, 240, 320), { width: 80, height: 320 });
  assert.deepEqual(fitPreviewSize(3000, 2000, 240, 160), { width: 240, height: 160 });
  assert.deepEqual(fitPreviewSize(100, 100, 240, 160), { width: 160, height: 160 });
  assert.equal(fitPreviewSize(0, 100, 240, 160), null);
  assert.equal(fitPreviewSize(100, 100, NaN, 160), null);
});
