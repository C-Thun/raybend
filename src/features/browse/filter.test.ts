/**
 * 筛选条件纯逻辑的测试。
 *
 * 三条最容易出错的：
 * 1. 摘一条 chip 时**别把别的条件带走**（改错字段 = 用户点一下丢掉一片筛选）；
 * 2. 「打开筛选」时的取同类：**无色 / 未标记**这些「空值」也是类别，
 *    漏掉它们的话用户明明选中了那几张，结果里却找不到；
 * 3. 关掉筛选只清**五组标记条件**（星/色/喜欢/锁/旗标），文本/日期/机型那些不该被一起清掉。
 *
 * 2026-09-19 的语义变更（人类口径）：
 *   * 星标从「精确等于某几档」改成**阈值**（`minRating`，≥N）；
 *   * 多了一组**旗标**条件（`pick` / `reject` / `none`）；
 *   * 默认组合方式从「任一」改成**「全部」**（组内或、组间与）。
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

test("filterChips：五组条件摊成 chips，顺序固定", () => {
  const chips = filterChips({
    minRating: 3,
    colors: ["red"],
    likes: ["like"],
    locks: [1, 2],
    flag: { mode: "pick" },
  });
  assert.deepEqual(chips, [
    { kind: "rating", value: 3 },
    { kind: "color", value: "red" },
    { kind: "like", value: "like" },
    { kind: "lock", value: 1 },
    { kind: "lock", value: 2 },
    { kind: "flag", value: "pick" },
  ]);
  assert.equal(conditionCount({ minRating: 3, colors: ["red"] }), 2);
  assert.equal(conditionCount({}), 0);
});

test("filterChips：阈值 0 也要出 chip（它是「≥0」而不是「没筛」）", () => {
  // 一般不会设到 0，但真设了就得看得见、摘得掉 —— 否则用户会以为筛选坏了
  assert.deepEqual(filterChips({ minRating: 0 }), [{ kind: "rating", value: 0 }]);
});

test("filterChips：空筛选与缺字段都不炸", () => {
  assert.deepEqual(filterChips({}), []);
  assert.equal(conditionCount({ combinator: "and" }), 0);
});

test("removeChip：只摘掉那一个值，别的条件原样", () => {
  const filter = { minRating: 3, colors: ["red", "blue"], likes: ["like"], locks: [2] };
  const next = removeChip(filter, { kind: "rating", value: 3 });
  assert.equal(next.minRating, null, "星标是阈值：摘掉就是清空");
  assert.deepEqual(next.colors, ["red", "blue"], "色标不该被动过");
  assert.deepEqual(next.locks, [2]);

  const again = removeChip(next, { kind: "color", value: "blue" });
  assert.deepEqual(again.colors, ["red"]);
  assert.equal(again.minRating, null);
});

test("removeChip：摘旗标条件只清旗标（别的组不动）", () => {
  const next = removeChip({ minRating: 4, flag: { mode: "none" } }, { kind: "flag", value: "none" });
  assert.equal(next.flag, null);
  assert.equal(next.minRating, 4);
});

test("chipKey：不同来源的 chip 不撞 key", () => {
  assert.notEqual(chipKey({ kind: "rating", value: 3 }), chipKey({ kind: "lock", value: 3 }));
  assert.equal(chipKey({ kind: "rating", value: 3 }), chipKey({ kind: "rating", value: 3 }));
});

test("clearMarkFilters：只清五组标记条件，其它条件留着", () => {
  const filter = {
    minRating: 3,
    colors: ["red"],
    likes: ["like"],
    locks: [1],
    flag: { mode: "pick" as const },
    text: "婚礼",
    cameras: ["X-T5"],
    combinator: "and" as const,
  };
  const next = clearMarkFilters(filter);
  assert.equal(next.minRating, null);
  assert.deepEqual(next.colors, []);
  assert.deepEqual(next.likes, []);
  assert.deepEqual(next.locks, []);
  assert.equal(next.flag, null);
  assert.equal(next.text, "婚礼", "文本条件不该被关筛选带走");
  assert.deepEqual(next.cameras, ["X-T5"]);
  assert.equal(next.combinator, "and");
});

// ─────────────────── 打开筛选时的「取同类」 ───────────────────

test("filterFromSelection：一张 3 星红标图 → ≥3 星 + 红色（人类点名的例子）", () => {
  const filter = filterFromSelection([
    { rating: 3, colorLabel: "red", likeState: null, lockLevel: 0 },
  ]);
  assert.equal(filter.minRating, 3, "星标是阈值：≥3（4、5 星也在结果里）");
  assert.deepEqual(filter.colors, ["red"]);
  assert.deepEqual(filter.likes, ["none"], "没标「喜欢」也是一种状态");
  assert.deepEqual(filter.locks, [], "没锁不作为一种同类（否则等于没筛）");
  assert.equal(filter.combinator, "and", "默认组间为「与」");
});

test("filterFromSelection：多张时取并集（星标取**最小值**当阈值）", () => {
  const filter = filterFromSelection([
    { rating: 3, colorLabel: "red", likeState: "like", lockLevel: 1 },
    { rating: 5, colorLabel: null, likeState: null, lockLevel: 2 },
  ]);
  assert.equal(filter.minRating, 3, "取最小的那颗：把选中这批都收进来");
  assert.deepEqual(filter.colors, ["none", "red"], "无序展示，排序后稳定");
  assert.deepEqual(filter.likes, ["like", "none"]);
  assert.deepEqual(filter.locks, [1, 2]);
});

test("filterFromSelection：选中的都有旗标时带上「有旗标」条件", () => {
  const items = [{ rating: 2, colorLabel: null, likeState: null, lockLevel: 0 }];
  assert.deepEqual(filterFromSelection(items, true).flag, { mode: "pick" });
  assert.equal(filterFromSelection(items, false).flag, undefined, "没旗标就不带这个条件");
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
  assert.equal(filter.minRating, 3);
  assert.deepEqual(filter.colors, ["red"]);
  assert.deepEqual(filter.likes, ["like"]);
  assert.deepEqual(filter.locks, [1]);
});
