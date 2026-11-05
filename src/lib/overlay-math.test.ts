import assert from "node:assert/strict";
import test from "node:test";

import { imageDeltaFromCss, normalizeFromCss } from "./overlay-math.ts";

test("逆变换：缩放 + 平移的矩阵能原样换算回来", () => {
  // zoom/dpr = 2，平移 (30, 10)，无旋转
  const matrix = [2, 0, 0, 2, 30, 10] as const;
  const point = normalizeFromCss(matrix, 400, 200, { x: 230, y: 110 });
  assert.ok(point !== null);
  assert.ok(Math.abs(point.x - 0.25) < 1e-9, `x = ${point?.x}`);
  assert.ok(Math.abs(point.y - 0.25) < 1e-9, `y = ${point?.y}`);
});

test("逆变换：带旋转也对得上（把图像像素正向算一遍再反算）", () => {
  const scale = 1.5;
  const radians = (30 * Math.PI) / 180;
  const cos = Math.cos(radians);
  const sin = Math.sin(radians);
  const matrix = [scale * cos, scale * sin, -scale * sin, scale * cos, 12, -7] as const;
  const px = 123.4;
  const py = 56.7;
  const css = {
    x: matrix[0] * px + matrix[2] * py + matrix[4],
    y: matrix[1] * px + matrix[3] * py + matrix[5],
  };
  const back = normalizeFromCss(matrix, 1000, 500, css);
  assert.ok(back !== null);
  assert.ok(Math.abs(back.x - px / 1000) < 1e-9);
  assert.ok(Math.abs(back.y - py / 500) < 1e-9);
});

test("退化的矩阵 / 尺寸返回 null（不猜）", () => {
  assert.equal(normalizeFromCss([0, 0, 0, 0, 0, 0], 100, 100, { x: 1, y: 1 }), null);
  assert.equal(normalizeFromCss([1, 0, 0, 1, 0, 0], 0, 100, { x: 1, y: 1 }), null);
  assert.equal(normalizeFromCss([Number.NaN, 0, 0, 1, 0, 0], 10, 10, { x: 1, y: 1 }), null);
});

test("拖动位移：屏幕像素 / 比例 = 图像像素，旋转时先反旋", () => {
  assert.deepEqual(imageDeltaFromCss(2, 0, 10, -6), { x: 5, y: -3 });
  const rotated = imageDeltaFromCss(1, 90, 10, 0);
  assert.ok(Math.abs(rotated.x) < 1e-9);
  assert.ok(Math.abs(rotated.y - -10) < 1e-9, `y = ${rotated.y}`);
  assert.deepEqual(imageDeltaFromCss(0, 0, 10, 10), { x: 0, y: 0 }, "比例退化不给位移");
});
