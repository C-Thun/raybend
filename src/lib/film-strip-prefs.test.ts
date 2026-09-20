import assert from "node:assert/strict";
import test from "node:test";

import {
  createFilmStripPreferenceStore,
  FILM_STRIP_PERSIST_DEBOUNCE_MS,
  parseFilmStripStep,
  type FilmStripScope,
} from "./film-strip-prefs.ts";
import { DEFAULT_FILM_STRIP_STEP } from "./film-strip-size.ts";

test("胶片带偏好：空值、非法值与边界都安全收敛", () => {
  assert.equal(parseFilmStripStep(null), DEFAULT_FILM_STRIP_STEP);
  assert.equal(parseFilmStripStep(undefined), DEFAULT_FILM_STRIP_STEP);
  assert.equal(parseFilmStripStep(4), DEFAULT_FILM_STRIP_STEP);
  assert.equal(parseFilmStripStep(""), DEFAULT_FILM_STRIP_STEP);
  assert.equal(parseFilmStripStep("不是数字"), DEFAULT_FILM_STRIP_STEP);
  assert.equal(parseFilmStripStep("-99"), 0);
  assert.equal(parseFilmStripStep("999"), 16);
  assert.equal(parseFilmStripStep("6"), 6);
});

test("胶片带偏好：import / browse 分开加载", async () => {
  const store = createFilmStripPreferenceStore({
    keys: { import: "i", browse: "b" },
    getSetting: async (key) => (key === "i" ? "2" : "11"),
    setSetting: async () => {},
  });
  await store.load();
  assert.equal(store.step("import"), 2);
  assert.equal(store.step("browse"), 11);
});

test("胶片带偏好：同一工作流连续变化只在静止两秒后写最后值", async () => {
  const scheduled: { run: () => void; delay: number; cancelled: boolean }[] = [];
  const writes: [string, string][] = [];
  const store = createFilmStripPreferenceStore({
    keys: { import: "i", browse: "b" },
    getSetting: async () => null,
    setSetting: async (key, value) => {
      writes.push([key, value]);
    },
    schedule: (run, delay) => {
      const task = { run, delay, cancelled: false };
      scheduled.push(task);
      return task;
    },
    cancel: (handle) => {
      (handle as (typeof scheduled)[number]).cancelled = true;
    },
  });

  store.setStep("import", 5);
  store.setStep("import", 6);
  store.setStep("import", 7);
  assert.deepEqual(writes, []);
  assert.deepEqual(scheduled.map((task) => task.delay), [
    FILM_STRIP_PERSIST_DEBOUNCE_MS,
    FILM_STRIP_PERSIST_DEBOUNCE_MS,
    FILM_STRIP_PERSIST_DEBOUNCE_MS,
  ]);
  assert.deepEqual(scheduled.map((task) => task.cancelled), [true, true, false]);

  scheduled[scheduled.length - 1]?.run();
  await Promise.resolve();
  assert.deepEqual(writes, [["i", "7"]]);
});

test("胶片带偏好：两个工作流拥有互不干扰的防抖计时器", async () => {
  const tasks = new Map<FilmStripScope, () => void>();
  const writes: [string, string][] = [];
  let nextScope: FilmStripScope = "import";
  const store = createFilmStripPreferenceStore({
    keys: { import: "i", browse: "b" },
    getSetting: async () => null,
    setSetting: async (key, value) => {
      writes.push([key, value]);
    },
    schedule: (run) => {
      const scope = nextScope;
      tasks.set(scope, run);
      return scope;
    },
    cancel: () => {},
  });

  nextScope = "import";
  store.setStep("import", 8);
  nextScope = "browse";
  store.setStep("browse", 3);
  tasks.get("browse")?.();
  tasks.get("import")?.();
  await Promise.resolve();
  assert.deepEqual(writes, [["b", "3"], ["i", "8"]]);
});

test("胶片带偏好：迟到的加载结果不覆盖用户刚改的新档位", async () => {
  let resolveLoad: ((value: string | null) => void) | undefined;
  const pending = new Promise<string | null>((resolve) => {
    resolveLoad = resolve;
  });
  const store = createFilmStripPreferenceStore({
    keys: { import: "i", browse: "b" },
    getSetting: (key) => (key === "i" ? pending : Promise.resolve(null)),
    setSetting: async () => {},
  });
  const loading = store.load();
  store.setStep("import", 9);
  resolveLoad?.("1");
  await loading;
  assert.equal(store.step("import"), 9);
  store.dispose();
});
