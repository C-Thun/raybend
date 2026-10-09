/**
 * 日期格式化（`lib/datetime.ts`）。
 *
 * 重点盖住 [`formatDay`]：崔总 2026-10-08 定的口径是「只到天、月份/日期零填充」——
 * 用本地时间构造、再用本地 getter 断言，所以跑在哪个时区结果都一样。
 */

import assert from "node:assert/strict";
import test from "node:test";

import { formatDay } from "./datetime.ts";

test("formatDay：YYYY-MM-DD，月/日零填充（不许出现单位数月份）", () => {
  assert.equal(formatDay(new Date(2026, 9, 8, 23, 30).getTime()), "2026-10-08");
  assert.equal(formatDay(new Date(2026, 0, 5, 0, 0).getTime()), "2026-01-05");
  assert.equal(formatDay(new Date(2026, 11, 31, 23, 59, 59).getTime()), "2026-12-31");
});

test("formatDay：按本地时区取「哪一天」，深夜/清晨都不串日", () => {
  // 同一天的 00:00 与 23:59 必须是同一个日期串
  const early = new Date(2026, 2, 1, 0, 0).getTime();
  const late = new Date(2026, 2, 1, 23, 59).getTime();
  assert.equal(formatDay(early), "2026-03-01");
  assert.equal(formatDay(late), "2026-03-01");
  // 跨过午夜才是下一天
  assert.equal(formatDay(new Date(2026, 2, 2, 0, 0).getTime()), "2026-03-02");
});

test("formatDay：非法值给空串（不抛、不产出 Invalid Date）", () => {
  assert.equal(formatDay(Number.NaN), "");
  assert.equal(formatDay(Number.POSITIVE_INFINITY), "");
});
