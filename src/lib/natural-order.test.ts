import assert from "node:assert/strict";
import { test } from "node:test";
import { compareNatural } from "./natural-order.ts";

/** 断言 `a < b < c …` 这个顺序（用比较器排出来的结果） */
function assertOrder(input: readonly string[], expected: readonly string[]): void {
  const sorted = [...input].sort(compareNatural);
  assert.deepEqual(sorted, [...expected]);
}

test("数字段按数值比，不按字符比", () => {
  // 这就是它存在的理由：字符串比较会给出 P1000020 < P1000019
  assertOrder(
    ["P1000020.JPG", "P1000019.JPG", "P1000100.JPG", "P1000009.JPG"],
    ["P1000009.JPG", "P1000019.JPG", "P1000020.JPG", "P1000100.JPG"],
  );
  assertOrder(["IMG_10", "IMG_2", "IMG_1"], ["IMG_1", "IMG_2", "IMG_10"]);
  assertOrder(["P2", "P10", "P1"], ["P1", "P2", "P10"]);
});

test("前导零不影响数值序；数值完全相同时由原始串兜底（顺序仍然确定）", () => {
  // P007 与 P7 的数值段相等 → 按规则 4 比原始串，`0` < `7` ⇒ P007 在前。
  // 这是**确定的**（相机文件名几乎不会出现前导零，这里只要求「不漂移」）。
  assertOrder(["P007.JPG", "P7.JPG", "P10.JPG"], ["P007.JPG", "P7.JPG", "P10.JPG"]);
  assertOrder(["a000", "a0", "a1"], ["a0", "a000", "a1"]);
  // 真正有意义的那条：前导零不许把 P9 排到 P10 后面
  assertOrder(["P09", "P10"], ["P09", "P10"]);
});

test("同一张照片的 JPG 与 RAW 会挨着（本功能的实际目的）", () => {
  const input = [
    "P1000019.RW2",
    "P1000019.JPG",
    "P1000020.JPG",
    "P1000020.RW2",
  ];
  const sorted = [...input].sort(compareNatural);
  assert.deepEqual(sorted, [
    "P1000019.JPG",
    "P1000019.RW2",
    "P1000020.JPG",
    "P1000020.RW2",
  ]);
  // 关键性质：同名的两张必须**相邻**（中间不许插别的照片）
  const at = sorted.indexOf("P1000019.JPG");
  assert.equal(sorted[at + 1], "P1000019.RW2");
});

test("大小写不敏感，全等时用原始串兜底（顺序确定）", () => {
  assertOrder(["b.JPG", "A.JPG", "a.JPG"], ["A.JPG", "a.JPG", "b.JPG"]);
  // 两次排序结果必须一致（不许漂移）
  const once = ["p1.JPG", "P1.jpg", "P1.JPG"].sort(compareNatural);
  const twice = [...once].sort(compareNatural);
  assert.deepEqual(twice, once);
});

test("前缀关系：短的在前", () => {
  assertOrder(["P12", "P1", "P123"], ["P1", "P12", "P123"]);
  assertOrder(["P1.RAW", "P1"], ["P1", "P1.RAW"]);
});

test("没有数字的串也能排；中文按码位（顺序确定但不承诺「拼音序」）", () => {
  assertOrder(["b", "a", "c"], ["a", "b", "c"]);
  assertOrder(["照片", "a"], ["a", "照片"]);
  const once = ["图", "片", "照"].sort(compareNatural);
  assert.deepEqual([...once].sort(compareNatural), once);
});

test("边界：空串、纯数字、只有标点", () => {
  assert.equal(compareNatural("", ""), 0);
  assert.ok(compareNatural("", "a") < 0);
  assertOrder(["1", "10", "2", ""], ["", "1", "2", "10"]);
  assertOrder(["-_1", "-_2", "-_10"], ["-_1", "-_2", "-_10"]);
  // 全角数字与带圈数字不按数值处理：只保证「能排且稳定」
  const odd = ["P②", "P①", "P10"].sort(compareNatural);
  assert.deepEqual([...odd].sort(compareNatural), odd);
});

test("库内相对路径整体参与比较（目录部分先决出先后）", () => {
  assertOrder(
    ["photos/2026-09-13/P2.JPG", "photos/2026-09-12/P1.JPG", "photos/2026-09-13/P10.JPG"],
    ["photos/2026-09-12/P1.JPG", "photos/2026-09-13/P2.JPG", "photos/2026-09-13/P10.JPG"],
  );
});
