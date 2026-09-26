import assert from "node:assert/strict";
import { test } from "node:test";

import { commitDevelopStack, developSettingsOf } from "./editor.ts";

test("提交 latest 时把 SOOC/RAW 来源原样送到 Rust", async () => {
  const scope = globalThis as Record<string, unknown>;
  const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
  scope["window"] = scope;
  scope["__TAURI_INTERNALS__"] = {
    invoke: async (command: string, args: Record<string, unknown>) => {
      calls.push({ command, args });
      return null;
    },
  };
  try {
    await commitDevelopStack("repo", 42, {
      sourceBase: "sooc",
      values: { exposure: 0.3 },
      curves: {},
    });
    await commitDevelopStack("repo", 42, {
      sourceBase: "raw",
      values: { exposure: 0.3 },
      curves: {},
      autoAdjust: { values: { exposure: 0.3 }, lensProfile: null, lensEnabled: null, nrMethod: null },
      baseCurveProfile: "3",
      baseCurvePoints: [[0, 0], [0.5, 0.65], [1, 1]],
    });
    assert.deepEqual(calls.map((call) => call.command), ["develop_commit", "develop_commit"]);
    assert.deepEqual(calls.map((call) => (call.args.stack as Record<string, unknown>).sourceBase), ["sooc", "raw"]);
    assert.deepEqual(calls.map((call) => (call.args.stack as Record<string, unknown>).baseCurveProfile), [null, "3"]);
    assert.deepEqual((calls[1]!.args.stack as Record<string, unknown>).autoAdjust,
      { values: { exposure: 0.3 }, lensProfile: null, lensEnabled: null, nrMethod: null });
    assert.deepEqual((calls[1]!.args.stack as Record<string, unknown>).baseCurvePoints,
      [[0, 0], [0.5, 0.65], [1, 1]]);
  } finally {
    delete scope["__TAURI_INTERNALS__"];
    delete scope["window"];
  }
});

test("普通载图与撤销共用完整基础曲线适配", () => {
  assert.deepEqual(developSettingsOf({ values: {}, curves: {}, sourceBase: "raw",
    baseCurveProfile: "8", baseCurvePoints: [[0, 0], [0.4, 0.55], [1, 1]],
    nrMethod: "high" }), {
    sourceBase: "raw", baseCurveProfile: "8", baseCurvePoints: [[0, 0], [0.4, 0.55], [1, 1]],
    autoAdjust: null, lutId: null, lutEnabled: null, asShotK: null, lensProfile: null, lensEnabled: null, nrMethod: "high", geometry: null,
  });
});
