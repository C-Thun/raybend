/**
 * 直方图画图数学的测试。
 *
 * 重点是**不会把界面搞崩的那几类**：峰值为 0（除零）、数组长度不齐、
 * 后端换了柱数、值里混进负数/NaN。
 */

import assert from "node:assert/strict";
import test from "node:test";
import {
  histogramPath,
  histogramBarHeights,
  histogramIsEmpty,
} from "./histogram.ts";
import type { Histogram } from "../../api/db.ts";

function hist(overrides: Partial<Histogram> = {}): Histogram {
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
  assert.equal(bars.bins, 4);
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
  assert.equal(bars.bins, 3);
  assert.equal(bars.r.length, 3);
});

test("histogramBarHeights：bins 非法（0/NaN）时不炸，给空柱", () => {
  const zero = histogramBarHeights(hist({ bins: 0 }));
  assert.deepEqual(zero, { bins: 0, r: [], g: [], b: [], max: 0 });
  const nan = histogramBarHeights(hist({ bins: Number.NaN }));
  assert.equal(nan?.bins, 0);
});

test("histogramBarHeights：后端少报了峰值时以实际数据为准", () => {
  // 峰值字段与数据不一致（后端加了缓存/降采样后可能发生）：按实际最大值归一化，
  // 否则柱子会超出 100% 或整体发扁
  const bars = histogramBarHeights(hist({ max: 4 }));
  assert.ok(bars !== null);
  assert.equal(bars.max, 40);
  assert.equal(bars.b[3], 1);
});

// ─────────────────────────── 填充曲线路径（新画法） ───────────────────────────

test("histogramPath：空数据与非法尺寸 → 空路径（不画）", () => {
  assert.equal(histogramPath([], 100, 50), "");
  assert.equal(histogramPath([0.5], 0, 50), "");
  assert.equal(histogramPath([0.5], 100, 0), "");
  assert.equal(histogramPath([0.5], Number.NaN, 50), "");
});

test("histogramPath：从左下基线出发、沿曲线、回基线闭合", () => {
  const d = histogramPath([0, 1, 0], 100, 40);
  assert.ok(d.startsWith("M0,40.00 L"), `起点必须是左下基线，实际 ${d.slice(0, 20)}`);
  assert.ok(d.endsWith("Z"), "路径要闭合（填充用）");
  assert.ok(d.includes("0.00,40.00"), "第一个点落在基线（值 0）");
  // 三等分：步长 100/3 = 33.33，所以峰值点在 x=33.33（不是 50）
  assert.ok(d.includes("33.33,0.00"), "中间点顶到上沿（值 1）");
});

test("histogramPath：每个桶一个采样点（像素级精度）", () => {
  const bins = 256;
  const values = Array.from({ length: bins }, (_, i) => (i % 7) / 7);
  const d = histogramPath(values, 256, 100);
  // 采样点 = 桶数（起点 M 不算）—— 密度大于像素密度，所以曲线天然平滑
  const samples = d.split(" L").length - 1 - 1; // 末尾还有一个「回基线」的点
  assert.equal(samples, bins, `采样点数应当等于桶数（实际 ${samples}）`);
});

test("histogramPath：越界高度夹到 0..1，不画出框外", () => {
  const d = histogramPath([-5, 2, Number.NaN], 3, 10);
  assert.ok(d.includes("0.00,10.00"), "负值 → 落在基线");
  assert.ok(d.includes("1.00,0.00"), "大于 1 → 顶到上沿");
  assert.ok(!d.includes("NaN"), "非数字不传播");
});

test("histogramPath：单桶时也要是一条完整可填充的路径", () => {
  const d = histogramPath([0.5], 10, 10);
  assert.ok(d.startsWith("M0,10.00 L"), "仍从基线起");
  assert.ok(d.endsWith("Z"), "仍闭合");
  assert.ok(d.includes("0.00,5.00"), "中点高度 = 0.5");
});
