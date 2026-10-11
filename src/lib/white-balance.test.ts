/**
 * 白平衡 mired 偏移换算的测试（`lib/white-balance.ts`）。
 *
 * 覆盖：往返、边界 clamp、asShot 未知、极端值、非法输入。
 * 契约正文：`specs/editor-presets.md` §3 v3。
 */

import assert from "node:assert/strict";
import test from "node:test";
import {
  WB_KELVIN_MAX,
  WB_KELVIN_MIN,
  kelvinToWbMired,
  mired,
  wbMiredToKelvin,
} from "./white-balance.ts";

test("mired 与 K 成反比（5000K = 200 mired）", () => {
  assert.equal(mired(5000), 200);
  assert.ok(mired(2500) > mired(10000));
});

test("往返：Δmired → 目标 K 回到原值（范围内）", () => {
  for (const asShot of [2500, 3200, 5000, 6500, 10000]) {
    for (const target of [2500, 3000, 4200, 5000, 8000, 10000]) {
      const delta = kelvinToWbMired(target, asShot);
      const back = wbMiredToKelvin(asShot, delta);
      assert.ok(
        Math.abs(back - target) < 0.5,
        `asShot=${asShot} target=${target} Δ=${delta} → ${back}`,
      );
    }
  }
});

test("Δ = 0 = 跟随拍摄色温", () => {
  assert.equal(mired(5000) - mired(5000), 0);
  assert.equal(wbMiredToKelvin(5000, 0), 5000);
  assert.equal(kelvinToWbMired(5000, 5000), 0);
});

test("符号：更暖（target 更低 K）Δ 为负，更冷为正", () => {
  assert.ok(kelvinToWbMired(3000, 5000) < 0, "5000K → 3000K 是更暖（负）");
  assert.ok(kelvinToWbMired(8000, 5000) > 0, "5000K → 8000K 是更冷（正）");
});

test("目标 K clamp 到 2500..10000", () => {
  assert.equal(wbMiredToKelvin(5000, 10_000), WB_KELVIN_MIN);
  assert.equal(wbMiredToKelvin(5000, -10_000), WB_KELVIN_MIN);
  assert.equal(wbMiredToKelvin(2500, 300), WB_KELVIN_MAX, "Δ 到极点 → 截到 10000");
  assert.equal(wbMiredToKelvin(5000, -100_000), WB_KELVIN_MIN, "反向极端 → 截到 2500");
  // 保存侧同样把超出范围的当前值夹进范围
  assert.equal(kelvinToWbMired(50_000, 5000), kelvinToWbMired(10_000, 5000), "50000K 按 10000K 算");
});

test("asShot 未知 / 非法输入 → Δ = 0（不写假偏移）", () => {
  assert.equal(kelvinToWbMired(5000, null), 0);
  assert.equal(kelvinToWbMired(5000, Number.NaN), 0);
  assert.equal(kelvinToWbMired(5000, 0), 0);
  assert.equal(kelvinToWbMired(Number.NaN, 5000), 0);
  assert.equal(kelvinToWbMired(0, 5000), 0);
  // 套用侧遇到非法基线退回静态默认 6250（不是 0/NaN）
  assert.equal(wbMiredToKelvin(Number.NaN, 0), 6250);
  assert.equal(wbMiredToKelvin(0, 0), 6250);
  assert.equal(wbMiredToKelvin(5000, Number.NaN), 5000);
});

test("极端偏移（数学上超出 0K）截到最暖端，不产生负值 / NaN", () => {
  for (const delta of [1e6, -1e6, Number.POSITIVE_INFINITY]) {
    const kelvin = wbMiredToKelvin(5000, delta);
    assert.ok(kelvin >= WB_KELVIN_MIN && kelvin <= WB_KELVIN_MAX, `Δ=${delta} → ${kelvin}`);
  }
});
