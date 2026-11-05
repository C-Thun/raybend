/** 建议输入的两源合并、前缀筛选与上下键移动（纯函数）。 */

import assert from "node:assert/strict";
import test from "node:test";

import {
  filterSuggestions,
  mergeSuggestions,
  moveHighlight,
  type LibraryValue,
} from "./suggest.ts";

const library = (value: string, count: number): LibraryValue => ({ value, count });

test("最近输入排在库内已有值前面；同值只留最近那条", () => {
  const merged = mergeSuggestions(["张三", "崔总"], [library("崔总", 9), library("李四", 3)], 20);
  assert.deepEqual(merged, [
    { value: "张三", count: null },
    { value: "崔总", count: null },
    { value: "李四", count: 3 },
  ]);
});

test("合并遵守候选上限（两源总额）", () => {
  const merged = mergeSuggestions(
    ["a", "b", "c"],
    [library("d", 1), library("e", 1)],
    4,
  );
  assert.deepEqual(merged.map((entry) => entry.value), ["a", "b", "c", "d"]);
  assert.equal(mergeSuggestions([], [library("x", 1)], 0).length, 0);
});

test("空白与首尾空格被规整：不会出现两条看起来一样的候选", () => {
  const merged = mergeSuggestions([" 杭州 "], [library("杭州", 2), library("  ", 1)], 20);
  assert.deepEqual(merged, [{ value: "杭州", count: null }]);
});

test("focus 时（空查询）返回全部候选", () => {
  const merged = mergeSuggestions(["崔总"], [library("杭州", 1)], 20);
  assert.deepEqual(filterSuggestions(merged, ""), merged);
  assert.deepEqual(filterSuggestions(merged, "   "), merged);
});

test("前缀命中优先；前缀一条都没有时退回子串", () => {
  const merged = mergeSuggestions(["杭州市", "浙江省杭州市", "杭州"], [], 20);
  assert.deepEqual(
    filterSuggestions(merged, "杭州").map((entry) => entry.value),
    ["杭州市", "杭州"],
    "只有以「杭州」开头的两条",
  );
  assert.deepEqual(
    filterSuggestions(merged, "省杭").map((entry) => entry.value),
    ["浙江省杭州市"],
    "没有前缀命中时用子串兜底",
  );
});

test("大小写不敏感（latin），中文逐字比较", () => {
  const merged = mergeSuggestions(["Hangzhou", "hangzhou lake"], [], 20);
  assert.deepEqual(
    filterSuggestions(merged, "HANGZHOU L").map((entry) => entry.value),
    ["hangzhou lake"],
    "前缀比对手小写，只命中真正以它开头的那条",
  );
  assert.deepEqual(filterSuggestions(merged, "西湖"), []);
});

test("上下键到头就停、不回绕；没选时上键回到 -1", () => {
  assert.equal(moveHighlight(-1, 1, 3), 0);
  assert.equal(moveHighlight(0, 1, 3), 1);
  assert.equal(moveHighlight(2, 1, 3), 2, "底端不回绕到第一条");
  assert.equal(moveHighlight(-1, -1, 3), -1);
  assert.equal(moveHighlight(0, -1, 3), -1);
  assert.equal(moveHighlight(-1, 1, 0), -1, "空列表没有可选项");
});
