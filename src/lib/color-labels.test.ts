/**
 * 色标取值表的单测（`DESIGN.md` §1.4）。
 *
 * 重点不是「表里有什么」，而是**加一个色标时不会被漏掉** ——
 * 这张表以前散在六个文件里（值表、合法性集合、三份类名映射、文案 switch），
 * 漏一处就表现为「某个视图上这颗色标是空的」。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  COLOR_DOT_CLASS,
  COLOR_LABELS,
  COLOR_TINT_CLASS,
  COLOR_VALUES,
  isColorLabel,
} from "./color-labels.ts";

test("六色 + 无色，顺序即面板顺序", () => {
  assert.deepEqual([...COLOR_LABELS], ["red", "yellow", "green", "cyan", "blue", "purple"]);
  assert.deepEqual([...COLOR_VALUES], [
    "red",
    "yellow",
    "green",
    "cyan",
    "blue",
    "purple",
    null,
  ]);
});

test("两个映射表覆盖全部色标，且逐个指向自己的令牌", () => {
  for (const label of COLOR_LABELS) {
    assert.equal(COLOR_DOT_CLASS[label], `bg-(--label-${label})`, `${label} 的实色类名`);
    assert.equal(
      COLOR_TINT_CLASS[label],
      `bg-(--label-${label}-tint)`,
      `${label} 的低浓底纹类名`,
    );
  }
  assert.equal(Object.keys(COLOR_DOT_CLASS).length, COLOR_LABELS.length);
  assert.equal(Object.keys(COLOR_TINT_CLASS).length, COLOR_LABELS.length);
});

test("合法性判断：只认表里的值", () => {
  for (const label of COLOR_LABELS) assert.equal(isColorLabel(label), true);
  for (const bad of ["magenta", "RED", "", " red", null, undefined, "0"]) {
    assert.equal(isColorLabel(bad), false, `${JSON.stringify(bad)} 不该当色标`);
  }
});

test("无色不在色标集合里（它是那个空心圈，不是第七种颜色）", () => {
  assert.equal(isColorLabel("none"), false);
  assert.equal(COLOR_VALUES.includes(null), true);
});
