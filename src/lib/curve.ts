/**
 * 曲线求值（**单调三次 / Fritsch–Carlson**）—— 与 Rust 侧 `develop::curve` 同一套数学。
 *
 * # 为什么前端也要有一份
 *
 * 管线的真值在 Rust（`crates/raybend/src/develop/curve.rs`），但**画那条曲线**是纯前端的事：
 * 拖动时每一帧都要把曲线画出来，为这个发 IPC 去问 Rust 是舍近求远。
 *
 * # 两份实现怎么保证不漂
 *
 * `src/lib/curve-vectors.json` 是**外部给定的测试向量**（由 Rust 侧算出来、存成文件），
 * **两侧都对着它断言**（Rust：`develop::curve` 的单测；TS：`curve.test.ts`）。
 * 公式一改，两边都得动，漏一处就红 —— 与 `dto-contract` / `develop-params.json` 同一套做法。
 *
 * # 与 Rust 侧的约定（逐条对齐）
 *
 * * 控制点按 x 严格递增，落在 [0,1]×[0,1]；
 * * **首尾两点可以左右拖**（黑场 / 白场）：曲线之外的部分取端点值；
 * * 相邻两点的 x 至少隔开 `MIN_X_GAP`；
 * * 切线按 Fritsch–Carlson 限幅 ⇒ 曲线**不会过冲**（不出现比相邻控制点更高/更低的取值）。
 */

/** 相邻控制点 x 的最小间距（与 Rust 的 `MIN_X_GAP` 同值）。 */
export const MIN_X_GAP = 1 / 256;

/** 一个控制点（归一化 0..1）。 */
export type CurvePoint = readonly [number, number];

/** 恒等曲线的控制点。 */
export const IDENTITY_CURVE: readonly CurvePoint[] = [
  [0, 0],
  [1, 1],
];

/** 一条曲线是不是恒等（界面上「这个通道动过没有」）。 */
export function isIdentityCurve(points: readonly CurvePoint[]): boolean {
  return (
    points.length === 2 &&
    points[0][0] === 0 &&
    points[0][1] === 0 &&
    points[1][0] === 1 &&
    points[1][1] === 1
  );
}

/** 控制点排序 + 去重（非法输入原样返回，由调用方校验）。 */
function normalize(points: readonly CurvePoint[]): CurvePoint[] {
  return [...points]
    .map(([x, y]) => [clamp01(x), clamp01(y)] as CurvePoint)
    .sort((a, b) => a[0] - b[0]);
}

function clamp01(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(1, Math.max(0, value));
}

/**
 * Fritsch–Carlson 切线（与 Rust 的 `tangents()` 逐步对应）。
 */
function tangents(points: readonly CurvePoint[]): number[] {
  const n = points.length;
  if (n < 2) return [0];
  if (n === 2) {
    const slope = (points[1][1] - points[0][1]) / (points[1][0] - points[0][0]);
    return [slope, slope];
  }
  const slopes: number[] = [];
  for (let index = 0; index + 1 < n; index += 1) {
    slopes.push(
      (points[index + 1][1] - points[index][1]) /
        (points[index + 1][0] - points[index][0]),
    );
  }
  const result = new Array<number>(n).fill(0);
  result[0] = slopes[0];
  result[n - 1] = slopes[n - 2];
  for (let index = 1; index < n - 1; index += 1) {
    // 局部极值处使用水平切线，与 Rust 保持一致，防止拐点过冲。
    result[index] = slopes[index - 1] * slopes[index] <= 0
      ? 0
      : (slopes[index - 1] + slopes[index]) / 2;
  }
  for (let index = 0; index + 1 < n; index += 1) {
    const delta = slopes[index];
    if (Math.abs(delta) < 1e-9) {
      result[index] = 0;
      result[index + 1] = 0;
      continue;
    }
    const alpha = result[index] / delta;
    const beta = result[index + 1] / delta;
    const magnitude = alpha * alpha + beta * beta;
    if (magnitude > 9) {
      const tau = 3 / Math.sqrt(magnitude);
      result[index] = tau * alpha * delta;
      result[index + 1] = tau * beta * delta;
    }
  }
  return result;
}

/**
 * 求值：给一条曲线，返回一个 `(x) => y` 的函数（x、y 都在 0..1）。
 *
 * 与 Rust 的 `Curve::eval` 同一个语义：**首尾之外取端点值**（黑场 / 白场）。
 */
export function curveFunction(points: readonly CurvePoint[]): (x: number) => number {
  const sorted = normalize(points);
  if (sorted.length < 2) return (x) => clamp01(x);
  const slopes = tangents(sorted);

  return (x: number): number => {
    if (!Number.isFinite(x)) return 0;
    const value = clamp01(x);
    if (value <= sorted[0][0]) return sorted[0][1];
    const last = sorted[sorted.length - 1];
    if (value >= last[0]) return last[1];
    let segment = 0;
    for (let index = 0; index + 1 < sorted.length; index += 1) {
      if (value >= sorted[index][0] && value <= sorted[index + 1][0]) {
        segment = index;
        break;
      }
    }
    const [x0, y0] = sorted[segment];
    const [x1, y1] = sorted[segment + 1];
    const h = x1 - x0;
    const t = h === 0 ? 0 : (value - x0) / h;
    const t2 = t * t;
    const t3 = t2 * t;
    const h00 = 2 * t3 - 3 * t2 + 1;
    const h10 = t3 - 2 * t2 + t;
    const h01 = -2 * t3 + 3 * t2;
    const h11 = t3 - t2;
    const y =
      h00 * y0 + h10 * h * slopes[segment] + h01 * y1 + h11 * h * slopes[segment + 1];
    return clamp01(y);
  };
}

/**
 * SVG 路径（`viewBox` 空间 0..`width` × 0..`height`，**y 轴向下**）。
 *
 * 采样 `steps` 段：曲线是低频的，128 段在 200px 宽的方块里已经看不出折线。
 */
export function curvePath(
  points: readonly CurvePoint[],
  width: number,
  height: number,
  steps = 128,
): string {
  const evaluate = curveFunction(points);
  const commands: string[] = [];
  for (let index = 0; index <= steps; index += 1) {
    const x = index / steps;
    const y = evaluate(x);
    const px = (x * width).toFixed(2);
    // y 轴翻转：曲线上的 1 在方块顶部
    const py = ((1 - y) * height).toFixed(2);
    commands.push(`${index === 0 ? "M" : "L"}${px} ${py}`);
  }
  return commands.join(" ");
}

/**
 * 在曲线**附近**找最近的控制点（点击命中用）。
 *
 * `tolerance` 是归一化距离（方块边长按 1 算）。找不到返回 `-1`。
 */
export function nearestPoint(
  points: readonly CurvePoint[],
  x: number,
  y: number,
  tolerance: number,
): number {
  let best = -1;
  let bestDistance = tolerance;
  points.forEach((point, index) => {
    const distance = Math.hypot(point[0] - x, point[1] - y);
    if (distance <= bestDistance) {
      best = index;
      bestDistance = distance;
    }
  });
  return best;
}

/**
 * 一个点能不能落在 `x` 处（不越过左右邻居 —— 曲线必须还是函数）。
 *
 * 与 Rust 的 `with_moved_point` 同一判据（那边越界会返回 `Err`，这边先问一句，
 * 拖到邻居旁边时就**停住**而不是报错）。
 */
export function canPlaceAt(
  points: readonly CurvePoint[],
  index: number,
  x: number,
): boolean {
  const previous = points[index - 1];
  if (previous !== undefined && x <= previous[0] + MIN_X_GAP) return false;
  const next = points[index + 1];
  if (next !== undefined && x >= next[0] - MIN_X_GAP) return false;
  return true;
}

/**
 * 把某个控制点拖到 `(x, y)`（**越界就停在边界上**，不报错 —— 拖动要跟手）。
 *
 * 返回新的控制点数组（原数组不动）。
 */
export function movePoint(
  points: readonly CurvePoint[],
  index: number,
  x: number,
  y: number,
): CurvePoint[] {
  const next = points.map((point) => [point[0], point[1]] as CurvePoint);
  if (index < 0 || index >= next.length) return next;
  const previous = next[index - 1];
  const after = next[index + 1];
  let targetX = clamp01(x);
  if (previous !== undefined && targetX < previous[0] + MIN_X_GAP) {
    targetX = previous[0] + MIN_X_GAP;
  }
  if (after !== undefined && targetX > after[0] - MIN_X_GAP) {
    targetX = after[0] - MIN_X_GAP;
  }
  next[index] = [clamp01(targetX), clamp01(y)];
  return next;
}

/** 在 `(x, y)` 处插一个控制点（x 与已有点太近就返回原数组）。 */
export function addPoint(
  points: readonly CurvePoint[],
  x: number,
  y: number,
): CurvePoint[] {
  const targetX = clamp01(x);
  if (points.some((point) => Math.abs(point[0] - targetX) < MIN_X_GAP)) {
    return [...points];
  }
  return normalize([...points, [targetX, clamp01(y)]]);
}

/** 删一个控制点（首尾不许删 —— 它们是黑场 / 白场）。 */
export function removePoint(
  points: readonly CurvePoint[],
  index: number,
): CurvePoint[] {
  if (index <= 0 || index + 1 >= points.length) return [...points];
  return points.filter((_, at) => at !== index);
}

/* ══════════════════════════════════════════════════════════════
 * 双击（删点）的判定 —— 纯逻辑抽出来是为了能单测
 * ══════════════════════════════════════════════════════════════
 *
 * # 为什么不能靠浏览器的 `dblclick` 事件
 *
 * 指针**按下**就已经在加点/抓点了（这是曲线的正常交互），等 `dblclick` 到来时
 * 第二次按下已经被上面两条吃掉了 —— 净效果是「点被加出来又删掉/或删错了那个」，
 * 用户看到的就是「双击没用」（2026-09-24 人类报的）。
 *
 * 所以下列判定在**第二次 `pointerdown`** 上做：够近、够快、且**第一下没拖动**
 * （拖一下再回来点一下不是双击，是两次独立操作）——命中就按双击处理，
 * 把这次按下吃掉，不再加点/抓点。
 */

/** 一次按下的指纹（记录「第一下」用）。 */
export interface ClickStamp {
  x: number;
  y: number;
  time: number;
  /** 按下之后**拖动过** —— 拖动不是双击的前半段 */
  moved: boolean;
}

/** 两次按下的最大间隔（ms；浏览器双击的典型判定是 500，取保守值）。 */
export const DOUBLE_CLICK_WINDOW_MS = 400;

/** 两次按下「同一点」的最大距离（归一化坐标；与命中控制点的容差同量级）。 */
export const DOUBLE_CLICK_TOLERANCE = 0.055;

/** 这一次按下是不是双击的第二次（`previous` 为 `null` = 没有「第一下」）。 */
export function isDoubleClick(
  previous: ClickStamp | null,
  x: number,
  y: number,
  time: number,
): boolean {
  if (previous === null || previous.moved) return false;
  if (time - previous.time > DOUBLE_CLICK_WINDOW_MS) return false;
  if (Number.isNaN(x) || Number.isNaN(y) || Number.isNaN(time)) return false;
  return Math.hypot(x - previous.x, y - previous.y) <= DOUBLE_CLICK_TOLERANCE;
}

/** 一条曲线在 `steps` 个采样点上的值（测试向量用）。 */
export function sampleCurve(
  points: readonly CurvePoint[],
  steps: number,
): number[] {
  const evaluate = curveFunction(points);
  const out: number[] = [];
  for (let index = 0; index <= steps; index += 1) {
    out.push(evaluate(index / steps));
  }
  return out;
}
