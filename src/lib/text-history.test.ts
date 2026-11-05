/** 「最近输入」的存储与合并规则（设备级 localStorage，版本化 key）。 */

import assert from "node:assert/strict";
import test from "node:test";

import {
  normalizeRecentValue,
  pushRecentValue,
  readTextHistory,
  recentTextValues,
  rememberTextValue,
  resetTextHistoryForTests,
  sanitizeTextHistory,
  TEXT_HISTORY_STORAGE_KEY,
  TEXT_SUGGEST_LIMIT,
  writeTextHistory,
  type TextHistoryStorage,
} from "./text-history.ts";

function fakeStorage(seed: Record<string, string> = {}): TextHistoryStorage & {
  data: Map<string, string>;
} {
  const data = new Map(Object.entries(seed));
  return {
    data,
    getItem: (key) => data.get(key) ?? null,
    setItem: (key, value) => void data.set(key, value),
  };
}

test("写入顺序 = 最近排在前面，重复只留一条", () => {
  let recent: readonly string[] = [];
  for (const value of ["崔总", "张三", "崔总", "  李四  "]) recent = pushRecentValue(recent, value);
  assert.deepEqual([...recent], ["李四", "崔总", "张三"]);
});

test("空串/纯空白不算一条输入", () => {
  assert.equal(normalizeRecentValue("   "), null);
  assert.equal(normalizeRecentValue("\t\n"), null);
  assert.equal(normalizeRecentValue(" 杭州 "), "杭州");
  assert.deepEqual([...pushRecentValue(["杭州"], "   ")], ["杭州"]);
});

test("每个字段最多保留 TEXT_SUGGEST_LIMIT 条（上限登记在 memory/LIMITS.md）", () => {
  let recent: readonly string[] = [];
  for (let index = 0; index < TEXT_SUGGEST_LIMIT + 8; index += 1) {
    recent = pushRecentValue(recent, `地点 ${index}`);
  }
  assert.equal(recent.length, TEXT_SUGGEST_LIMIT);
  assert.equal(recent[0], `地点 ${TEXT_SUGGEST_LIMIT + 7}`, "最新的在队首");
  assert.equal(recent[recent.length - 1], `地点 8`, "最旧的先被挤掉");
});

test("存储消毒：版本不符 / 未知字段 / 非字符串 / 超长一律丢掉", () => {
  assert.deepEqual(sanitizeTextHistory({ version: 2, fields: { author: ["a"] } }), {});
  assert.deepEqual(sanitizeTextHistory(null), {});
  assert.deepEqual(sanitizeTextHistory("崔总"), {});
  assert.deepEqual(
    sanitizeTextHistory({
      version: 1,
      fields: {
        author: ["崔总", "  ", "崔总", 42, "张三"],
        city: "杭州",
        nope: ["x"],
      },
    }),
    { author: ["崔总", "张三"] },
  );
  const many = Array.from({ length: 100 }, (_, index) => `值${index}`);
  assert.equal(sanitizeTextHistory({ version: 1, fields: { author: many } }).author?.length, TEXT_SUGGEST_LIMIT);
});

test("读失败/写不进去都不抛错（偏好不是数据）", () => {
  const broken: TextHistoryStorage = {
    getItem: () => "{坏的 json",
    setItem: () => {
      throw new Error("quota");
    },
  };
  assert.deepEqual(readTextHistory(broken), {});
  writeTextHistory({ author: ["崔总"] }, broken); // 不抛
});

test("记住一条值会同时落到存储里，且读回来还在", () => {
  const storage = fakeStorage();
  writeTextHistory({ author: ["张三"] }, storage);
  assert.deepEqual(readTextHistory(storage), { author: ["张三"] });
  assert.ok(storage.data.get(TEXT_HISTORY_STORAGE_KEY)?.includes("\"version\":1"));
});

test("rememberTextValue 走信号：同字段连续记两次只留一条并置顶，字段之间互不影响", () => {
  resetTextHistoryForTests(fakeStorage());
  rememberTextValue("author", "崔总");
  rememberTextValue("author", "张三");
  rememberTextValue("author", "崔总");
  rememberTextValue("city", "杭州");
  assert.deepEqual([...recentTextValues("author")], ["崔总", "张三"]);
  assert.deepEqual([...recentTextValues("city")], ["杭州"]);
  assert.deepEqual([...recentTextValues("country")], [], "没记过的字段是空的");
});

test("记住空白值不会清掉历史", () => {
  resetTextHistoryForTests(fakeStorage());
  rememberTextValue("author", "崔总");
  rememberTextValue("author", "   ");
  assert.deepEqual([...recentTextValues("author")], ["崔总"]);
});
