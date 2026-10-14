import { test } from "node:test";
import assert from "node:assert/strict";
import {
  BETA_CHANNEL_ENABLED,
  UPDATE_ENDPOINTS,
  UPDATE_PREF_KEY,
  autoCheckAllowed,
  readUpdatePrefs,
  updateSource,
  updatesAvailable,
} from "./update-source.ts";

test("v2 偏好优先，坏值不拦路", () => {
  assert.deepEqual(readUpdatePrefs('{"channel":"beta","autoCheck":false}', null, null), {
    channel: "beta",
    autoCheck: false,
  });
  assert.deepEqual(readUpdatePrefs('{"channel":"stable"}', null, null), {
    channel: "stable",
    autoCheck: true,
  });
  assert.deepEqual(readUpdatePrefs("garbage", null, null), { channel: "stable", autoCheck: true });
  assert.deepEqual(readUpdatePrefs('{"channel":"编写错了"}', null, null), {
    channel: "stable",
    autoCheck: true,
  });
  assert.equal(UPDATE_PREF_KEY, "raybend.updates.v2");
});

test("从 v1 迁移：off 等价于关掉自动检查，custom 归并到稳定", () => {
  assert.deepEqual(readUpdatePrefs(null, '{"mode":"off"}', null), { channel: "stable", autoCheck: false });
  assert.deepEqual(readUpdatePrefs(null, '{"mode":"stable"}', null), { channel: "stable", autoCheck: true });
  assert.deepEqual(readUpdatePrefs(null, '{"mode":"beta"}', null), { channel: "beta", autoCheck: true });
  assert.deepEqual(readUpdatePrefs(null, '{"mode":"custom"}', null), { channel: "stable", autoCheck: true });
  // 两个旧键都关过才算关（任一为 off 就不自动查）
  assert.deepEqual(readUpdatePrefs(null, '{"mode":"stable"}', "off"), { channel: "stable", autoCheck: false });
  assert.deepEqual(readUpdatePrefs(null, null, null), { channel: "stable", autoCheck: true });
});

test("自动检查的准入：通道对得上、用户没关、构建带公钥", () => {
  const on = { channel: "stable" as const, autoCheck: true };
  assert.equal(autoCheckAllowed(on, "release", "pub"), true);
  assert.equal(autoCheckAllowed({ ...on, autoCheck: false }, "release", "pub"), false);
  assert.equal(autoCheckAllowed(on, "release", "  "), false, "没有公钥就没有可验证的源");
  assert.equal(autoCheckAllowed(on, "test", "pub"), false, "测试包不自动查，免得被拉到正式版");
  assert.equal(autoCheckAllowed(on, "dev", "pub"), false);
  assert.equal(autoCheckAllowed({ channel: "beta", autoCheck: true }, "release", "pub"), false, "正式包不追预览源");
  assert.equal(autoCheckAllowed({ channel: "beta", autoCheck: true }, "beta", "pub"), true);
});

test("源组装与「预览先灰着」的口子", () => {
  const source = updateSource({ channel: "stable", autoCheck: true }, "pub");
  assert.deepEqual(source, { mode: "stable", endpoint: UPDATE_ENDPOINTS.stable, publicKey: "pub" });
  assert.match(UPDATE_ENDPOINTS.stable, /^https:\/\/github\.com\//);
  assert.match(UPDATE_ENDPOINTS.beta, /^https:\/\//);
  assert.equal(BETA_CHANNEL_ENABLED, false, "预览通道留口但先别开");
  assert.equal(updatesAvailable("pub"), true);
  assert.equal(updatesAvailable(""), false);
});
