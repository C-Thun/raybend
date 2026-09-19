/**
 * tiles 网格的**行模型**（纯函数；`DESIGN.md` §12.6 的 Tile 流 + §12.7 的按时间分组）。
 *
 * ## 两侧唯一一份
 *
 * 行里放的是**显示序下标**（`slots`），不是照片对象 —— 渲染时问数据源要
 * （`source.itemAt(slot)`）。于是两侧的差异全部留在**数据层**：
 *
 * * 导入：整目录一次性拿到，`itemAt(i)` 就是数组第 i 个；
 * * 浏览：按页取（未取回的格子是**洞**）、显示序还被「片内按文件名自然序」**置换**过。
 *
 * 那些都在数据源里做掉，行模型既不知道也不关心 —— 这份文件不许出现
 * 「分页 / 置换 / 洞」这类概念，那是数据层的词。
 *
 * ```text
 * 平铺：    [ tiles: 0 1 2 3 ] [ tiles: 4 5 6 7 ]
 * 按时间：  [ day: 2026-08-15 ] [ slice: 09:12–09:47 ] [ tiles: 0 1 2 3 ] ...
 * ```
 *
 * 行高都在这里算好（tile 行 = 画面高 + 字幕条高），视图只按 `row.height` 渲染 ——
 * 虚拟化需要像素数，「高度的唯一来源」只能有一个。
 */

import { tileRowHeight } from "../../../lib/tile-flow.ts";
import type { TimeGrouping } from "../../../lib/time-group.ts";

/** 一行 tile（不跨视觉组）；`slots` 是**显示序下标** */
export interface TileRowModel {
  kind: "tiles";
  key: string;
  height: number;
  slots: readonly number[];
}

/** 分组标题行（日 / 时间片 / 未知时间） */
export interface GroupRowModel {
  kind: "group";
  key: string;
  level: "day" | "slice";
  height: number;
  /** 覆盖的**显示序区间** `[start, start + count)` */
  start: number;
  count: number;
  dayId: string;
  startMs: number | null;
  endMs: number | null;
  offsetMinutes: number | null;
  unknown: boolean;
}

export type GridRowModel = TileRowModel | GroupRowModel;

/*
 * 分组标题的排版（`DESIGN.md` §12.7）：没有横线、没有色块，靠留白与大小分层。
 * `rowHeight = contentHeight + 上方留白` —— 留白只加在上面，标题贴着自己这一组。
 * **单一事实源**：渲染方从这两个对象里同时取行高与留白，不另写一套 `pt-*`。
 */
export const DAY_HEADER = { rowHeight: 46, contentHeight: 26 } as const;
export const SLICE_HEADER = { rowHeight: 32, contentHeight: 22 } as const;

/**
 * 一个**时间片**在显示序里的位置与它所属的日。
 *
 * 两侧各自算好这个数组（导入：从 `store.grouping()` 转；浏览：从时间线 + 片内排序算），
 * 行模型只认它 —— 于是「怎么分组」只有 `lib/time-group.ts` 一份规则，
 * 「怎么排成行」只有这里一份规则。
 */
export interface RowSlice {
  start: number;
  count: number;
  /** 日键（`YYYY-MM-DD`）或 `"unknown"` */
  dayId: string;
  dayStart: number;
  dayCount: number;
  startMs: number | null;
  endMs: number | null;
  offsetMinutes: number | null;
  unknown: boolean;
  /** 片 id（行 key 用；同一天里第几片） */
  id: string;
}

export interface GridRowsInput {
  /** 显示序下一共有几格 */
  count: number;
  /** 一行放几个（由 `computeTileFlow` 给出） */
  columns: number;
  /** 单元格边长（当前档位） */
  cellSize: number;
  /** 分组；**省略 = 平铺模式** */
  slices?: readonly RowSlice[];
}

/** 构建行数组。空列表 → 空数组（视图显示空态）。 */
export function buildGridRows(input: GridRowsInput): GridRowModel[] {
  const columns = Number.isFinite(input.columns) ? Math.max(1, Math.floor(input.columns)) : 1;
  const rowHeight = tileRowHeight(input.cellSize);

  if (input.slices === undefined) {
    return chunkTiles(input.count, 0, columns, rowHeight, "tiles");
  }

  const rows: GridRowModel[] = [];
  let lastDay: string | null = null;
  for (const slice of input.slices) {
    if (slice.count <= 0) continue;
    if (slice.dayId !== lastDay) {
      lastDay = slice.dayId;
      rows.push({
        kind: "group",
        key: `day:${slice.dayId}`,
        level: "day",
        height: DAY_HEADER.rowHeight,
        start: slice.dayStart,
        count: slice.dayCount,
        dayId: slice.dayId,
        startMs: slice.startMs,
        endMs: slice.endMs,
        offsetMinutes: slice.offsetMinutes,
        unknown: slice.unknown,
      });
    }
    if (!slice.unknown) {
      rows.push({
        kind: "group",
        key: `slice:${slice.dayId}:${slice.id}`,
        level: "slice",
        height: SLICE_HEADER.rowHeight,
        start: slice.start,
        count: slice.count,
        dayId: slice.dayId,
        startMs: slice.startMs,
        endMs: slice.endMs,
        offsetMinutes: slice.offsetMinutes,
        unknown: false,
      });
    }
    rows.push(...chunkTiles(slice.count, slice.start, columns, rowHeight, slice.id));
  }
  return rows;
}

/** 显示序上连续的一段 → 等高的 tile 行 */
function chunkTiles(
  count: number,
  from: number,
  columns: number,
  height: number,
  keyPrefix: string,
): TileRowModel[] {
  const rows: TileRowModel[] = [];
  for (let at = 0; at < count; at += columns) {
    const slots: number[] = [];
    for (let index = at; index < Math.min(at + columns, count); index += 1) slots.push(from + index);
    rows.push({ kind: "tiles", key: `${keyPrefix}@${at / columns}`, height, slots });
  }
  return rows;
}

/** 行数组里一共有几格 */
export function countRowPhotos(rows: readonly GridRowModel[]): number {
  return rows.reduce((total, row) => (row.kind === "tiles" ? total + row.slots.length : total), 0);
}

/**
 * 把 `lib/time-group.ts` 的分组结果转成**行模型要的片**。
 *
 * 只在「显示序 = 数据序」时成立（导入侧就是这样：网格按拿到的顺序铺）。
 * 浏览侧的显示序是置换过的，它自己算（见 `features/browse/grid-source.ts`）。
 */
export function groupingToSlices(grouping: TimeGrouping): RowSlice[] {
  const out: RowSlice[] = [];
  let at = 0;
  for (const day of grouping.days) {
    const dayStart = at;
    const dayCount = day.slices.reduce((total, slice) => total + slice.photoIds.length, 0);
    for (const [index, slice] of day.slices.entries()) {
      out.push({
        start: at,
        count: slice.photoIds.length,
        dayId: day.id,
        dayStart,
        dayCount,
        startMs: slice.startMs,
        endMs: slice.endMs,
        offsetMinutes: slice.offsetMinutes,
        unknown: false,
        id: `${day.id}#${index}`,
      });
      at += slice.photoIds.length;
    }
  }
  if (grouping.unknown) {
    const count = grouping.unknown.photoIds.length;
    out.push({
      start: at,
      count,
      dayId: "unknown",
      dayStart: at,
      dayCount: count,
      startMs: null,
      endMs: null,
      offsetMinutes: null,
      unknown: true,
      id: "unknown",
    });
  }
  return out;
}
