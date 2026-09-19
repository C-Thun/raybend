/**
 * 对比态数学的测试。
 *
 * 这些数字**必须钉死**：画幅不一致时的扣取、位移的百分比换算，错一点的表现是
 * 「一边拖一边两边错位」——人眼立刻看得出来，但很难反推出是哪一步算错了。
 * 人类点名的例子（4:3 与 3:2 混）单独一条。
 */

import assert from "node:assert/strict";
import test from "node:test";
import {
  baselineAspect,
  COMPARE_MAX,
  compareFrames,
  compareIds,
  compareLayout,
  cropToAspect,
  panPercent,
  percentToPan,
} from "./viewer-compare.ts";
import type { ComparablePhoto } from "./viewer-compare.ts";

/** 测试用照片：满足 `ComparablePhoto`，另外带几个真实对象才有的字段（验泛型会不会丢字段） */
type TestPhoto = ComparablePhoto & { path: string; fileName: string };

/** 不传宽高 = 尺寸未知（真实场景：元数据还没读到） */
function photo(id: string, width?: number, height?: number): TestPhoto {
  return {
    id,
    path: `/lib/${id}.JPG`,
    fileName: `${id}.JPG`,
    ...(width !== undefined && height !== undefined ? { natural: { width, height } } : {}),
  };
}

// ─────────────────── 该对比哪几张 ───────────────────

test("compareIds：选中不足 2 张时不是对比态", () => {
  assert.deepEqual(compareIds(new Set(), ["a", "b"]), []);
  assert.deepEqual(compareIds(new Set(["a"]), ["a", "b"]), []);
});

test("compareIds：窗口按**选择先后**排（最早选中的在最左，它就是基准）", () => {
  // 先点 c 再点 a：Set 的插入顺序就是选择先后 ⇒ [c, a]
  assert.deepEqual(compareIds(new Set(["c", "a"]), ["a", "b", "c"]), ["c", "a"]);
});

test("compareIds：列表里没有的 id 不算（换目录后的残留）", () => {
  assert.deepEqual(compareIds(new Set(["a", "zzz"]), ["a", "b"]), []);
  // 两个都在列表里才算两件（一个残留 + 一个在场 → 不足 2 张）
  assert.deepEqual(compareIds(new Set(["a", "zzz"]), ["a", "b", "zzz"]), ["a", "zzz"]);
});

test("compareIds：反选到只剩一张 → 自然退出对比", () => {
  const list = ["a", "b", "c"];
  assert.deepEqual(compareIds(new Set(["a", "b"]), list), ["a", "b"]);
  assert.deepEqual(compareIds(new Set(["a"]), list), [], "反选掉 b 之后不再是对比态");
});

// ─────────────────── 超过 4 张：只对比最近选中的 4 张（人类 2026-09-19 定） ───────────────────

test("compareIds：Ctrl 依次点 A..F → 窗口只留最近 4 张 C D E F，基准是 C", () => {
  assert.equal(COMPARE_MAX, 4);
  const list = ["A", "B", "C", "D", "E", "F"];
  // Set 的插入顺序 = 点选先后
  const selected = new Set(["A", "B", "C", "D", "E", "F"]);
  assert.deepEqual(compareIds(selected, list), ["C", "D", "E", "F"]);
});

test("compareIds：再点一张（G）→ 窗口继续滚动（D E F G），最早的那张被挤掉", () => {
  const selected = new Set(["A", "B", "C", "D", "E", "F", "G"]);
  assert.deepEqual(compareIds(selected, ["A", "B", "C", "D", "E", "F", "G"]), [
    "D",
    "E",
    "F",
    "G",
  ]);
});

test("compareIds：正好 4 张时一张都不挤", () => {
  const selected = new Set(["A", "B", "C", "D"]);
  assert.deepEqual(compareIds(selected, ["A", "B", "C", "D"]), ["A", "B", "C", "D"]);
});

test("compareIds：锚点被区间批量选挤出窗口时，挤掉窗口最早的那张、锚点放末尾", () => {
  // 场景：先点 A（锚点），再 Shift 点 F → 一次翻转把 B..F 都加进来（插入顺序 B C D E F）
  const selected = new Set(["A", "B", "C", "D", "E", "F"]);
  // 窗口本来是 C D E F，锚点 A 在外面 → 挤掉 C，锚点补到末尾
  assert.deepEqual(compareIds(selected, ["A", "B", "C", "D", "E", "F"], "A"), [
    "D",
    "E",
    "F",
    "A",
  ]);
});

test("compareIds：锚点本来就在窗口里就不动它", () => {
  const selected = new Set(["A", "B", "C", "D", "E", "F"]);
  assert.deepEqual(compareIds(selected, ["A", "B", "C", "D", "E", "F"], "F"), [
    "C",
    "D",
    "E",
    "F",
  ]);
});

test("compareIds：锚点不在选择集合里时不管它（不硬塞）", () => {
  const selected = new Set(["A", "B", "C", "D", "E", "F"]);
  assert.deepEqual(compareIds(selected, ["A", "B", "C", "D", "E", "F"], "ZZZ"), [
    "C",
    "D",
    "E",
    "F",
  ]);
});

test("compareIds：批量选择（全选那种，插入顺序 = 列表顺序）行为确定：取列表最后 4 张", () => {
  // Ctrl+A：selectAll 的插入顺序就是列表顺序；锚点 = 组内第一张（既有行为）⇒ 被拉进窗口
  const list = ["A", "B", "C", "D", "E", "F"];
  const all = new Set(list);
  assert.deepEqual(compareIds(all, list), ["C", "D", "E", "F"]);
  assert.deepEqual(compareIds(all, list, "A"), ["D", "E", "F", "A"]);
});

// ─────────────────── 画幅怎么摆 ───────────────────

test("compareLayout：2 张一排、3 张一排三个、4 张 2×2（人类 2026-09-19 定）", () => {
  assert.deepEqual(compareLayout(2), { columns: 2, rows: 1 });
  assert.deepEqual(compareLayout(3), { columns: 3, rows: 1 });
  assert.deepEqual(compareLayout(4), { columns: 2, rows: 2 });
});

test("compareLayout：非法与越界输入不炸（0 给 1×1，超过 4 张按 2×2）", () => {
  assert.deepEqual(compareLayout(0), { columns: 1, rows: 1 });
  assert.deepEqual(compareLayout(-3), { columns: 1, rows: 1 });
  assert.deepEqual(compareLayout(Number.NaN), { columns: 1, rows: 1 });
  assert.deepEqual(compareLayout(5), { columns: 2, rows: 2 });
  assert.deepEqual(compareLayout(99), { columns: 2, rows: 2 });
  assert.deepEqual(compareLayout(2.7), { columns: 2, rows: 1 }, "小数向下取整");
});

// ─────────────────── 基准比例 ───────────────────

test("baselineAspect：以**第一幅**为准", () => {
  assert.equal(baselineAspect([photo("a", 4000, 3000), photo("b", 6000, 4000)]), 4000 / 3000);
});

test("baselineAspect：第一幅尺寸未知时返回 null（不编比例）", () => {
  assert.equal(baselineAspect([photo("a")]), null);
  assert.equal(baselineAspect([photo("a", 0, 0)]), null);
  assert.equal(baselineAspect([]), null);
});

// ─────────────────── 扣等比例区域 ───────────────────

test("cropToAspect：比例一致时一张都不扣", () => {
  assert.deepEqual(cropToAspect({ width: 3000, height: 2000 }, 1.5), {
    x: 0,
    y: 0,
    width: 3000,
    height: 2000,
  });
});

test("cropToAspect：比基准**宽**时扣两侧（保留高度、居中）", () => {
  // 16:9 的图，要 4:3 的区域 → 保留高度，两边各扣掉
  const crop = cropToAspect({ width: 1920, height: 1080 }, 4 / 3);
  assert.equal(crop.height, 1080);
  assert.equal(Math.round(crop.width), 1440);
  assert.equal(crop.x, (1920 - 1440) / 2);
  assert.equal(crop.y, 0);
});

test("cropToAspect：比基准**窄/高**时扣上下（保留宽度、居中）", () => {
  // 竖向 3:4 的图，要 4:3 的区域 → 保留宽度，上下扣掉
  const crop = cropToAspect({ width: 3000, height: 4000 }, 4 / 3);
  assert.equal(crop.width, 3000);
  assert.equal(crop.height, 2250);
  assert.equal(crop.y, 875);
  assert.equal(crop.x, 0);
});

test("cropToAspect：竖向照片也能被扣（横基准配竖图）", () => {
  const crop = cropToAspect({ width: 3000, height: 4000 }, 3 / 2);
  assert.equal(crop.width, 3000);
  assert.equal(crop.height, 2000);
  assert.equal(crop.y, 1000);
});

test("cropToAspect：极端长边（100:1）不会算出负数或 NaN", () => {
  const crop = cropToAspect({ width: 10_000, height: 100 }, 1);
  assert.equal(crop.height, 100);
  assert.equal(crop.width, 100);
  assert.ok(crop.x > 0);
  const wide = cropToAspect({ width: 100, height: 10_000 }, 1);
  assert.equal(wide.width, 100);
  assert.equal(wide.height, 100);
  assert.ok(wide.y > 0);
});

test("cropToAspect：尺寸或比例非法时原样返回，不猜", () => {
  assert.deepEqual(cropToAspect({ width: 0, height: 0 }, 1.5), { x: 0, y: 0, width: 0, height: 0 });
  assert.deepEqual(cropToAspect({ width: 100, height: 50 }, 0), {
    x: 0,
    y: 0,
    width: 100,
    height: 50,
  });
  assert.deepEqual(cropToAspect({ width: 100, height: 50 }, Number.NaN), {
    x: 0,
    y: 0,
    width: 100,
    height: 50,
  });
});

// ─────────────────── 整组：人类点名的例子 ───────────────────

test("compareFrames：4:3 打头、后面 3:2 → 后面的被扣成 4:3（居中）", () => {
  const frames = compareFrames([photo("a", 4000, 3000), photo("b", 6000, 4000)]);
  assert.equal(frames.length, 2);
  const [first, second] = frames;
  assert.deepEqual(first!.crop, { x: 0, y: 0, width: 4000, height: 3000 }, "第一幅是基准，整张");
  // 6000×4000（3:2）扣成 4:3 → 高 4000、宽 5333.33…
  assert.equal(second!.crop.width, 4000 * (4 / 3));
  assert.equal(second!.crop.height, 4000);
  assert.equal(second!.crop.x, (6000 - 4000 * (4 / 3)) / 2);
  assert.equal(second!.crop.y, 0);
});

test("compareFrames：比例全都一样时谁都不扣", () => {
  const frames = compareFrames([photo("a", 6000, 4000), photo("b", 3000, 2000), photo("c", 900, 600)]);
  for (const frame of frames) {
    assert.equal(frame.crop.x, 0);
    assert.equal(frame.crop.y, 0);
    assert.equal(frame.crop.width, frame.photo.natural!.width);
    assert.equal(frame.crop.height, frame.photo.natural!.height);
  }
});

test("compareFrames：尺寸未知的那一帧不参与扣取（退回整张/零）", () => {
  const frames = compareFrames([photo("a", 4000, 3000), photo("b")]);
  assert.deepEqual(frames[1]!.crop, { x: 0, y: 0, width: 0, height: 0 });
});

test("compareFrames：空列表给空数组", () => {
  assert.deepEqual(compareFrames([]), []);
});

// ─────────────────── 位移的百分比换算 ───────────────────

test("panPercent / percentToPan：互为反函数", () => {
  const size = { width: 4000, height: 3000 };
  const percent = panPercent({ x: 200, y: -150 }, size);
  assert.deepEqual(percent, { x: 0.05, y: -0.05 });
  assert.deepEqual(percentToPan(percent, size), { x: 200, y: -150 });
});

test("panPercent：同一百分比在两幅不同尺寸上给出不同像素（这正是要的效果）", () => {
  const percent = { x: 0.05, y: 0.05 };
  assert.deepEqual(percentToPan(percent, { width: 4000, height: 3000 }), { x: 200, y: 150 });
  assert.deepEqual(percentToPan(percent, { width: 6000, height: 4000 }), { x: 300, y: 200 });
});

test("panPercent：尺寸未知时不编百分比（返回 0）", () => {
  assert.deepEqual(panPercent({ x: 200, y: 200 }, { width: 0, height: 0 }), { x: 0, y: 0 });
  assert.deepEqual(percentToPan({ x: 0.5, y: 0.5 }, { width: 0, height: 0 }), { x: 0, y: 0 });
});
