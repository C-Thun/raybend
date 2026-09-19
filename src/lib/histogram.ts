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
 * 2. **点与点之间走平滑曲线**：用**单调三次插值**（Fritsch–Carlson）——
 *    Catmull-Rom 之类会过冲（直方图上表现为「冲出框外」或「掉到基线以下」），
 *    单调插值既平滑又不会越过相邻采样点的高度，正是直方图要的形状；
 * 3. **颜色分层**：三条通道曲线叠起来一共 7 种区域 —— 单通道（红/绿/蓝）、
 *    两两重叠（黄/青/紫）、三色重叠（中间灰）。**不靠混合模式取色**，
 *    而是按「每列排序后分层」把 7 个区域**显式切开**，每层填自己的颜色：
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
 * 把三条通道曲线切成 7 个**互不重叠**的区域（每列按高度排序后切层）：
 *
 * ```text
 *         ┌── 单通道（最高那条）        ← 它的色
 *         ├── 两两重叠（中间那条）      ← 那一对的色（黄/青/紫）
 *         └── 三色重叠（最低那条）      ← 中间灰
 * ```
 *
 * 这样每一列的颜色构成与「加色叠加」的观感一致，但**颜色是我们指定的**，
 * 而不是交给 `mix-blend-screen` 去算（后者在通道色不是纯红绿蓝时，混不出规定的黄/青/紫）。
 *
 * 并列（两通道等高）时按固定顺序（r → g → b）打破平局，保证结果**可复现**。
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

  for (let index = 0; index < samples; index += 1) {
    // 排序（并列时按 r → g → b 的固定顺序，结果可复现）
    const triple: [number, string][] = [
      [clamp01(r[index] ?? 0), "r"],
      [clamp01(g[index] ?? 0), "g"],
      [clamp01(b[index] ?? 0), "b"],
    ];
    triple.sort((left, right) => left[0] - right[0] || left[1].localeCompare(right[1]));
    const [low, mid, high] = triple;

    // 三色重叠：0 → 最低那条
    layers.rgb.bottom[index] = 0;
    layers.rgb.top[index] = low[0];

    /*
     * 两两重叠：最低 → 中间那条。
     * 哪一对「重叠」取决于**哪两个通道不是最低的** —— 所以用「最低的那个」反推：
     * 最低是 r ⇒ 另外两个是 g+b；最低是 g ⇒ r+b；最低是 b ⇒ r+g。
     */
    const pairBand =
      low[1] === "r"
        ? layers.gb
        : low[1] === "g"
          ? layers.rb
          : layers.rg;
    pairBand.bottom[index] = low[0];
    pairBand.top[index] = mid[0];

    // 单通道：中间 → 最高那条
    const singleBand = high[1] === "r" ? layers.r : high[1] === "g" ? layers.g : layers.b;
    singleBand.bottom[index] = mid[0];
    singleBand.top[index] = high[0];
  }

  return layers;
}

/* ══════════════════════════════════════════════════════════════
 * 三、平滑曲线：单调三次插值（Fritsch–Carlson）+ 带形路径
 * ══════════════════════════════════════════════════════════════ */

/**
 * 单调三次插值的切线（Fritsch–Carlson 限制器）。
 *
 * 直观解释：先用中心差分估斜率，再把斜率「夹」到不超过相邻斜率之和的一半 ——
 * 这一步保证了曲线**不会在两点之间冒出一个比两端都高（或都低）的鼓包**。
 * 返回的是每个采样点处的斜率（相对于 x 的，x 间距已归一化为 1）。
 */
function monotoneSlopes(values: readonly number[]): number[] {
  const n = values.length;
  if (n <= 1) return [0];

  const delta: number[] = [];
  for (let i = 0; i < n - 1; i += 1) delta.push(values[i + 1] - values[i]);

  const slopes: number[] = new Array(n).fill(0);
  slopes[0] = delta[0];
  slopes[n - 1] = delta[n - 2];
  for (let i = 1; i < n - 1; i += 1) {
    // 极值点（前后差分异号或有一侧为 0）→ 斜率取 0，曲线在那里「停一下」
    if (delta[i - 1] * delta[i] <= 0) slopes[i] = 0;
    else slopes[i] = (delta[i - 1] + delta[i]) / 2;
  }
  for (let i = 0; i < n - 1; i += 1) {
    if (delta[i] === 0) {
      slopes[i] = 0;
      slopes[i + 1] = 0;
      continue;
    }
    const a = slopes[i] / delta[i];
    const b = slopes[i + 1] / delta[i];
    const squared = a * a + b * b;
    if (squared > 9) {
      const scale = 3 / Math.sqrt(squared);
      slopes[i] = scale * a * delta[i];
      slopes[i + 1] = scale * b * delta[i];
    }
  }
  return slopes;
}

/**
 * 在**同一条单调插值曲线**上均匀取样（`count` 个点）。
 *
 * 用途有两个，都与「曲线就是形状」这件事有关：
 *   * 单测：验证插值不过冲、单调段不下降 —— 直接对数值断言，比解析路径文本可靠；
 *   * 将来编辑模块要在曲线上做交互（比如按亮度范围取点），也需要「曲线在 x 处多高」。
 *
 * 与 [`fitCurve`] 用的是**同一组切线**（Hermite 形式 = 贝塞尔控制点那种写法），
 * 所以这里取到的形状就是屏幕上画出来的形状。
 */
export function monotoneSample(values: readonly number[], count: number): number[] {
  if (values.length === 0 || count <= 0) return [];
  if (values.length === 1) return Array.from({ length: count }, () => values[0] ?? 0);

  const slopes = monotoneSlopes(values);
  const segments = values.length - 1;
  const out: number[] = [];
  for (let k = 0; k < count; k += 1) {
    const position = count === 1 ? 0 : (k / (count - 1)) * segments;
    const index = Math.min(segments - 1, Math.floor(position));
    const t = position - index;
    const p0 = values[index] ?? 0;
    const p1 = values[index + 1] ?? 0;
    const m0 = slopes[index] ?? 0;
    const m1 = slopes[index + 1] ?? 0;
    const t2 = t * t;
    const t3 = t2 * t;
    out.push(
      (2 * t3 - 3 * t2 + 1) * p0 +
        (t3 - 2 * t2 + t) * m0 +
        (-2 * t3 + 3 * t2) * p1 +
        (t3 - t2) * m1,
    );
  }
  return out;
}

/** 一条曲线拆成的三次贝塞尔段（已换算成像素坐标，y 轴向下）。 */
interface Curve {
  /** 起点坐标 */
  x0: number;
  y0: number;
  /** 依次的段：每个段两个控制点 + 终点 */
  segments: { c1x: number; c1y: number; c2x: number; c2y: number; x: number; y: number }[];
}

/** 把 0..1 的高度序列拟合成曲线（x 均分到 `width`，y 翻转成屏幕坐标）。 */
function fitCurve(values: readonly number[], width: number, height: number): Curve | null {
  if (values.length === 0 || !(width > 0) || !(height > 0)) return null;
  const n = values.length;
  const step = n > 1 ? width / (n - 1) : 0;
  const px = (index: number): number => index * step;
  const py = (value: number): number => height - value * height;

  const slopes = monotoneSlopes(values);
  const segments: Curve["segments"] = [];
  for (let i = 0; i < n - 1; i += 1) {
    const dx = step / 3;
    segments.push({
      c1x: px(i) + dx,
      c1y: py(values[i]) - slopes[i] * dx * height,
      c2x: px(i + 1) - dx,
      c2y: py(values[i + 1]) + slopes[i + 1] * dx * height,
      x: px(i + 1),
      y: py(values[i + 1]),
    });
  }
  return { x0: px(0), y0: py(values[0]), segments };
}

const fmt = (value: number): string => value.toFixed(2);

/** 曲线 → SVG 路径片段（从起点依次画到终点）。 */
function curveToPath(curve: Curve): string {
  let d = `M${fmt(curve.x0)},${fmt(curve.y0)}`;
  for (const segment of curve.segments) {
    d += ` C${fmt(segment.c1x)},${fmt(segment.c1y)} ${fmt(segment.c2x)},${fmt(segment.c2y)} ${fmt(segment.x)},${fmt(segment.y)}`;
  }
  return d;
}

/**
 * 一条通道的**填充曲线路径**（SVG `d`，从基线闭合）。
 *
 * 用在「这条带从基线往上」的层（三色重叠那层就是这种）。
 */
export function histogramPath(
  values: readonly number[],
  width: number,
  height: number,
): string {
  const curve = fitCurve(values, width, height);
  if (curve === null) return "";
  const last = values.length - 1;
  const step = values.length > 1 ? width / last : 0;
  const top = curveToPath(curve);
  // 回到右下基线 → 左下基线 → 闭合
  return `${top} L${fmt(last * step)},${fmt(height)} L${fmt(0)},${fmt(height)} Z`;
}

/**
 * 一条**带形**的路径（`bottom` → `top` 之间那一条），用于两两重叠与单通道那两层。
 *
 * 做法：沿 `top` 从左画到右，再沿 `bottom` 从右画回左，闭合。
 * 两条边都走同一种单调插值，所以带宽在采样点之间也平滑变化。
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
  const topCurve = fitCurve(topValues, width, height);
  const bottomCurve = fitCurve(bottomValues, width, height);
  if (topCurve === null || bottomCurve === null) return "";

  const forward = curveToPath(topCurve);

  // 反向：把 bottom 的段倒着写（控制点也互换位置）
  const reversed: string[] = [];
  for (let i = bottomCurve.segments.length - 1; i >= 0; i -= 1) {
    const segment = bottomCurve.segments[i];
    const startX = i === 0 ? bottomCurve.x0 : bottomCurve.segments[i - 1].x;
    const startY = i === 0 ? bottomCurve.y0 : bottomCurve.segments[i - 1].y;
    reversed.push(
      `C${fmt(segment.c2x)},${fmt(segment.c2y)} ${fmt(segment.c1x)},${fmt(segment.c1y)} ${fmt(startX)},${fmt(startY)}`,
    );
  }
  // 不用 `Array.prototype.at`：本仓的 TS lib 目标不含 ES2022（`at` 会直接编译报错）
  const lastBottomSegment = bottomCurve.segments[bottomCurve.segments.length - 1];
  const lastBottomX = lastBottomSegment?.x ?? bottomCurve.x0;
  const lastBottomY = lastBottomSegment?.y ?? bottomCurve.y0;

  return `${forward} L${fmt(lastBottomX)},${fmt(lastBottomY)} ${reversed.join(" ")} Z`;
}
