/**
 * 看图四态循环的测试：**按 Tab 四下必须回到原样**。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  chromeShowsFilm,
  chromeShowsLeft,
  chromeShowsRight,
  CHROME_CYCLE,
  nextChrome,
  type ViewerChrome,
} from "./viewer-chrome.ts";

test("循环顺序是 ①默认 → ②只关左 → ③关两侧 → ④仅 view → ①", () => {
  assert.deepEqual([...CHROME_CYCLE], ["default", "film-right", "film-only", "view-only"]);
  assert.equal(nextChrome("default"), "film-right");
  assert.equal(nextChrome("film-right"), "film-only");
  assert.equal(nextChrome("film-only"), "view-only");
  assert.equal(nextChrome("view-only"), "default");
});

test("按四下回到原样（循环不变式）", () => {
  let chrome: ViewerChrome = "default";
  for (let i = 0; i < 4; i += 1) chrome = nextChrome(chrome);
  assert.equal(chrome, "default");
});

test("左右列：第二档只关左，第三档开始两侧都关", () => {
  assert.equal(chromeShowsLeft("default"), true);
  assert.equal(chromeShowsRight("default"), true);
  assert.equal(chromeShowsLeft("film-right"), false);
  assert.equal(chromeShowsRight("film-right"), true);
  assert.equal(chromeShowsLeft("film-only"), false);
  assert.equal(chromeShowsRight("film-only"), false);
  assert.equal(chromeShowsLeft("view-only"), false);
  assert.equal(chromeShowsRight("view-only"), false);
});

test("胶片带：前三档显示、第四档不显示", () => {
  assert.equal(chromeShowsFilm("default"), true);
  assert.equal(chromeShowsFilm("film-right"), true);
  assert.equal(chromeShowsFilm("film-only"), true);
  assert.equal(chromeShowsFilm("view-only"), false);
});

test("认不出来的状态退回默认态（将来加状态时不至于卡死）", () => {
  assert.equal(nextChrome("something-else" as ViewerChrome), "default");
  assert.equal(chromeShowsLeft("something-else" as ViewerChrome), true);
  assert.equal(chromeShowsRight("something-else" as ViewerChrome), true);
  assert.equal(chromeShowsFilm("something-else" as ViewerChrome), true);
});
