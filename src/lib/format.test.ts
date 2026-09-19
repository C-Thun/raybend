/**
 * 数字格式化的单元测试。
 *
 * 覆盖重点（AGENTS.md §2.10）：零 / 边界位（3 位 vs 4 位）/ 负数 /
 * 超大值与小数 / 非法输入（NaN、Infinity）/ 两套语言的分组差异。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import { fileNameOf, formatCount, formatCountCapped } from "./format.ts";

test("三位以内不分组（中英一致）", () => {
  for (const n of [0, 1, 12, 999]) {
    assert.equal(formatCount(n, "zh-CN"), String(n));
    assert.equal(formatCount(n, "en-US"), String(n));
  }
});

test("四位起分组，中文用空格、英文用逗号", () => {
  assert.equal(formatCount(1000, "zh-CN"), "1 000");
  assert.equal(formatCount(1000, "en-US"), "1,000");
  assert.equal(formatCount(1248, "zh-CN"), "1 248");
  assert.equal(formatCount(1248, "en-US"), "1,248");
});

test("设计稿实例：1 248 张 与 1,248 items", () => {
  assert.equal(formatCount(1248), "1 248", "默认语言是中文");
  assert.equal(formatCount(1248, "en-US"), "1,248");
});

test("六位与七位：分组边界不会错位", () => {
  assert.equal(formatCount(123456, "zh-CN"), "123 456");
  assert.equal(formatCount(1234567, "zh-CN"), "1 234 567");
});

test("负数保留符号并照常分组", () => {
  assert.equal(formatCount(-1, "zh-CN"), "-1");
  assert.equal(formatCount(-1248, "en-US"), "-1,248");
  assert.equal(formatCount(-0, "zh-CN"), "0", "-0 不该显示成 -0");
});

test("小数按四舍五入到整数", () => {
  assert.equal(formatCount(1248.4, "zh-CN"), "1 248");
  assert.equal(formatCount(1248.6, "zh-CN"), "1 249");
  assert.equal(formatCount(0.5, "zh-CN"), "1");
});

test("非法值不传播：NaN / ±Infinity 都按 0 处理", () => {
  for (const bad of [
    Number.NaN,
    Number.POSITIVE_INFINITY,
    Number.NEGATIVE_INFINITY,
  ]) {
    assert.equal(formatCount(bad, "zh-CN"), "0");
    assert.equal(formatCountCapped(bad, 999, "zh-CN"), "0");
  }
});

test("超大值不丢位数（不产出 1e+21 这种写法）", () => {
  assert.equal(formatCount(1e21, "zh-CN"), "1 000 000 000 000 000 000 000");
  assert.equal(formatCount(1e21, "en-US").includes("e"), false);
  assert.equal(
    formatCount(Number.MAX_SAFE_INTEGER, "en-US"),
    "9,007,199,254,740,991",
  );
});

test("封顶写法：超过上限显示 999+，未超过照常", () => {
  assert.equal(formatCountCapped(1000, 999, "zh-CN"), "999+");
  assert.equal(formatCountCapped(999, 999, "zh-CN"), "999");
  assert.equal(formatCountCapped(1248, 999, "en-US"), "999+");
});

test("封顶参数非法时不封顶（不因为传了 0/NaN 就把数字全吃掉）", () => {
  assert.equal(formatCountCapped(1248, 0, "zh-CN"), "1 248");
  assert.equal(formatCountCapped(1248, Number.NaN, "zh-CN"), "1 248");
  assert.equal(formatCountCapped(1248, -5, "zh-CN"), "1 248");
});

test("路径末级名字：两种分隔符都吃", () => {
  assert.equal(fileNameOf("C:\\src\\pic\\2026-08-15\\MY0001.JPG"), "MY0001.JPG");
  assert.equal(fileNameOf("photos/2026-08-15/MY0001.JPG"), "MY0001.JPG");
  assert.equal(fileNameOf("photos/2026-08-15"), "2026-08-15");
});

test("路径末级名字的边界：末尾分隔符 / 纯分隔符 / 空串 / 无分隔符", () => {
  assert.equal(fileNameOf("photos/2026-08-15/"), "2026-08-15", "末尾分隔符不该给出空名字");
  assert.equal(fileNameOf("C:\\src\\"), "src");
  assert.equal(fileNameOf("/"), "/", "纯分隔符没有名字可言，原样返回");
  assert.equal(fileNameOf(""), "");
  assert.equal(fileNameOf("P1000023.RW2"), "P1000023.RW2");
  assert.equal(fileNameOf("照片/中文名.jpg"), "中文名.jpg", "中文名照常");
});
