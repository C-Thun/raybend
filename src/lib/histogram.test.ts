/**
 * 直方图画图数学的测试。
 *
 * 重点是**不会把界面搞崩的那几类**：峰值为 0（除零）、数组长度不齐、
 * 后端换了柱数、值里混进负数/NaN。
 */

import assert from "node:assert/strict";
import test from "node:test";
import {
  HISTOGRAM_SAMPLES,
  histogramBarHeights,
  histogramIsEmpty,
  histogramPath,
} from "./histogram.ts";
import type { HistogramCounts } from "./histogram.ts";

function hist(overrides: Partial<HistogramCounts> = {}): HistogramCounts {
  return {
    bins: 4,
    r: [10, 0, 5, 0],
    g: [0, 20, 0, 0],
    b: [0, 0, 0, 40],
    max: 40,
    ...overrides,
  };
}

test("histogramBarHeights：按三通道共用峰值归一化", () => {
  const bars = histogramBarHeights(hist());
  assert.ok(bars !== null);
  assert.equal(bars.samples, 4);
  assert.deepEqual(bars.r, [0.25, 0, 0.125, 0]);
  assert.deepEqual(bars.g, [0, 0.5, 0, 0]);
  assert.deepEqual(bars.b, [0, 0, 0, 1]);
  assert.equal(bars.max, 40);
});

test("histogramBarHeights：峰值为 0（全黑/空图）不除零", () => {
  const bars = histogramBarHeights(
    hist({ r: [0, 0, 0, 0], g: [0, 0, 0, 0], b: [0, 0, 0, 0], max: 0 }),
  );
  assert.ok(bars !== null);
  assert.deepEqual(bars.r, [0, 0, 0, 0]);
  assert.equal(bars.max, 0);
  assert.equal(histogramIsEmpty(bars), true);
});

test("histogramBarHeights：null 原样传回（界面画空态）", () => {
  assert.equal(histogramBarHeights(null), null);
  assert.equal(histogramIsEmpty(null), true);
});

test("histogramBarHeights：数组长度不齐时补齐到 bins", () => {
  const bars = histogramBarHeights(
    hist({ r: [1], g: [], b: [1, 2, 3, 4, 5], max: 5 }),
  );
  assert.ok(bars !== null);
  assert.equal(bars.r.length, 4);
  assert.equal(bars.g.length, 4);
  assert.equal(bars.b.length, 4, "多出来的截断");
  assert.equal(bars.r[0], 1 / 5, "只有第一个值，其余补 0");
  assert.equal(bars.r[3], 0);
  assert.equal(bars.b[3], 4 / 5);
});

test("histogramBarHeights：脏值不会渗进柱高", () => {
  const bars = histogramBarHeights(
    hist({
      r: [-5, Number.NaN, 8, Number.POSITIVE_INFINITY],
      g: [0, 0, 0, 0],
      b: [0, 0, 0, 0],
      max: 8,
    }),
  );
  assert.ok(bars !== null);
  assert.deepEqual(bars.r, [0, 0, 1, 0]);
});

test("histogramBarHeights：后端换了柱数，界面跟着走", () => {
  const bars = histogramBarHeights(hist({ bins: 3, r: [1, 1, 1], g: [1, 1, 1], b: [2, 2, 2], max: 2 }));
  assert.ok(bars !== null);
  assert.equal(bars.samples, 3);
  assert.equal(bars.r.length, 3);
});

test("histogramBarHeights：bins 非法（0/NaN）时不炸，给空柱", () => {
  const zero = histogramBarHeights(hist({ bins: 0 }));
  assert.deepEqual(zero, { samples: 0, r: [], g: [], b: [], max: 0 });
  const nan = histogramBarHeights(hist({ bins: Number.NaN }));
  assert.equal(nan?.samples, 0);
});

test("histogramBarHeights：后端少报了峰值时以实际数据为准", () => {
  // 峰值字段与数据不一致（后端加了缓存/降采样后可能发生）：按实际最大值归一化，
  // 否则柱子会超出 100% 或整体发扁
  const bars = histogramBarHeights(hist({ max: 4 }));
  assert.ok(bars !== null);
  assert.equal(bars.max, 40);
  assert.equal(bars.b[3], 1);
});

// ─────────────────── 填充折线路径与带形路径（逐点 + 分层） ───────────────────

test("histogramPath：空数据与非法尺寸 → 空路径（不画）", () => {
  assert.equal(histogramPath([], 100, 50), "");
  assert.equal(histogramPath([0.5], 0, 50), "");
  assert.equal(histogramPath([0.5], 100, 0), "");
  assert.equal(histogramPath([0.5], Number.NaN, 50), "");
});

test("histogramPath：从左下基线出发、逐点直连、回基线闭合", () => {
  const d = histogramPath([0, 1, 0], 100, 40);
  assert.ok(d.startsWith("M0.00,40.00"), `起点必须是左下基线，实际 ${d.slice(0, 20)}`);
  assert.ok(d.endsWith("Z"), "路径要闭合（填充用）");
  assert.ok(!d.includes(" C"), "不得拟合三次曲线（重叠色带的上下边界会在点间交叉）");
  assert.ok(d.includes(" L"), "采样点之间应当是直线段");
  assert.ok(d.includes("50.00,0.00"), "中间点顶到上沿（值 1 → y=0）");
});

test("histogramPath：86 个采样点 → 85 段", () => {
  const values = Array.from({ length: HISTOGRAM_SAMPLES }, (_, i) => (i % 7) / 7);
  assert.equal(values.length, 86);
  const d = histogramPath(values, 256, 100);
  const segments = d.split(" L").length - 1 - 2; // 扣掉回基线的两段
  assert.equal(segments, 85, `86 个点应当连成 85 段（实际 ${segments}）`);
});

test("直线路径：脏值被夹在画布内且不出 NaN", () => {
  const d = histogramPath([Number.NaN, -2, 2], 100, 100);
  assert.ok(!d.includes("NaN"));
  assert.ok(d.includes("M0.00,100.00"), "NaN 退到基线");
  assert.ok(d.includes("L50.00,100.00"), "负数夹到 0");
  assert.ok(d.includes("L100.00,0.00"), "大于 1 夹到 1");
});
