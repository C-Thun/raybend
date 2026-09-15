/**
 * 选择模型的单元测试。
 *
 * 区间选择是「一次误操作就选错一大片」的地方，所以边界必须钉死：
 * 反向区间（目标在锚点之前）、没有锚点、锚点已失效、目标不在列表里、
 * 空列表、重复 id。批量排除的**反转**语义也在这里钉住。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  applySelection,
  clearSelection,
  EMPTY_SELECTION,
  extendSelection,
  hasSelection,
  invertSet,
  pruneSelection,
  selectAll,
  selectionCount,
  type SelectionState,
} from "./selection.ts";

const LIST = ["a", "b", "c", "d", "e"];

function state(ids: string[], anchor: string | null = null): SelectionState {
  return { ids: new Set(ids), anchor };
}

function sorted(state: SelectionState): string[] {
  return [...state.ids].sort();
}

test("单张点选：替换掉之前的选择，锚点跟着走", () => {
  const next = applySelection(state(["a", "b"], "a"), LIST, "c", "replace");
  assert.deepEqual(sorted(next), ["c"]);
  assert.equal(next.anchor, "c");
});

test("Ctrl 增减：只影响这一张，其余不动", () => {
  const added = applySelection(state(["a"]), LIST, "c", "toggle");
  assert.deepEqual(sorted(added), ["a", "c"]);
  assert.equal(added.anchor, "c");

  const removed = applySelection(added, LIST, "a", "toggle");
  assert.deepEqual(sorted(removed), ["c"]);
  assert.equal(removed.anchor, "a", "取消选中也要留下锚点（接着 Shift 才顺");
});

test("Shift 区间：从锚点到目标（含两端），替换旧选择", () => {
  const next = applySelection(state(["a"], "b"), LIST, "d", "range");
  assert.deepEqual(sorted(next), ["b", "c", "d"]);
  assert.equal(next.anchor, "d");
});

test("Shift 区间：目标在锚点之前也能算（反向）", () => {
  const next = applySelection(state([], "d"), LIST, "b", "range");
  assert.deepEqual(sorted(next), ["b", "c", "d"]);
});

test("Shift 区间：锚点与目标相同 → 只选它自己", () => {
  const next = applySelection(state(["a", "b"], "c"), LIST, "c", "range");
  assert.deepEqual(sorted(next), ["c"]);
});

test("Shift 区间：没有锚点 → 退化成单张（不猜位置）", () => {
  const next = applySelection(state(["a", "b"], null), LIST, "d", "range");
  assert.deepEqual(sorted(next), ["d"]);
  assert.equal(next.anchor, "d");
});

test("Shift 区间：锚点已不在列表里 → 退化成单张", () => {
  const next = applySelection(state(["a"], "gone"), LIST, "c", "range");
  assert.deepEqual(sorted(next), ["c"]);
  assert.equal(next.anchor, "c");
});

test("目标不在当前列表里：选择原样不动（不静默乱选）", () => {
  const before = state(["a", "b"], "a");
  const next = applySelection(before, LIST, "zzz", "replace");
  assert.equal(next, before);
});

test("空列表：怎么点都不动", () => {
  for (const mode of ["replace", "toggle", "range"] as const) {
    const next = applySelection(EMPTY_SELECTION, [], "a", mode);
    assert.equal(next.ids.size, 0);
  }
});

test("全选当天 / 全选此段：并进现有选择，锚点保留", () => {
  const first = extendSelection(state(["z"], "z"), ["a", "b"]);
  assert.deepEqual(sorted(first), ["a", "b", "z"]);
  assert.equal(first.anchor, "z", "已有锚点不动");

  const second = extendSelection(EMPTY_SELECTION, ["a", "b"]);
  assert.equal(second.anchor, "a", "没有锚点时用这一组的开头");

  const empty = extendSelection(state(["z"]), []);
  assert.deepEqual(sorted(empty), ["z"]);
});

test("全选与清空", () => {
  const all = selectAll(LIST);
  assert.deepEqual(sorted(all), [...LIST].sort());
  assert.equal(all.anchor, "a");
  assert.equal(selectAll([]), EMPTY_SELECTION);
  assert.equal(clearSelection().ids.size, 0);
});

test("hasSelection / selectionCount：按当前列表算", () => {
  const mixed = state(["a", "zzz"]);
  assert.equal(hasSelection(mixed), true);
  assert.equal(selectionCount(mixed, LIST), 1, "不在列表里的旧选择不算数");
  assert.equal(hasSelection(EMPTY_SELECTION), false);
});

test("pruneSelection：换目录后旧选择被收敛", () => {
  const pruned = pruneSelection(state(["a", "zzz"], "zzz"), LIST);
  assert.deepEqual(sorted(pruned), ["a"]);
  assert.equal(pruned.anchor, null, "锚点没了就该清掉");

  const kept = pruneSelection(state(["a", "b"], "b"), LIST);
  assert.deepEqual(sorted(kept), ["a", "b"]);
  assert.equal(kept.anchor, "b");

  // 没变化时返回**同一个对象**（避免无谓的重渲染）
  const same = state(["a"], "a");
  assert.equal(pruneSelection(same, LIST), same);
});

test("批量排除 = 反转（不是一律排除）", () => {
  const inverted = invertSet(new Set(["a"]), ["a", "b"]);
  assert.deepEqual([...inverted].sort(), ["b"], "已排除的 a 被取消，b 被排除");

  const again = invertSet(inverted, ["a", "b"]);
  assert.equal(again.size, 1, "再反转一次就回来");
  assert.ok(again.has("a"));
});

test("反转：空基准、空 key、重复 key 都不出错", () => {
  assert.deepEqual([...invertSet(new Set(), [])], []);
  assert.deepEqual([...invertSet(new Set(), ["a", "a"])], [], "同一个 id 反转两次 = 没变");
  assert.deepEqual([...invertSet(new Set(["a"]), [])], ["a"]);
});
