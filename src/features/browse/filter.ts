/**
 * **筛选条件**的纯逻辑（`BROWSE.md` §3.1、`plans/M2-W2-tail.md` 3.3/3.4）。
 *
 * ## 筛选是什么
 *
 * 打开筛选开关后，toolsbar 上的标记控件**不再给选中的照片设值**，而是变成
 * 「对当前 tiles / 胶片带做筛选」的条件。条件之间默认**「任一」（或）**，
 * 也可以切成「全部」（与）—— 引擎两种都支持（`store/query.rs` 的 `Combinator`）。
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
  | { kind: "lock"; value: number };

/** chip 的稳定 key（渲染列表与测试用；`kind` + `value` 唯一确定一条） */
export function chipKey(chip: FilterChip): string {
  return `${chip.kind}:${chip.value}`;
}

/** 把筛选条件摊成 chips（顺序固定：星 → 色 → 喜欢 → 锁，与控件顺序一致） */
export function filterChips(filter: BrowseFilter): FilterChip[] {
  const out: FilterChip[] = [];
  for (const value of filter.ratings ?? []) out.push({ kind: "rating", value });
  for (const value of filter.colors ?? []) out.push({ kind: "color", value });
  for (const value of filter.likes ?? []) out.push({ kind: "like", value });
  for (const value of filter.locks ?? []) out.push({ kind: "lock", value });
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
      return { ...filter, ratings: (filter.ratings ?? []).filter((v) => v !== chip.value) };
    case "color":
      return { ...filter, colors: (filter.colors ?? []).filter((v) => v !== chip.value) };
    case "like":
      return { ...filter, likes: (filter.likes ?? []).filter((v) => v !== chip.value) };
    case "lock":
      return { ...filter, locks: (filter.locks ?? []).filter((v) => v !== chip.value) };
  }
}

/** 只保留这四组条件、其余（文本/日期/机型…）原样 —— 「关掉筛选」时清的也就是这四组 */
export function clearMarkFilters(filter: BrowseFilter): BrowseFilter {
  return { ...filter, ratings: [], colors: [], likes: [], locks: [] };
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
): BrowseFilter {
  if (items.length === 0) return {};
  const ratings = new Set<number>();
  const colors = new Set<string>();
  const likes = new Set<string>();
  const locks = new Set<number>();

  for (const item of items) {
    ratings.add(item.rating);
    colors.add(item.colorLabel ?? "none");
    likes.add(item.likeState ?? "none");
    if (item.lockLevel > 0) locks.add(item.lockLevel);
  }

  return {
    ratings: [...ratings].sort((a, b) => a - b),
    colors: [...colors].sort(),
    likes: [...likes].sort(),
    locks: [...locks].sort((a, b) => a - b),
  };
}
