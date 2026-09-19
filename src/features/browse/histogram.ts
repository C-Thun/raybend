/**
 * 直方图的**画图数学**（纯函数，好测）。
 *
 * 组件只该关心「每根柱子多高」，不该在里面做归一化和长度对齐 ——
 * 那两件事恰恰是最容易出错的地方（除零、后端换了柱数、数组长度不齐）。
 *
 * 归一化口径：**三通道共用同一个峰值**（`Histogram.max`）。
 * 每个通道各自归一化的话，R 通道的噪声会被放大成满格、图像看起来像偏色；
 * 共用峰值才能看出「这张图的红比蓝高」这种真实关系。
 */

import type { Histogram } from "../../api/db.ts";

/** 一根柱子：0..1 的高度比例，`r[i]` / `g[i]` / `b[i]` 三层叠着画 */
export interface HistogramBars {
  bins: number;
  r: number[];
  g: number[];
  b: number[];
  /** 峰值（原样带出来，界面要不要显示「最大计数」时用得上） */
  max: number;
}

/**
 * 把后端给的计数换算成 0..1 的柱高。
 *
 * * `null`（还没取到 / 取不到）→ `null`，界面画空态；
 * * 峰值为 0（全黑或空图）→ 全 0，不除零；
 * * 数组长度与 `bins` 不一致（后端将来改柱数、或数据缺一截）→ **补齐/截断**到 `bins`，
 *   界面永远拿得到刚好 `bins` 根柱子。
 */
export function histogramBarHeights(hist: Histogram | null): HistogramBars | null {
  if (hist === null) return null;
  const bins = Number.isFinite(hist.bins) && hist.bins > 0 ? Math.floor(hist.bins) : 0;
  if (bins === 0) return { bins: 0, r: [], g: [], b: [], max: 0 };

  const fit = (values: readonly number[] | undefined): number[] =>
    Array.from({ length: bins }, (_, index) => {
      const value = values?.[index];
      return typeof value === "number" && Number.isFinite(value) && value > 0 ? value : 0;
    });

  const r = fit(hist.r);
  const g = fit(hist.g);
  const b = fit(hist.b);
  const actualMax = Math.max(...r, ...g, ...b);
  const max =
    Number.isFinite(hist.max) && hist.max > 0 ? Math.max(hist.max, actualMax) : actualMax;
  if (max <= 0) return { bins, r, g, b, max: 0 };

  const scale = (values: number[]): number[] => values.map((value) => value / max);
  return { bins, r: scale(r), g: scale(g), b: scale(b), max };
}

/** 「这张照片到底有没有可画的东西」—— 空直方图不画柱子，改画一句提示 */
export function histogramIsEmpty(bars: HistogramBars | null): boolean {
  return bars === null || bars.max <= 0;
}

/**
 * 一条通道的**填充曲线路径**（SVG `d`）—— 人类 2026-09-19 定的新画法。
 *
 * 为什么不做插值：**采样密度本身就大于像素密度**（256 桶画在几百像素宽的框里，
 * 一桶不到一个像素），所以直接连点就已经平滑、看不出台阶。
 * 「按像素级精度构筑曲线」指的就是这件事 —— 一个色阶一个采样点，不抽样、不平均。
 *
 * 形状：从左下基线出发 → 依次经过每个桶 → 回到右下基线闭合（填充用）。
 * 越界的高度会被夹到 0..1（后端换了桶数、或数据不齐时不至于画出框外）。
 */
export function histogramPath(
  values: readonly number[],
  width: number,
  height: number,
): string {
  if (values.length === 0 || !(width > 0) || !(height > 0)) return "";
  const step = width / values.length;
  const points = values.map((value, index) => {
    const clamped = Number.isFinite(value) ? Math.min(1, Math.max(0, value)) : 0;
    const x = index * step;
    const y = height - clamped * height;
    return `${x.toFixed(2)},${y.toFixed(2)}`;
  });
  return `M0,${height.toFixed(2)} L${points.join(" L")} L${width.toFixed(2)},${height.toFixed(2)} Z`;
}
