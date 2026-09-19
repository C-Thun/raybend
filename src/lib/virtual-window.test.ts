/**
 * 虚拟化行窗口的单元测试。
 *
 * 这类纯函数的价值就在于**边界可以穷举**：滚动到顶/底、视口比内容还高、
 * 行高不等（分组标题）、空列表、脏输入（NaN / 负数 / 超大滚动值）。
 * 这些情况在界面上表现为「白屏」「跳一下」「最后一行露不出来」，
 * 靠肉眼在真机上撞很不划算。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  computeVirtualWindow,
  isUniformHeight,
  type VirtualRowLike,
} from "./virtual-window.ts";

/** 等高的 n 行 */
function uniform(count: number, height: number): VirtualRowLike[] {
  return Array.from({ length: count }, () => ({ height }));
}

test("顶部：只渲染视口内的行（外加 overscan）", () => {
  const window = computeVirtualWindow({
    rows: uniform(100, 100),
    viewportHeight: 300,
    scrollTop: 0,
    overscan: 1,
  });
  assert.deepEqual(
    [window.startIndex, window.endIndex],
    [0, 4],
    "0..3 可见，overscan 1 只多出第 4 行",
  );
  assert.equal(window.paddingTop, 0);
  assert.equal(window.paddingBottom, 100 * 100 - 4 * 100);
  assert.equal(window.totalHeight, 10_000);
});

test("中部：窗口跟着滚动位置走", () => {
  const window = computeVirtualWindow({
    rows: uniform(100, 100),
    viewportHeight: 300,
    scrollTop: 1000,
    overscan: 1,
  });
  // 视口盖住 1000..1300 → 行 10、11、12；overscan 1 → 9..13
  assert.deepEqual([window.startIndex, window.endIndex], [9, 14]);
  assert.equal(window.paddingTop, 900);
  assert.equal(window.paddingBottom, 10_000 - 1400);
});

test("底部：滚动到底时不会把一个空白窗口交出去", () => {
  const window = computeVirtualWindow({
    rows: uniform(100, 100),
    viewportHeight: 300,
    scrollTop: 999_999,
    overscan: 0,
  });
  // 最大滚动位置是 9700 → 可见 97、98、99
  assert.deepEqual([window.startIndex, window.endIndex], [97, 100]);
  assert.equal(window.paddingBottom, 0);
});

test("overscan=0 时不额外渲染任何行", () => {
  const window = computeVirtualWindow({
    rows: uniform(50, 20),
    viewportHeight: 100,
    scrollTop: 0,
    overscan: 0,
  });
  assert.deepEqual([window.startIndex, window.endIndex], [0, 5]);
});

test("视口比内容高：全部可见，且没有留白", () => {
  const window = computeVirtualWindow({
    rows: uniform(3, 40),
    viewportHeight: 1000,
    scrollTop: 0,
  });
  assert.deepEqual([window.startIndex, window.endIndex], [0, 3]);
  assert.equal(window.paddingTop, 0);
  assert.equal(window.paddingBottom, 0);
  assert.equal(window.totalHeight, 120);
});

test("行高不等（按时间分组的标题行）：边界按真实高度算", () => {
  // 40（日标题）+ 100 + 100 + 24（片标题）+ 100
  const rows = [{ height: 40 }, { height: 100 }, { height: 100 }, { height: 24 }, { height: 100 }];
  const window = computeVirtualWindow({
    rows,
    viewportHeight: 120,
    scrollTop: 0,
    overscan: 0,
  });
  // 视口 0..120：盖住行 0（40 高）与行 1（40..140，部分可见），
  // 行 2 从 140 开始 —— 整个在视口之下，不算可见
  assert.deepEqual([window.startIndex, window.endIndex], [0, 2]);
  assert.equal(window.paddingBottom, 364 - 140);

  // 滚到 140：第 3 行（100 高）开始
  const scrolled = computeVirtualWindow({
    rows,
    viewportHeight: 120,
    scrollTop: 140,
    overscan: 0,
  });
  assert.deepEqual([scrolled.startIndex, scrolled.endIndex], [2, 4]);
  assert.equal(scrolled.paddingTop, 140);
});

test("空列表：空窗口，总高 0", () => {
  const window = computeVirtualWindow({
    rows: [],
    viewportHeight: 300,
    scrollTop: 0,
  });
  assert.deepEqual(window, {
    startIndex: 0,
    endIndex: 0,
    paddingTop: 0,
    paddingBottom: 0,
    totalHeight: 0,
  });
});

test("单行：渲染它一个", () => {
  const window = computeVirtualWindow({
    rows: [{ height: 80 }],
    viewportHeight: 300,
    scrollTop: 0,
  });
  assert.deepEqual([window.startIndex, window.endIndex], [0, 1]);
  assert.equal(window.paddingTop, 0);
  assert.equal(window.paddingBottom, 0);
});

test("视口高为 0（还没量到）：不渲染任何行，但总高照给", () => {
  const window = computeVirtualWindow({
    rows: uniform(10, 50),
    viewportHeight: 0,
    scrollTop: 0,
  });
  assert.equal(window.startIndex, 0);
  assert.equal(window.endIndex, 0);
  assert.equal(window.totalHeight, 500, "滚动条长度不能因为还没量高就变成 0");
});

test("脏输入：NaN / 负数 / 无意义的 overscan 都要被夹住", () => {
  const rows = uniform(10, 50);
  const nan = computeVirtualWindow({
    rows,
    viewportHeight: Number.NaN,
    scrollTop: Number.NaN,
    overscan: Number.NaN,
  });
  assert.equal(nan.endIndex, 0, "viewportHeight 是 NaN → 空窗口");

  const negative = computeVirtualWindow({
    rows,
    viewportHeight: 100,
    scrollTop: -500,
    overscan: -3,
  });
  assert.equal(negative.paddingTop, 0, "负滚动位置按 0");
  assert.deepEqual([negative.startIndex, negative.endIndex], [0, 2]);

  const weirdHeights = computeVirtualWindow({
    rows: [{ height: -10 }, { height: Number.NaN }, { height: 50 }],
    viewportHeight: 100,
    scrollTop: 0,
  });
  assert.equal(weirdHeights.totalHeight, 50, "非法行高按 0 算");
  assert.deepEqual([weirdHeights.startIndex, weirdHeights.endIndex], [0, 3]);
});

test("全部行高为 0：不渲染（避免给出一堆看不见的行）", () => {
  const window = computeVirtualWindow({
    rows: uniform(5, 0),
    viewportHeight: 100,
    scrollTop: 0,
  });
  assert.equal(window.totalHeight, 0);
  assert.equal(window.endIndex, 0);
});

test("大列表（10 万行）也能算对边界", () => {
  const window = computeVirtualWindow({
    rows: uniform(100_000, 10),
    viewportHeight: 800,
    scrollTop: 500_000,
    overscan: 2,
  });
  assert.deepEqual([window.startIndex, window.endIndex], [49_998, 50_082]);
  assert.equal(window.totalHeight, 1_000_000);
});

test("isUniformHeight：等行高为真，混入一行不等就为假", () => {
  assert.equal(isUniformHeight([]), true);
  assert.equal(isUniformHeight([{ height: 10 }]), true);
  assert.equal(isUniformHeight(uniform(5, 20)), true);
  assert.equal(isUniformHeight([{ height: 20 }, { height: 21 }]), false);
  assert.equal(isUniformHeight([{ height: 20 }, { height: 0 }]), false);
});

// ─────────────────── 滚到某一行（键盘导航用） ───────────────────

import { rowScrollTop, rowTop } from "./virtual-window.ts";

const ROWS = [{ height: 100 }, { height: 100 }, { height: 100 }, { height: 100 }];

test("rowScrollTop：目标在下方 → 底对齐（刚好看得见）", () => {
  assert.equal(rowScrollTop({ rows: ROWS, target: 2, scrollTop: 0, viewportHeight: 150 }), 150);
});

test("rowScrollTop：目标在上方 → 顶对齐", () => {
  assert.equal(rowScrollTop({ rows: ROWS, target: 0, scrollTop: 250, viewportHeight: 150 }), 0);
});

test("rowScrollTop：已经看得见 → 一个像素都不动（不然每按一次方向键画面都抖）", () => {
  assert.equal(rowScrollTop({ rows: ROWS, target: 1, scrollTop: 50, viewportHeight: 150 }), 50);
  assert.equal(rowScrollTop({ rows: ROWS, target: 0, scrollTop: 0, viewportHeight: 150 }), 0);
});

test("rowScrollTop：越界 / 空列表 / 非法输入 → 原样返回", () => {
  assert.equal(rowScrollTop({ rows: ROWS, target: 9, scrollTop: 40, viewportHeight: 150 }), 40);
  assert.equal(rowScrollTop({ rows: ROWS, target: -1, scrollTop: 40, viewportHeight: 150 }), 40);
  assert.equal(rowScrollTop({ rows: [], target: 0, scrollTop: 40, viewportHeight: 150 }), 40);
  assert.equal(rowScrollTop({ rows: ROWS, target: 0, scrollTop: 40, viewportHeight: 0 }), 40);
  assert.equal(
    rowScrollTop({ rows: ROWS, target: 0, scrollTop: Number.NaN, viewportHeight: 150 }),
    0,
  );
});

test("rowScrollTop：行高不齐时按前缀和算（不是 target × 平均高）", () => {
  const rows = [{ height: 40 }, { height: 300 }, { height: 60 }];
  assert.equal(rowScrollTop({ rows: rows, target: 2, scrollTop: 0, viewportHeight: 100 }), 300);
});
// ─────────────────────── 把某一行钉在指定位置（锚定） ───────────────────────

test("rowTop：前面所有行的高度之和", () => {
  const rows = [{ height: 100 }, { height: 100 }, { height: 100 }];
  assert.equal(rowTop(rows, 0), 0);
  assert.equal(rowTop(rows, 1), 100);
  assert.equal(rowTop(rows, 2), 200);
});

test("rowTop：高度不齐（分组标题那种）也按实际累加", () => {
  const rows = [{ height: 30 }, { height: 120 }, { height: 30 }, { height: 120 }];
  assert.equal(rowTop(rows, 2), 150);
});

test("rowTop：越界与非法值给出安全结果（不返回 NaN）", () => {
  const rows = [{ height: 100 }, { height: 100 }];
  assert.equal(rowTop(rows, -3), 0);
  assert.equal(rowTop(rows, 99), 200, "超过行数 ⇒ 总高（调用方自己夹）");
  assert.equal(rowTop(rows, Number.NaN), 0);
  assert.equal(rowTop([], 3), 0);
});

test("rowTop 与 rowScrollTop 用的是同一份行高口径", () => {
  const rows = [{ height: 40 }, { height: 40 }, { height: 40 }];
  // 目标行高 40、视口 100：第 2 行已经在视野里 ⇒ 不动；第 3 行要滚到刚好露出来
  assert.equal(rowScrollTop({ rows, target: 1, scrollTop: 0, viewportHeight: 100 }), 0);
  assert.equal(rowScrollTop({ rows, target: 2, scrollTop: 0, viewportHeight: 100 }), 20);
});

