/**
 * 拖动纯逻辑的测试（`specs/library-recycle-bin.md` §5、§10）。
 *
 * 拖动本身（指针捕获、阈值、帧节流）在 `lib/pointer-drag.ts` 有测试；
 * 这里钉的是与业务有关、最容易写错的一条：**拖哪几张**（多选语义）。
 *
 * 落点判定（`photoDropTargetAt`）走 `document.elementFromPoint`，需要真实 DOM ——
 * 归冒烟/实机（与 export 是**同一套判定**：两边都用 `lib/pointer-drag.ts` 的 `dropHostAt`）。
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import { dragIdsFor } from "./photo-drag.ts";

test("dragIdsFor：拖选中项带整批，拖未选中项只带它一个", () => {
  const selected = new Set(["1", "2", "3"]);
  assert.deepEqual(dragIdsFor("2", selected), ["1", "2", "3"], "选中项 → 整批");
  assert.deepEqual(dragIdsFor("9", selected), ["9"], "未选中项 → 只它自己");
  assert.deepEqual(dragIdsFor("1", new Set<string>()), ["1"], "没有任何选中时也只它自己");
});
