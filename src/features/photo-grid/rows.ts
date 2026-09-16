/**
 * 照片网格的行模型（纯函数；`DESIGN.md` §12.6 的 Tile 流 + §12.7 的按时间分组）。
 *
 * 两种模式共用同一份行数组：
 *
 * ```text
 * 平铺模式：  [ tiles: a b c d ] [ tiles: e f g h ]        ← 每行等宽等高的 tile
 * 按时间模式：[ day: 2026-08-15 ]
 *            [ slice: 09:12–09:47 ]
 *            [ tiles: a b c d ]
 *            [ slice: 13:02–13:20 ]
 *            [ tiles: e f g h ]
 *            [ day: 2026-08-14 ] ...                        ← 最近的在前
 * ```
 *
 * 行高都在这里算好（tile 行 = 画面高 + 字幕条高），视图只按 `row.height` 渲染 ——
 * 虚拟化需要像素数，而「高度的唯一来源」只能有一个，否则模型与视图会各算各的。
 *
 * ⚠️ 除了 tile 行，其它行都是**分组标题行**（日 / 时间片）。它们也参与虚拟化，
 * 所以同样是行 —— 这也是不能用「固定行高相乘」那种简化做法的原因。
 */

import type { SourceItem } from "../../api/types.ts";
import { tileImageHeight } from "../../lib/tile-flow.ts";
import type { TimeGrouping } from "../../lib/time-group.ts";

/** 照片在网格里的 id：直接用路径（唯一、稳定、与后端对齐） */
export function itemId(item: SourceItem): string {
  return item.path;
}

/** 一行 tile（不跨视觉组） */
export interface TileRowModel {
  kind: "tiles";
  key: string;
  height: number;
  items: readonly SourceItem[];
}

/** 分组标题行（日 / 时间片 / 未知时间） */
export interface GroupRowModel {
  kind: "group";
  key: string;
  level: "day" | "slice";
  height: number;
  /** 这一组包含的照片 id（「全选当天」「全选此段」用） */
  photoIds: readonly string[];
  /** 日组 id（`YYYY-MM-DD`，或 `unknown`） */
  dayId: string;
  /** 片内首尾时间（视图据此显示时间范围）；未知时间组是 `null` */
  startMs: number | null;
  endMs: number | null;
  /** 显示时间范围用的偏移（分钟）；`null` = 不显示时间 */
  offsetMinutes: number | null;
  /** 这一组里有多少张（视图显示计数） */
  count: number;
  /** 是否是「未知时间」那一组 */
  unknown: boolean;
}

export type GridRowModel = TileRowModel | GroupRowModel;

/*
 * 分组标题的排版（`DESIGN.md` §12.7）：**没有横线、没有色块，全靠留白与大小的层次**。
 *
 * `rowHeight = contentHeight + 上方留白` —— 留白只加在上面，标题就贴着自己这一组、
 * 与上一组拉开距离。**单一事实源**：渲染方从这两个对象里同时取行高与留白，
 * 不另写一套 `pt-*`（两处各写会悄悄漂开，标题就偏了）。
 *
 * 数值来历（2026-09-16 人类截图反馈「按时间样式下排版过于紧凑，叫人怎么区分」）：
 * 原来日组 34、时间片 26 且都居中，看起来就是两行小字。现在：
 *   * 日标题 14px **semibold** + `fg-1` + 日历图标（设计稿 `DayHeader`）—— 一级标题；
 *   * 时间片 13px `fg-2` —— 二级标题；
 *   * 上方留白 20 / 10，与字号一起表达层级。
 */
export const DAY_HEADER = { rowHeight: 46, contentHeight: 26 } as const;
export const SLICE_HEADER = { rowHeight: 32, contentHeight: 22 } as const;

export interface GridRowsInput {
  items: readonly SourceItem[];
  /** 一行放几个（由 `computeTileFlow` 给出） */
  columns: number;
  /** 单元格宽（当前档位） */
  cellWidth: number;
  /** 字幕条高（读 `--caption-h` 令牌） */
  captionHeight: number;
  /**
   * 按时间分组的结果；**省略 = 平铺模式**。
   * 由调用方用 `groupByTime` 算好传进来（本函数保持纯函数，不自己算分组）。
   */
  grouping?: TimeGrouping;
}

/** 构建行数组。空列表 → 空数组（视图显示空态）。 */
export function buildGridRows(input: GridRowsInput): GridRowModel[] {
  const columns = Number.isFinite(input.columns)
    ? Math.max(1, Math.floor(input.columns))
    : 1;
  const captionHeight =
    Number.isFinite(input.captionHeight) && input.captionHeight > 0
      ? input.captionHeight
      : 0;
  const rowHeight = tileImageHeight(input.cellWidth) + captionHeight;

  if (!input.grouping) {
    return chunkTiles(input.items, columns, rowHeight, "tiles");
  }

  const byId = new Map(input.items.map((item) => [itemId(item), item]));
  const rows: GridRowModel[] = [];

  for (const day of input.grouping.days) {
    rows.push({
      kind: "group",
      key: `day:${day.id}`,
      level: "day",
      height: DAY_HEADER.rowHeight,
      photoIds: day.photoIds,
      dayId: day.id,
      startMs: day.slices.length > 0 ? day.slices[0].startMs : null,
      endMs:
        day.slices.length > 0
          ? day.slices[day.slices.length - 1].endMs
          : null,
      offsetMinutes: day.slices.length > 0 ? day.slices[0].offsetMinutes : null,
      count: day.photoIds.length,
      unknown: false,
    });

    for (const [index, slice] of day.slices.entries()) {
      rows.push({
        kind: "group",
        key: `slice:${slice.id}`,
        level: "slice",
        height: SLICE_HEADER.rowHeight,
        photoIds: slice.photoIds,
        dayId: day.id,
        startMs: slice.startMs,
        endMs: slice.endMs,
        offsetMinutes: slice.offsetMinutes,
        count: slice.photoIds.length,
        unknown: false,
      });
      const sliceItems = slice.photoIds
        .map((id) => byId.get(id))
        .filter((item): item is SourceItem => item !== undefined);
      rows.push(
        ...chunkTiles(sliceItems, columns, rowHeight, `${slice.id}#${index}`),
      );
    }
  }

  if (input.grouping.unknown) {
    const { photoIds } = input.grouping.unknown;
    rows.push({
      kind: "group",
      key: "day:unknown",
      level: "day",
      height: DAY_HEADER.rowHeight,
      photoIds,
      dayId: "unknown",
      startMs: null,
      endMs: null,
      offsetMinutes: null,
      count: photoIds.length,
      unknown: true,
    });
    const items = photoIds
      .map((id) => byId.get(id))
      .filter((item): item is SourceItem => item !== undefined);
    rows.push(...chunkTiles(items, columns, rowHeight, "unknown"));
  }

  return rows;
}

/** 把一串照片切成等高的 tile 行 */
function chunkTiles(
  items: readonly SourceItem[],
  columns: number,
  height: number,
  keyPrefix: string,
): TileRowModel[] {
  const rows: TileRowModel[] = [];
  for (let start = 0; start < items.length; start += columns) {
    const slice = items.slice(start, start + columns);
    rows.push({
      kind: "tiles",
      key: `${keyPrefix}@${start / columns}`,
      height,
      items: slice,
    });
  }
  return rows;
}

/** 行数组里一共有几张照片（统计「N 张」时用，比 `items.length` 更权威） */
export function countRowPhotos(rows: readonly GridRowModel[]): number {
  return rows.reduce(
    (total, row) => (row.kind === "tiles" ? total + row.items.length : total),
    0,
  );
}
