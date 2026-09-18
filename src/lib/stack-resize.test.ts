import { test } from "node:test";
import assert from "node:assert/strict";

import { minRatiosFrom, moveBoundary, parseSize, toPercents } from "./stack-resize.ts";

const close = (actual: number, expected: number, tolerance = 1e-9): void => {
  assert.ok(
    Math.abs(actual - expected) <= tolerance,
    `期望 ${expected}，实际 ${actual}`,
  );
};

test("moveBoundary：正常拖动，两段总和守恒", () => {
  const next = moveBoundary({ sizes: [0.3, 0.7], index: 0, delta: 0.1 });
  close(next[0] as number, 0.4);
  close(next[1] as number, 0.6);
  close((next[0] as number) + (next[1] as number), 1);
});

test("moveBoundary：被前一段的下限挡住", () => {
  const next = moveBoundary({
    sizes: [0.2, 0.8],
    index: 0,
    delta: -0.5,
    minRatios: [0.15, 0],
  });
  close(next[0] as number, 0.15);
  close(next[1] as number, 0.85);
});

test("moveBoundary：被后一段的下限挡住", () => {
  const next = moveBoundary({
    sizes: [0.2, 0.8],
    index: 0,
    delta: 0.9,
    minRatios: [0, 0.3],
  });
  close(next[0] as number, 0.7);
  close(next[1] as number, 0.3);
});

test("moveBoundary：下限之和塞不下时不动（容器太矮）", () => {
  const sizes = [0.1, 0.1, 0.8];
  const next = moveBoundary({
    sizes,
    index: 0,
    delta: 0.05,
    minRatios: [0.2, 0.3, 0],
  });
  assert.deepEqual(next, sizes);
});

test("moveBoundary：边界下标越界时原样返回（不抛）", () => {
  const sizes = [0.5, 0.5];
  assert.deepEqual(moveBoundary({ sizes, index: -1, delta: 0.1 }), sizes);
  assert.deepEqual(moveBoundary({ sizes, index: 1, delta: 0.1 }), sizes);
  assert.deepEqual(moveBoundary({ sizes: [], index: 0, delta: 0.1 }), []);
});

test("moveBoundary：只动相邻两段，别的段一个字节都不许变", () => {
  // [0.2, 0.3, 0.5] 的边界 1 往下推 0.1 → 后两段变成 [0.4, 0.4]，第 0 段纹丝不动
  const next = moveBoundary({ sizes: [0.2, 0.3, 0.5], index: 1, delta: 0.1 });
  close(next[0] as number, 0.2);
  close(next[1] as number, 0.4);
  close(next[2] as number, 0.4);
  close((next[0] as number) + (next[1] as number) + (next[2] as number), 1);
});

test("moveBoundary：零位移是恒等变换", () => {
  const sizes = [0.25, 0.25, 0.5];
  assert.deepEqual(moveBoundary({ sizes, index: 0, delta: 0 }), sizes);
});

test("moveBoundary：不修改传入的数组（纯函数）", () => {
  const sizes = [0.4, 0.6];
  moveBoundary({ sizes, index: 0, delta: 0.2 });
  assert.deepEqual(sizes, [0.4, 0.6]);
});

test("parseSize：数字按百分比、字符串按像素（口径与旧 Ark API 一致）", () => {
  close(parseSize(30, 800).ratio, 0.3);
  close(parseSize(30, 800).pixels, 240);
  close(parseSize("120px", 800).ratio, 0.15);
  close(parseSize("120px", 800).pixels, 120);
});

test("parseSize：非法输入与零高度不产生 NaN", () => {
  close(parseSize("abc", 800).ratio, 0);
  close(parseSize("abc", 800).pixels, 0);
  close(parseSize("120px", 0).ratio, 0);
  close(parseSize("120px", 0).pixels, 0);
});

test("minRatiosFrom：缺省项当 0，且不产生除零", () => {
  assert.deepEqual(minRatiosFrom([undefined, "120px", 20], 0), [0, 0, 0]);
  const ratios = minRatiosFrom([undefined, "120px", 20], 800);
  close(ratios[0] as number, 0);
  close(ratios[1] as number, 0.15);
  close(ratios[2] as number, 0.2);
});

test("toPercents：比例 → 百分比（保留精度）", () => {
  const percents = toPercents([0.32, 0.68]);
  close(percents[0] as number, 32);
  close(percents[1] as number, 68);
  assert.deepEqual(toPercents([]), []);
});
