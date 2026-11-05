/**
 * 右栏建议输入的**纯逻辑**：两个来源合并 + 前缀筛选。
 *
 * 两个来源（都是「字符串 + 可选次数」的列表）：
 *   * **最近输入** —— 设备级、跟着人走（`lib/text-history.ts`）；
 *   * **库内已有值** —— 跟着库走（`browse_text_values`，带出现次数）。
 *
 * 合并规则：先最近（顺序 = 最近用过的先后），再库内按次数降序；同值（trim 后逐字相等）
 * 只留一条、保留先出现的那条。这样「刚打过的值」永远在最上面 ——
 * 连续给几张照片填同一个地方时，第一个候选就是它。
 *
 * 筛选规则（崔总 2026-11-03：「输入时根据前缀进行筛选」）：
 *   1. 前缀命中优先（`杭州` → 「杭州」「杭州市」）；
 *   2. **前缀一条都没有**时退回子串命中，免得「记不住开头」就什么都选不到；
 *   3. 大小写不敏感（latin），中文按逐字比较。
 *
 * 纯函数：不碰 DOM、不碰存储，单测直接钉行为。
 */

export interface LibraryValue {
  value: string;
  /** 库里有多少张照片用了这个值（下拉里显示来源，也用来排序）。 */
  count: number;
}

export interface Suggestion {
  value: string;
  /** `null` = 只来自最近输入（库里没有这个值）。 */
  count: number | null;
}

function keyOf(value: string): string {
  return value.trim();
}

/** 合并两个来源；`limit` 是**最终候选条数**（两个源的分配也受它约束）。 */
export function mergeSuggestions(
  recent: readonly string[],
  library: readonly LibraryValue[],
  limit: number,
): Suggestion[] {
  if (limit <= 0) return [];
  const seen = new Set<string>();
  const out: Suggestion[] = [];
  const push = (suggestion: Suggestion): boolean => {
    const key = keyOf(suggestion.value);
    if (key === "" || seen.has(key)) return true;
    seen.add(key);
    out.push({ value: key, count: suggestion.count });
    return out.length < limit;
  };
  for (const value of recent) {
    if (!push({ value, count: null })) return out;
  }
  for (const entry of library) {
    if (!push({ value: entry.value, count: entry.count })) return out;
  }
  return out;
}

/**
 * 按输入筛选：前缀优先，前缀没有命中时退回子串；空查询返回全部（focus 即列表）。
 *
 * 返回的是**原数组的子序列**（保持合并时的顺序），调用方直接渲染。
 */
export function filterSuggestions(
  items: readonly Suggestion[],
  query: string,
): Suggestion[] {
  const needle = query.trim().toLocaleLowerCase();
  if (needle === "") return [...items];
  const prefix = items.filter((item) => item.value.toLocaleLowerCase().startsWith(needle));
  if (prefix.length > 0) return prefix;
  return items.filter((item) => item.value.toLocaleLowerCase().includes(needle));
}

/**
 * 上下键在候选里移动：`current = -1` 表示还没选；到头就停（不回绕）。
 *
 * 不回绕是**故意的**：列表常常是「最近 20 条」，回绕会让人越按越不知道自己选了谁。
 */
export function moveHighlight(current: number, delta: -1 | 1, length: number): number {
  if (length <= 0) return -1;
  const next = current + delta;
  if (next < 0) return -1;
  return Math.min(length - 1, next);
}
