import assert from "node:assert/strict";
import { test } from "node:test";

import {
  captureFilmStripAnchor,
  restoreFilmStripScroll,
} from "./film-strip-anchor.ts";

const items = [
  { id: "甲", start: 8, size: 96 },
  { id: "乙", start: 110, size: 96 },
  { id: "丙", start: 212, size: 96 },
];

test("抓第一项仍露出像素的照片，并保留负偏移", () => {
  assert.deepEqual(captureFilmStripAnchor(items, 50), { id: "甲", offsetPx: -42 });
  assert.deepEqual(captureFilmStripAnchor(items, 104), { id: "乙", offsetPx: 6 });
  assert.equal(captureFilmStripAnchor([], 10), null);
  assert.equal(captureFilmStripAnchor(items, 999), null);
});

test("新列表里同一照片回到完全相同的横向像素", () => {
  const changed = [
    { id: "零", start: 8, size: 96 },
    { id: "甲", start: 110, size: 96 },
  ];
  assert.equal(
    restoreFilmStripScroll(changed, { id: "甲", offsetPx: -42 }, null),
    152,
  );
});

test("参考照片消失时退到当前照片；都消失时不乱滚", () => {
  assert.equal(
    restoreFilmStripScroll(items, { id: "不存在", offsetPx: 6 }, "丙"),
    206,
  );
  assert.equal(
    restoreFilmStripScroll(items, { id: "不存在", offsetPx: 6 }, "也不存在"),
    null,
  );
});

test("非法几何与负滚动量不会产出 NaN 或负数", () => {
  assert.deepEqual(
    captureFilmStripAnchor([{ id: "x", start: Number.NaN, size: 10 }], -5),
    { id: "x", offsetPx: 0 },
  );
  assert.equal(
    restoreFilmStripScroll(
      [{ id: "x", start: Number.POSITIVE_INFINITY, size: 10 }],
      { id: "x", offsetPx: Number.NaN },
      null,
    ),
    0,
  );
});
