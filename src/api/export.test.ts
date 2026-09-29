import assert from "node:assert/strict";
import test from "node:test";
import {
  getExportSnapshots,
  getExportVariantImage,
  getExportVariants,
  getVariantThumb,
  validateExportPreset,
} from "./export.ts";
test("browser export IPC fallback is explicit and does not produce simulated issues or images", async () => {
  assert.deepEqual(await getExportVariants("repo", [1]), []);
  assert.deepEqual(
    await getExportSnapshots("repo", [{ assetId: 1, variant: "sooc" }]),
    [],
  );
  assert.equal(
    await getExportVariantImage(
      "repo",
      { assetId: 1, variant: "sooc" },
      "grid",
    ),
    null,
  );
  const result = await validateExportPreset({
    id: "new",
    name: "",
    format: "jpeg",
    quality: 90,
    maxEdge: 0, sizeMode: "original" as const, percent: 100,
    directory: "",
    template: ":FILENAME",
    existingFile: "append",
  });
  assert.ok(result.errors.name);
  assert.ok(result.errors.directory);
});


test("RAW 真小图命中不读模拟；未命中则把内嵌字节和滤镜标记一起返回", async () => {
  const scope = globalThis as Record<string, unknown>;
  const calls: Array<Record<string, unknown>> = [];
  let hasRaw = true;
  scope["window"] = scope;
  scope["__TAURI_INTERNALS__"] = { invoke: async (_command: string, args: Record<string, unknown>) => {
    calls.push(args);
    return args.rawOriginal === true ? (hasRaw ? [1, 2] : []) : [3, 4];
  } };
  try {
    const ref = { assetId: 1, variant: "raw" };
    assert.deepEqual(await getVariantThumb("repo", ref, "strip"), { bytes: new Uint8Array([1, 2]), approximate: false });
    assert.equal(calls.length, 1);
    hasRaw = false; calls.length = 0;
    assert.deepEqual(await getVariantThumb("repo", ref, "grid"), { bytes: new Uint8Array([3, 4]), approximate: true });
    assert.deepEqual(calls.map((call) => call.rawOriginal), [true, false]);
    calls.length = 0;
    assert.deepEqual(await getVariantThumb("repo", { assetId: 1, variant: "issue:2" }, "strip"),
      { bytes: new Uint8Array([3, 4]), approximate: false });
    assert.equal(calls.length, 1);
    assert.equal(calls[0]?.rawOriginal, null);
  } finally {
    delete scope["__TAURI_INTERNALS__"];
    delete scope["window"];
  }
});
