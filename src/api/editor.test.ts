import assert from "node:assert/strict";
import { test } from "node:test";

import { commitDevelopStack } from "./editor.ts";

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
    });
    assert.deepEqual(calls.map((call) => call.command), ["develop_commit", "develop_commit"]);
    assert.deepEqual(calls.map((call) => (call.args.stack as Record<string, unknown>).sourceBase), ["sooc", "raw"]);
  } finally {
    delete scope["__TAURI_INTERNALS__"];
    delete scope["window"];
  }
});
