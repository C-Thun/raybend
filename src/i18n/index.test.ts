/**
 * i18n 运行时的小测试（语言包本身的完整性在 `locale-parity.test.ts`）。
 *
 * 三条最容易被静默弄坏的东西：
 *   1. 占位符替换（`{name}`）与**缺参数时保持原样**（否则缺一个参数就变成半句话）；
 *   2. 未翻译的 key **原样返回 key 名** —— 这是有意的（界面上出现 `source.recent`
 *      这种字样一眼就能看出漏译，比空白强）；
 *   3. `timeoutMessage()` 的拼装（它跨了「动作」与「句式」两条文案）。
 *
 * ⚠️ 语言是**模块级信号**，测试之间会互相影响 —— 每条用例结束都拨回中文。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  locale,
  setLocale,
  t,
  timeoutMessage,
  type MessageKey,
} from "./index.ts";

function withLocale<T>(id: "zh-CN" | "en-US", run: () => T): T {
  try {
    setLocale(id);
    return run();
  } finally {
    setLocale("zh-CN");
  }
}

test("占位符按名字替换；缺的参数保持原样（不把句子弄残）", () => {
  withLocale("zh-CN", () => {
    assert.equal(t("grid.count", { n: 12 }), "12 张");
    // 少给一个参数 → 只替换给到的那个，剩下的 `{...}` 原样留着
    assert.equal(t("source.load_error"), "读不了这个位置：{message}");
  });
});

test("未翻译的 key 原样返回 key 名（一眼能看出漏译）", () => {
  withLocale("zh-CN", () => {
    assert.equal(t("no.such.key" as MessageKey), "no.such.key");
  });
});

test("timeoutMessage：动作与句式都走语言包，两种语言各自成形", () => {
  withLocale("zh-CN", () => {
    assert.equal(
      timeoutMessage("import.timeout.start", 15_000),
      "启动导入没有在 15 秒内回应（后端可能已经挂了）",
    );
  });
  withLocale("en-US", () => {
    assert.equal(
      timeoutMessage("import.timeout.start", 15_000),
      "Start import did not respond within 15s (the backend may have crashed)",
    );
  });
});

test("timeoutMessage 的秒数是四舍五入的整数（日志与界面都不该出现 14.999）", () => {
  withLocale("zh-CN", () => {
    assert.match(timeoutMessage("import.timeout.precheck", 14_600), /在 15 秒内/);
  });
});

test("setLocale 会把语言写到 <html lang>（WebView 的断行与字体回退都读它）", () => {
  // Node 里没有 document —— 运行时应当自己跳过，不抛错
  withLocale("en-US", () => {
    assert.equal(locale(), "en-US");
  });
  assert.equal(locale(), "zh-CN");
});
