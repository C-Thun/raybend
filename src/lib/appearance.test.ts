/**
 * 外观状态的单元测试。
 *
 * 重点不是「能存能读」，而是**存坏了也不能让界面起不来**：
 * 存储里可能是旧版本写的值、手动改过的值、空串，或者存储本身被禁用。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  DEFAULT_APPEARANCE,
  DENSITY_STORAGE_KEY,
  THEME_STORAGE_KEY,
  normalizeDensity,
  normalizeTheme,
  readAppearance,
  writeAppearance,
  type AppearanceStorage,
} from "./appearance.ts";

function memoryStorage(seed: Record<string, string> = {}): AppearanceStorage & {
  data: Record<string, string>;
} {
  const data = { ...seed };
  return {
    data,
    getItem: (key) => (key in data ? data[key] : null),
    setItem: (key, value) => {
      data[key] = value;
    },
  };
}

test("默认外观：深色 + 紧凑（DESIGN.md §3 默认暗房）", () => {
  assert.deepEqual(DEFAULT_APPEARANCE, { theme: "dark", density: "compact" });
});

test("normalize：只接受两个合法值，其余一律回落默认", () => {
  assert.equal(normalizeTheme("light"), "light");
  assert.equal(normalizeTheme("dark"), "dark");
  for (const bad of ["Dark", "", null, undefined, 0, "system", "auto", {}]) {
    assert.equal(normalizeTheme(bad), "dark", `非法主题 ${String(bad)} 应回落`);
  }

  assert.equal(normalizeDensity("loose"), "loose");
  assert.equal(normalizeDensity("compact"), "compact");
  for (const bad of ["Loose", "", null, undefined, 1, "medium"]) {
    assert.equal(normalizeDensity(bad), "compact", `非法密度 ${String(bad)} 应回落`);
  }
});

test("读取：存储为空 → 默认值", () => {
  assert.deepEqual(readAppearance(memoryStorage()), DEFAULT_APPEARANCE);
});

test("读取：没有存储（隐私模式）→ 默认值，且不抛错", () => {
  assert.deepEqual(readAppearance(undefined), DEFAULT_APPEARANCE);
});

test("读取：存了合法值 → 如实取回", () => {
  const storage = memoryStorage({
    [THEME_STORAGE_KEY]: "light",
    [DENSITY_STORAGE_KEY]: "loose",
  });
  assert.deepEqual(readAppearance(storage), { theme: "light", density: "loose" });
});

test("读取：部分缺失 → 缺的那一项回落，另一项保留", () => {
  const onlyTheme = memoryStorage({ [THEME_STORAGE_KEY]: "light" });
  assert.deepEqual(readAppearance(onlyTheme), {
    theme: "light",
    density: "compact",
  });
});

test("读取：存储里的垃圾值 → 回落默认，不把非法值带到 DOM 上", () => {
  const junk = memoryStorage({
    [THEME_STORAGE_KEY]: "system",
    [DENSITY_STORAGE_KEY]: "  ",
  });
  assert.deepEqual(readAppearance(junk), DEFAULT_APPEARANCE);
});

test("写入：两个键都落盘", () => {
  const storage = memoryStorage();
  writeAppearance({ theme: "light", density: "loose" }, storage);
  assert.equal(storage.data[THEME_STORAGE_KEY], "light");
  assert.equal(storage.data[DENSITY_STORAGE_KEY], "loose");
});

test("读写的往返一致性", () => {
  const storage = memoryStorage();
  const written = { theme: "light", density: "loose" } as const;
  writeAppearance(written, storage);
  assert.deepEqual(readAppearance(storage), written);
});

test("存储抛错（配额 / 被禁用）时不冒泡", () => {
  const hostile: AppearanceStorage = {
    getItem: () => {
      throw new Error("SecurityError");
    },
    setItem: () => {
      throw new Error("QuotaExceededError");
    },
  };
  assert.deepEqual(readAppearance(hostile), DEFAULT_APPEARANCE);
  assert.doesNotThrow(() =>
    writeAppearance({ theme: "light", density: "loose" }, hostile),
  );
});
