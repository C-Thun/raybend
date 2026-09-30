/** 当前筛选 → 自动桶草稿。导航目录/标签/桶从来不在输入中。 */

import type { BrowseFilter, Tag } from "../../api/types.ts";
import type { RuleGroup, RuleSet } from "../../api/organization.ts";

export type DraftResult =
  | { ok: true; rules: RuleSet }
  | { ok: false; unsupported: string[] };

const has = (value: unknown): boolean =>
  Array.isArray(value) ? value.length > 0 : value !== null && value !== undefined && value !== "";

export function rulesFromBrowseFilter(filter: BrowseFilter, tags: readonly Tag[]): DraftResult {
  const unsupported = [
    ["flag", filter.flag],
    ["takenFrom", filter.takenFrom],
    ["takenTo", filter.takenTo],
    ["cameras", filter.cameras],
    ["lenses", filter.lenses],
    ["isoFrom", filter.isoFrom],
    ["isoTo", filter.isoTo],
    ["focalFrom", filter.focalFrom],
    ["focalTo", filter.focalTo],
    ["text", filter.text],
  ].filter(([, value]) => has(value)).map(([name]) => name as string);
  const names = new Map(tags.map((tag) => [tag.id, tag.name]));
  const unknown = (filter.tags ?? []).filter((id) => !names.has(id));
  if (unknown.length > 0) unsupported.push("unknownTags");
  if (unsupported.length > 0) return { ok: false, unsupported };

  const values: Partial<RuleGroup>[] = [];
  if (has(filter.minRating)) values.push({ minRating: filter.minRating });
  if (has(filter.colors)) values.push({ colors: [...new Set(filter.colors)] });
  if (has(filter.likes)) values.push({ likes: [...new Set(filter.likes)] });
  if (has(filter.locks)) values.push({ locks: [...new Set(filter.locks)] });
  if (has(filter.tags)) values.push({ tagKeys: [...new Set((filter.tags ?? []).map((id) => names.get(id)!))] });

  if (filter.combinator === "or") {
    return {
      ok: true,
      rules: { groups: (values.length > 0 ? values : [{}]).map((value) => ({
        repositoryIds: [],
        ...value,
      })) },
    };
  }
  return {
    ok: true,
    rules: { groups: [{ repositoryIds: [], ...Object.assign({}, ...values) }] },
  };
}

export function emptyRuleDraft(): RuleSet {
  return { groups: [{ repositoryIds: [] }] };
}
