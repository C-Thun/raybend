import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { PRODUCT_PACKAGES, productBuildArgs, productBuildCommand, productBuildEnv } from "./product-build.mjs";

/** AI 关 / 开两种能力（`resolveAiBuild()` 的真实返回形状，只取与本文件相关的字段）。 */
const AI_OFF = { enabled: false, features: ["custom-protocol"], resources: {} };
const AI_ON = {
  enabled: true,
  features: ["custom-protocol", "photo-ai-runtime"],
  resources: { "../.release/ai-input-v1/abc/ai-model/*": "ai-model/" },
};

test("产品构建包含主程序、worker 所在的包与崩溃助手 —— 少一个就会被当成垃圾清掉", () => {
  assert.deepEqual(PRODUCT_PACKAGES, ["raybend-desktop", "raybend", "raybend-crashpad"]);
  assert.deepEqual(productBuildArgs({ aiPlan: AI_OFF }), [
    "build",
    "-p", "raybend-desktop",
    "-p", "raybend",
    "-p", "raybend-crashpad",
    "--features", "custom-protocol",
  ]);
});

test("--message-format=json 只在要清单时出现，并重定向到给定路径（空格/中文路径不破坏）", () => {
  assert.equal(productBuildArgs({ aiPlan: AI_OFF }).includes("--message-format=json"), false);
  const command = productBuildCommand({ aiPlan: AI_OFF, jsonPath: "C:\\构建 缓存\\last-build.json" });
  assert.ok(command.startsWith("cargo build "));
  assert.ok(command.endsWith("--message-format=json > C:\\构建 缓存\\last-build.json"));
  assert.equal(productBuildCommand({ aiPlan: AI_OFF }).includes(">"), false);
});

test("构建环境：target 目录、AI 开关与 feature 成对，且经 WSLENV 透传给 cmd.exe", () => {
  const on = productBuildEnv({ aiPlan: AI_ON, cargoTargetDir: "C:\\rb-target\\raybend" });
  assert.equal(on.CARGO_TARGET_DIR, "C:\\rb-target\\raybend");
  assert.equal(on.RAYBEND_PHOTO_AI, "1");
  assert.match(on.TAURI_CONFIG, /ai-model/);
  assert.ok(on.WSLENV.split(":").includes("CARGO_TARGET_DIR"));
  assert.ok(on.WSLENV.split(":").includes("RAYBEND_PHOTO_AI"));

  const off = productBuildEnv({ aiPlan: AI_OFF, cargoTargetDir: "E:\\build\\target" });
  assert.equal(off.CARGO_TARGET_DIR, "E:\\build\\target");
  assert.equal(off.RAYBEND_PHOTO_AI, "0");
  assert.doesNotMatch(off.TAURI_CONFIG, /ai-model/);
});

test("debug:win 与 clean:win 都必须走这份定义（各写一份必然漂成两套产物）", () => {
  for (const name of ["debug-win.mjs", "clean-target.mjs"]) {
    const source = readFileSync(new URL(`../${name}`, import.meta.url), "utf8");
    assert.match(source, /productBuildCommand\(/, `${name} 没有用 productBuildCommand`);
    assert.match(source, /productBuildEnv\(/, `${name} 没有用 productBuildEnv`);
  }
});
