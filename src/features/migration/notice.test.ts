/**
 * `notice.ts` 的单测：升级遮罩的状态机（按库种类记账）。
 *
 * 重点覆盖**边界**而不是正常路径：没 Start 就来的 Done、同一类库连发两次 Start、
 * 两类库交错、以及「取哪一条显示」的稳定性。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { activeNotice, applyNotice, NO_MIGRATIONS, type MigrationMap } from "./notice.ts";
import type { MigrationNotice } from "../../api/types.ts";

function notice(
  kind: MigrationNotice["kind"],
  running: boolean,
  from = 3,
  to = 4,
): MigrationNotice {
  return { kind, label: `库-${kind}`, from, to, running };
}

/** 把一串通知喂进去，返回最终状态。 */
function feed(...notices: MigrationNotice[]): MigrationMap {
  return notices.reduce(applyNotice, NO_MIGRATIONS);
}

test("空表不显示任何东西", () => {
  assert.equal(activeNotice(NO_MIGRATIONS), null);
});

test("Start 显示、Done 撤掉", () => {
  const started = feed(notice("app", true));
  assert.equal(activeNotice(started)?.kind, "app");

  const done = feed(notice("app", true), notice("app", false));
  assert.equal(done.size, 0);
  assert.equal(activeNotice(done), null);
});

test("迁移失败也会发 Done —— 遮罩不会卡在屏幕上", () => {
  // 外壳把「失败」表达为 Done + 命令 reject；状态机这一侧只认 Done
  const map = feed(notice("catalog", true), notice("catalog", false));
  assert.equal(map.size, 0);
});

test("没 Start 过的 Done 是空操作（宁可什么都不发生）", () => {
  const map = feed(notice("catalog", false));
  assert.equal(map.size, 0);
  assert.equal(activeNotice(map), null);
});

test("同一类库连发两次 Start 不重复计数", () => {
  const map = feed(notice("app", true, 1, 2), notice("app", true, 1, 3));
  assert.equal(map.size, 1);
  assert.equal(activeNotice(map)?.to, 3, "后来的通知应当覆盖前一条的版本信息");
});

test("两类库交错：一类结束不影响另一类", () => {
  const map = feed(
    notice("app", true),
    notice("catalog", true),
    notice("app", false),
  );
  assert.equal(map.size, 1);
  assert.equal(activeNotice(map)?.kind, "catalog");
});

test("同时升级两类库时按版本跨度取，且顺序稳定", () => {
  const small = notice("app", true, 3, 4);
  const big = notice("thumbs", true, 1, 5);
  const a = activeNotice(feed(small, big));
  const b = activeNotice(feed(big, small));
  assert.equal(a?.kind, "thumbs", "跨度大的更相关");
  assert.equal(b?.kind, "thumbs", "与喂入顺序无关");

  const tieA = activeNotice(feed(notice("thumbs", true), notice("app", true)));
  assert.equal(tieA?.kind, "app", "跨度相同按 kind 字典序（渲染稳定）");
});

test("入参不被修改（纯函数）", () => {
  const before = feed(notice("app", true));
  const after = applyNotice(before, notice("catalog", true));
  assert.equal(before.size, 1, "旧表不能被动过");
  assert.equal(after.size, 2);
  assert.notEqual(before, after);
});
