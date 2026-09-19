/**
 * 对比态数学的测试。
 *
 * 这些数字**必须钉死**：虚拟画布的尺寸与每张图的落点错一点，表现就是「一边拖一边
 * 两边错位」或者「小图被拉大 / 大图被裁掉」—— 人眼立刻看得出来，但很难反推出是哪一步
 * 算错了。人类点名的例子（竖图打头 + 横图）单独一条。
 */

import assert from "node:assert/strict";
import test from "node:test";
import {
  canvasAspect,
  COMPARE_MAX,
  compareCanvas,
  compareIds,
  compareLayout,
  fitAspectWithin,
  smallestByPixels,
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

test("fitAspectWithin：窗口横竖变化时始终保持画幅比例", () => {
  assert.deepEqual(fitAspectWithin({ width: 600, height: 300 }, 4 / 3), {
    width: 400,
    height: 300,
  });
  assert.deepEqual(fitAspectWithin({ width: 240, height: 500 }, 4 / 3), {
    width: 240,
    height: 180,
  });
  assert.deepEqual(fitAspectWithin({ width: 0, height: 500 }, 4 / 3), {
    width: 0,
    height: 0,
  });
});

// ─────────────────── 基准比例 ───────────────────

// ─────────────────── 虚拟画布（2026-09-20 新方案） ───────────────────

test("compareCanvas：尺寸一样时，画布就是那张图，落点都在原点", () => {
  const canvas = compareCanvas([photo("a", 4000, 3000), photo("b", 3000, 2250)]);
  // 注意：同比例但**尺寸不同**时，画布取最大宽 × 最大高，小的图居中（不拉伸）
  assert.deepEqual(canvas.size, { width: 4000, height: 3000 });
  assert.deepEqual(canvas.placements[0]!.offset, { x: 0, y: 0 });
  assert.deepEqual(canvas.placements[1]!.offset, { x: 500, y: 375 });
});

test("compareCanvas：人类点名的例子 —— 竖图打头 + 横图，谁都不裁", () => {
  /*
   * 旧方案会「以第一幅比例扣等比例区域」：竖图打头就把横图裁成竖比例 —— 一放大露馅。
   * 新方案：画布 = 最大宽 × 最大高，两张图各自按原尺寸居中放进去。
   */
  const tall = photo("tall", 3750, 5000);
  const wide = photo("wide", 6000, 4000);
  const canvas = compareCanvas([tall, wide]);
  assert.deepEqual(canvas.size, { width: 6000, height: 5000 });
  // 竖图：宽度方向两头留空 ((6000−3750)/2 = 1125)，高度方向正好贴着
  assert.deepEqual(canvas.placements[0]!.offset, { x: 1125, y: 0 });
  assert.deepEqual(canvas.placements[0]!.natural, { width: 3750, height: 5000 });
  // 横图：高度方向两头留空 ((5000−4000)/2 = 500)，宽度方向正好贴着
  assert.deepEqual(canvas.placements[1]!.offset, { x: 0, y: 500 });
  // 两张图都**完整**落在画布内（没有被裁掉的部分）
  for (const placement of canvas.placements) {
    assert.ok(placement.offset.x >= 0 && placement.offset.y >= 0);
    assert.ok(placement.offset.x + placement.natural.width <= canvas.size.width);
    assert.ok(placement.offset.y + placement.natural.height <= canvas.size.height);
  }
});

test("compareCanvas：小的图 x / y 两头都挨不到边（居中）", () => {
  const canvas = compareCanvas([photo("big", 6000, 5000), photo("small", 2000, 1200)]);
  assert.deepEqual(canvas.size, { width: 6000, height: 5000 });
  assert.deepEqual(canvas.placements[1]!.offset, { x: 2000, y: 1900 });
});

test("compareCanvas：尺寸未知的图照样占一个落点（0 尺寸），画布由已知的算", () => {
  const canvas = compareCanvas([photo("a", 4000, 3000), photo("b")]);
  assert.deepEqual(canvas.size, { width: 4000, height: 3000 });
  assert.deepEqual(canvas.placements[1]!.natural, { width: 0, height: 0 });
  assert.deepEqual(canvas.placements[1]!.offset, { x: 2000, y: 1500 });
});

test("compareCanvas：全都未知 / 空列表 / 脏尺寸都不炸", () => {
  assert.deepEqual(compareCanvas([]).size, { width: 0, height: 0 });
  assert.deepEqual(compareCanvas([]).placements, []);
  const unknown = compareCanvas([photo("a"), photo("b")]);
  assert.deepEqual(unknown.size, { width: 0, height: 0 });
  assert.equal(unknown.placements.length, 2);
  const dirty = compareCanvas([photo("a", 0, 0), photo("b", -100, 200)]);
  assert.deepEqual(dirty.size, { width: 0, height: 0 });
});

test("compareCanvas：泛型不丢照片自己的字段", () => {
  const canvas = compareCanvas([photo("a", 4000, 3000)]);
  assert.equal(canvas.placements[0]!.photo.fileName, "a.JPG");
  assert.equal(canvas.placements[0]!.photo.path, "/lib/a.JPG");
});

// ─────────────────── 画布比例与「最小那张」 ───────────────────

test("canvasAspect：正常给宽高比，空画布给 1（调用方走空态）", () => {
  assert.equal(canvasAspect(compareCanvas([photo("a", 6000, 5000)])), 6000 / 5000);
  assert.equal(canvasAspect(compareCanvas([])), 1);
  assert.equal(canvasAspect(compareCanvas([photo("a")])), 1);
});

test("smallestByPixels：按像素数最小的那张（进入对比时的「适合窗口」基准）", () => {
  const small = photo("small", 2000, 1500); // 3.0M
  const big = photo("big", 6000, 4000); // 24M
  const mid = photo("mid", 3000, 3000); // 9.0M
  assert.equal(smallestByPixels([big, small, mid]), small);
  assert.equal(smallestByPixels([big, mid]), mid);
});

test("smallestByPixels：尺寸未知的不参与；全未知 / 空列表给 null", () => {
  const known = photo("known", 4000, 3000);
  assert.equal(smallestByPixels([photo("a"), known]), known);
  assert.equal(smallestByPixels([photo("a"), photo("b")]), null);
  assert.equal(smallestByPixels([]), null);
  // 0 / 负数 / 非有限值都不算「尺寸已知」
  assert.equal(smallestByPixels([photo("a", 0, 0)]), null);
  assert.equal(smallestByPixels([photo("a", -5, 100)]), null);
  assert.equal(smallestByPixels([photo("a", Number.NaN, 100)]), null);
});

test("smallestByPixels：并列时取先遇到的（列表顺序 = 显示顺序，可复现）", () => {
  const first = photo("first", 4000, 3000);
  const second = photo("second", 6000, 2000); // 也是 12M
  assert.equal(smallestByPixels([first, second]), first);
  assert.equal(smallestByPixels([second, first]), second);
});
