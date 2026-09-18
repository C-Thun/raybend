/**
 * 筛选条件纯逻辑的测试。
 *
 * 三条最容易出错的：
 * 1. 摘一条 chip 时**别把别的条件带走**（改错字段 = 用户点一下丢掉一片筛选）；
 * 2. 「打开筛选」时的取同类：**0 星 / 无色 / 未标记**这些「空值」也是类别，
 *    漏掉它们的话用户明明选中了那几张，结果里却找不到；
 * 3. 关掉筛选只清**四组标记条件**，文本/日期/机型那些（将来加的）不该被一起清掉。
 */

import assert from "node:assert/strict";
import test from "node:test";
import {
  chipKey,
  clearMarkFilters,
  conditionCount,
  filterChips,
  filterFromSelection,
  removeChip,
} from "./filter.ts";

test("filterChips：四组条件摊成 chips，顺序固定", () => {
  const chips = filterChips({
    ratings: [3, 5],
    colors: ["red"],
    likes: ["like"],
    locks: [1, 2],
  });
  assert.deepEqual(chips, [
    { kind: "rating", value: 3 },
    { kind: "rating", value: 5 },
    { kind: "color", value: "red" },
    { kind: "like", value: "like" },
    { kind: "lock", value: 1 },
    { kind: "lock", value: 2 },
  ]);
  assert.equal(conditionCount({ ratings: [3, 5], colors: ["red"] }), 3);
  assert.equal(conditionCount({}), 0);
});

test("filterChips：空筛选与缺字段都不炸", () => {
  assert.deepEqual(filterChips({}), []);
  assert.equal(conditionCount({ combinator: "and" }), 0);
});

test("removeChip：只摘掉那一个值，别的条件原样", () => {
  const filter = { ratings: [3, 5], colors: ["red", "blue"], likes: ["like"], locks: [2] };
  const next = removeChip(filter, { kind: "rating", value: 3 });
  assert.deepEqual(next.ratings, [5]);
  assert.deepEqual(next.colors, ["red", "blue"], "色标不该被动过");
  assert.deepEqual(next.locks, [2]);

  const again = removeChip(next, { kind: "color", value: "blue" });
  assert.deepEqual(again.colors, ["red"]);
  assert.deepEqual(again.ratings, [5]);
});

test("removeChip：摘最后一颗时给空数组（不是 undefined）", () => {
  const next = removeChip({ ratings: [3] }, { kind: "rating", value: 3 });
  assert.deepEqual(next.ratings, []);
});

test("removeChip：摘一个本来就没有的值不改变选择（幂等）", () => {
  const filter = { ratings: [3] };
  const next = removeChip(filter, { kind: "rating", value: 4 });
  assert.deepEqual(next.ratings, [3]);
});

test("chipKey：不同来源的 chip 不撞 key", () => {
  assert.notEqual(chipKey({ kind: "rating", value: 3 }), chipKey({ kind: "lock", value: 3 }));
  assert.equal(chipKey({ kind: "rating", value: 3 }), chipKey({ kind: "rating", value: 3 }));
});

test("clearMarkFilters：只清四组标记条件，其它条件留着", () => {
  const filter = {
    ratings: [3],
    colors: ["red"],
    likes: ["like"],
    locks: [1],
    text: "婚礼",
    cameras: ["X-T5"],
    combinator: "and" as const,
  };
  const next = clearMarkFilters(filter);
  assert.deepEqual(next.ratings, []);
  assert.deepEqual(next.colors, []);
  assert.deepEqual(next.likes, []);
  assert.deepEqual(next.locks, []);
  assert.equal(next.text, "婚礼", "文本条件不该被关筛选带走");
  assert.deepEqual(next.cameras, ["X-T5"]);
  assert.equal(next.combinator, "and");
});

// ─────────────────── 打开筛选时的「取同类」 ───────────────────

test("filterFromSelection：一张 3 星红标图 → 3 星 + 红色（人类点名的例子）", () => {
  const filter = filterFromSelection([
    { rating: 3, colorLabel: "red", likeState: null, lockLevel: 0 },
  ]);
  assert.deepEqual(filter.ratings, [3]);
  assert.deepEqual(filter.colors, ["red"]);
  assert.deepEqual(filter.likes, ["none"], "没标「喜欢」也是一种状态");
  assert.deepEqual(filter.locks, [], "没锁不作为一种同类（否则等于没筛）");
});

test("filterFromSelection：多张时取并集（任一同类都进来）", () => {
  const filter = filterFromSelection([
    { rating: 3, colorLabel: "red", likeState: "like", lockLevel: 1 },
    { rating: 5, colorLabel: null, likeState: null, lockLevel: 2 },
  ]);
  assert.deepEqual(filter.ratings, [3, 5]);
  assert.deepEqual(filter.colors, ["none", "red"], "无序展示，排序后稳定");
  assert.deepEqual(filter.likes, ["like", "none"]);
  assert.deepEqual(filter.locks, [1, 2]);
});

test("filterFromSelection：0 星也是类别（选中的照片就是 0 星，结果里必须找得到）", () => {
  const filter = filterFromSelection([
    { rating: 0, colorLabel: null, likeState: null, lockLevel: 0 },
  ]);
  assert.deepEqual(filter.ratings, [0]);
});

test("filterFromSelection：空选中给空筛选（别凭空造条件）", () => {
  assert.deepEqual(filterFromSelection([]), {});
});

test("filterFromSelection：结果去重（同一种标记只出现一次）", () => {
  const filter = filterFromSelection([
    { rating: 3, colorLabel: "red", likeState: "like", lockLevel: 1 },
    { rating: 3, colorLabel: "red", likeState: "like", lockLevel: 1 },
    { rating: 3, colorLabel: "red", likeState: "like", lockLevel: 1 },
  ]);
  assert.deepEqual(filter.ratings, [3]);
  assert.deepEqual(filter.colors, ["red"]);
  assert.deepEqual(filter.likes, ["like"]);
  assert.deepEqual(filter.locks, [1]);
});
