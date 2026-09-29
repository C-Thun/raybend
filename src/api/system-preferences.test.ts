import assert from "node:assert/strict";
import { test } from "node:test";
import { browserSystemPreferencesAdapter, readSystemPreferences, type SystemPreferencesAdapter } from "./system-preferences.ts";
import { systemDefaults } from "../lib/system-preferences.ts";

test("浏览器 adapter 检测 light/dark；媒体查询缺失或不匹配表示未知", async () => {
  for (const selected of ["dark", "light", "unknown"]) {
    const adapter = browserSystemPreferencesAdapter({
      matchMedia: (query) => ({ matches: query.includes(selected) }),
      language: () => "zh-TW",
    });
    assert.deepEqual(await adapter.read(), { theme: selected === "unknown" ? null : selected, language: "zh-TW" });
  }
  assert.deepEqual(await browserSystemPreferencesAdapter({ matchMedia: () => undefined, language: () => undefined }).read(), { theme: null, language: null });
});

test("外观与语言异常分别降级，独立保留另一项", async () => {
  const hostile = (): never => { throw new Error("blocked"); };
  assert.deepEqual(await browserSystemPreferencesAdapter({ matchMedia: hostile, language: () => "zh-CN" }).read(), { theme: null, language: "zh-CN" });
  assert.deepEqual(await browserSystemPreferencesAdapter({ matchMedia: () => ({ matches: true }), language: hostile }).read(), { theme: "dark", language: null });
});

test("原生首选语言优先，即使浏览器备用值是中文；缺失项由可移植 adapter 补齐", async () => {
  const fallbackAdapter: SystemPreferencesAdapter = { read: async () => ({ theme: "light", language: "zh-CN" }) };
  assert.deepEqual(await readSystemPreferences({
    adapter: { read: async () => ({ theme: "dark", language: "en-US" }) }, fallbackAdapter,
  }), { theme: "dark", language: "en-US" });
  assert.deepEqual(await readSystemPreferences({
    adapter: { read: async () => ({ theme: null, language: "zh-Hant" }) }, fallbackAdapter,
  }), { theme: "light", language: "zh-Hant" });
});

test("异常、同步抛错和非法载荷不阻断首屏；全部失败默认 dark + en-US", async () => {
  const fallbackAdapter: SystemPreferencesAdapter = { read: () => { throw new Error("failed"); } };
  for (const adapter of [fallbackAdapter, { read: async () => { throw new Error("failed"); } }, { read: async () => null as never }]) {
    assert.deepEqual(systemDefaults(await readSystemPreferences({ adapter, fallbackAdapter })), { theme: "dark", locale: "en-US" });
  }
});

test("无响应或迟到的原生探测受时限约束，迟到结果不修改已返回的首次默认", async () => {
  let finish: ((value: { theme: "light"; language: string }) => void) | undefined;
  const adapter: SystemPreferencesAdapter = { read: () => new Promise((resolve) => { finish = resolve; }) };
  const fallbackAdapter: SystemPreferencesAdapter = { read: async () => ({ theme: "dark", language: "en-US" }) };
  const initial = await readSystemPreferences({ adapter, fallbackAdapter, timeoutMs: 5 });
  assert.deepEqual(initial, { theme: "dark", language: "en-US" });
  finish?.({ theme: "light", language: "zh-CN" });
  await Promise.resolve();
  assert.deepEqual(initial, { theme: "dark", language: "en-US" });
});

test("普通浏览器只有一个 adapter 时只探测一次", async () => {
  let reads = 0;
  const adapter: SystemPreferencesAdapter = { read: async () => { reads += 1; return { theme: null, language: "en-US" }; } };
  await readSystemPreferences({ adapter, fallbackAdapter: adapter });
  assert.equal(reads, 1);
});
