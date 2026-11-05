import assert from "node:assert/strict";
import test from "node:test";

import { rowDropTarget, rowShift } from "./row-drag-sort.ts";

test("lutDropTarget 按槽位四舍五入并夹在 0..count-1", () => {
  // 行高 54 + 间距 6 = 60
  const slot = 60;
  assert.equal(rowDropTarget(100, 100, slot, 3), 0, "停在第一行 = 第 0 槽");
  assert.equal(rowDropTarget(130, 100, slot, 3), 1, "过半即算下一槽");
  assert.equal(rowDropTarget(129, 100, slot, 3), 0);
  assert.equal(rowDropTarget(1000, 100, slot, 3), 2, "超过末尾夹到最后一槽");
  assert.equal(rowDropTarget(0, 100, slot, 3), 0, "越过顶部夹到第 0 槽");
  // 退化输入不炸
  assert.equal(rowDropTarget(50, 0, 0, 3), 0);
  assert.equal(rowDropTarget(50, 0, 60, 0), 0);
});

test("lutLayerShift：源在目标之前时中间行上移一格，反之亦然", () => {
  const slot = 60;
  // 0 → 2：1 和 2 上移
  assert.equal(rowShift(0, 2, 0, slot), 0, "源行自己不动（隐形）");
  assert.equal(rowShift(0, 2, 1, slot), -slot);
  assert.equal(rowShift(0, 2, 2, slot), -slot);
  assert.equal(rowShift(0, 2, 3, slot), 0, "目标之后的行不动");
  // 2 → 0：0 和 1 下移
  assert.equal(rowShift(2, 0, 0, slot), slot);
  assert.equal(rowShift(2, 0, 1, slot), slot);
  assert.equal(rowShift(2, 0, 2, slot), 0);
  assert.equal(rowShift(2, 0, 3, slot), 0);
  // 没换位
  assert.equal(rowShift(1, 1, 0, slot), 0);
  assert.equal(rowShift(1, 1, 2, slot), 0);
});

test("lutLayerShift 覆盖相邻一格（拖动最容易出错的那一档）", () => {
  const slot = 60;
  assert.equal(rowShift(1, 2, 2, slot), -slot);
  assert.equal(rowShift(2, 1, 1, slot), slot);
});
