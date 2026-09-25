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
  /** 实际像素的显示亮度计数；旧缓存可缺省。 */
  luma?: readonly number[];
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

/** 曲线底纹：真实亮度 / 单色通道的计数归一化为 SVG 高度。 */
export function curveHistogramValues(
  histogram: HistogramCounts | null,
  channel: "rgb" | "r" | "g" | "b",
): number[] {
  if (histogram === null) return [];
  const counts = channel === "rgb"
    ? histogram.luma ?? histogram.r.map((value, index) => Math.max(value, histogram.g[index] ?? 0, histogram.b[index] ?? 0))
    : histogram[channel];
  const peak = Math.max(0, ...counts);
  return peak > 0 ? counts.map((value) => Math.max(0, value) / peak) : counts.map(() => 0);
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

/* ══════════════════════════════════════════════════════════════
 * 二、合成：7 个区域用**固定色**（人类 2026-09-19 定、2026-09-23 重申）
 * ══════════════════════════════════════════════════════════════ */

/**
 * 7 个区域的**绘制顺序**：先画的在下、后画的盖上去。
 *
 * 顺序不是随便定的 —— 它是「区域分类」能正确落色的**唯一**保证：
 * 每一层都从基线画到自己那条包络，后画的层若也够高就会盖住先画的。
 * 于是某一列的某个高度上，**最后画到那儿的层**就是那一处的正确区域：
 *
 * ```text
 *   rgb（三者最小） ≤ 两两重叠 ≤ 单通道      ← 高度关系
 *   按 r → g → b → rg → gb → rb → rgb 画，最后盖上去的总是更「专」的那一类
 * ```
 *
 * 举例（`r=.9 g=.7 b=.2`）：红 0→.9，绿 0→.7 盖住下半，蓝 0→.2 再盖，
 * 两两重叠 0→.7/.2/.2 继续盖，三色重叠 0→.2 最后盖 —— 最终看得的是
 * `0–.2 灰（三色）/ .2–.7 黄（红+绿）/ .7–.9 红（仅红）`，与「按交叠关系切 7 块」视觉等价。
 *
 * **但比切块好**：相邻区域之间不存在两条各自抗锯齿的边界（后画的那层直接盖在前一层上），
 * 所以不会出现 2026-09-20 人类报的「细小覆盖区域出现空白」—— 那正是硬切边界的毛病。
 * 颜色也**不靠混合模式**：每层填令牌里那个色，**改色只改令牌**。
 */
export const HISTOGRAM_LAYER_ORDER = ["r", "g", "b", "rg", "gb", "rb", "rgb"] as const;

/** 区域键名就是「这一块里有哪几种通道」 */
export type HistogramLayerKey = (typeof HISTOGRAM_LAYER_ORDER)[number];

/** 每个区域的**上边界**（0..1 高度数组）；填充一律从基线画到它 */
export type HistogramLayers = Record<HistogramLayerKey, number[]>;

/**
 * 把三条通道曲线拆成 7 个区域的上边界。
 *
 * ```text
 *   单通道     r / g / b      = 自己
 *   两两重叠   rg / gb / rb   = 那一对的**较小者**
 *   三色重叠   rgb            = 三者的**最小者**
 * ```
 *
 * 脏值（`NaN` / 越界）在这里就夹到 `0..1`，绝不画出画布；
 * 三条长度不一致时按**最短**的来（后端换了柱数也不会画出斜线）。
 */
export function histogramLayers(
  r: readonly number[],
  g: readonly number[],
  b: readonly number[],
): HistogramLayers {
  const samples = Math.max(0, Math.min(r.length, g.length, b.length));
  const zeros = (): number[] => Array.from({ length: samples }, () => 0);
  const layers: HistogramLayers = {
    r: zeros(),
    g: zeros(),
    b: zeros(),
    rg: zeros(),
    gb: zeros(),
    rb: zeros(),
    rgb: zeros(),
  };
  const clamp01 = (value: number | undefined): number => {
    const safe = value ?? 0;
    return Number.isFinite(safe) ? Math.min(1, Math.max(0, safe)) : 0;
  };

  for (let index = 0; index < samples; index += 1) {
    const rv = clamp01(r[index]);
    const gv = clamp01(g[index]);
    const bv = clamp01(b[index]);
    layers.r[index] = rv;
    layers.g[index] = gv;
    layers.b[index] = bv;
    layers.rg[index] = Math.min(rv, gv);
    layers.gb[index] = Math.min(gv, bv);
    layers.rb[index] = Math.min(rv, bv);
    layers.rgb[index] = Math.min(rv, gv, bv);
  }
  return layers;
}

/**
 * 某一列在某个高度上**最终看得的**是哪个区域（与绘制顺序同一套判据）。
 *
 * 给单测用：它能直接锁住「7 个区域的固定色语义」而不用跑浏览器 ——
 * 绘制顺序或包络一旦写错，这里立刻红。
 */
export function histogramRegionAt(
  layers: HistogramLayers,
  index: number,
  height: number,
): HistogramLayerKey | null {
  let winner: HistogramLayerKey | null = null;
  for (const key of HISTOGRAM_LAYER_ORDER) {
    if ((layers[key][index] ?? 0) >= height) winner = key;
  }
  return winner;
}
