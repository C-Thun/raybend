/**
 * 发版计划的单元测试。
 *
 * 这类逻辑出错是**不可原谅**的：版本号写错、正式包带上 `-beta`、把测试包当正式包发出去，
 * 任何一条都要靠测试钉死。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  BUMP_TYPES,
  bumpVersion,
  createReleasePlan,
  inBetaSeries,
  nextPrerelease,
  parseReleaseArgs,
  parseVersion,
  PRERELEASE_LABEL,
  stableVersion,
} from "./release-plan.ts";
import type { ReleaseRequest } from "./release-plan.ts";

const request = (over: Partial<ReleaseRequest> = {}): ReleaseRequest => ({
  bump: "test",
  // 不传 channel 时实现里是显式的 undefined，断言要跟着写
  channel: undefined,
  dryRun: false,
  allowDirty: false,
  skipBuild: false,
  ...over,
});

/* ─── 版本号解析与升迁 ─────────────────────────────────── */

test("parseVersion：稳定版与预发布版都能解析", () => {
  assert.deepEqual(parseVersion("0.1.0"), {
    major: 0,
    minor: 1,
    patch: 0,
    prerelease: undefined,
  });
  assert.deepEqual(parseVersion("1.2.3-beta.4"), {
    major: 1,
    minor: 2,
    patch: 3,
    prerelease: "beta.4",
  });
  assert.deepEqual(parseVersion("  2.0.1  "), {
    major: 2,
    minor: 0,
    patch: 1,
    prerelease: undefined,
  });
});

test("parseVersion：非法版本号抛错（宁可不发，也不要发出一个坏版本）", () => {
  for (const bad of [
    "1.2",
    "1.2.3.4",
    "v1.2.3",
    "",
    "abc",
    "1.2.x",
    "-1.0.0",
  ]) {
    assert.throws(() => parseVersion(bad), /版本号必须是/, `${bad} 应该被拒绝`);
  }
});

test("stableVersion：去掉预发布标识", () => {
  assert.equal(stableVersion("1.2.3-beta.4"), "1.2.3");
  assert.equal(stableVersion("1.2.3"), "1.2.3");
});

test("bumpVersion：patch / minor / major 各自进位", () => {
  assert.equal(bumpVersion("1.2.3", "patch"), "1.2.4");
  assert.equal(bumpVersion("1.2.3", "minor"), "1.3.0");
  assert.equal(bumpVersion("1.2.3", "major"), "2.0.0");
});

test("bumpVersion：test 不动版本号（同一个版本反复验证）", () => {
  assert.equal(bumpVersion("1.2.3", "test"), "1.2.3");
  assert.equal(bumpVersion("1.2.3-beta.2", "test"), "1.2.3-beta.2");
});

test("bumpVersion：从预发布升到稳定版是「定稿」，不是再跳一级", () => {
  assert.equal(bumpVersion("0.2.0-beta.3", "patch"), "0.2.0");
  assert.equal(bumpVersion("0.2.0-rc.1", "minor"), "0.2.0");
  assert.equal(bumpVersion("1.0.0-beta.1", "major"), "1.0.0");
});

test("nextPrerelease：同标签递增，换标签从 1 开始", () => {
  assert.equal(nextPrerelease("1.2.3", "beta"), "1.2.3-beta.1");
  assert.equal(
    nextPrerelease("1.2.3" + `-${PRERELEASE_LABEL}.1`),
    "1.2.3-beta.2",
    "默认标签就是 PRERELEASE_LABEL",
  );
  assert.equal(nextPrerelease("1.2.3-beta.9", "beta"), "1.2.3-beta.10");
  assert.equal(nextPrerelease("1.2.3-beta.9", "rc"), "1.2.3-rc.1");
  assert.equal(nextPrerelease("1.2.4", "beta"), "1.2.4-beta.1");
});

/* ─── 参数解析 ─────────────────────────────────────────── */

test("parseReleaseArgs：必填一个 bump 类型，另外三个开关可选", () => {
  assert.deepEqual(parseReleaseArgs(["patch"]), request({ bump: "patch" }));
  assert.deepEqual(
    parseReleaseArgs(["test", "--dry-run"]),
    request({ dryRun: true }),
  );
  assert.deepEqual(
    parseReleaseArgs(["minor", "--channel=beta"]),
    request({ bump: "minor", channel: "beta" }),
  );
  for (const bump of BUMP_TYPES) {
    assert.equal(parseReleaseArgs([bump]).bump, bump);
  }
});

test("parseReleaseArgs：缺参数 / 多参数 / 未知类型都抛错并给出用法", () => {
  assert.throws(() => parseReleaseArgs([]), /用法：pnpm release/);
  assert.throws(
    () => parseReleaseArgs(["patch", "minor"]),
    /用法：pnpm release/,
  );
  assert.throws(() => parseReleaseArgs(["nightly"]), /未知的版本升迁类型/);
});

/* ─── 计划 ─────────────────────────────────────────────── */

test("测试包：版本不变、通道 test、产物落在独立目录", () => {
  const plan = createReleasePlan({
    version: "0.1.0",
    request: request({ bump: "test" }),
    dirty: true,
    gitHash: "abc1234",
  });
  assert.equal(plan.targetVersion, "0.1.0");
  assert.equal(plan.changesVersion, false);
  assert.equal(plan.channel, "test");
  assert.equal(plan.outputDir, "dist/test-build");
  assert.deepEqual(plan.warnings, [], "测试包本来就该能带着未提交改动打");
});

test("正式包：升版 + 通道 release + 产物在 dist", () => {
  const plan = createReleasePlan({
    version: "0.1.0",
    request: request({ bump: "minor" }),
    dirty: false,
    gitHash: "abc1234",
  });
  assert.equal(plan.targetVersion, "0.2.0");
  assert.equal(plan.channel, "release");
  assert.equal(plan.changesVersion, true);
  assert.equal(plan.outputDir, "dist");
  assert.deepEqual(plan.warnings, []);
});

test("正式包在脏树上：给出警告；显式 --allow-dirty 才放行", () => {
  const dirty = createReleasePlan({
    version: "0.1.0",
    request: request({ bump: "patch" }),
    dirty: true,
    gitHash: "abc1234",
  });
  assert.equal(dirty.warnings.length, 1);
  assert.match(dirty.warnings[0], /工作树是脏的/);

  const allowed = createReleasePlan({
    version: "0.1.0",
    request: request({ bump: "patch", allowDirty: true }),
    dirty: true,
    gitHash: "abc1234",
  });
  assert.deepEqual(allowed.warnings, []);
});

test("拿不到 git hash：警告包装好后无法回溯来源", () => {
  const plan = createReleasePlan({
    version: "0.1.0",
    request: request({ bump: "test" }),
    dirty: false,
  });
  assert.equal(plan.warnings.length, 1);
  assert.match(plan.warnings[0], /git/);
});

test("beta 通道：已在 beta 线上就继续递增，不重复出版本号", () => {
  const continuing = createReleasePlan({
    version: "0.1.0-beta.2",
    request: request({ bump: "minor", channel: "beta" }),
    dirty: false,
    gitHash: "abc1234",
  });
  assert.equal(continuing.targetVersion, "0.1.0-beta.3");
  assert.equal(continuing.channel, "beta");

  // 同一目标版本上的第二个 beta：不能又是 0.2.0-beta.1（那是重复发行）
  const another = createReleasePlan({
    version: "0.2.0-beta.1",
    request: request({ bump: "patch", channel: "beta" }),
    dirty: false,
    gitHash: "abc1234",
  });
  assert.equal(another.targetVersion, "0.2.0-beta.2");
});

test("beta 通道：从稳定版出发则先升版再开 beta 线", () => {
  const plan = createReleasePlan({
    version: "0.1.0",
    request: request({ bump: "minor", channel: "beta" }),
    dirty: false,
    gitHash: "abc1234",
  });
  assert.equal(plan.targetVersion, "0.2.0-beta.1");
});

test("inBetaSeries：只认 beta.N 形式", () => {
  assert.equal(inBetaSeries("0.2.0-beta.1"), true);
  assert.equal(inBetaSeries("0.2.0"), false);
  assert.equal(inBetaSeries("0.2.0-rc.1"), false, "别的预发布标签不算同一条线");
  assert.equal(
    inBetaSeries("0.2.0-betamax"),
    false,
    "前缀相同但标签不同也不算",
  );
});

test("正式通道却带着预发布标识：报警（别把 beta 当正式版发）", () => {
  const plan = createReleasePlan({
    version: "0.1.0-beta.2",
    request: request({ bump: "test", channel: "release" }),
    dirty: false,
    gitHash: "abc1234",
  });
  assert.ok(plan.warnings.some((warning) => /预发布标识/.test(warning)));
});

test("人要做的事被明确列出来（脚本只准备，不推送）", () => {
  const plan = createReleasePlan({
    version: "0.1.0",
    request: request({ bump: "patch" }),
    dirty: false,
    gitHash: "abc1234",
  });
  assert.equal(plan.humanCommands.length, 3);
  assert.ok(plan.humanCommands.some((cmd) => cmd.startsWith("git tag")));
  assert.ok(plan.humanCommands.some((cmd) => cmd.includes("git push")));
  assert.ok(
    plan.humanCommands.every((cmd) => !cmd.includes("--force")),
    "发版命令里不该出现 force",
  );
});
