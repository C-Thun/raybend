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
  createAppearanceStore,
  normalizeDensity,
  normalizeTheme,
  readAppearance,
  readAppearanceOverride,
  writeAppearance,
  type Appearance,
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
    assert.equal(
      normalizeDensity(bad),
      "compact",
      `非法密度 ${String(bad)} 应回落`,
    );
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
  assert.deepEqual(readAppearance(storage), {
    theme: "light",
    density: "loose",
  });
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

/* ══════════════════════════════════════════════════════════════
 * readAppearanceOverride：开发期用 URL 切外观（截图自查的支点）
 * ══════════════════════════════════════════════════════════════ */

test("URL 覆盖：认 theme / density 两个参数", () => {
  assert.deepEqual(readAppearanceOverride("?theme=light&density=loose"), {
    theme: "light",
    density: "loose",
  });
  assert.deepEqual(readAppearanceOverride("?theme=dark"), { theme: "dark" });
  assert.deepEqual(readAppearanceOverride(""), {});
});

test("URL 覆盖：非法值一律忽略（不能因为手快打了个错参数就白屏）", () => {
  assert.deepEqual(readAppearanceOverride("?theme=solarized&density=medium"), {});
  assert.deepEqual(readAppearanceOverride("?theme=LIGHT"), {}, "大小写必须严格");
  assert.deepEqual(readAppearanceOverride("?theme=&density="), {});
});

test("URL 覆盖：多余参数不影响（带 ?a=1&theme=light 也认）", () => {
  assert.deepEqual(readAppearanceOverride("?a=1&theme=light&b=2"), { theme: "light" });
});

test("URL 覆盖：没有前导 ? 也能解析（手工拼字符串的容错）", () => {
  assert.deepEqual(readAppearanceOverride("theme=light"), { theme: "light" });
});

/* ══════════════════════════════════════════════════════════════
 * createAppearanceStore：改一下就要「同时」落到 DOM 与存储
 * ══════════════════════════════════════════════════════════════ */

/** 测试用：记录每次「落到 DOM」的值 */
function tracker() {
  const applied: Appearance[] = [];
  return {
    applied,
    apply: (appearance: Appearance) => {
      applied.push(appearance);
    },
  };
}

test("建店时就落一次 DOM：首帧必须与 store 一致", () => {
  const seen = tracker();
  createAppearanceStore({ storage: memoryStorage(), apply: seen.apply });
  assert.deepEqual(seen.applied, [DEFAULT_APPEARANCE]);
});

test("初值可从存储读：上次选的主题与密度会延续", () => {
  const seen = tracker();
  const storage = memoryStorage({
    [THEME_STORAGE_KEY]: "light",
    [DENSITY_STORAGE_KEY]: "loose",
  });
  const store = createAppearanceStore({ storage, apply: seen.apply });

  assert.equal(store.theme(), "light");
  assert.equal(store.density(), "loose");
  assert.deepEqual(seen.applied[0], { theme: "light", density: "loose" });
});

test("显式初值优先于存储（kitchen-sink 这类场景）", () => {
  const store = createAppearanceStore({
    storage: memoryStorage({ [THEME_STORAGE_KEY]: "light" }),
    initial: { theme: "dark" },
    apply: tracker().apply,
  });
  assert.equal(store.theme(), "dark");
  assert.equal(store.density(), DEFAULT_APPEARANCE.density);
});

test("切主题：状态、DOM、存储三处同时变", () => {
  const seen = tracker();
  const storage = memoryStorage();
  const store = createAppearanceStore({ storage, apply: seen.apply });
  seen.applied.length = 0;

  store.setTheme("light");
  assert.equal(store.theme(), "light");
  assert.deepEqual(seen.applied, [{ theme: "light", density: "compact" }]);
  assert.equal(
    storage.data[THEME_STORAGE_KEY],
    "light",
    "不持久化的话重启就回去了",
  );
});

test("toggleTheme：深↔浅来回切", () => {
  const store = createAppearanceStore({
    storage: memoryStorage(),
    apply: () => {},
  });
  assert.equal(store.theme(), "dark");
  store.toggleTheme();
  assert.equal(store.theme(), "light");
  store.toggleTheme();
  assert.equal(store.theme(), "dark");
});

test("切密度不影响主题（两轴独立）", () => {
  const seen = tracker();
  const store = createAppearanceStore({
    storage: memoryStorage(),
    apply: seen.apply,
  });
  store.setTheme("light");
  store.setDensity("loose");

  assert.deepEqual(store.appearance(), { theme: "light", density: "loose" });
  assert.deepEqual(seen.applied[seen.applied.length - 1], {
    theme: "light",
    density: "loose",
  });
});

test("非法输入被规范化，不会把非法值写进存储 / DOM", () => {
  const seen = tracker();
  const storage = memoryStorage();
  const store = createAppearanceStore({ storage, apply: seen.apply });
  seen.applied.length = 0;

  store.setTheme("solarized" as never);
  store.setDensity("medium" as never);

  assert.equal(store.theme(), DEFAULT_APPEARANCE.theme);
  assert.equal(store.density(), DEFAULT_APPEARANCE.density);
  assert.equal(storage.data[THEME_STORAGE_KEY], DEFAULT_APPEARANCE.theme);
  assert.deepEqual(seen.applied[seen.applied.length - 1], DEFAULT_APPEARANCE);
});

test("没有存储（隐私模式）时依然可用，只是不持久化", () => {
  const seen = tracker();
  const store = createAppearanceStore({
    storage: undefined,
    apply: seen.apply,
  });
  assert.doesNotThrow(() => store.toggleTheme());
  assert.equal(store.theme(), "light");
  assert.deepEqual(seen.applied[seen.applied.length - 1], {
    theme: "light",
    density: "compact",
  });
});
