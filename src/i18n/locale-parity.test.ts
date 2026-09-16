/**
 * 两种语言的**键必须完全一致**。
 *
 * 为什么单独钉一条：`MessageKey = keyof typeof zhCN`，所以
 * * 中文有、英文没有 → 英文界面直接显示成 key（`repo.settings_title` 这种）；
 * * 英文有、中文没有 → 类型报错，但只在有人跑 `tsc` 时才发现。
 *
 * 两种都是「加一条文案时只改了一个文件」造成的，而且都要等到切语言才看得见。
 * 这条测试让它在 `pnpm test` 里当场炸。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { zhCN } from "./zh-CN.ts";
import { enUS } from "./en-US.ts";

test("中英两边的键集合完全一致（多的少的一个都不行）", () => {
  const zh = new Set(Object.keys(zhCN));
  const en = new Set(Object.keys(enUS));
  const missingInEn = [...zh].filter((key) => !en.has(key)).sort();
  const missingInZh = [...en].filter((key) => !zh.has(key)).sort();
  assert.deepEqual(missingInEn, [], `英文缺这些键：${missingInEn.join(", ")}`);
  assert.deepEqual(missingInZh, [], `中文缺这些键：${missingInZh.join(", ")}`);
});

test("没有空文案（控件会变成一片空白）", () => {
  for (const [locale, table] of [
    ["zh-CN", zhCN],
    ["en-US", enUS],
  ] as const) {
    for (const [key, value] of Object.entries(table)) {
      assert.notEqual(
        String(value).trim(),
        "",
        `${locale} 的 ${key} 是空的`,
      );
    }
  }
});

test("插值占位符两边对得上（少一个 {name} 文案就成了半句话）", () => {
  const placeholders = (text: string): string[] =>
    [...String(text).matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
  for (const key of Object.keys(zhCN) as (keyof typeof zhCN)[]) {
    const zh = placeholders(zhCN[key]);
    const en = placeholders(enUS[key as keyof typeof enUS] ?? "");
    assert.deepEqual(en, zh, `${String(key)} 的占位符不一致：中文 ${zh} / 英文 ${en}`);
  }
});
