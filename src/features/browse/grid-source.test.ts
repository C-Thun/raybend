/**
 * 浏览侧数据源（`browseSource`）的回归测试。
 *
 * 这里钉住的是**缓存键**：显示序按「时间线数组的身份」缓存，而「按时间」开关只改偏好、
 * 不动时间线 —— 开关没进缓存键时，`slices()` 会一直返回切换前那一份，表现为
 * 「按了没反应」（人类 2026-09-20 报的「库目录下全部照片在同一时段时，按时间不起效」）。
 *
 * 这条路径没有响应式（Solid 的依赖追踪不在单测范围内），所以直接验缓存逻辑本身：
 * **同一个时间线数组、只换开关**，`slices()` 必须跟着变。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { browseSource } from "./grid-source.ts";
import type { BrowseStore } from "./store.ts";

/** 东八区（与 `lib/time-group.ts` 的测试同一口径） */
const CST = 480;
/** 2026-08-15 09:00（+08:00） */
const T0 = Date.UTC(2026, 7, 15, 9, 0) - CST * 60_000;

/** 两分钟内的两张 —— 同一天、**同一时间片**（这正是人类报的那一档） */
const ENTRIES = [
  { id: 1, relPath: "photos/2026-08-15/MY0001.JPG", takenAt: T0 },
  { id: 2, relPath: "photos/2026-08-15/MY0002.JPG", takenAt: T0 + 60_000 },
] as const;

function sourceWith(byTime: () => boolean) {
  const store = {
    timeline: () => ENTRIES,
    itemById: (id: number) => ({ id, takenAtOffsetMin: CST }),
  } as unknown as BrowseStore;
  return browseSource({
    store,
    root: () => "C:/Photos/demo",
    thumbs: { get: () => ({ status: "idle", url: null }), request: () => {}, clear: () => {} },
    tileStep: () => 4,
    setTileStep: () => {},
    commitTileStep: () => {},
    grouped: byTime,
    infoMode: () => "off",
  });
}

test("按时间：开关一变，slices 立刻重算（时间线没换也要变）", () => {
  let byTime = false;
  const source = sourceWith(() => byTime);

  assert.equal(source.slices(), undefined, "关着时是平铺，没有分组");

  byTime = true;
  const slices = source.slices();
  assert.notEqual(slices, undefined, "开关打开必须立刻有分组（不能被旧缓存挡住）");
  assert.equal(slices?.length, 1, "两张在同一时间片里 → 只有一片");
  assert.equal(slices?.[0]?.count, 2);
  assert.equal(slices?.[0]?.dayId, "2026-08-15");

  byTime = false;
  assert.equal(source.slices(), undefined, "关掉也要立刻恢复平铺");

  // 再开一次：仍然要算得出来（缓存不能在两个状态之间来回错位）
  byTime = true;
  assert.equal(source.slices()?.length, 1);
});
