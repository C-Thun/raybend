import { test } from "node:test";
import assert from "node:assert/strict";
import { windowsBuildEnv } from "./dav1d-win.mjs";

test("Windows 构建复用 dav1d 配置，覆盖路径并去重 WSLENV，已翻译路径不再 /p 转换", () => {
  const env = windowsBuildEnv({ CARGO_TARGET_DIR: "E:\\构建 空格\\target", RAYBEND_CHANNEL: "test" }, ["CARGO_TARGET_DIR"], {
    PATH: "fixture", RAYBEND_DAV1D_WIN_DIR: "F:\\依赖\\dav1d", WSLENV: "KEEP/p:CARGO_TARGET_DIR/p:SYSTEM_DEPS_DAV1D_INCLUDE/p:RAYBEND_CHANNEL",
  });
  assert.equal(env.SYSTEM_DEPS_DAV1D_SEARCH_NATIVE, "F:\\依赖\\dav1d\\lib");
  assert.equal(env.SYSTEM_DEPS_DAV1D_INCLUDE, "F:\\依赖\\dav1d\\include");
  assert.equal(env.CARGO_TARGET_DIR, "E:\\构建 空格\\target");
  assert.ok(env.WSLENV.includes("KEEP/p")); assert.ok(!env.WSLENV.includes("CARGO_TARGET_DIR/p"));
  const names = env.WSLENV.split(":"); assert.equal(names.length, new Set(names).size);
  assert.equal(env.PATH, "fixture");
});
