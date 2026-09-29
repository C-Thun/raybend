import assert from "node:assert/strict";
import { test } from "node:test";
import { sanitizeSystemPreferences, systemDefaults } from "./system-preferences.ts";

test("中文显示语言变体统一使用中文，包括大小写、繁体、脚本和地区", () => {
  for (const language of ["zh", "zh-CN", "zh-TW", "zh-Hans", "zh-Hant-HK", "ZH-cn", " zh_CN.UTF-8 "]) {
    assert.deepEqual(systemDefaults({ theme: "light", language }), { theme: "light", locale: "zh-CN" });
  }
});

test("其它语言及未知语言默认英文，未知外观默认 dark", () => {
  for (const language of ["en-US", "ja-JP", "fr", "中文", "zho", "zhong", "", null, 42]) {
    assert.deepEqual(systemDefaults({ theme: null, language }), { theme: "dark", locale: "en-US" });
  }
  for (const value of [undefined, null, "zh-CN", [], { theme: "Light", language: {} }]) {
    assert.deepEqual(systemDefaults(value), { theme: "dark", locale: "en-US" });
  }
});

test("载荷按项清洗：非法项不污染另一个合法项，长的非中文语言保持英文", () => {
  assert.deepEqual(sanitizeSystemPreferences({ theme: "dark", language: false }), { theme: "dark", language: null });
  assert.deepEqual(systemDefaults({ theme: "auto", language: "zh-Hant" }), { theme: "dark", locale: "zh-CN" });
  assert.equal(systemDefaults({ language: "en-" + "x".repeat(10_000) }).locale, "en-US");
});
