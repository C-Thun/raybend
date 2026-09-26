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
  assert.equal(issueExtraHeight(4, 236), 244);
  assert.equal(issueExtraHeight(5, 236), 364);
  assert.equal(issueExtraHeight(8, 116), 204);
  assert.equal(issueExtraHeight(1, 32), 22);
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

test("日组/时间片药丸 = 整段开关（再点一次取消），不是「只加不减」", async () => {
  const s = createExportStore({
    getSetting: async () => null,
    setSetting: async () => {},
    // 每张照片 3 个定稿 ⇒ 下面用「选中了几个 key」就能反推选中了几张
    variants: async (_, ids) =>
      ids.map((assetId) => ({
        assetId,
        variants: ["sooc", "latest", "issue:1"].map((variant) => ({
          reference: { assetId, variant },
          name: variant,
          sourceBase: "sooc",
          profileHash: null,
          relPath: "photos/中文.jpg",
        })),
      })),
    snapshots: async () => [],
    validate: async () => ({ errors: {}, warnings: [] }),
    preferences: createExportPreferences({
      getItem: () => null,
      setItem: () => {},
    }),
  });
  s.context("repo", "photos");
  // 先把定稿预载上：`group` 内部的 `ensure` 会因此短路，下面的等待才只需要让
  // `setSelection` 落地（仓里既有做法：`store.test.ts` 的 `selectAllActive` 用例同型）。
  await s.ensure([1, 2, 3]);
  const base = {
    count: () => 3,
    idAt: (i: number) => String(i + 1),
    itemAt: () => null,
    itemById: () => null,
    slices: () => undefined,
    scopeKey: () => "scope",
  } as unknown as TilesSource;
  const source = exportGallerySource(base, s);
  // `selectGroupRange` 不返回 promise（内部 `void store.group(...)`）—— 让它落地
  const settle = async () => {
    for (let i = 0; i < 4; i += 1) await Promise.resolve();
  };

  source.selectGroupRange(0, 3);
  await settle();
  assert.equal(s.selection().ids.size, 9, "第一次点：整段全开（3 张 × 3 个定稿）");

  source.selectGroupRange(0, 3);
  await settle();
  assert.equal(
    s.selection().ids.size,
    0,
    "同一段再点一次：整段取消 —— 这才是开关；若实现停在「只加不减」，这里会是 9",
  );

  source.selectGroupRange(0, 2);
  await settle();
  assert.equal(s.selection().ids.size, 6, "只点前两格：只选中这一段（并进现有选择）");
  s.dispose();
});
