/**
 * 虚拟化的**行窗口**计算（`DESIGN.md` §12.6 的 Tile 流 + 分组标题）。
 *
 * 为什么不用「固定行高 + 简单乘除」：照片网格在**按时间分组**模式下，行高是不等的
 * （日组标题行、时间片标题行都比 tile 行矮），而设计里两种模式共用同一套网格。
 * 所以这里按**每行高度**来算窗口，行高相等只是它的一个特例。
 *
 * 算法：前缀和 + 二分找视口边界，再往外扩 `overscan` 行。
 * 复杂度 O(n)（前缀和）—— n 是**行数**不是照片数（5000 张照片 ≈ 800 行），
 * 每次滚动重算一次完全够用；将来真到十万行再上缓存。
 *
 * 返回的 `paddingTop` / `paddingBottom` 让调用方只渲染可见的那几行，
 * 同时保持滚动条长度正确（总高不变）。
 */

/** 一行（只要高度参与计算） */
export interface VirtualRowLike {
  height: number;
}

export interface VirtualWindowInput {
  /** 所有行（顺序即显示顺序） */
  rows: readonly VirtualRowLike[];
  /** 视口高（px）。`<= 0`（还没量到）时返回空窗口 */
  viewportHeight: number;
  /** 滚动位置（px）；负数 / NaN / 超出范围都会被夹到合法区间 */
  scrollTop: number;
  /** 上下各多渲染几行（默认 2）—— 滚动时不至于看到空白 */
  overscan?: number;
}

export interface VirtualWindow {
  /** 第一行下标（含） */
  startIndex: number;
  /** 最后一行下标（**不含**） */
  endIndex: number;
  /** 上方要垫多高 */
  paddingTop: number;
  /** 下方要垫多高 */
  paddingBottom: number;
  /** 内容总高（滚动容器用它撑出滚动条） */
  totalHeight: number;
}

export const EMPTY_WINDOW: VirtualWindow = {
  startIndex: 0,
  endIndex: 0,
  paddingTop: 0,
  paddingBottom: 0,
  totalHeight: 0,
};

/** 行高规范化：非法值当 0（宁可少算一行，不要产出 NaN 把布局带崩） */
function saneHeight(value: number): number {
  return Number.isFinite(value) && value > 0 ? value : 0;
}

function saneScrollTop(value: number, max: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(Math.max(0, value), Math.max(0, max));
}

function saneOverscan(value: number | undefined): number {
  if (value === undefined) return 2;
  if (!Number.isFinite(value) || value <= 0) return 0;
  return Math.floor(value);
}

export function computeVirtualWindow(input: VirtualWindowInput): VirtualWindow {
  const rows = input.rows;
  if (rows.length === 0) return EMPTY_WINDOW;

  // 前缀和：offsets[i] = 第 i 行**之前**的总高；最后一项是总高
  const offsets = new Array<number>(rows.length + 1);
  offsets[0] = 0;
  for (let i = 0; i < rows.length; i += 1) {
    offsets[i + 1] = offsets[i] + saneHeight(rows[i].height);
  }
  const totalHeight = offsets[rows.length];
  const viewport = Number.isFinite(input.viewportHeight)
    ? Math.max(0, input.viewportHeight)
    : 0;
  if (viewport <= 0 || totalHeight <= 0) {
    return { ...EMPTY_WINDOW, totalHeight };
  }

  const scrollTop = saneScrollTop(input.scrollTop, totalHeight - viewport);
  const viewBottom = scrollTop + viewport;

  // 第一个「下边界在视口之上」的行 —— 二分，避免大列表里线性扫
  let lo = 0;
  let hi = rows.length - 1;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (offsets[mid + 1] <= scrollTop) lo = mid + 1;
    else hi = mid;
  }
  const visibleStart = lo;

  // 最后一个「上边界在视口之下」的行
  lo = visibleStart;
  hi = rows.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (offsets[mid] < viewBottom) lo = mid;
    else hi = mid - 1;
  }
  const visibleEnd = lo + 1; // 转成「不含」

  const overscan = saneOverscan(input.overscan);
  const startIndex = Math.max(0, visibleStart - overscan);
  const endIndex = Math.min(rows.length, visibleEnd + overscan);

  return {
    startIndex,
    endIndex,
    paddingTop: offsets[startIndex],
    paddingBottom: totalHeight - offsets[endIndex],
    totalHeight,
  };
}

/**
 * 行高相等时的快捷判断（调用方用得上：等行高时可以直接乘除算位置，
 * 不必走前缀和）。
 */
/**
 * 「把第 `target` 行滚进视野」需要的新 `scrollTop`（纯函数，好测）。
 *
 * 键盘导航要的就是这件事：`←`/`→` 换了「当前那张」之后，网格得跟上 ——
 * 否则焦点在屏幕外飘，用户以为按键没反应。
 *
 * 三种情况：
 * * 行整个在视野**上方** → 顶对齐（`top`）；
 * * 行整个在视野**下方** → 底对齐（`top + height - viewport`）；
 * * 已经看得见 → **原样返回**（一条像素都不动：不然每按一次方向键画面都抖一下）。
 *
 * 行高与视口高都按 `computeVirtualWindow` 的同一套规范化（非法值当 0）——
 * 两处口径不一致的话会出现「滚了但没滚到位」。
 *
 * （2026-09-19：本函数上线时类型检查器一度报「没有这个导出」——陈旧快照，
 * 判据与配方见 `ASSISTANCE.md` §二第 3 条；`pnpm test` 里那 5 条就是它的靶子。）
 */
export function rowScrollTop(input: {
  rows: readonly VirtualRowLike[];
  /** 目标行下标；越界时原样返回当前滚动位置 */
  target: number;
  scrollTop: number;
  viewportHeight: number;
}): number {
  const rows = input.rows;
  const viewport = Number.isFinite(input.viewportHeight)
    ? Math.max(0, input.viewportHeight)
    : 0;
  const current = Number.isFinite(input.scrollTop) ? Math.max(0, input.scrollTop) : 0;
  if (
    rows.length === 0 ||
    viewport <= 0 ||
    !Number.isInteger(input.target) ||
    input.target < 0 ||
    input.target >= rows.length
  ) {
    return current;
  }

  let top = 0;
  for (let i = 0; i < input.target; i += 1) top += saneHeight(rows[i].height);
  const height = saneHeight(rows[input.target].height);
  const bottom = top + height;

  if (top < current) return top;
  if (bottom > current + viewport) return Math.max(0, bottom - viewport);
  return current;
}

export function isUniformHeight(rows: readonly VirtualRowLike[]): boolean {
  if (rows.length < 2) return true;
  const first = saneHeight(rows[0].height);
  return rows.every((row) => saneHeight(row.height) === first);
}
