import assert from "node:assert/strict";
import test from "node:test";

import { percentOf } from "./progress.ts";

test("没有总数就是不确定态（null），不编百分比", () => {
  assert.equal(percentOf(0, 0), null);
  assert.equal(percentOf(7, 0), null);
  assert.equal(percentOf(0, -3), null);
});

test("正常推进：四舍五入到整数", () => {
  assert.equal(percentOf(0, 12), 0);
  assert.equal(percentOf(1, 3), 33);
  assert.equal(percentOf(2, 3), 67);
  assert.equal(percentOf(7, 12), 58);
  assert.equal(percentOf(12, 12), 100);
});

test("越界要夹住（后端收尾动作可能让 done 比 total 多）", () => {
  assert.equal(percentOf(13, 12), 100);
  assert.equal(percentOf(-1, 12), 0);
});

test("非有限数当不确定态", () => {
  assert.equal(percentOf(Number.NaN, 12), null);
  assert.equal(percentOf(3, Number.POSITIVE_INFINITY), null);
  assert.equal(percentOf(3, Number.NaN), null);
});
