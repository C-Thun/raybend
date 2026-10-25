/**
 * 标签筛选菜单的候选计算（纯函数，便于单测）。
 *
 * 规则：
 * 1. **已选中的标签永远置顶**（哪怕搜索词不匹配）—— 用户要能一眼看到/取消它；
 * 2. 其余按词典顺序（use_count 降序）给匹配项；
 * 3. 只给一屏（`TAG_MENU_LIMIT`）：系统上限 5000，平铺既慢又没法用。
 */

import type { Tag } from "../../api/types.ts";
import { tagKey } from "./tag-draft.ts";

export const TAG_MENU_LIMIT = 60;

export function filterTagMenu(
  tags: readonly Tag[],
  selectedIds: readonly number[],
  query: string,
  limit = TAG_MENU_LIMIT,
): Tag[] {
  const selected = new Set(selectedIds);
  const pinned = tags.filter((tag) => selected.has(tag.id));
  const folded = tagKey(query.trim());
  const matched =
    folded === "" ? tags : tags.filter((tag) => tagKey(tag.name).includes(folded));
  const rest = matched.filter((tag) => !selected.has(tag.id));
  return [...pinned, ...rest].slice(0, limit);
}
