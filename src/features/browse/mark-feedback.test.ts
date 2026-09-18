/**
 * 「标记之后说什么」的测试。
 *
 * 口径：**只在出问题时说话**（被锁挡住 / 什么都没改）；
 * 正常改动了就不说 —— 照片上的标记变化本身就是反馈。
 */

import assert from "node:assert/strict";
import test from "node:test";
import { markNotice } from "./mark-feedback.ts";
import type { MarkResult } from "../../api/types.ts";

function result(overrides: Partial<MarkResult> = {}): MarkResult {
  return {
    changed: 3,
    skippedLocked: [],
    undoLabel: null,
    redoLabel: null,
    canUndo: true,
    canRedo: false,
    ...overrides,
  };
}

test("markNotice：改成功了就不说话（照片上的变化就是反馈）", () => {
  assert.equal(markNotice(result({ changed: 5 })), null);
});

test("markNotice：没做动作（null）也不说话", () => {
  assert.equal(markNotice(null), null);
});

test("markNotice：有照片被锁挡住 → 报数量（这是用户看不出来的那类）", () => {
  const notice = markNotice(result({ changed: 2, skippedLocked: [7, 9] }));
  assert.deepEqual(notice, { key: "browse.markSkippedLocked", count: 2 });
});

test("markNotice：被锁挡住优先于「什么都没改」（信息量更大）", () => {
  const notice = markNotice(result({ changed: 0, skippedLocked: [3] }));
  assert.equal(notice?.key, "browse.markSkippedLocked");
  assert.equal(notice?.count, 1);
});

test("markNotice：一个都没改（例如全都已经是这个值）→ 说一句", () => {
  const notice = markNotice(result({ changed: 0, skippedLocked: [] }));
  assert.deepEqual(notice, { key: "browse.markNothingChanged", count: 0 });
});

test("markNotice：全部被锁挡住时两个条件都成立，仍然先报锁", () => {
  const notice = markNotice(result({ changed: 0, skippedLocked: [1, 2, 3] }));
  assert.equal(notice?.key, "browse.markSkippedLocked");
  assert.equal(notice?.count, 3);
});

test("markNotice：畸形响应（后端没给这个字段 / 直接 undefined）不会炸", () => {
  assert.equal(markNotice(undefined as unknown as MarkResult), null);
  assert.equal(
    markNotice({ changed: 2, undoLabel: null, redoLabel: null, canUndo: true, canRedo: false } as MarkResult),
    null,
    "少了 skippedLocked 也不炸",
  );
  assert.deepEqual(
    markNotice({ changed: 1, skippedLocked: "bad" } as unknown as MarkResult),
    null,
  );
});
