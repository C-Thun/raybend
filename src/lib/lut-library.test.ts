/** LUT 分类表的纯逻辑测试（边界：重名 / 大小写 / 空白 / 超长 / 垃圾输入）。 */
import assert from "node:assert/strict";
import test from "node:test";

import {
  addLutCategory,
  hasCategoryNamed,
  LUT_NAME_MAX,
  lutCount,
  newLutCategoryId,
  sanitizeLutCategories,
  toggleExpandedCategory,
} from "./lut-library.ts";

test("sanitizeLutCategories：坏输入一律丢干净，不抛错", () => {
  assert.deepEqual(sanitizeLutCategories(null), []);
  assert.deepEqual(sanitizeLutCategories("x"), []);
  assert.deepEqual(sanitizeLutCategories([null, 42, {}, { id: "", name: "a" }]), []);
  // 合法的留下，条目里的垃圾字段丢掉
  const kept = sanitizeLutCategories([
    { id: "a", name: "旅行", entries: [{ id: "l1", name: "Kodak", path: "/x.cube" }, { id: "" }] },
  ]);
  assert.deepEqual(kept, [
    { id: "a", name: "旅行", entries: [{ id: "l1", name: "Kodak", path: "/x.cube" }] },
  ]);
});

test("名字做长度截断（超长名字会把标题行撑破）", () => {
  const long = "甲".repeat(LUT_NAME_MAX + 20);
  const out = sanitizeLutCategories([{ id: "a", name: long, entries: [] }]);
  assert.equal(out[0]?.name.length, LUT_NAME_MAX);
});

test("重名判定忽略大小写与首尾空格", () => {
  const categories = [{ id: "a", name: "Travel", entries: [] }];
  assert.equal(hasCategoryNamed(categories, "travel"), true);
  assert.equal(hasCategoryNamed(categories, "  TRAVEL "), true);
  assert.equal(hasCategoryNamed(categories, "Portrait"), false);
});

test("新建分类：重名不建（不改原数组）、正常建时返回新 id", () => {
  const before = [{ id: "a", name: "旅行", entries: [] }];
  const dup = addLutCategory(before, " 旅行 ");
  assert.equal(dup.createdId, null);
  assert.deepEqual(dup.categories, before, "重名不该改动原数组");

  const ok = addLutCategory(before, "人像", () => 0.5);
  assert.equal(ok.categories.length, 2);
  assert.equal(ok.categories[1]?.name, "人像");
  assert.equal(ok.createdId, ok.categories[1]?.id);

  // 空白名字等于没输入
  assert.equal(addLutCategory(before, "   ").createdId, null);
});

test("id 生成在随机源被固定住时也不会撞（退化到按序号）", () => {
  const categories = [{ id: "lutcat-1", name: "a", entries: [] }];
  const id = newLutCategoryId(categories, () => 0);
  assert.equal(id.startsWith("lutcat-"), true);
  assert.notEqual(id, "lutcat-1");
});

test("展开规则：同一时刻只开一个，点同一个 = 全部收起", () => {
  assert.equal(toggleExpandedCategory(null, "a"), "a");
  assert.equal(toggleExpandedCategory("a", "b"), "b", "点另一个 = 换过去（同时只开一个）");
  assert.equal(toggleExpandedCategory("a", "a"), null, "再点同一个 = 收起");
});

test("LUT 计数把各分类加起来（空表是 0）", () => {
  assert.equal(lutCount([]), 0);
  assert.equal(
    lutCount([
      { id: "a", name: "a", entries: [{ id: "1", name: "1" }] },
      { id: "b", name: "b", entries: [] },
    ]),
    1,
  );
});
