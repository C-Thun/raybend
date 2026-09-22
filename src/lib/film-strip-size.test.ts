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

test("胶片带尺寸：17 档单调，约 100–240px，默认落在 130–140px", () => {
  assert.equal(FILM_STRIP_TILE_HEIGHT_STEPS.length, 17);
  assert.equal(FILM_STRIP_TILE_HEIGHT_STEPS[0], 96);
  assert.equal(FILM_STRIP_TILE_HEIGHT_STEPS[16], 240);
  for (let index = 1; index < FILM_STRIP_TILE_HEIGHT_STEPS.length; index += 1) {
    assert.ok(
      (FILM_STRIP_TILE_HEIGHT_STEPS[index] ?? 0) >
        (FILM_STRIP_TILE_HEIGHT_STEPS[index - 1] ?? 0),
    );
  }
  assert.equal(filmStripMetric(DEFAULT_FILM_STRIP_STEP).tileHeight, 132);
});

test("胶片带尺寸：总高度只由 tile 与固定上下边距推导", () => {
  assert.deepEqual(filmStripMetric(DEFAULT_FILM_STRIP_STEP), {
    step: 4,
    tileHeight: 132,
    tileWidth: 158,
    stripHeight: 148,
  });
  assert.deepEqual(filmStripMetric(16), {
    step: 16,
    tileHeight: 240,
    tileWidth: 288,
    stripHeight: 256,
  });
});

test("胶片带尺寸：越界、非法值与滚轮方向都会夹取", () => {
  assert.equal(clampFilmStripStep(-100), 0);
  assert.equal(clampFilmStripStep(100), 16);
  assert.equal(clampFilmStripStep(Number.NaN), DEFAULT_FILM_STRIP_STEP);
  assert.equal(nextFilmStripStep(0, -1), 0);
  assert.equal(nextFilmStripStep(16, 1), 16);
  assert.equal(nextFilmStripStep(4, 1), 5);
  assert.equal(nextFilmStripStep(4, -1), 3);
});

test("胶片带拖拉条：向上放大、向下缩小，并按真实档位吸附", () => {
  assert.equal(filmStripStepFromDrag(4, -18), 6);
  assert.equal(filmStripStepFromDrag(4, 18), 2);
  assert.equal(filmStripStepFromDrag(4, -4), 4, "未越过半档时保持原档");
  assert.equal(filmStripStepFromDrag(4, -4.5), 5, "正好半档时顺拖动方向跨档");
  assert.equal(filmStripStepFromDrag(4, 4.5), 3);
});

test("胶片带拖拉条：边界与非法位移不会产生越界档位", () => {
  assert.equal(filmStripStepFromDrag(0, 10_000), 0);
  assert.equal(filmStripStepFromDrag(16, -10_000), 16);
  assert.equal(filmStripStepFromDrag(-100, -9), 1);
  assert.equal(filmStripStepFromDrag(100, 9), 15);
  assert.equal(filmStripStepFromDrag(4, Number.NaN), 4);
  assert.equal(filmStripStepFromDrag(4, Number.POSITIVE_INFINITY), 4);
});
