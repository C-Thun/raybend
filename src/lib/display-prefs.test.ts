/**
 * 网格显示偏好（`lib/display-prefs.ts`）的测试。
 *
 * 两个重点：
 * ① **任何非法存储值都不许抛错、也不许算出非法状态**（越界档位、脏 infoMode、非布尔）；
 * ② 共享偏好是**单例**：写进去之后，后开的读者立刻看到同一个值
 *   （浏览侧以前每次进都重置，就是因为它压根没读这份状态）。
 */

import assert from "node:assert/strict";
import test from "node:test";

import { DEFAULT_TILE_STEP_INDEX, TILE_SIZE_STEPS } from "./tile-flow.ts";
import {
  DEFAULT_DISPLAY_PREFS,
  displayByTime,
  displayInfoMode,
  displayPrefs,
  displayTileStep,
  DISPLAY_STORAGE_KEY,
  readDisplayPrefs,
  resetDisplayPrefsForTests,
  sanitizeDisplayPrefs,
  setDisplayByTime,
  setDisplayInfoMode,
  setDisplayTileStep,
  commitDisplayTileStep,
  writeDisplayPrefs,
  type DisplayStorage,
} from "./display-prefs.ts";

/** 内存版存储（Node 里没有 localStorage） */
function fakeStorage(seed: Record<string, string> = {}): DisplayStorage & {
  data: Map<string, string>;
} {
  const data = new Map(Object.entries(seed));
  return {
    data,
    getItem: (key) => data.get(key) ?? null,
    setItem: (key, value) => {
      data.set(key, value);
    },
  };
}

test("sanitizeDisplayPrefs：非法输入落回默认，绝不抛错", () => {
  for (const raw of [null, undefined, 42, "x", [], true]) {
    assert.deepEqual(sanitizeDisplayPrefs(raw), DEFAULT_DISPLAY_PREFS);
  }
  assert.deepEqual(
    sanitizeDisplayPrefs({ byTime: "yes", infoMode: "huge", tileStep: "3" }),
    DEFAULT_DISPLAY_PREFS,
    "类型不对就当没有这一项",
  );
});

test("sanitizeDisplayPrefs：合法值原样保留，越界档位夹回范围", () => {
  assert.deepEqual(
    sanitizeDisplayPrefs({ byTime: true, infoMode: "marks-name", tileStep: 6 }),
    { byTime: true, infoMode: "marks-name", tileStep: 6 },
  );
  assert.equal(sanitizeDisplayPrefs({ tileStep: 99 }).tileStep, TILE_SIZE_STEPS.length - 1);
  assert.equal(sanitizeDisplayPrefs({ tileStep: -3 }).tileStep, 0);
  assert.equal(
    sanitizeDisplayPrefs({ tileStep: Number.NaN }).tileStep,
    DEFAULT_TILE_STEP_INDEX,
  );
});

test("readDisplayPrefs：没有存储 / 坏 JSON / 空串都退回默认", () => {
  assert.deepEqual(readDisplayPrefs(undefined), DEFAULT_DISPLAY_PREFS);
  assert.deepEqual(readDisplayPrefs(fakeStorage()), DEFAULT_DISPLAY_PREFS);
  assert.deepEqual(readDisplayPrefs(fakeStorage({ [DISPLAY_STORAGE_KEY]: "{oops" })), DEFAULT_DISPLAY_PREFS);
  assert.deepEqual(readDisplayPrefs(fakeStorage({ [DISPLAY_STORAGE_KEY]: "" })), DEFAULT_DISPLAY_PREFS);
});

test("writeDisplayPrefs / readDisplayPrefs：来回一趟不丢字段", () => {
  const storage = fakeStorage();
  const prefs = { byTime: true, infoMode: "marks" as const, tileStep: 2 };
  writeDisplayPrefs(prefs, storage);
  assert.deepEqual(readDisplayPrefs(storage), prefs);
  assert.equal(storage.data.has(DISPLAY_STORAGE_KEY), true, "写在约定的键上");
});

test("旧 9 档表存过的下标：按尺寸换算一次，不直接夹取", () => {
  /*
   * 2026-09-20 把 9 档表换成 17 档表（512 → 400 封顶）。旧记录没有 `tileStepScale`，
   * 下标含义不同：旧 8（512）在新表里是 228，直接读就会“偷偷缩小”；
   * 正确行为是按尺寸找最接近的那档（400）。
   */
  const legacyMax = fakeStorage({
    [DISPLAY_STORAGE_KEY]: JSON.stringify({ byTime: false, infoMode: "off", tileStep: 8 }),
  });
  assert.equal(readDisplayPrefs(legacyMax).tileStep, TILE_SIZE_STEPS.length - 1, "旧最大档 → 新最大档 400");

  const legacyDefault = fakeStorage({
    [DISPLAY_STORAGE_KEY]: JSON.stringify({ byTime: false, infoMode: "off", tileStep: 4 }),
  });
  assert.equal(readDisplayPrefs(legacyDefault).tileStep, DEFAULT_TILE_STEP_INDEX, "旧默认 256 → 新表里的 256");

  // 新记录（带 `tileStepScale`）原样读，不再换算
  const fresh = fakeStorage({
    [DISPLAY_STORAGE_KEY]: JSON.stringify({
      byTime: false,
      infoMode: "off",
      tileStep: 8,
      tileStepScale: TILE_SIZE_STEPS.length,
    }),
  });
  assert.equal(readDisplayPrefs(fresh).tileStep, 8);
});

test("共享单例：写进去之后任何读者立刻看到同一个值", () => {
  resetDisplayPrefsForTests();
  setDisplayByTime(true);
  setDisplayInfoMode("marks");
  setDisplayTileStep(7);

  assert.equal(displayByTime(), true);
  assert.equal(displayInfoMode(), "marks");
  assert.equal(displayTileStep(), 7);
  assert.deepEqual(displayPrefs(), { byTime: true, infoMode: "marks", tileStep: 7 });

  resetDisplayPrefsForTests();
  assert.deepEqual(displayPrefs(), DEFAULT_DISPLAY_PREFS);
});

test("单例：非法值不写进状态（sanitize 在写入路径上也生效）", () => {
  resetDisplayPrefsForTests();
  setDisplayTileStep(999);
  assert.equal(displayTileStep(), TILE_SIZE_STEPS.length - 1, "越界夹回最大档");
  setDisplayInfoMode("nonsense" as never);
  assert.equal(displayInfoMode(), "off", "认不出的档位不接受");
  resetDisplayPrefsForTests();
});

test("commitDisplayTileStep：只在这时落盘（拖动中不写）", () => {
  // 模块级单例的存储走 `globalThis.localStorage`；Node 里没有 → 写入是静默空操作。
  // 这条用例只保证「调它不会炸」，真正的落盘由浏览器冒烟（读 localStorage）覆盖。
  resetDisplayPrefsForTests();
  setDisplayTileStep(3);
  commitDisplayTileStep();
  assert.equal(displayTileStep(), 3);
  resetDisplayPrefsForTests();
});
