import assert from "node:assert/strict";
import test from "node:test";
import {
  presetErrors,
  queueProgress,
  readPresets,
  serializePresets,
  variantKey,
  visibleVariants,
  type ExportPreset,
  type ExportQueueItem,
  type VariantSummary,
} from "./export-model.ts";
const preset: ExportPreset = {
  id: "stable",
  name: "中文 📷",
  format: "jpeg",
  quality: 90,
  maxEdge: 0,
  directory: "C:\\输出",
  template: ":FILENAME",
};
test("variant identity distinguishes repositories, assets and multiple issues", () => {
  assert.notEqual(
    variantKey("one", { assetId: 1, variant: "issue:1" }),
    variantKey("one", { assetId: 1, variant: "issue:2" }),
  );
  assert.notEqual(
    variantKey("one", { assetId: 1, variant: "latest" }),
    variantKey("two", { assetId: 1, variant: "latest" }),
  );
});
test("scope keeps RAW baseline in all; SOOC only includes real bitmaps; edited includes named/latest", () => {
  const variants = ["raw", "latest", "issue:1"].map(
    (variant) => ({ reference: { assetId: 1, variant } }) as VariantSummary,
  );
  assert.equal(visibleVariants(variants, "all").length, 3);
  assert.deepEqual(
    visibleVariants(variants, "edited").map((v) => v.reference.variant),
    ["latest", "issue:1"],
  );
  assert.deepEqual(
    visibleVariants(variants, "sooc").map((v) => v.reference.variant),
    [],
  );
  assert.equal(visibleVariants([{reference:{assetId:2,variant:"sooc"}} as VariantSummary], "sooc").length, 1);
  assert.deepEqual(visibleVariants([], "all"), []);
});
test("queue counter includes completed in total; failure remains unfinished; only actual running animates", () => {
  const entries = ["pending", "done", "failed"].map(
    (status) => ({ status }) as ExportQueueItem,
  );
  assert.deepEqual(queueProgress(entries), {
    remaining: 2,
    total: 3,
    processing: false,
  });
  assert.deepEqual(queueProgress([]), {
    remaining: 0,
    total: 0,
    processing: false,
  });
  assert.equal(
    queueProgress([{ status: "running" } as ExportQueueItem]).processing,
    true,
  );
  assert.deepEqual(queueProgress([{ status: "done" } as ExportQueueItem]), {
    remaining: 0,
    total: 1,
    processing: false,
  });
});
test("presets persist explicit settings only, never queue, processing or enabled", () => {
  const payload = serializePresets([
    { ...preset, queue: [1], enabled: true, status: "running" } as ExportPreset,
  ]);
  assert.deepEqual(readPresets(payload), [preset]);
  assert.doesNotMatch(payload, /queue|enabled|running/);
});
test("damaged, future version, primitive, duplicates and invalid fields rejected", () => {
  for (const raw of [
    null,
    "bad",
    "[]",
    "null",
    "2",
    '{"version":2,"presets":[]}',
  ])
    assert.deepEqual(readPresets(raw), []);
  assert.deepEqual(
    readPresets(
      JSON.stringify({
        version: 1,
        presets: [
          preset,
          { ...preset, id: "two" },
          { ...preset, name: "other" },
          { ...preset, id: "bad", name: "third", quality: 0 },
        ],
      }),
    ),
    [preset],
  );
});
test("preset Unicode, number edges, NUL and fractional/NaN values", () => {
  assert.deepEqual(presetErrors(preset), {});
  assert.equal(
    presetErrors({ ...preset, quality: 1, maxEdge: 65535 }).quality,
    undefined,
  );
  assert.ok(
    presetErrors({
      ...preset,
      name: "\0",
      quality: NaN,
      maxEdge: 2.4,
      directory: "\0",
      template: " ",
    }).name,
  );
  assert.ok(presetErrors({ ...preset, name: "字".repeat(129) }).name);
  for (const q of [0, 101, 1.5, Infinity])
    assert.ok(presetErrors({ ...preset, quality: q }).quality);
});
