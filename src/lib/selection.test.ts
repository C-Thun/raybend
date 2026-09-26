/**
 * 选择模型的单元测试。
 *
 * 区间选择是「一次误操作就选错一大片」的地方，所以边界必须钉死：
 * 反向区间（目标在主选之前）、没有主选、主选已失效、目标不在列表里、
 * 空列表、重复 id。批量排除的**反转**语义与整段开关也在这里钉住。
 *
 * ⚠️ **两端算法固定不变**（人类 2026-09-26）：区间永远是
 * 「上一次的主选（不含） → 本次点的主选（含）」，与「当前所有选中范围」无关。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  applySelection,
  clearSelection,
  clickMode,
  EMPTY_SELECTION,
  extendSelection,
  focusSelection,
  hasSelection,
  invertSet,
  pruneSelection,
  selectAll,
  selectionCount,
  toggleGroupSelection,
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

test("Shift 区间：把「上一次的主选（不含）」到「本次点的主选（含）」都置为选中", () => {
  // 主选是 b（刚点过的那张），另外 a 与 z 是散在外面的选中项
  const next = applySelection(state(["a", "b", "z"], "b"), LIST, "d", "range");
  assert.deepEqual(
    sorted(next),
    ["a", "b", "c", "d", "z"],
    "c / d 被选中，起点主选 b 不动，区间外的 a / z 原样",
  );
  assert.equal(next.anchor, "d", "本次点的那张成为新的主选");
});

test("Shift 区间：已经选中的不会被清掉（区间外的一律不动）", () => {
  const next = applySelection(state(["a", "c", "e"], "c"), LIST, "e", "range");
  assert.deepEqual(sorted(next), ["a", "c", "d", "e"], "d 补进来，e 本来就在");
});

test("Shift 区间：目标在主选之前也能算（反向）", () => {
  const next = applySelection(state(["d"], "d"), LIST, "b", "range");
  assert.deepEqual(sorted(next), ["b", "c", "d"], "主选 d 保留，b / c 被选中");
});

test("Shift 区间：主选与目标相同 → 选择集合不变，主选仍是它", () => {
  const next = applySelection(state(["a", "b"], "c"), LIST, "c", "range");
  assert.deepEqual(sorted(next), ["a", "b"], "主选就是自己，中间没有别人");
  assert.equal(next.anchor, "c");
});

test("Shift 区间：`BROWSE.md` §5.2 的 ABCDE 走查（人类 2026-09-26 重新描述）", () => {
  const letters = ["a", "b", "c", "d", "e"];
  // 1. 点 A → A 选中，A 成为主选
  let state2 = applySelection(state([], null), letters, "a", "replace");
  assert.deepEqual(sorted(state2), ["a"]);
  // 2. Shift 点 C → B、C 置为选中（**不含**主选 A，它本来就在）
  state2 = applySelection(state2, letters, "c", "range");
  assert.deepEqual(sorted(state2), ["a", "b", "c"]);
  assert.equal(state2.anchor, "c", "主选跟着走");
  // 3. Shift 点 E → 从 C 开始计，D、E 置为选中（B 不能被清掉）
  state2 = applySelection(state2, letters, "e", "range");
  assert.deepEqual(sorted(state2), ["a", "b", "c", "d", "e"], "之前选中的不能被清掉");
  // 4. Shift 再点 A → A..E 本来就全选中，集合不变（**不再翻转**）
  state2 = applySelection(state2, letters, "a", "range");
  assert.deepEqual(sorted(state2), ["a", "b", "c", "d", "e"]);
  assert.equal(state2.anchor, "a", "主选回到 A");
});

test("Shift 区间：没有主选 → 退化成单张（不猜位置）", () => {
  const next = applySelection(state(["a", "b"], null), LIST, "d", "range");
  assert.deepEqual(sorted(next), ["d"]);
  assert.equal(next.anchor, "d");
});

test("Shift 区间：主选已不在列表里 → 退化成单张", () => {
  const next = applySelection(state(["a"], "gone"), LIST, "c", "range");
  assert.deepEqual(sorted(next), ["c"]);
  assert.equal(next.anchor, "c");
});

test("主选被 Ctrl 取消后按 Shift：起点仍然是那个主选（两端算法固定不变）", () => {
  // 人类 2026-09-26：“主选 = 最后鼠标点到哪里”，而区间两端永远是“前一个主选 → 当前主选”，
  // 与“当前所有选中范围”无关。这里把那个差别钉住：主选自己不在选中集里，区间仍然从它算。
  let state2 = applySelection(state(["a", "b", "c"], "c"), LIST, "b", "toggle");
  assert.deepEqual(sorted(state2), ["a", "c"], "b 被 Ctrl 取消，主选却落到 b");
  state2 = applySelection(state2, LIST, "e", "range");
  assert.deepEqual(sorted(state2), ["a", "c", "d", "e"], "从 b（主选）到 e，b 自己不补回来");
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

test("`extendSelection`（导出画廊的只加不减）：并进现有选择，锚点保留", () => {
  const first = extendSelection(state(["z"], "z"), ["a", "b"]);
  assert.deepEqual(sorted(first), ["a", "b", "z"]);
  assert.equal(first.anchor, "z", "已有锚点不动");

  const second = extendSelection(EMPTY_SELECTION, ["a", "b"]);
  assert.equal(second.anchor, "a", "没有锚点时用这一组的开头");

  const empty = extendSelection(state(["z"]), []);
  assert.deepEqual(sorted(empty), ["z"]);
});

test("整段开关：全选中 → 全取消；其它段不动", () => {
  const onBoth = toggleGroupSelection(state(["a", "b", "z"], "z"), ["a", "b"]);
  assert.deepEqual(sorted(onBoth), ["z"], "a / b 已全选 → 整段取消");
  assert.equal(onBoth.anchor, "a", "主选走到这一组的开头");

  const off = toggleGroupSelection(state(["z"], "z"), ["a", "b"]);
  assert.deepEqual(sorted(off), ["a", "b", "z"], "未全选 → 整段置为选中");
  assert.equal(off.anchor, "a");
});

test("整段开关：部分选中也是“补齐”，不是逐项反转", () => {
  const next = toggleGroupSelection(state(["b"]), ["a", "b", "c"]);
  assert.deepEqual(sorted(next), ["a", "b", "c"], "已选的 b 不能被翻掉");
});

test("整段开关：空组不动选择", () => {
  const before = state(["a"], "a");
  assert.equal(toggleGroupSelection(before, []), before);
});

test("全选与清空", () => {
  const all = selectAll(LIST);
  assert.deepEqual(sorted(all), [...LIST].sort());
  assert.equal(all.anchor, "a");
  assert.equal(selectAll([]), EMPTY_SELECTION);
  assert.equal(clearSelection().ids.size, 0);
});

test("focusSelection：只挪锚点，不改多选集合", () => {
  const current = state(["a", "b", "c"], "a");
  const focused = focusSelection(current, "c");
  assert.equal(focused.ids, current.ids, "集合沿用同一份，只改变锚点");
  assert.deepEqual(sorted(focused), ["a", "b", "c"]);
  assert.equal(focused.anchor, "c");

  assert.equal(focusSelection(focused, "c"), focused, "重复聚焦不制造新状态");
  assert.equal(focusSelection(focused, "z"), focused, "目标不在选择中就不动");
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

// ─────────────────── 修饰键 → 选择模式（tiles 与胶片带共用）───────────────────

test("clickMode：不加修饰键 = 替换（先清掉原选中）", () => {
  assert.equal(clickMode({ shiftKey: false, ctrlKey: false, metaKey: false }), "replace");
});

test("clickMode：Shift = 区间选中，Ctrl / Cmd = 自由多选，不加修饰键 = 替换", () => {
  assert.equal(clickMode({ shiftKey: true, ctrlKey: false, metaKey: false }), "range");
  assert.equal(clickMode({ shiftKey: false, ctrlKey: true, metaKey: false }), "toggle");
  assert.equal(clickMode({ shiftKey: false, ctrlKey: false, metaKey: true }), "toggle");
});

test("clickMode：Shift 与 Ctrl 同按 → 两个都不算，退化成单击（人类 2026-09-26）", () => {
  assert.equal(clickMode({ shiftKey: true, ctrlKey: true, metaKey: false }), "replace");
  assert.equal(clickMode({ shiftKey: true, ctrlKey: false, metaKey: true }), "replace");
  assert.equal(clickMode({ shiftKey: true, ctrlKey: true, metaKey: true }), "replace");
});

test("导出反 Ctrl：默认多选，Ctrl/Meta 单选，Shift 优先，同按退化成单击", () => {
  assert.equal(clickMode({ shiftKey: false, ctrlKey: false, metaKey: false }, true), "toggle");
  assert.equal(clickMode({ shiftKey: false, ctrlKey: true, metaKey: false }, true), "replace");
  assert.equal(clickMode({ shiftKey: false, ctrlKey: false, metaKey: true }, true), "replace");
  assert.equal(clickMode({ shiftKey: true, ctrlKey: false, metaKey: false }, true), "range");
  // 同按退化到**这一侧的「单击」**（导出是反 Ctrl，所以是 toggle，不是 replace）
  assert.equal(clickMode({ shiftKey: true, ctrlKey: true, metaKey: false }, true), "toggle");
  assert.equal(clickMode({ shiftKey: true, ctrlKey: false, metaKey: true }, true), "toggle");
});
