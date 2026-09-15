/**
 * 构建信息读取与展示的单元测试。
 *
 * 重点在「注入值是垃圾」这一路：打包脚本传错环境变量、手工改 package.json、
 * 或者构建机没装 git —— 这些都会让注入值缺字段，而**界面不能因此挂掉**。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  BUILD_CHANNELS,
  channelLabel,
  formatBuildLabel,
  isReleaseReady,
  readBuildInfo,
  showsDebugInfo,
  UNKNOWN_BUILD,
  type BuildInfo,
} from "./build-info.ts";

test("完整注入值：逐字段取回", () => {
  const info = readBuildInfo({
    version: "1.2.3",
    channel: "release",
    builtAt: "2026-09-15T12:00:00.000Z",
    gitHash: "abc1234",
    dirty: false,
  });
  assert.deepEqual(info, {
    version: "1.2.3",
    channel: "release",
    builtAt: "2026-09-15T12:00:00.000Z",
    gitHash: "abc1234",
    dirty: false,
  });
});

test("没有注入值 / 注入值不是对象 → 回落默认，不抛错", () => {
  const fallback: BuildInfo = {
    ...UNKNOWN_BUILD,
    gitHash: undefined,
    dirty: false,
  };
  for (const raw of [undefined, null, "1.2.3", 42, [], {}]) {
    assert.deepEqual(
      readBuildInfo(raw),
      fallback,
      `${JSON.stringify(raw)} 应该回落默认`,
    );
  }
});

test("版本号为空串 / 只有空白 → 回落默认版本（界面上不能出现空版本）", () => {
  assert.equal(readBuildInfo({ version: "" }).version, UNKNOWN_BUILD.version);
  assert.equal(
    readBuildInfo({ version: "   " }).version,
    UNKNOWN_BUILD.version,
  );
  assert.equal(
    readBuildInfo({ version: " 1.2.3 " }).version,
    "1.2.3",
    "顺手去掉首尾空白",
  );
});

test("未知通道 → 回落 dev（宁可显示成开发版，也不要显示怪字符串）", () => {
  assert.equal(readBuildInfo({ channel: "nightly" }).channel, "dev");
  assert.equal(readBuildInfo({ channel: 123 }).channel, "dev");
  for (const channel of BUILD_CHANNELS) {
    assert.equal(readBuildInfo({ channel }).channel, channel);
  }
});

test("gitHash 空串按「没有」处理（构建机没 git 时会注入空串）", () => {
  assert.equal(readBuildInfo({ gitHash: "" }).gitHash, undefined);
  assert.equal(readBuildInfo({ gitHash: "abc1234" }).gitHash, "abc1234");
});

test("dirty 只认真正的 true（字符串 'true' 不算）", () => {
  assert.equal(readBuildInfo({ dirty: true }).dirty, true);
  assert.equal(readBuildInfo({ dirty: "true" }).dirty, false);
  assert.equal(readBuildInfo({}).dirty, false);
});

test("通道显示名：四个通道都有短名", () => {
  assert.equal(channelLabel("dev"), "Dev");
  assert.equal(channelLabel("test"), "Test");
  assert.equal(channelLabel("beta"), "Beta");
  assert.equal(channelLabel("release"), "Release");
});

test("标签：版本 · 通道 · hash；没有 hash 时不留下多余的分隔点", () => {
  assert.equal(
    formatBuildLabel({
      version: "0.1.0",
      channel: "dev",
      builtAt: "",
      gitHash: "3f2a1c9",
    }),
    "0.1.0 · Dev · 3f2a1c9",
  );
  assert.equal(
    formatBuildLabel({ version: "0.1.0", channel: "release", builtAt: "" }),
    "0.1.0 · Release",
  );
});

test("是否算正式发布：必须 release 且工作树干净", () => {
  const base = { version: "1.0.0", builtAt: "" };
  assert.equal(isReleaseReady({ ...base, channel: "release" }), true);
  assert.equal(
    isReleaseReady({ ...base, channel: "release", dirty: false }),
    true,
  );
  assert.equal(
    isReleaseReady({ ...base, channel: "release", dirty: true }),
    false,
  );
  assert.equal(
    isReleaseReady({ ...base, channel: "test", dirty: false }),
    false,
  );
  assert.equal(
    isReleaseReady({ ...base, channel: "beta", dirty: false }),
    false,
  );
  assert.equal(isReleaseReady({ ...base, channel: "dev", dirty: true }), false);
});

test("调试信息：开发包与测试包看得到，公测与正式包看不到", () => {
  assert.equal(showsDebugInfo("dev"), true);
  assert.equal(showsDebugInfo("test"), true);
  assert.equal(showsDebugInfo("beta"), false);
  assert.equal(showsDebugInfo("release"), false);
});
