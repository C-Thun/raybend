/** import / browse 独立显示偏好与 v1 → v2 迁移测试。 */

import assert from "node:assert/strict";
import test from "node:test";

import { DEFAULT_TILE_STEP_INDEX, TILE_SIZE_STEPS } from "./tile-flow.ts";
import {
  browseDisplayByTime,
  browseDisplayInfoMode,
  browseDisplayTileStep,
  commitBrowseDisplayTileStep,
  commitImportDisplayTileStep,
  DEFAULT_DISPLAY_PREFS,
  displayPrefs,
  DISPLAY_STORAGE_KEY,
  importDisplayByTime,
  importDisplayInfoMode,
  importDisplayTileStep,
  LEGACY_DISPLAY_STORAGE_KEY,
  readDisplayPrefs,
  resetDisplayPrefsForTests,
  sanitizeDisplayPrefs,
  setBrowseDisplayByTime,
  setBrowseDisplayInfoMode,
  setBrowseDisplayTileStep,
  setImportDisplayByTime,
  setImportDisplayInfoMode,
  setImportDisplayTileStep,
  TILE_STEP_SCALE,
  writeDisplayPrefs,
  type DisplayStorage,
} from "./display-prefs.ts";

function fakeStorage(seed: Record<string, string> = {}): DisplayStorage & {
  data: Map<string, string>;
} {
  const data = new Map(Object.entries(seed));
  return {
    data,
    getItem: (key) => data.get(key) ?? null,
    setItem: (key, value) => data.set(key, value),
  };
}

test("sanitizeDisplayPrefs：非法输入落回两份默认，绝不抛错", () => {
  for (const raw of [null, undefined, 42, "x", [], true]) {
    assert.deepEqual(sanitizeDisplayPrefs(raw), DEFAULT_DISPLAY_PREFS);
  }
  assert.deepEqual(
    sanitizeDisplayPrefs({
      import: { byTime: "yes", infoMode: "huge", tileStep: "3" },
      browse: false,
    }),
    DEFAULT_DISPLAY_PREFS,
  );
});

test("sanitizeDisplayPrefs：两边分别校验，import 的 marks 收敛成显示文件名", () => {
  assert.deepEqual(
    sanitizeDisplayPrefs({
      import: { byTime: true, infoMode: "marks", tileStep: 6.25 },
      browse: { byTime: false, infoMode: "marks", tileStep: 999 },
    }),
    {
      import: { byTime: true, infoMode: "marks-name", tileStep: 6.25 },
      browse: {
        byTime: false,
        infoMode: "marks",
        tileStep: TILE_SIZE_STEPS.length - 1,
      },
    },
  );
});

test("readDisplayPrefs：没有存储 / 坏 JSON / 空串都退回默认", () => {
  assert.deepEqual(readDisplayPrefs(undefined), DEFAULT_DISPLAY_PREFS);
  assert.deepEqual(readDisplayPrefs(fakeStorage()), DEFAULT_DISPLAY_PREFS);
  assert.deepEqual(
    readDisplayPrefs(fakeStorage({ [DISPLAY_STORAGE_KEY]: "{oops" })),
    DEFAULT_DISPLAY_PREFS,
  );
  assert.deepEqual(
    readDisplayPrefs(fakeStorage({ [DISPLAY_STORAGE_KEY]: "" })),
    DEFAULT_DISPLAY_PREFS,
  );
});

test("writeDisplayPrefs / readDisplayPrefs：两份配置来回不串线，小数档位不丢", () => {
  const storage = fakeStorage();
  const prefs = {
    import: { byTime: true, infoMode: "marks-name" as const, tileStep: 2.4 },
    browse: { byTime: false, infoMode: "marks" as const, tileStep: 9.75 },
  };
  writeDisplayPrefs(prefs, storage);
  assert.deepEqual(readDisplayPrefs(storage), prefs);
  const written = JSON.parse(storage.data.get(DISPLAY_STORAGE_KEY) ?? "{}") as Record<string, unknown>;
  assert.equal(written.version, 2);
  assert.equal(written.tileStepScale, TILE_STEP_SCALE);
});

test("v1 共享配置迁移成两份：旧档位按尺寸换算，import 信息只保留关/开", () => {
  const storage = fakeStorage({
    [LEGACY_DISPLAY_STORAGE_KEY]: JSON.stringify({
      byTime: true,
      infoMode: "marks",
      tileStep: 8,
    }),
  });
  const prefs = readDisplayPrefs(storage);
  assert.deepEqual(prefs, {
    import: {
      byTime: true,
      infoMode: "marks-name",
      tileStep: TILE_SIZE_STEPS.length - 1,
    },
    browse: {
      byTime: true,
      infoMode: "marks",
      tileStep: TILE_SIZE_STEPS.length - 1,
    },
  });
  assert.equal(storage.data.has(DISPLAY_STORAGE_KEY), true, "迁移结果立刻写入 v2 键");
});

test("v1 带当前档位尺度时不重复迁移", () => {
  const storage = fakeStorage({
    [LEGACY_DISPLAY_STORAGE_KEY]: JSON.stringify({
      byTime: false,
      infoMode: "off",
      tileStep: 8.5,
      tileStepScale: TILE_STEP_SCALE,
    }),
  });
  assert.equal(readDisplayPrefs(storage).browse.tileStep, 8.5);
});

test("模块单例：import / browse 更新完全独立", () => {
  resetDisplayPrefsForTests();
  setImportDisplayByTime(true);
  setImportDisplayInfoMode("marks");
  setImportDisplayTileStep(3.5);
  setBrowseDisplayInfoMode("marks");
  setBrowseDisplayTileStep(10.25);

  assert.equal(importDisplayByTime(), true);
  assert.equal(importDisplayInfoMode(), "marks-name");
  assert.equal(importDisplayTileStep(), 3.5);
  assert.equal(browseDisplayByTime(), false);
  assert.equal(browseDisplayInfoMode(), "marks");
  assert.equal(browseDisplayTileStep(), 10.25);
  assert.notDeepEqual(displayPrefs().import, displayPrefs().browse);

  setBrowseDisplayByTime(true);
  assert.equal(importDisplayByTime(), true);
  assert.equal(browseDisplayByTime(), true);
  resetDisplayPrefsForTests();
});

test("非法值被夹取；两边 commit 在无 localStorage 环境也不抛错", () => {
  resetDisplayPrefsForTests();
  setImportDisplayTileStep(999);
  setBrowseDisplayTileStep(Number.NaN);
  assert.equal(importDisplayTileStep(), TILE_SIZE_STEPS.length - 1);
  assert.equal(browseDisplayTileStep(), DEFAULT_TILE_STEP_INDEX);
  commitImportDisplayTileStep();
  commitBrowseDisplayTileStep();
  resetDisplayPrefsForTests();
});
