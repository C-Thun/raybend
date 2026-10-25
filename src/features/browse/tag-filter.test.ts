import { test } from "node:test";
import assert from "node:assert/strict";

import type { Tag } from "../../api/types.ts";
import { filterTagMenu, TAG_MENU_LIMIT } from "./tag-filter.ts";

const tag = (id: number, name: string, useCount = 0): Tag => ({ id, name, useCount });

const tags: Tag[] = [
  tag(1, "婚礼"),
  tag(2, "旅行"),
  tag(3, "Wedding"),
  tag(4, "海边"),
];

test("空查询：按词典顺序给候选", () => {
  assert.deepEqual(
    filterTagMenu(tags, [], "").map((t) => t.id),
    [1, 2, 3, 4],
  );
});

test("已选中的标签置顶，且搜索时也不丢", () => {
  const picked = filterTagMenu(tags, [4, 2], "wed");
  assert.deepEqual(picked.map((t) => t.id), [2, 4, 3], "选中的 2/4 在前，再给匹配的 3");
});

test("搜索折叠大小写与 Unicode（NFC + 小写）", () => {
  assert.deepEqual(
    filterTagMenu(tags, [], "WED").map((t) => t.id),
    [3],
  );
  assert.deepEqual(
    filterTagMenu(tags, [], "海边").map((t) => t.id),
    [4],
  );
});

test("一屏上限：选中项优先占名额", () => {
  const many = Array.from({ length: TAG_MENU_LIMIT + 10 }, (_, index) => tag(index + 1, `tag-${index + 1}`));
  const out = filterTagMenu(many, [many.length], "");
  assert.equal(out.length, TAG_MENU_LIMIT);
  assert.equal(out[0]?.id, many.length, "最后那条被选中，仍要出现在第一位");
});

test("没有匹配时返回空（选中项仍保留）", () => {
  assert.deepEqual(filterTagMenu(tags, [], "不存在"), []);
  assert.deepEqual(
    filterTagMenu(tags, [2], "不存在").map((t) => t.id),
    [2],
  );
});
