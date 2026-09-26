import assert from "node:assert/strict";
import test from "node:test";
import {
  getExportSnapshots,
  getExportVariantImage,
  getExportVariants,
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
    maxEdge: 0,
    directory: "",
    template: ":FILENAME",
  });
  assert.ok(result.errors.name);
  assert.ok(result.errors.directory);
});
