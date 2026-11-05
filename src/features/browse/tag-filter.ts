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
  /**
   * 标签词典。**允许 `null`**：词典还没拉回来时 store 给的是 `null`（不是 `[]`），
   * 而这里的调用点是一个 Solid 的 `createMemo` —— 在那里抛异常会带崩整个响应式根
   * （2026-11-05 实测：导出 smoke 里进过一次浏览后再切工作流就不刷新了，
   * 根因就是 `tags.filter` 打在 null 上）。
   */
  tags: readonly Tag[] | null,
  selectedIds: readonly number[],
  query: string,
  limit = TAG_MENU_LIMIT,
): Tag[] {
  const list = tags ?? [];
  const selected = new Set(selectedIds);
  const pinned = list.filter((tag) => selected.has(tag.id));
  const folded = tagKey(query.trim());
  const matched =
    folded === "" ? list : list.filter((tag) => tagKey(tag.name).includes(folded));
  const rest = matched.filter((tag) => !selected.has(tag.id));
  return [...pinned, ...rest].slice(0, limit);
}
