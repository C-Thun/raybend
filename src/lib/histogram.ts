/**
 * 直方图的**画图数学**（纯函数，好测）。
 *
 * 组件只管把三条完整通道曲线叠起来；这里负责归一化与 SVG 折线路径。
 *
 * 口径（人类 2026-09-20 定）：
 *
 * 1. **86 个点**：亮度 0 单独；1..255 每三个色阶的计数取平均；
 * 2. **点与点之间走直线**，不拟合出数据里没有的过冲；
 * 3. **三条完整闭合路径**交给浏览器合成，不再切七块几何色带，因此通道交叉处不会留白；
 * 4. 每次数据变化只重建 3 × 86 个点，能承受后续实时输入。
 */

/**
 * 后端给的原始计数（**结构类型**，不从 `api/` import 具体 DTO —— 分层纪律：
 * `lib` 只能依赖 `assets`）。`api/db.ts` 的 `Histogram` 恰好长这样，直接传就行。
 */
export interface HistogramCounts {
  bins: number;
  r: readonly number[];
  g: readonly number[];
  b: readonly number[];
  /** 三通道合并后的峰值 */
  max: number;
}

/** 采样点数：0 单独 + 1..255 每 3 级平均 ⇒ 86 点。 */
export const HISTOGRAM_SAMPLES = 86;

/** 一根柱子/一个采样点的位置与高度（0..1） */
export interface HistogramBars {
  /** 采样点数（正常等于 [`HISTOGRAM_SAMPLES`]） */
  samples: number;
  /** 三条通道的高度比例（0..1），三通道**共用同一个峰值** */
  r: number[];
  g: number[];
  b: number[];
  /** 峰值（原样带出来，界面要不要显示「最大计数」时用得上） */
  max: number;
}

/**
 * 把后端给的计数换算成 0..1 的采样高度。
 *
 * * `null`（还没取到 / 取不到）→ `null`，界面画空态；
 * * 峰值为 0（全黑或空图）→ 全 0，不除零；
 * * 数组长度与 `bins` 不一致（后端将来改柱数、或数据缺一截）→ **补齐/截断**到 `bins`，
 *   界面永远拿得到刚好 `bins` 个点。
 *
 * ⚠️ **共用峰值**（不是每通道各自归一化）：各自归一化会把 R 通道的噪声放大成满格、
 * 图像看起来像偏色；共用峰值才能看出「这张图的红比蓝高」这种真实关系。
 */
export function histogramBarHeights(hist: HistogramCounts | null): HistogramBars | null {
  if (hist === null) return null;
  const samples =
    Number.isFinite(hist.bins) && hist.bins > 0 ? Math.floor(hist.bins) : 0;
  if (samples === 0) return { samples: 0, r: [], g: [], b: [], max: 0 };

  const fit = (values: readonly number[] | undefined): number[] =>
    Array.from({ length: samples }, (_, index) => {
      const value = values?.[index];
      return typeof value === "number" && Number.isFinite(value) && value > 0 ? value : 0;
    });

  const r = fit(hist.r);
  const g = fit(hist.g);
  const b = fit(hist.b);
  const actualMax = Math.max(...r, ...g, ...b);
  const max =
    Number.isFinite(hist.max) && hist.max > 0 ? Math.max(hist.max, actualMax) : actualMax;
  if (max <= 0) return { samples, r, g, b, max: 0 };

  const scale = (values: number[]): number[] => values.map((value) => value / max);
  return { samples, r: scale(r), g: scale(g), b: scale(b), max };
}

/** 「这张照片到底有没有可画的东西」—— 空直方图不画曲线，改画一句提示 */
export function histogramIsEmpty(bars: HistogramBars | null): boolean {
  return bars === null || bars.max <= 0;
}

const fmt = (value: number): string => value.toFixed(2);

/** 把 0..1 高度逐点换成 SVG 折线路径；脏值在这里再夹一道，绝不画出画布。 */
function linePath(values: readonly number[], width: number, height: number): string {
  if (values.length === 0 || !(width > 0) || !(height > 0)) return "";
  const step = values.length > 1 ? width / (values.length - 1) : 0;
  const y = (value: number): number => {
    const safe = Number.isFinite(value) ? Math.min(1, Math.max(0, value)) : 0;
    return height - safe * height;
  };
  return values
    .map((value, index) => `${index === 0 ? "M" : "L"}${fmt(index * step)},${fmt(y(value))}`)
    .join(" ");
}

/**
 * 一条通道的**填充折线路径**（SVG `d`，从基线闭合）。
 *
 * 用在「这条带从基线往上」的层（三色重叠那层就是这种）。
 */
export function histogramPath(
  values: readonly number[],
  width: number,
  height: number,
): string {
  const top = linePath(values, width, height);
  if (top === "") return "";
  const last = values.length - 1;
  const step = values.length > 1 ? width / last : 0;
  // 回到右下基线 → 左下基线 → 闭合
  return `${top} L${fmt(last * step)},${fmt(height)} L${fmt(0)},${fmt(height)} Z`;
}
