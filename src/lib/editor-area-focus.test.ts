import assert from "node:assert/strict";
import { test } from "node:test";

import { nextAreaFocus } from "./editor-area-focus.ts";

test("空格：只有默认层时什么都不做", () => {
  assert.equal(nextAreaFocus(null, [], null), undefined);
  assert.equal(nextAreaFocus(3, [], 3), undefined, "层被删光后也不该回到一个不存在的 id");
});

test("空格：在层里 → 回默认层", () => {
  assert.equal(nextAreaFocus(7, [7, 8], 7), null);
  assert.equal(nextAreaFocus(8, [7, 8], 7), null, "当前层与「刚才那一层」无关");
});

test("空格：从默认层回到刚才那一层（不是列表末尾）", () => {
  assert.equal(nextAreaFocus(null, [7, 8, 9], 7), 7);
});

test("空格：记着的那层已被删 → 退到最后一层", () => {
  assert.equal(nextAreaFocus(null, [7, 8], 9), 8);
  assert.equal(nextAreaFocus(null, [7, 8], null), 8);
});

test("空格：两层之间反复横跳是稳定的", () => {
  const ids = [4, 5];
  let current: number | null = 5;
  let last: number | null = 5;
  for (let i = 0; i < 4; i += 1) {
    const next = nextAreaFocus(current, ids, last);
    assert.notEqual(next, undefined);
    if (next === null) current = null;
    else if (next !== undefined) {
      current = next;
      last = next;
    }
  }
  assert.equal(last, 5, "横跳不会把「刚才那一层」换成别的层");
});
