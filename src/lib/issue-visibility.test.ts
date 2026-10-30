import assert from "node:assert/strict";
import test from "node:test";

import { showsLatestRow } from "./issue-visibility.ts";

test("最近编辑：与定稿指纹相同就整行不画", () => {
  assert.equal(showsLatestRow({ latestMatchesIssue: true }), false);
});

test("最近编辑：改过参数（指纹不同）就回来", () => {
  assert.equal(showsLatestRow({ latestMatchesIssue: false }), true);
});

test("最近编辑：列表还没回来时先显示（不让人以为左侧坏了）", () => {
  assert.equal(showsLatestRow(null), true);
  assert.equal(showsLatestRow({}), true, "缺字段按「没匹配」处理");
});
