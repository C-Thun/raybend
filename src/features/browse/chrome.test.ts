/**
 * 看图三态循环的测试：**按 Tab 三下必须回到原样**，且 ①③ 左右在、③ 胶片带不在。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  chromeShowsFilm,
  chromeShowsSides,
  CHROME_CYCLE,
  nextChrome,
  type ViewerChrome,
} from "./chrome.ts";

test("循环顺序是 ①默认 → ②关左右 → ③关胶片带 → ①（并列，不是叠加）", () => {
  assert.deepEqual([...CHROME_CYCLE], ["default", "no-sides", "no-film"]);
  assert.equal(nextChrome("default"), "no-sides");
  assert.equal(nextChrome("no-sides"), "no-film");
  assert.equal(nextChrome("no-film"), "default");
});

test("按三下回到原样（循环不变式）", () => {
  let chrome: ViewerChrome = "default";
  for (let i = 0; i < 3; i += 1) chrome = nextChrome(chrome);
  assert.equal(chrome, "default");
});

test("左右列：①③ 显示、② 不显示", () => {
  assert.equal(chromeShowsSides("default"), true);
  assert.equal(chromeShowsSides("no-sides"), false);
  assert.equal(chromeShowsSides("no-film"), true, "③ 只关胶片带，左右要回来");
});

test("胶片带：①② 显示、③ 不显示", () => {
  assert.equal(chromeShowsFilm("default"), true);
  assert.equal(chromeShowsFilm("no-sides"), true, "② 是「关左右」，胶片带留着");
  assert.equal(chromeShowsFilm("no-film"), false);
});

test("认不出来的状态退回默认态（将来加状态时不至于卡死）", () => {
  assert.equal(nextChrome("something-else" as ViewerChrome), "default");
  assert.equal(chromeShowsSides("something-else" as ViewerChrome), true);
  assert.equal(chromeShowsFilm("something-else" as ViewerChrome), true);
});
