import assert from "node:assert/strict";
import test from "node:test";
import {
  issueExtraHeight,
  exportGallerySource,
  exportQueueSource,
} from "./source.ts";
import { createExportStore } from "./store.ts";
import { createExportPreferences } from "../../lib/export-prefs.ts";
import type { TilesSource } from "../../components/ui/tiles/source.ts";
import { IDLE_THUMB } from "../../components/ui/thumb-queue.ts";
test("issue wrap follows cell width; more than four and small tiles add correct rows", () => {
  assert.equal(issueExtraHeight(0, 240), 0);
  assert.equal(issueExtraHeight(4, 236), 78);
  assert.equal(issueExtraHeight(5, 236), 152);
  assert.equal(issueExtraHeight(8, 116), 300);
  assert.equal(issueExtraHeight(1, 32), 78);
});
test("gallery adapter preserves shared photo source including unloaded asset IDs", async () => {
  const s = createExportStore({
    getSetting: async () => null,
    setSetting: async () => {},
    variants: async (_, ids) =>
      ids.map((assetId) => ({ assetId, variants: [] })),
    snapshots: async () => [],
    validate: async () => ({ errors: {}, warnings: [] }),
    preferences: createExportPreferences({
      getItem: () => null,
      setItem: () => {},
    }),
  });
  s.context("repo", "photos");
  const ranges: number[][] = [];
  const base = {
    count: () => 3,
    idAt: (i: number) => String(i + 1),
    itemAt: () => null,
    itemById: () => null,
    slices: () => undefined,
    ensureRange: async (start: number, end: number) => {
      ranges.push([start, end]);
    },
    scopeKey: () => "scope",
  } as unknown as TilesSource;
  const source = exportGallerySource(base, s);
  assert.equal(source.count(), 3);
  assert.equal(source.idAt?.(2), "3");
  await source.ensureRange?.(1, 3);
  assert.deepEqual(ranges, [[1, 3]]);
  assert.ok(s.variants().has(2));
  assert.ok(s.variants().has(3));
  assert.equal(source.invertedCtrl, true);
  const queue = exportQueueSource(s, {
    get: () => IDLE_THUMB,
    request: () => {},
  });
  assert.equal(queue.count(), 0);
  assert.equal(queue.itemAt(0), null);
  assert.equal(queue.scopeKey(), "queue:");
  s.dispose();
});
