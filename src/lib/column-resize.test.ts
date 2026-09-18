/**
 * 侧栏拖拽数学的测试。
 *
 * 重点全在**符号**上：左列往右拖是变宽、右列往右拖是变窄 —— 搞反了表现很明显，
 * 但光看代码很难一眼看出哪边错。
 */

import assert from "node:assert/strict";
import test from "node:test";
import { clampWidth, nudgeWidth, resizeWidth } from "./column-resize.ts";

const bounds = { min: 220, max: 520 };

test("resizeWidth：左列往右拖变宽、往左拖变窄", () => {
  assert.equal(resizeWidth({ start: 300, dx: 40, bounds }), 340);
  assert.equal(resizeWidth({ start: 300, dx: -40, bounds }), 260);
});

test("resizeWidth：右列（invert）往右拖是**变窄**", () => {
  assert.equal(resizeWidth({ start: 300, dx: 40, invert: true, bounds }), 260);
  assert.equal(resizeWidth({ start: 300, dx: -40, invert: true, bounds }), 340);
});

test("resizeWidth：拖过头被夹在上下限（不会把侧栏拖没）", () => {
  assert.equal(resizeWidth({ start: 300, dx: -500, bounds }), 220);
  assert.equal(resizeWidth({ start: 300, dx: 500, bounds }), 520);
  assert.equal(resizeWidth({ start: 300, dx: 500, invert: true, bounds }), 220);
});

test("clampWidth：非法输入给下限（不是 NaN 也不是 0）", () => {
  assert.equal(clampWidth(Number.NaN, bounds), 220);
  assert.equal(clampWidth(Number.POSITIVE_INFINITY, bounds), 220);
  assert.equal(clampWidth(-10, bounds), 220);
});

test("clampWidth / nudgeWidth：取整", () => {
  assert.equal(clampWidth(300.6, bounds), 301);
  assert.equal(nudgeWidth(300.4, 0, bounds), 300);
  assert.equal(nudgeWidth(300, 8, bounds), 308);
  assert.equal(nudgeWidth(510, 40, bounds), 520);
});
