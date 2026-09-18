/**
 * 分段布局的尺寸数学（`components/ui/SplitStack.tsx` 的唯一算法来源，有单测）。
 *
 * 模型：各段**比例**（`sizes` 之和恒为 1）。拖动发生在**相邻两段之间的边界**上 ——
 * 一段长多少，邻段就短多少，总和守恒（这正是用户对手感的要求：拖到哪儿停在哪儿，
 * 不会像 Ark 那样「量了又写、写了又量」）。
 */

/** 把「像素」换算成比例时需要容器主轴尺寸；容器没量到时调用方自己跳过这次拖动 */
export interface BoundaryMove {
  /** 当前各段比例（长度 = 段数，和约为 1） */
  sizes: readonly number[];
  /** 边界的下标：它是 `sizes[index]` 与 `sizes[index + 1]` 之间的那条把手 */
  index: number;
  /** 拖动位移换算成比例后的值（正 = 前一段变长） */
  delta: number;
  /** 各段的比例下限（像素下限 ÷ 容器主轴尺寸）；缺省 0 */
  minRatios?: readonly number[];
}

/**
 * 拖动一条边界：把 `delta` 从后一段挪到前一段。
 *
 * 夹取规则：两段都不许低于各自的下限。若下限之和已经超过两段总和
 * （容器太矮/太窄，塞不下两个下限），则**不移动** —— 保持原状比给出错乱的比例好。
 */
export function moveBoundary({ sizes, index, delta, minRatios }: BoundaryMove): number[] {
  const next = [...sizes];
  if (index < 0 || index + 1 >= sizes.length) return next;

  const first = sizes[index] as number;
  const second = sizes[index + 1] as number;
  const pair = first + second;
  const minFirst = Math.max(0, minRatios?.[index] ?? 0);
  const minSecond = Math.max(0, minRatios?.[index + 1] ?? 0);

  // 下限之和已经塞不下这一对 → 整体就没得挪
  if (minFirst + minSecond >= pair) return next;

  const lower = minFirst;
  const upper = pair - minSecond;
  const moved = Math.min(upper, Math.max(lower, first + delta));
  next[index] = moved;
  next[index + 1] = pair - moved;
  return next;
}

/**
 * 把各段的比例换算成**百分比**（`SplitStack` 的 `onResizeEnd` 口径）。
 * 保留小数：调用方可能按比例持久化（例如「最近列表占 32%」）。
 */
export function toPercents(sizes: readonly number[]): number[] {
  return sizes.map((size) => size * 100);
}

/**
 * 解析段尺寸写法：数字 = **百分比**，带单位的字符串 = 像素。
 *
 * 为什么必须区分：Ark 时代「`120`」与「`"120px"`」含义完全不同，
 * 写错会让布局直接畸变（历史上真实发生过）。这里保留同一个口径，避免调用方迁移时踩坑。
 */
export function parseSize(
  value: number | string,
  mainAxis: number,
): { ratio: number; pixels: number } {
  if (typeof value === "number") {
    const ratio = value / 100;
    return { ratio, pixels: ratio * mainAxis };
  }
  const pixels = Number.parseFloat(value);
  if (!Number.isFinite(pixels) || mainAxis <= 0) return { ratio: 0, pixels: 0 };
  return { ratio: pixels / mainAxis, pixels };
}

/** 各段的像素下限 → 比例下限（容器主轴尺寸为 0 时返回全 0，避免除零） */
export function minRatiosFrom(
  segments: readonly (number | string | undefined)[],
  mainAxis: number,
): number[] {
  return segments.map((value) => {
    if (value === undefined || mainAxis <= 0) return 0;
    return parseSize(value, mainAxis).ratio;
  });
}
