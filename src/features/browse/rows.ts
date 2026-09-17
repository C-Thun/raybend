/**
 * 浏览网格的行模型。
 *
 * 与导入网格（`features/photo-grid/rows.ts`）差在**数据是稀疏的**：
 * 浏览的数据按页从库里取，`(AssetItem | null)[]` 里可能有洞。所以行只记**下标区间**，
 * 具体内容由视图在渲染时用 `store.itemAt(index)` 取（取到 `null` 就是占位 tile）。
 *
 * 分组的口径与导入网格**完全一致**（`lib/time-group.ts`）：
 * 同一天内相邻两张的间隔超过阈值（默认 1 小时）就断成新的一片。
 * 这里没有直接复用 `groupByTime`，因为它返回的是「按天倒序 + 片内升序」的树，
 * 而网格要的是「按当前列表顺序的下标区间」；`rows.test.ts` 里有一条**交叉校验**
 * 用同一个输入比对两者的边界 —— 口径一旦漂了测试就红。
 */

import { compareNatural } from "../../lib/natural-order.ts";
import { localOffsetMinutes } from "../../lib/time-group.ts";

/** 一天有多少毫秒。 */
const MS_PER_DAY = 86_400_000;

/** 默认断片阈值（分钟）——与导入网格同口径。 */
export const DEFAULT_GAP_MINUTES = 60;

/** 一行照片（区间 `[start, start + count)`）。 */
export interface BrowseTileRow {
  kind: "tiles";
  key: string;
  height: number;
  /** 起始下标（全列表）。 */
  start: number;
  /** 这一行几张（最后一行可能不满）。 */
  count: number;
}

/** 一行分组标题。 */
export interface BrowseGroupRow {
  kind: "group";
  key: string;
  height: number;
  /**
   * 标题文字。**「未知时间」那一组的 `label` 是空串** ——
   * 它不是数据而是**文案**，得由视图按当前语言渲染（见 `unknown`）。
   */
  label: string;
  /** 这一组是不是「未知时间」那一组（视图据此换成 `t("grid.unknown_time")`） */
  unknown: boolean;
  /** 这一组一共几张（标题右侧显示）。 */
  count: number;
  /** 这一组的起始下标。 */
  start: number;
}

export type BrowseRowModel = BrowseTileRow | BrowseGroupRow;

/** 时间分组的边界：从 `start` 开始连续 `count` 张属于同一片。 */
export interface BrowseGroupBoundary {
  start: number;
  count: number;
  /** 日键（`YYYY-MM-DD`）；「未知时间」组是空串 */
  label: string;
  /** 「未知时间」组（没有拍摄时间的那些） */
  unknown: boolean;
}

/** 分组标题行的高度（px）——比 tile 行矮得多，只是个带文字的间隔。 */
export const GROUP_ROW_HEIGHT = 28;

/**
 * 依据**当前列表顺序**的时间线算出分组边界。
 *
 * `ordered` 必须与网格的显示顺序一致（默认是拍摄时间倒序）。
 * 规则：
 * * 拍摄时间缺失的一律归到「未知时间」一组，且只可能是最后一组；
 * * 跨天必断；
 * * 同一天内，相邻两张间隔 > `gapMinutes` 也断。
 */
export function browseGroups(
  ordered: readonly { takenAt: number | null; takenAtOffsetMin?: number | null }[],
  gapMinutes: number = DEFAULT_GAP_MINUTES,
): BrowseGroupBoundary[] {
  const gapMs =
    Number.isFinite(gapMinutes) && gapMinutes > 0
      ? gapMinutes * 60_000
      : Number.POSITIVE_INFINITY;

  const groups: BrowseGroupBoundary[] = [];
  let currentDay: string | null = null;
  let previousMs: number | null = null;

  for (let index = 0; index < ordered.length; index += 1) {
    const entry = ordered[index];
    const ms = entry?.takenAt ?? null;
    if (ms === null || !Number.isFinite(ms)) {
      // 未知时间：整段归一组（排序把没有时间的都排在最后）
      const last = groups[groups.length - 1];
      if (last?.unknown === true) {
        last.count += 1;
      } else {
        groups.push({ start: index, count: 1, label: "", unknown: true });
      }
      previousMs = null;
      currentDay = null;
      continue;
    }

    const offset = entry?.takenAtOffsetMin ?? localOffsetMinutes(ms);
    const day = dayKey(ms, offset);
    const gapBroken = previousMs !== null && Math.abs(previousMs - ms) > gapMs;
    const dayBroken = day !== currentDay;

    if (groups.length === 0 || dayBroken || gapBroken) {
      groups.push({ start: index, count: 1, label: day, unknown: false });
    } else {
      groups[groups.length - 1].count += 1;
    }
    currentDay = day;
    previousMs = ms;
  }

  return groups;
}

/**
 * 按当地时间算这一天的键（`YYYY-MM-DD`）。
 *
 * 与 `lib/time-group.ts` 同一套算法：**先加偏移、再取 UTC 字段** —— 这样
 * 「相机上的数字」才是分组依据，而不是本机时区的日期。
 */
function dayKey(ms: number, offsetMinutes: number): string {
  const shifted = new Date(ms + offsetMinutes * 60_000);
  const year = shifted.getUTCFullYear();
  const month = String(shifted.getUTCMonth() + 1).padStart(2, "0");
  const day = String(shifted.getUTCDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

/** 两个时间戳是不是同一天（按给定偏移看）。 */
export function sameDay(a: number, b: number, offsetMinutes: number): boolean {
  const startOf = (ms: number): number =>
    Math.floor((ms + offsetMinutes * 60_000) / MS_PER_DAY);
  return startOf(a) === startOf(b);
}

export interface BuildBrowseRowsInput {
  /** 筛选后的总数（**不是已加载的数量**）。 */
  total: number;
  /** 一行放几个（由 `computeTileFlow` 给出）。 */
  columns: number;
  /** 正方外框边长（读档位；信息条是覆盖层，不占高度）。 */
  cellSize: number;
  /** 分组边界；不给 = 平铺模式。 */
  groups?: readonly BrowseGroupBoundary[];
  /** 行间距（由密度档位决定）——与导入网格同口径。 */
  gap?: number;
}

/** 构建行数组。空列表 → 空数组（视图显示空态）。 */
export function buildBrowseRows(input: BuildBrowseRowsInput): BrowseRowModel[] {
  const columns = Number.isFinite(input.columns)
    ? Math.max(1, Math.floor(input.columns))
    : 1;
  const total = Number.isFinite(input.total) ? Math.max(0, Math.floor(input.total)) : 0;
  if (total === 0) return [];

  const gap =
    input.gap !== undefined && Number.isFinite(input.gap)
      ? Math.max(0, input.gap)
      : 0;
  const tileRowHeight = input.cellSize + gap;
  const rows: BrowseRowModel[] = [];

  const pushTiles = (start: number, end: number, groupKey: string): void => {
    for (let at = start; at < end; at += columns) {
      const count = Math.min(columns, end - at);
      rows.push({
        kind: "tiles",
        key: `${groupKey}:${at}`,
        height: tileRowHeight,
        start: at,
        count,
      });
    }
  };

  if (!input.groups || input.groups.length === 0) {
    pushTiles(0, total, "all");
    return rows;
  }

  for (const group of input.groups) {
    const start = Math.max(0, Math.min(group.start, total));
    const end = Math.max(start, Math.min(group.start + group.count, total));
    if (end <= start) continue;
    // 未知时间组的 label 是空串 —— 键里用 `unknown` 占位，否则多组会撞键
    const groupKey = group.unknown ? "unknown" : group.label;
    rows.push({
      kind: "group",
      key: `g:${groupKey}:${start}`,
      height: GROUP_ROW_HEIGHT,
      label: group.label,
      unknown: group.unknown,
      count: end - start,
      start,
    });
    pushTiles(start, end, `g:${groupKey}:${start}`);
  }
  return rows;
}

/**
 * 片内按**文件名自然序**排的显示顺序：返回「显示位次 → 原下标」的映射。
 *
 * 为什么要它（人类 2026-09-17 定）：同一时间段内，一张照片的 JPG 与 RAW 可能因为
 * 拍摄时间来源不同（EXIF / 文件名兜底 / mtime）差上几秒，于是按时间排就“按格式分了层”——
 * 而人期望的是「一个时间段内就按文件名自然排」：`P1000019.JPG` 与 `P1000019.RW2` 挨着。
 *
 * 两条边界，别搞混：
 * * **片的顺序与片的边界完全不动**（那是时间的事，见 {@link browseGroups}）；
 * * 只在**每一片内部**重排 → 每片仍然是连续区间，分组不会被破坏。
 *
 * 返回的是置换（`0..n-1` 的一个排列），直接交给 `store.setDisplayOrder`。
 */
export function sliceOrder(
  timeline: readonly { relPath?: string }[],
  groups: readonly BrowseGroupBoundary[],
): number[] {
  const order: number[] = new Array(timeline.length);

  for (const group of groups) {
    const start = Math.max(0, Math.min(group.start, timeline.length));
    const end = Math.max(start, Math.min(group.start + group.count, timeline.length));
    if (end <= start) continue;

    const indices: number[] = [];
    for (let at = start; at < end; at += 1) indices.push(at);
    // 稳定排序：比较器在“数值相等”时还有原始串兜底，而相同比较结果保持原顺序
    indices.sort((a, b) =>
      compareNatural(timeline[a]?.relPath ?? "", timeline[b]?.relPath ?? ""),
    );
    for (let at = start; at < end; at += 1) {
      order[at] = indices[at - start] as number;
    }
  }

  // 不在任何组里的下标（正常不该有）保持原位，免得置换里出现空洞
  for (let at = 0; at < order.length; at += 1) {
    if (order[at] === undefined) order[at] = at;
  }
  return order;
}
