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
  histogramBandPath,
  histogramBands,
  histogramBarHeights,
  histogramIsEmpty,
  histogramPath,
  type HistogramBand,
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

test("histogramPath：52 个采样点 → 51 段", () => {
  const values = Array.from({ length: HISTOGRAM_SAMPLES }, (_, i) => (i % 7) / 7);
  assert.equal(values.length, 52);
  const d = histogramPath(values, 256, 100);
  const segments = d.split(" L").length - 1 - 2; // 扣掉回基线的两段
  assert.equal(segments, 51, `52 个点应当连成 51 段（实际 ${segments}）`);
});

test("直线路径：脏值被夹在画布内且不出 NaN", () => {
  const d = histogramPath([Number.NaN, -2, 2], 100, 100);
  assert.ok(!d.includes("NaN"));
  assert.ok(d.includes("M0.00,100.00"), "NaN 退到基线");
  assert.ok(d.includes("L50.00,100.00"), "负数夹到 0");
  assert.ok(d.includes("L100.00,0.00"), "大于 1 夹到 1");
});

test("histogramBandPath：带形从下边界的右下角回到左下角闭合", () => {
  const d = histogramBandPath([0.8, 0.8], [0.5, 0.5], 100, 100);
  assert.ok(d.startsWith("M0.00,20.00"), `起点应当是上边界的左端，实际 ${d.slice(0, 16)}`);
  assert.ok(d.endsWith("Z"));
  assert.ok(!d.includes(" C"), "带的上下边界都不得单独拟合曲线");
  assert.ok(d.includes("100.00,50.00"), "要走到上边界右端");
  assert.ok(d.includes("0.00,50.00"), "再沿下边界回到左端（y = 100-0.5*100）");
});

test("histogramBandPath：长度不齐时按短的截断（不炸）", () => {
  assert.equal(histogramBandPath([], [], 100, 50), "");
  const d = histogramBandPath([1, 1, 1], [0], 30, 10);
  assert.ok(d.endsWith("Z"));
});

// ─────────────────────────── 分层：7 个区域拼回原通道 ───────────────────────────

test("histogramBands：三色重叠是最低那条，单通道是最高那条", () => {
  const layers = histogramBands([0.9], [0.5], [0.2]);
  assert.deepEqual(layers.rgb.top, [0.2], "三色重叠 = 三者最小");
  assert.deepEqual(layers.rgb.bottom, [0]);
  // 最低的是 b ⇒ 另外两个（r+g）形成「黄」那层；最高的 r 是单通道
  assert.deepEqual(layers.rg.top, [0.5]);
  assert.deepEqual(layers.rg.bottom, [0.2]);
  assert.deepEqual(layers.r.top, [0.9]);
  assert.deepEqual(layers.r.bottom, [0.5]);
  // 没被用到的层是 0 厚度（不是缺项）
  assert.deepEqual(layers.gb.top, [0]);
  assert.deepEqual(layers.b.top, [0]);
});

test("histogramBands：任何列上层与层之间都不重叠、拼起来正好等于三条通道", () => {
  const r = [0.9, 0, 0.3, 1];
  const g = [0.5, 0.7, 0.3, 0.4];
  const b = [0.2, 0.7, 0.8, 0.4];
  const layers = histogramBands(r, g, b);
  for (let i = 0; i < r.length; i += 1) {
    // 每层的 [bottom, top] 必须是有效区间
    for (const band of Object.values(layers)) {
      assert.ok(band.top[i] >= band.bottom[i] - 1e-9, "层的上边界不能低于下边界");
      assert.ok(band.top[i] <= 1 + 1e-9 && band.bottom[i] >= -1e-9, "层必须落在 0..1 内");
    }
    // 单通道那三层的厚度 = 该通道超出「中位数」的部分
    const sorted = [r[i], g[i], b[i]].sort((a, c) => a - c);
    assert.ok(Math.abs(layers.rgb.top[i] - sorted[0]) < 1e-9, "灰层 = 最小值");
  }
});

test("histogramBands：并列时按固定顺序打破平局（结果可复现）", () => {
  const a = histogramBands([0.5], [0.5], [0.5]);
  const b = histogramBands([0.5], [0.5], [0.5]);
  assert.deepEqual(a, b, "同样的输入必须给出同样的输出");
  // 三色等高 ⇒ 全部落在「三色重叠」那层；其余各层的**厚度**都必须是 0
  assert.deepEqual(a.rgb.top, [0.5]);
  const thickness = (band: HistogramBand): number =>
    (band.top[0] ?? 0) - (band.bottom[0] ?? 0);
  const rest = ["rg", "gb", "rb", "r", "g", "b"] as const;
  for (const key of rest) {
    assert.equal(thickness(a[key]), 0, `${key} 层不该有厚度`);
  }
});

test("histogramBands：长度不齐按短的截断", () => {
  const layers = histogramBands([1, 1], [1], []);
  assert.equal(layers.rgb.top.length, 0);
});
