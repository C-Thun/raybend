import assert from "node:assert/strict";
import test from "node:test";

import {
  DEFAULT_FILM_STRIP_STEP,
  FILM_STRIP_TILE_HEIGHT_STEPS,
  clampFilmStripStep,
  filmStripMetric,
  filmStripStepFromDrag,
  nextFilmStripStep,
} from "./film-strip-size.ts";

test("胶片带尺寸：17 档单调，约 96–192px（最大档缩了 20%），默认落在 130–140px", () => {
  assert.equal(FILM_STRIP_TILE_HEIGHT_STEPS.length, 17);
  assert.equal(FILM_STRIP_TILE_HEIGHT_STEPS[0], 96);
  assert.equal(FILM_STRIP_TILE_HEIGHT_STEPS[16], 192, "最大档 240 → 192（−20%）");
  for (let index = 1; index < FILM_STRIP_TILE_HEIGHT_STEPS.length; index += 1) {
    assert.ok(
      (FILM_STRIP_TILE_HEIGHT_STEPS[index] ?? 0) >
        (FILM_STRIP_TILE_HEIGHT_STEPS[index - 1] ?? 0),
    );
  }
  assert.equal(filmStripMetric(DEFAULT_FILM_STRIP_STEP).tileHeight, 130);
});

test("胶片带尺寸：总高度只由 tile 与固定上下边距推导", () => {
  assert.deepEqual(filmStripMetric(DEFAULT_FILM_STRIP_STEP), {
    step: 7,
    tileHeight: 130,
    tileWidth: 156,
    stripHeight: 146,
  });
  assert.deepEqual(filmStripMetric(16), {
    step: 16,
    tileHeight: 192,
    tileWidth: 230,
    stripHeight: 208,
  });
});

test("胶片带尺寸：越界、非法值与滚轮方向都会夹取", () => {
  assert.equal(clampFilmStripStep(-100), 0);
  assert.equal(clampFilmStripStep(100), 16);
  assert.equal(clampFilmStripStep(Number.NaN), DEFAULT_FILM_STRIP_STEP);
  assert.equal(nextFilmStripStep(0, -1), 0);
  assert.equal(nextFilmStripStep(16, 1), 16);
  assert.equal(nextFilmStripStep(7, 1), 8);
  assert.equal(nextFilmStripStep(4, -1), 3);
});

test("胶片带拖拉条：向上放大、向下缩小，并按真实档位吸附", () => {
  // 按**像素高度**吸附（不是按档）：130 + 18 = 148，正好命中第 10 档
  assert.equal(filmStripStepFromDrag(7, -18), 10);
  // 130 − 18 = 112 → 110 与 114 等距，按拖动方向取低档
  assert.equal(filmStripStepFromDrag(7, 18), 3);
  assert.equal(filmStripStepFromDrag(7, -2), 7, "未越过半档时保持原档（132 离 130 更近）");
  assert.equal(filmStripStepFromDrag(7, -3), 8, "正好半档时顺拖动方向跨档");
  assert.equal(filmStripStepFromDrag(7, 3), 6);
});

test("胶片带拖拉条：边界与非法位移不会产生越界档位", () => {
  assert.equal(filmStripStepFromDrag(0, 10_000), 0);
  assert.equal(filmStripStepFromDrag(16, -10_000), 16);
  assert.equal(filmStripStepFromDrag(-100, -9), 2);
  assert.equal(filmStripStepFromDrag(100, 9), 15);
  assert.equal(filmStripStepFromDrag(4, Number.NaN), 4);
  assert.equal(filmStripStepFromDrag(4, Number.POSITIVE_INFINITY), 4);
});

test("胶片带 17 档是等比阶梯：最大 192（−20%），相邻比例大致恒定", () => {
  assert.equal(FILM_STRIP_TILE_HEIGHT_STEPS[16], 192, "封顶 240 → 192");
  const ratios = FILM_STRIP_TILE_HEIGHT_STEPS.slice(1).map(
    (size, index) => size / FILM_STRIP_TILE_HEIGHT_STEPS[index],
  );
  const min = Math.min(...ratios);
  const max = Math.max(...ratios);
  assert.ok(
    min > 1.02 && max < 1.08,
    `相邻比例应落在 1.02–1.08（实测 ${min.toFixed(3)}–${max.toFixed(3)}）`,
  );
});
