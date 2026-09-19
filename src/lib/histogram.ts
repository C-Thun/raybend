/**
 * 直方图的**画图数学**（纯函数，好测）。
 *
 * 组件只该关心「每一段用什么颜色、多高」，不该在里面做归一化、重采样或曲线拟合 ——
 * 那几件事恰恰是最容易出错的地方（除零、后端换了柱数、数组长度不齐、插值过冲）。
 *
 * 三条口径（人类 2026-09-19 定）：
 *
 * 1. **采样 52 个点**（0..255 每 5 级一个点，51 格）。点数由前端向 Rust 请求
 *    （`getHistogram(path, HISTOGRAM_SAMPLES)`），后端按 `value * bins / 256` 分桶；
 * 2. **点与点之间走直线**：重叠色带的上下边界若分别做三次插值，即便各自不过冲，
 *    也可能在两个采样点之间互相穿过，画出数据里没有的细线/尖刺；逐点折线严格经过
 *    后端采样值，不在采样点之间编造额外形状；
 * 3. **每个区域在每一列都有定义**（哪怕厚度是 0）：谁高谁低**逐列**都在变，
 *    如果只在「自己当主角」的列上给值、其余列置 0，跨列连线就会**掉回基线** ——
 *    那就是 2026-09-20 人类报的「像尖刺一样的边缘异常」（见 `histogramBands`）；
 * 4. **颜色分层**：三条通道曲线叠起来一共 7 种区域 —— 单通道（红/绿/蓝）、
 *    两两重叠（黄/青/紫）、三色重叠（中间灰）。**不靠混合模式取色**，
 *    而是把 7 个区域**显式切开**，每层填自己的颜色：
 *    混合模式（`screen`）在改用非纯色通道后混不出规定的黄/青/紫，
 *    显式分层则颜色就是令牌里那个色，改色只改令牌。
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

/** 采样点数：0 到 255 每隔 5 级取一个点 ⇒ **52 个点、51 格**（人类 2026-09-19）。 */
export const HISTOGRAM_SAMPLES = 52;

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

/* ══════════════════════════════════════════════════════════════
 * 二、分层：把三通道叠加切成 7 个互不重叠的区域
 * ══════════════════════════════════════════════════════════════ */

/** 一条带：从 `bottom` 到 `top` 之间的区域（都是 0..1 的高度数组）。 */
export interface HistogramBand {
  top: number[];
  bottom: number[];
}

/** 7 个区域。键名就是「这个区域里有哪几种通道」。 */
export interface HistogramBands {
  /** 三色重叠：中间灰 */
  rgb: HistogramBand;
  /** 两两重叠：红+绿=黄、绿+蓝=青、红+蓝=紫 */
  rg: HistogramBand;
  gb: HistogramBand;
  rb: HistogramBand;
  /** 单通道：红 / 绿 / 蓝 */
  r: HistogramBand;
  g: HistogramBand;
  b: HistogramBand;
}

/**
 * 把三条通道曲线切成 7 个**互不重叠**的区域。
 *
 * 每一层都用「自己的边界曲线」表达，**处处有定义**（厚度可以为 0）：
 *
 * ```text
 *         ┌── 单通道 = [max(另两个), 自己]      ← 自己不是最高时厚度 0（贴在 max 上）
 *         ├── 两两重叠 = [min(自己, min(另两个)), min(另两个)]   ← 那一对里的低者最高时才厚
 *         └── 三色重叠 = [0, min(三者的最小)]
 * ```
 *
 * 为什么不用「每列先排序、再把 low/mid/high 分给对应层」那种写法：那样只有
 * 「本列的冠军」才有值，其余层是 0 —— 相邻两列的冠军一换，那条折线就从曲线上
 * 直直落回基线，画出来就是一根**细尖刺**（人类 2026-09-20 报的异常）。
 * 现在的写法两层边界都是连续曲线，冠军切换时只是厚度连续地变成 0，
 * 不会编造出数据里没有的区域。
 *
 * 颜色语义仍然是「加色叠加」：黄 = 红+绿（b 最低时那一对）、青 = 绿+蓝、紫 = 红+蓝；
 * 三色重叠 = 中间灰。并列（两通道等高）时厚度自然是 0，不靠“平局规则”去指定颜色。
 */
export function histogramBands(
  r: readonly number[],
  g: readonly number[],
  b: readonly number[],
): HistogramBands {
  const samples = Math.min(r.length, g.length, b.length);
  const zeros = (): number[] => Array.from({ length: samples }, () => 0);

  const band = (): HistogramBand => ({ top: zeros(), bottom: zeros() });

  const layers: HistogramBands = {
    rgb: band(),
    rg: band(),
    gb: band(),
    rb: band(),
    r: band(),
    g: band(),
    b: band(),
  };

  const clamp01 = (value: number): number =>
    Number.isFinite(value) ? Math.min(1, Math.max(0, value)) : 0;

  /** 把一条带子写进两层数组（`bottom ≤ top` 由调用方保证） */
  const set = (target: HistogramBand, index: number, bottom: number, top: number): void => {
    target.bottom[index] = bottom;
    target.top[index] = top;
  };

  for (let index = 0; index < samples; index += 1) {
    const rv = clamp01(r[index] ?? 0);
    const gv = clamp01(g[index] ?? 0);
    const bv = clamp01(b[index] ?? 0);

    // 三色重叠：0 → 三者最小（最低那条包络，本来就连续）
    set(layers.rgb, index, 0, Math.min(rv, gv, bv));

    /*
     * 单通道：另一个更大的那个 → 自己。
     * 自己不是最高时 `max(另两个) ≥ 自己` ⇒ 被夹到 0 厚度、贴在自己这条曲线上。
     */
    set(layers.r, index, Math.min(rv, Math.max(gv, bv)), rv);
    set(layers.g, index, Math.min(gv, Math.max(rv, bv)), gv);
    set(layers.b, index, Math.min(bv, Math.max(rv, gv)), bv);

    /*
     * 两两重叠：黄 = 红+绿（只在 b 最低时出现）⇒ [min(b, min(r,g)), min(r,g)]。
     * 其余情况厚度 0：贴着 `min(r,g)`，而不是贴着基线。
     */
    set(layers.rg, index, Math.min(bv, Math.min(rv, gv)), Math.min(rv, gv));
    set(layers.gb, index, Math.min(rv, Math.min(gv, bv)), Math.min(gv, bv));
    set(layers.rb, index, Math.min(gv, Math.min(rv, bv)), Math.min(rv, bv));
  }

  return layers;
}

/* ══════════════════════════════════════════════════════════════
 * 三、逐点折线 + 带形路径
 * ══════════════════════════════════════════════════════════════ */

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

/**
 * 一条**带形**的路径（`bottom` → `top` 之间那一条），用于两两重叠与单通道那两层。
 *
 * 做法：沿 `top` 从左画到右，再沿 `bottom` 从右画回左，闭合。
 * 两条边只连接同一批采样点，不分别拟合曲线，所以不会在点与点之间互相穿过。
 */
export function histogramBandPath(
  top: readonly number[],
  bottom: readonly number[],
  width: number,
  height: number,
): string {
  const count = Math.min(top.length, bottom.length);
  if (count === 0) return "";
  const topValues = top.slice(0, count);
  const bottomValues = bottom.slice(0, count);
  const forward = linePath(topValues, width, height);
  if (forward === "" || !(width > 0) || !(height > 0)) return "";
  const step = count > 1 ? width / (count - 1) : 0;
  const reverse = [...bottomValues]
    .reverse()
    .map((value, reverseIndex) => {
      const index = count - 1 - reverseIndex;
      const safe = Number.isFinite(value) ? Math.min(1, Math.max(0, value)) : 0;
      return `L${fmt(index * step)},${fmt(height - safe * height)}`;
    })
    .join(" ");
  return `${forward} ${reverse} Z`;
}
