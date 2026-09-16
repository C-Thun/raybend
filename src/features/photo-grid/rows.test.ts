/**
 * 网格行模型的测试。
 *
 * 行模型是「数据」到「虚拟滚动」之间的那一步：切错行 → 滚动位置跳、
 * 分组标题跑到别的组里去。两种模式（平铺 / 按时间）都要钉住。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import type { SourceItem } from "../../api/types.ts";
import { groupByTime } from "../../lib/time-group.ts";
import { tileImageHeight } from "../../lib/tile-flow.ts";
import {
  buildGridRows,
  countRowPhotos,
  DAY_HEADER,
  itemId,
  SLICE_HEADER,
  type GroupRowModel,
  type TileRowModel,
} from "./rows.ts";

const CST = 480;
const CAPTION = 26;

function item(name: string, takenAtMs: number | null = null): SourceItem {
  return {
    path: `/src/${name}`,
    fileName: name,
    ext: "jpg",
    kind: "image",
    sizeBytes: 1000,
    mtimeMs: null,
    takenAtMs,
    takenAtSource: takenAtMs === null ? null : "exif",
    takenAtOffsetMin: takenAtMs === null ? null : CST,
  };
}

function at(hour: number, minute = 0, day = 15): number {
  return Date.UTC(2026, 7, day, hour, minute) - CST * 60_000;
}

function tileRows(rows: readonly { kind: string }[]): TileRowModel[] {
  return rows.filter((row): row is TileRowModel => row.kind === "tiles");
}

function groupRows(rows: readonly { kind: string }[]): GroupRowModel[] {
  return rows.filter((row): row is GroupRowModel => row.kind === "group");
}

/** 只看**日组**标题（片标题也带 `dayId`，别混进断言里） */
function dayGroups(rows: readonly { kind: string }[]): GroupRowModel[] {
  return groupRows(rows).filter((row) => row.level === "day");
}

/* ══════════════════════════════════════════════════════════════
 * 平铺模式
 * ══════════════════════════════════════════════════════════════ */

test("平铺：按列数切行，行高 = 画面高 + 字幕条高", () => {
  const items = Array.from({ length: 7 }, (_, i) => item(`p${i}.jpg`));
  const rows = buildGridRows({
    items,
    columns: 3,
    cellWidth: 200,
    captionHeight: CAPTION,
      tilePad: 0,
      tileGap: 0,
  });
  assert.equal(rows.length, 3, "7 张按 3 列切成 3 行");
  const expectedHeight = tileImageHeight(200) + CAPTION;
  for (const row of rows) {
    assert.equal(row.kind, "tiles");
    assert.equal(row.height, expectedHeight);
  }
  assert.deepEqual(
    tileRows(rows).map((row) => row.items.length),
    [3, 3, 1],
  );
  assert.equal(countRowPhotos(rows), 7);
});

test("平铺：列数非法时按一行一张（不产出空行）", () => {
  const items = [item("a.jpg"), item("b.jpg")];
  for (const columns of [0, -1, Number.NaN]) {
    const rows = buildGridRows({
      items,
      columns,
      cellWidth: 200,
      captionHeight: CAPTION,
      tilePad: 0,
      tileGap: 0,
    });
    assert.equal(tileRows(rows).length, 2, `columns=${columns}`);
  }
});

test("空列表：没有行", () => {
  const rows = buildGridRows({
    items: [],
    columns: 4,
    cellWidth: 200,
    captionHeight: CAPTION,
      tilePad: 0,
      tileGap: 0,
  });
  assert.deepEqual(rows, []);
  assert.equal(countRowPhotos(rows), 0);
});

test("字幕条高为 0 / 非法：行高退化但不能是 NaN", () => {
  const rows = buildGridRows({
    items: [item("a.jpg")],
    columns: 1,
    cellWidth: 200,
    captionHeight: Number.NaN,
      tilePad: 0,
      tileGap: 0,
  });
  assert.equal(rows[0].height, tileImageHeight(200));
  assert.ok(Number.isFinite(rows[0].height));
});

/* ══════════════════════════════════════════════════════════════
 * 按时间模式
 * ══════════════════════════════════════════════════════════════ */

test("按时间：日标题 + 片标题 + tile 行，顺序正确", () => {
  const items = [
    item("morning-1.jpg", at(9, 0)),
    item("morning-2.jpg", at(9, 30)),
    item("night-1.jpg", at(20, 0)),
  ];
  const grouping = groupByTime(
    items.map((entry) => ({ id: itemId(entry), takenAtMs: entry.takenAtMs })),
    { gapMinutes: 60, offsetMinutes: CST },
  );
  const rows = buildGridRows({
    items,
    columns: 2,
    cellWidth: 200,
    captionHeight: CAPTION,
      tilePad: 0,
      tileGap: 0,
    grouping,
  });

  assert.deepEqual(
    rows.map((row) => (row.kind === "tiles" ? "tiles" : `${row.level}`)),
    ["day", "slice", "tiles", "slice", "tiles"],
  );
  const groups = groupRows(rows);
  assert.equal(groups[0].level, "day");
  assert.equal(groups[0].dayId, "2026-08-15");
  assert.equal(groups[0].count, 3);
  assert.equal(groups[0].height, DAY_HEADER.rowHeight);
  assert.deepEqual(
    groups.slice(1).map((row) => [row.startMs, row.endMs, row.count]),
    [
      [at(9, 0), at(9, 30), 2],
      [at(20, 0), at(20, 0), 1],
    ],
  );
  assert.equal(groups[1].height, SLICE_HEADER.rowHeight);
  assert.equal(countRowPhotos(rows), 3);
});

test("按时间：一天里两段，各自从新的一行开始（不跨片拼行）", () => {
  const items = [
    item("a.jpg", at(9, 0)),
    item("b.jpg", at(9, 10)),
    item("c.jpg", at(9, 20)),
    item("d.jpg", at(21, 0)),
  ];
  const grouping = groupByTime(
    items.map((entry) => ({ id: itemId(entry), takenAtMs: entry.takenAtMs })),
    { gapMinutes: 60, offsetMinutes: CST },
  );
  const rows = buildGridRows({
    items,
    columns: 2,
    cellWidth: 200,
    captionHeight: CAPTION,
      tilePad: 0,
      tileGap: 0,
    grouping,
  });
  const tiles = tileRows(rows);
  assert.deepEqual(
    tiles.map((row) => row.items.map((entry) => entry.fileName)),
    [
      ["a.jpg", "b.jpg"],
      ["c.jpg"],
      ["d.jpg"],
    ],
    "第一段占了 2 列，第二段不能把 c 塞进同一行",
  );
});

test("按时间：未知时间组排在最后，且没有任何时间范围", () => {
  const items = [item("timed.jpg", at(9, 0)), item("no-time.jpg", null)];
  const grouping = groupByTime(
    items.map((entry) => ({ id: itemId(entry), takenAtMs: entry.takenAtMs })),
    { gapMinutes: 60, offsetMinutes: CST },
  );
  const rows = buildGridRows({
    items,
    columns: 4,
    cellWidth: 200,
    captionHeight: CAPTION,
      tilePad: 0,
      tileGap: 0,
    grouping,
  });
  const days = dayGroups(rows);
  assert.deepEqual(
    days.map((row) => row.dayId),
    ["2026-08-15", "unknown"],
  );
  assert.equal(days[1].unknown, true);
  assert.equal(days[1].startMs, null);
  assert.equal(days[1].endMs, null);
  assert.deepEqual(days[1].photoIds, ["/src/no-time.jpg"]);
  assert.equal(countRowPhotos(rows), 2);
});

test("按时间：多个日组，最近的在前", () => {
  const items = [item("d14.jpg", at(9, 0, 14)), item("d15.jpg", at(9, 0, 15))];
  const grouping = groupByTime(
    items.map((entry) => ({ id: itemId(entry), takenAtMs: entry.takenAtMs })),
    { gapMinutes: 60, offsetMinutes: CST },
  );
  const rows = buildGridRows({
    items,
    columns: 1,
    cellWidth: 200,
    captionHeight: CAPTION,
      tilePad: 0,
      tileGap: 0,
    grouping,
  });
  assert.deepEqual(
    dayGroups(rows).map((row) => row.dayId),
    ["2026-08-15", "2026-08-14"],
  );
});

test("按时间：分组里出现了列表里没有的 id → 跳过，不产出空洞", () => {
  const items = [item("a.jpg", at(9, 0))];
  const grouping = groupByTime(
    [
      { id: itemId(items[0]), takenAtMs: at(9, 0) },
      { id: "/src/ghost.jpg", takenAtMs: at(9, 5) },
    ],
    { gapMinutes: 60, offsetMinutes: CST },
  );
  const rows = buildGridRows({
    items,
    columns: 4,
    cellWidth: 200,
    captionHeight: CAPTION,
      tilePad: 0,
      tileGap: 0,
    grouping,
  });
  assert.equal(countRowPhotos(rows), 1, "幽灵 id 不该变成一张空 tile");
  assert.deepEqual(tileRows(rows)[0].items.map((entry) => entry.fileName), [
    "a.jpg",
  ]);
});

test("行键唯一（虚拟列表靠它复用 DOM）", () => {
  const items = Array.from({ length: 12 }, (_, i) => item(`p${i}.jpg`, at(9, i)));
  const grouping = groupByTime(
    items.map((entry) => ({ id: itemId(entry), takenAtMs: entry.takenAtMs })),
    { gapMinutes: 60, offsetMinutes: CST },
  );
  const flat = buildGridRows({
    items,
    columns: 3,
    cellWidth: 200,
    captionHeight: CAPTION,
      tilePad: 0,
      tileGap: 0,
  });
  const grouped = buildGridRows({
    items,
    columns: 3,
    cellWidth: 200,
    captionHeight: CAPTION,
      tilePad: 0,
      tileGap: 0,
    grouping,
  });
  for (const rows of [flat, grouped]) {
    const keys = rows.map((row) => row.key);
    assert.equal(new Set(keys).size, keys.length, `键重复：${keys.join(",")}`);
  }
});

test("按时间：空分组结果 → 只有未知组时才出行", () => {
  const items = [item("x.jpg", null), item("y.jpg", null)];
  const grouping = groupByTime(
    items.map((entry) => ({ id: itemId(entry), takenAtMs: null })),
    { gapMinutes: 60, offsetMinutes: CST },
  );
  const rows = buildGridRows({
    items,
    columns: 2,
    cellWidth: 200,
    captionHeight: CAPTION,
      tilePad: 0,
      tileGap: 0,
    grouping,
  });
  assert.deepEqual(
    rows.map((row) => (row.kind === "group" ? row.dayId : "tiles")),
    ["unknown", "tiles"],
  );
  assert.equal(countRowPhotos(rows), 2);
});
