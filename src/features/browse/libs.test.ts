/**
 * 库列表紧缩 / 展开规则的测试（`BROWSE.md` §4.2）。
 *
 * 重点钉的是上一版实现做错的那几条：
 * * **≤3 个库时不显示「查看所有库」**、也不留 3.5 张卡的空高；
 * * 只有 >3 时才出现伪卡片，且容器要**裁**（3.5 张卡）；
 * * 展开态一律全部显示、且不裁。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  compactList,
  COMPACT_REPO_LIMIT,
  EXPANDED_IDLE_MS,
  listHeightStyle,
  TREE_MIN_HEIGHT_PX,
} from "./libs.ts";

const libs = (n: number): string[] => Array.from({ length: n }, (_v, i) => `库${i + 1}`);

test("≤3 个库：全部显示、没有伪卡片、也不裁", () => {
  for (const n of [0, 1, 2, 3]) {
    const view = compactList(libs(n), false);
    assert.deepEqual(view.visible, libs(n), `${n} 个库应当全显示`);
    assert.equal(view.showAll, false, `${n} 个库不该出现「查看所有库」`);
    assert.equal(view.clipped, false, `${n} 个库不该留 3.5 张卡的空高`);
  }
});

test("4 个库：显示 3 张 + 伪卡片，容器要裁到 3.5 张", () => {
  const view = compactList(libs(4), false);
  assert.deepEqual(view.visible, libs(COMPACT_REPO_LIMIT));
  assert.equal(view.showAll, true);
  assert.equal(view.clipped, true);
});

test("库多时也只显示 3 张（剩下的靠展开看）", () => {
  const view = compactList(libs(40), false);
  assert.equal(view.visible.length, 3);
  assert.equal(view.showAll, true);
});

test("展开态：全部显示、可滚、不裁（伪卡片也不该再出现）", () => {
  const view = compactList(libs(40), true);
  assert.equal(view.visible.length, 40);
  assert.equal(view.showAll, false);
  assert.equal(view.clipped, false);
});

test("空列表不炸：不高也不裁", () => {
  const view = compactList([], false);
  assert.deepEqual(view.visible, []);
  assert.equal(view.showAll, false);
  assert.equal(view.clipped, false);
});

test("可见的是**前**几张（排序前 3 名才有资格进缩起态）", () => {
  const view = compactList(["A", "B", "C", "D", "E"], false);
  assert.deepEqual(view.visible, ["A", "B", "C"]);
});

test("高度样式：要裁才给高度，用的是密度令牌而不是写死的像素", () => {
  assert.equal(listHeightStyle(false), undefined);
  const style = listHeightStyle(true);
  assert.ok(style !== undefined);
  assert.match(String(style), /--card-h/, "卡高必须走令牌（紧凑/宽松两档不同）");
  assert.match(String(style), /3\.5/);
});

test("常量的量级与语义（改的时候要连着文档一起改）", () => {
  assert.equal(COMPACT_REPO_LIMIT, 3);
  assert.equal(EXPANDED_IDLE_MS, 15_000, "15 秒没再点库就收起");
  assert.ok(TREE_MIN_HEIGHT_PX > 0 && TREE_MIN_HEIGHT_PX < 200, "目录树的最小高度是个小数字");
});
