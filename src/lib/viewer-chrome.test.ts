/**
 * 看图档位的测试（`lib/viewer-chrome.ts`）。
 *
 * 三张表各自的循环不变式都要验：
 * **按满一圈回到第一档**、**每档的左右/胶片带读数与设计稿一致**，
 * 以及「认不出的输入不卡死」（老代码/存储里的垃圾值一律回默认档）。
 *
 * 2026-09-23 从「一套硬编码四档」改成「带 flow 参数的三张表」，
 * 所以这里按 flow 分别钉住 —— **browse 的四档是回归测试**（人类明确要求它不变）。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  chromeAt,
  chromeCycle,
  chromeName,
  chromeShowsFilm,
  chromeShowsLeft,
  chromeShowsRight,
  CHROME_CYCLES,
  isLastChromeStep,
  maxChromeStep,
  nextChromeStep,
  normalizeChromeMode,
  normalizeChromeStep,
} from "./viewer-chrome.ts";

/** 把一圈走完，返回档位名序列（从默认档出发）。 */
function walk(mode: "browse" | "import" | "editor"): string[] {
  const cycle = chromeCycle(mode);
  const names: string[] = [];
  let step = 0;
  for (let i = 0; i < cycle.length; i += 1) {
    names.push(chromeName(mode, step));
    step = nextChromeStep(mode, step);
  }
  // 走满一圈必须回到第一档 —— 这就是「按 N 下回到原样」的不变式
  assert.equal(step, 0, `${mode}：走一圈之后没有回到第一档`);
  return names;
}

test("browse 仍是四档：①默认 → ②只关左 → ③关两侧 → ④仅 view", () => {
  assert.deepEqual(walk("browse"), ["default", "film-right", "film-only", "view-only"]);
  assert.equal(chromeShowsLeft("browse", 0), true);
  assert.equal(chromeShowsRight("browse", 0), true);
  assert.equal(chromeShowsLeft("browse", 1), false);
  assert.equal(chromeShowsRight("browse", 1), true);
  assert.equal(chromeShowsFilm("browse", 1), true);
  assert.equal(chromeShowsFilm("browse", 2), true, "第三档还留着胶片带");
  assert.equal(chromeShowsFilm("browse", 3), false);
  assert.equal(chromeShowsLeft("browse", 3), false);
  assert.equal(chromeShowsRight("browse", 3), false);
});

test("import 是三档：② 先丢左右两列、留胶片带（2026-09-23 改口径）", () => {
  assert.deepEqual(walk("import"), ["default", "film-only", "view-only"]);
  assert.equal(chromeShowsLeft("import", 1), false);
  assert.equal(chromeShowsRight("import", 1), false);
  assert.equal(chromeShowsFilm("import", 1), true, "中间那档要有胶片带");
  assert.equal(chromeShowsFilm("import", 2), false);
});

test("editor 是三档：② 先丢胶片带、左右两列都留着", () => {
  assert.deepEqual(walk("editor"), ["default", "view", "view-only"]);
  assert.equal(chromeShowsLeft("editor", 1), true);
  assert.equal(chromeShowsRight("editor", 1), true);
  assert.equal(chromeShowsFilm("editor", 1), false, "第二档先藏胶片带");
  assert.equal(chromeShowsLeft("editor", 2), false);
  assert.equal(chromeShowsRight("editor", 2), false);
  assert.equal(chromeShowsFilm("editor", 2), false);
});

test("最后一档就是「仅 view」——editor-chrome 的进出判定靠它", () => {
  for (const mode of ["browse", "import", "editor"] as const) {
    assert.equal(isLastChromeStep(mode, maxChromeStep(mode)), true);
    assert.equal(isLastChromeStep(mode, 0), maxChromeStep(mode) === 0);
  }
});

test("非法档位一律夹回合法范围（不抛错、不返回 undefined）", () => {
  assert.deepEqual(normalizeChromeStep("editor", -3), 0);
  assert.deepEqual(normalizeChromeStep("editor", 99), 2);
  assert.deepEqual(normalizeChromeStep("editor", 1.7), 1);
  assert.deepEqual(normalizeChromeStep("editor", Number.NaN), 0);
  assert.deepEqual(normalizeChromeStep("editor", "1"), 0);
  assert.equal(chromeName("editor", 99), "view-only");
});

test("非法 mode 按 browse 处理（老代码里传进来的字符串不能把界面搞崩）", () => {
  assert.equal(normalizeChromeMode("nope"), "browse");
  assert.equal(normalizeChromeMode(undefined), "browse");
  assert.deepEqual(chromeAt("nope", 1), CHROME_CYCLES.browse[1]);
});
