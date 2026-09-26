/**
 * **筛选条件**的纯逻辑（`BROWSE.md` §3.1、`specs/M2-W2-tail.md` 3.3/3.4）。
 *
 * ## 筛选是什么
 *
 * 打开筛选开关后，toolsbar 上的标记控件**不再给选中的照片设值**，而是变成
 * 「对当前 tiles / 胶片带做筛选」的条件。
 *
 * ## 组合语义（人类 2026-09-19 定的口径）
 *
 * * **组内是「或」**：在同一组里点多个值（例：绿 + 蓝）⇒ 绿**或**蓝；
 * * **组间是「与」**：星 ≥3 **且** 绿色 **且** 有旗标 ⇒ 三者都要满足；
 * * 所以默认 `combinator = "and"`（组与组之间为与）。引擎两种都支持
 *   （`store/query.rs` 的 `Combinator`），切成 `"or"` 是「任一条件命中即可」的宽口径。
 *
 * ## 星标是「阈值」不是「精确值」
 *
 * 人类 2026-09-19：选 3 星时，**4 星、5 星也要出现** —— 所以条件是 `rating >= 3`
 * （`minRating`），不是 `rating IN (3)`。
 *
 * ## 人类点名的那个「爽用法」
 *
 * 「选中一张 3 星的红标图 → 打开筛选 → 所有 3 星或红标图留下」——
 * 所以[`filterFromSelection`] 会在**打开筛选的那一刻**把选中照片的标记
 * 变成初始条件。没有它，「打开筛选」只是一个空开关，还得手点两下才等价。
 *
 * ## 为什么要有 chips
 *
 * 条件是以「开关按下去」的形式设的，用户看不见到底设了哪些；
 * [`filterChips`] 把条件摊成一行可逐个摘掉的 chip（画布 ② 的那一行）。
 *
 * 语义都与引擎对齐（**精确匹配**，不是「及以上」）：`rating IN (3)` 就是「正好 3 星」。
 */

import type { BrowseFilter, MarkingItem } from "../../api/types.ts";

/** 一行 chip 描述（只讲结构，文案由界面查语言包 —— `DESIGN.md` §11.1） */
export type FilterChip =
  | { kind: "rating"; value: number }
  | { kind: "color"; value: string }
  | { kind: "like"; value: string }
  | { kind: "lock"; value: number }
  | { kind: "flag"; value: "pick" | "reject" | "none" };

/** chip 的稳定 key（渲染列表与测试用；`kind` + `value` 唯一确定一条） */
export function chipKey(chip: FilterChip): string {
  return `${chip.kind}:${chip.value}`;
}

/** 把筛选条件摊成 chips（顺序固定：星 → 色 → 喜欢 → 锁，与控件顺序一致） */
export function filterChips(filter: BrowseFilter): FilterChip[] {
  const out: FilterChip[] = [];
  if (filter.minRating !== null && filter.minRating !== undefined) {
    out.push({ kind: "rating", value: filter.minRating });
  }
  for (const value of filter.colors ?? []) out.push({ kind: "color", value });
  for (const value of filter.likes ?? []) out.push({ kind: "like", value });
  for (const value of filter.locks ?? []) out.push({ kind: "lock", value });
  // 旗标排在最后（它与其他四组不同：不是「取值」，而是「有没有」）
  if (filter.flag !== null && filter.flag !== undefined) {
    out.push({ kind: "flag", value: filter.flag.mode });
  }
  return out;
}

/** 现在有几个条件（≥2 才需要问「任一 / 全部」） */
export function conditionCount(filter: BrowseFilter): number {
  return filterChips(filter).length;
}

/** 摘掉一条 chip（只动它对应的那一个值，别的条件原样保留） */
export function removeChip(filter: BrowseFilter, chip: FilterChip): BrowseFilter {
  switch (chip.kind) {
    case "rating":
      return { ...filter, minRating: null };
    case "color":
      return { ...filter, colors: (filter.colors ?? []).filter((v) => v !== chip.value) };
    case "like":
      return { ...filter, likes: (filter.likes ?? []).filter((v) => v !== chip.value) };
    case "lock":
      return { ...filter, locks: (filter.locks ?? []).filter((v) => v !== chip.value) };
    case "flag":
      return { ...filter, flag: null };
  }
}

/** 只保留这四组条件、其余（文本/日期/机型…）原样 —— 「关掉筛选」时清的也就是这四组 */
export function clearMarkFilters(filter: BrowseFilter): BrowseFilter {
  return { ...filter, minRating: null, colors: [], likes: [], locks: [], flag: null };
}

/**
 * 打开筛选时的**初始条件**：从选中照片的标记里「取同类」。
 *
 * 规则（都按「任一」的直觉）：
 *
 * * 星：取选中的那些**非 0** 星值（去重）；选中里有 0 星 → 也带上 0（0 星也是一类）；
 * * 色标 / 喜欢：取**非空**的值；只要有照片是「无色 / 未标记」，就把 `"none"` 也算一类
 *   （否则那几张永远不会出现在结果里，而用户明明选中了它们）；
 * * 锁：只取**非 0** 的级别（「没锁」不是一种要找的同类 —— 它占大多数，带上等于没筛）。
 */
export function filterFromSelection(
  items: readonly Pick<
    MarkingItem,
    "rating" | "colorLabel" | "likeState" | "lockLevel"
  >[],
  /**
   * 选中的这些照片**是不是都有旗标**（旗标只活在内存里，只有调用方知道）。
   *
   * `true` ⇒ 初始条件带上「有旗标」；`false` / 不给 ⇒ 不带旗标条件
   * （「无旗标」不能从选中推出来：用户选中的照片没旗标，不等于他想看没旗标的）。
   */
  picked = false,
): BrowseFilter {
  if (items.length === 0) return {};
  const colors = new Set<string>();
  const likes = new Set<string>();
  const locks = new Set<number>();

  /*
   * 星级：阈值语义下「与这张图一样」= **它那颗星**（≥N 会把它连同更高的都收进来）。
   * 多选时取**最小值**：那是这批照片里最宽的同类口子（取最大值会把大部分选中项筛掉，
   * 而用户此刻的意图是「看看和这些图类似的」）。
   */
  const minRating = Math.min(...items.map((item) => item.rating));

  for (const item of items) {
    colors.add(item.colorLabel ?? "none");
    likes.add(item.likeState ?? "none");
    if (item.lockLevel > 0) locks.add(item.lockLevel);
  }

  return {
    minRating,
    colors: [...colors].sort(),
    likes: [...likes].sort(),
    locks: [...locks].sort((a, b) => a - b),
    combinator: "and",
    ...(picked ? { flag: { mode: "pick" as const } } : {}),
  };
}
