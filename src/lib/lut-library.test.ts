/** LUT 分类表的纯逻辑测试（边界：重名 / 大小写 / 空白 / 超长 / 垃圾输入）。 */
import assert from "node:assert/strict";
import test from "node:test";

import {
  LUT_NAME_MAX,
  addLutCategory,
  ensureDefaultLutCategory,
  hasCategoryNamed,
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

/* ══════════════════════════════════════════════════════════════
 * 默认分类（人类 2026-09-23：每次启动查一遍）
 * ══════════════════════════════════════════════════════════════ */

test("默认分类：没有就建一个，名字跟当前语言走", () => {
  const zh = ensureDefaultLutCategory([], "默认分类", () => 0.5);
  assert.equal(zh.length, 1);
  assert.equal(zh[0]!.name, "默认分类");
  assert.equal(zh[0]!.entries.length, 0);

  const en = ensureDefaultLutCategory([], "Default", () => 0.5);
  assert.equal(en[0]!.name, "Default", "英文界面下不该凭空冒出一个中文分类");
});

test("默认分类：两个名字任意一个在就不新建（双语互认）", () => {
  // 中文名在 → 英文界面也不新建
  const hasZh = [{ id: "a", name: "默认分类", entries: [] }];
  assert.deepEqual(ensureDefaultLutCategory(hasZh, "Default"), hasZh);
  // 英文名在 → 中文界面也不新建
  const hasEn = [{ id: "a", name: "Default", entries: [] }];
  assert.deepEqual(ensureDefaultLutCategory(hasEn, "默认分类"), hasEn);
  // 大小写 / 首尾空格算同一个（不造近似重名）
  const hasLower = [{ id: "a", name: " default ", entries: [] }];
  assert.deepEqual(ensureDefaultLutCategory(hasLower, "默认分类"), hasLower);
});

test("默认分类：用户删了不拦，下次调用照建（幂等）", () => {
  const withDefault = ensureDefaultLutCategory([], "默认分类", () => 0.25);
  // 用户把它删了 → 再查一次又回来了
  const rebuilt = ensureDefaultLutCategory([], "默认分类", () => 0.25);
  assert.deepEqual(rebuilt, withDefault, "同样的输入得到同样的一份（不漂）");
  // 已经有了 → 原样返回（长度不变，不重复）
  assert.equal(ensureDefaultLutCategory(withDefault, "默认分类").length, 1);
  // 顺序：新建的排在**最前**（它是导入 LUT 时的默认项）
  const withUser = ensureDefaultLutCategory(
    [{ id: "u", name: "旅行", entries: [] }],
    "默认分类",
    () => 0.1,
  );
  assert.equal(withUser[0]!.name, "默认分类");
  assert.equal(withUser[1]!.name, "旅行");
});

test("默认分类：默认名字是垃圾时退回中文名（存储里的脏值不能让面板起不来）", () => {
  const built = ensureDefaultLutCategory([], "   ", () => 0.5);
  assert.equal(built[0]!.name, "默认分类");
});
