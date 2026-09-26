/**
 * tiles 行模型（`components/ui/tiles/rows.ts`）的测试。
 *
 * 行模型是「数据源」与「虚拟滚动」之间的那一步：行切错 → 滚动位置跳、标题跑到别的组里。
 * 两种模式（平铺 / 按时间）都要钉住，尤其是这条**人类 2026-09-20 明确要求**的：
 *
 * > 即使全部图片就是同一天同一时段内，那也必须改变显示样式使之生效。
 *
 * 也就是说：只要开了「按时间」，哪怕只有**一片**，日标题与片标题也要出现 ——
 * 它们本身就是「开关生效」的可见证据，不许被「只有一组就不画标题」那种优化吞掉。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  buildGridRows,
  countRowPhotos,
  DAY_HEADER,
  SLICE_HEADER,
  type GroupRowModel,
  type RowSlice,
  type TileRowModel,
} from "./rows.ts";

const CELL = 200;

function slice(overrides: Partial<RowSlice> = {}): RowSlice {
  return {
    start: 0,
    count: 4,
    dayId: "2026-08-15",
    dayStart: 0,
    dayCount: 4,
    startMs: 1_789_000_000_000,
    endMs: 1_789_000_060_000,
    offsetMinutes: 480,
    unknown: false,
    id: "2026-08-15#0",
    ...overrides,
  };
}

const groups = (rows: readonly { kind: string }[]): GroupRowModel[] =>
  rows.filter((row): row is GroupRowModel => row.kind === "group");
const tiles = (rows: readonly { kind: string }[]): TileRowModel[] =>
  rows.filter((row): row is TileRowModel => row.kind === "tiles");

test("平铺：没有 slices 时按列数切行，槽位是连续下标", () => {
  const rows = buildGridRows({ count: 5, columns: 2, cellSize: CELL });
  assert.equal(rows.length, 3);
  assert.deepEqual(
    tiles(rows).map((row) => [...row.slots]),
    [
      [0, 1],
      [2, 3],
      [4],
    ],
  );
  assert.equal(countRowPhotos(rows), 5);
});

test("按时间：**只有一片**也要出日标题 + 片标题（开关必须看得见地生效）", () => {
  const rows = buildGridRows({
    count: 4,
    columns: 4,
    cellSize: CELL,
    slices: [slice()],
  });
  assert.deepEqual(
    rows.map((row) => (row.kind === "tiles" ? "tiles" : row.level)),
    ["day", "slice", "tiles"],
  );
  const [day, atSlice] = groups(rows);
  assert.equal(day?.height, DAY_HEADER.rowHeight);
  assert.equal(atSlice?.height, SLICE_HEADER.rowHeight);
  assert.equal(countRowPhotos(rows), 4);
});

test("按时间：同一天的两片共用**一个**日标题，片标题各一个", () => {
  const rows = buildGridRows({
    count: 6,
    columns: 3,
    cellSize: CELL,
    slices: [
      slice({ count: 3, dayCount: 6, id: "2026-08-15#0" }),
      slice({ start: 3, count: 3, dayCount: 6, id: "2026-08-15#1" }),
    ],
  });
  assert.deepEqual(
    rows.map((row) => (row.kind === "tiles" ? "tiles" : `${row.level}:${row.count}`)),
    ["day:6", "slice:3", "tiles", "slice:3", "tiles"],
  );
});

test("按时间：未知时间组只有日标题，没有片标题", () => {
  const rows = buildGridRows({
    count: 2,
    columns: 2,
    cellSize: CELL,
    slices: [
      slice({
        count: 2,
        dayCount: 2,
        dayId: "unknown",
        startMs: null,
        endMs: null,
        offsetMinutes: null,
        unknown: true,
        id: "unknown",
      }),
    ],
  });
  assert.deepEqual(
    rows.map((row) => (row.kind === "tiles" ? "tiles" : `${row.level}:${row.unknown}`)),
    ["day:true", "tiles"],
  );
});

test("空片被跳过（不产出只带标题的空组）", () => {
  const rows = buildGridRows({
    count: 1,
    columns: 1,
    cellSize: CELL,
    slices: [slice({ count: 0, dayCount: 1, id: "empty" }), slice({ count: 1, dayCount: 1, id: "full" })],
  });
  assert.deepEqual(
    rows.map((row) => (row.kind === "tiles" ? "tiles" : `${row.level}:${row.start}:${row.count}`)),
    ["day:0:1", "slice:0:1", "tiles"],
  );
});

test("行键唯一（虚拟列表靠它复用 DOM）", () => {
  const rows = buildGridRows({
    count: 8,
    columns: 2,
    cellSize: CELL,
    slices: [
      slice({ count: 4, dayCount: 8, id: "2026-08-15#0" }),
      slice({ start: 4, count: 4, dayCount: 8, id: "2026-08-15#1" }),
    ],
  });
  const keys = rows.map((row) => row.key);
  assert.equal(new Set(keys).size, keys.length, `键重复：${keys.join(",")}`);
});

test("issue 附加行高：每行取成员最大值，正常网格不改变，异常数值归零",()=>{
 const rows=buildGridRows({count:5,columns:2,cellSize:CELL,extraHeight:(index)=>[40,80,NaN,-1,150][index]??0});
 assert.deepEqual(tiles(rows).map(row=>row.height),[CELL+80,CELL,CELL+150]);
 const grouped=buildGridRows({count:4,columns:2,cellSize:CELL,slices:[slice()],extraHeight:(index)=>index*10});
 assert.deepEqual(tiles(grouped).map(row=>row.height),[CELL+10,CELL+30]);
});
