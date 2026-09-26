import assert from "node:assert/strict";
import test from "node:test";
import {
  createExportStore,
  EXPORT_PRESETS_KEY,
  type ExportStoreDeps,
} from "./store.ts";
import { createExportPreferences } from "../../lib/export-prefs.ts";
import {
  serializePresets,
  variantKey,
  type AssetVariants,
  type ExportPreset,
  type VariantSnapshot,
} from "../../lib/export-model.ts";
const preset: ExportPreset = {
  id: "preset-a",
  name: "网页",
  format: "jpeg",
  quality: 90,
  maxEdge: 0,
  directory: "C:\\输出",
  template: ":FILENAME",
};
const variants = (ids: readonly number[]): AssetVariants[] =>
  ids.map((assetId) => ({
    assetId,
    variants: ["sooc", "latest", "issue:1"].map((variant) => ({
      reference: { assetId, variant },
      name: variant,
      sourceBase: "sooc",
      profileHash: null,
      relPath: "photos/中文.jpg",
    })),
  }));
const snapshot = (reference: {
  assetId: number;
  variant: string;
}): VariantSnapshot => ({
  reference,
  name: reference.variant,
  profileHash: "hash",
  relPath: "photos/中文.jpg",
  sourceSignature: "sig",
  stack: { values: { exposure: 1 } },
});
function setup(patch: Partial<ExportStoreDeps> = {}) {
  const writes: string[] = [];
  const s = createExportStore({
    getSetting: async (key) => {
      assert.equal(key, EXPORT_PRESETS_KEY);
      return serializePresets([preset]);
    },
    setSetting: async (key, value) => {
      assert.equal(key, EXPORT_PRESETS_KEY);
      writes.push(value);
    },
    variants: async (_, ids) => variants(ids),
    snapshots: async (_, refs) => refs.map(snapshot),
    validate: async () => ({ errors: {}, warnings: [] }),
    id: () => "preset-b",
    preferences: createExportPreferences({
      getItem: () => null,
      setItem: () => {},
    }),
    ...patch,
  });
  s.context("repo", "photos");
  return { s, writes };
}
function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
test("one asset contributes several independently selectable issues", async () => {
  const { s } = setup();
  await s.ready;
  await s.ensure([1]);
  await s.selectIssue({ assetId: 1, variant: "sooc" }, "toggle", [1]);
  await s.selectIssue({ assetId: 1, variant: "issue:1" }, "toggle", [1]);
  assert.equal(s.selection().ids.size, 2);
  await s.selectIssue({ assetId: 1, variant: "latest" }, "replace", [1]);
  assert.deepEqual(
    [...s.selection().ids],
    [variantKey("repo", { assetId: 1, variant: "latest" })],
  );
  s.dispose();
});
test("Shift reads unloaded pages and respects every issue between anchor and target", async () => {
  const { s } = setup();
  await s.ensure([1]);
  await s.selectIssue({ assetId: 1, variant: "latest" }, "replace", [1, 2, 3]);
  await s.selectIssue({ assetId: 3, variant: "sooc" }, "range", [1, 2, 3]);
  assert.equal(s.selection().ids.size, 6);
  assert.equal(s.variants().size, 3);
  s.dispose();
});
test("asset toggle selects/removes its issue group, control replace only that group", async () => {
  const { s } = setup();
  await s.selectAssets([1], "toggle", [1, 2]);
  assert.equal(s.selection().ids.size, 3);
  await s.selectAssets([2], "toggle", [1, 2]);
  assert.equal(s.selection().ids.size, 6);
  await s.selectAssets([1], "toggle", [1, 2]);
  assert.equal(s.selection().ids.size, 3);
  await s.selectAssets([1], "replace", [1, 2]);
  assert.ok(
    [...s.selection().ids].every(
      (key) => (JSON.parse(key) as unknown[])[1] === 1,
    ),
  );
  s.dispose();
});
test("overlapping requests deduplicate, chunk at 128, cache negative result", async () => {
  const calls: number[][] = [];
  const wait = deferred<AssetVariants[]>();
  const { s } = setup({
    variants: async (_, ids) => {
      calls.push([...ids]);
      return calls.length === 1 ? wait.promise : [];
    },
  });
  const a = s.ensure([1, 2]);
  const b = s.ensure([2, 3]);
  wait.resolve(variants([1, 2]));
  await Promise.all([a, b]);
  assert.deepEqual(calls, [[1, 2], [3]]);
  await s.ensure([3]);
  assert.equal(calls.length, 2);
  await s.ensure(Array.from({ length: 257 }, (_, i) => i + 10));
  assert.deepEqual(
    calls.slice(2).map((x) => x.length),
    [128, 128, 1],
  );
  s.dispose();
});
test("late response after scope switch and clear cannot resurrect selection", async () => {
  const d = deferred<AssetVariants[]>();
  const { s } = setup({ variants: async () => d.promise });
  const pending = s.selectAssets([1], "replace", [1]);
  s.clear();
  d.resolve(variants([1]));
  await pending;
  assert.equal(s.selection().ids.size, 0);
  const d2 = deferred<AssetVariants[]>();
  const { s: other } = setup({ variants: async () => d2.promise });
  const p = other.ensure([1]);
  other.context("other", "scope");
  d2.resolve(variants([1]));
  await p;
  assert.equal(other.variants().size, 0);
  s.dispose();
  other.dispose();
});
test("new gesture wins over slow range; scope cycle clears hidden selection", async () => {
  const d = deferred<AssetVariants[]>();
  const { s } = setup({
    variants: async (_, ids) => (ids.includes(2) ? d.promise : variants(ids)),
  });
  await s.selectAssets([1], "replace", [1, 2]);
  const p = s.selectAssets([2], "range", [1, 2]);
  await s.selectIssue({ assetId: 1, variant: "sooc" }, "replace", [1, 2]);
  d.resolve(variants([2]));
  await p;
  assert.equal(s.selection().ids.size, 1);
  s.cycleScope();
  assert.equal(s.selection().ids.size, 0);
  assert.equal(s.preferences.value().scope, "edited");
  s.dispose();
});
test("issue enqueue snapshots, deduplicates and private queues survive context changes without any persistence", async () => {
  const { s, writes } = setup();
  await s.ready;
  s.choosePreset(preset.id);
  await s.selectAssets([1], "replace", [1]);
  await s.enqueue("C:\\库");
  assert.equal(s.progress(preset.id).total, 3);
  await s.enqueue("C:\\库");
  assert.equal(s.progress(preset.id).total, 3);
  assert.equal(writes.length, 0);
  assert.equal(s.processing(), false);
  assert.equal(s.enabled().size, 0);
  s.context("other", "other");
  assert.equal(s.progress(preset.id).total, 3);
  s.edit({ ...preset, name: "第二个" });
  await s.save();
  await s.selectAssets([1], "replace", [1]);
  await s.enqueue("C:\\另一个库");
  assert.equal(s.progress("preset-b").total, 3);
  assert.equal(s.progress(preset.id).total, 3);
  s.reset();
  assert.equal(s.queues().size, 0);
  assert.equal(s.presets().length, 2);
  s.dispose();
});
test("newest batch at top; captured objects independent; saved parameter changes leave old entries intact", async () => {
  const captured = snapshot({ assetId: 1, variant: "latest" });
  const { s } = setup({ snapshots: async () => [captured] });
  await s.ready;
  s.choosePreset(preset.id);
  await s.ensure([1]);
  await s.selectIssue(captured.reference, "replace", [1]);
  await s.enqueue("root");
  (captured.stack as { values: { exposure: number } }).values.exposure = 9;
  assert.equal(
    (
      s.queues().get(preset.id)![0]!.snapshot.stack as {
        values: { exposure: number };
      }
    ).values.exposure,
    1,
  );
  s.edit({ quality: 20 });
  await s.save();
  assert.equal(s.queues().get(preset.id)![0]!.preset.quality, 90);
  s.dispose();
});
test("partial snapshots reject whole batch, IPC errors visible, no partial queue", async () => {
  const { s } = setup({ snapshots: async () => [] });
  await s.ready;
  s.choosePreset(preset.id);
  await s.selectAssets([1], "replace", [1]);
  await s.enqueue("root");
  assert.equal(s.queues().size, 0);
  assert.ok(s.error());
  assert.equal(s.busy(), false);
  s.dispose();
});
test("save trims/normalizes names, overwrites by existing stable ID and persists only presets", async () => {
  const { s, writes } = setup();
  await s.ready;
  s.edit({ ...preset, id: "new", name: " 网页 ", quality: 80 });
  await s.save();
  assert.equal(s.selectedPreset()?.id, preset.id);
  assert.equal(s.presets().length, 1);
  assert.equal(s.dirty(), false);
  assert.doesNotMatch(writes[0]!, /enabled|queues/);
  s.choosePreset(null);
  assert.equal(s.draft().name, "");
  s.dispose();
});
test("invalid settings and failed writes do not claim successful save", async () => {
  const { s } = setup({
    setSetting: async () => {
      throw Error("disk denied");
    },
  });
  await s.ready;
  s.edit({ ...preset, quality: 0 });
  await s.save();
  assert.ok(s.validation().errors.quality);
  s.edit({ quality: 70 });
  await s.save();
  assert.match(s.error() ?? "", /denied/);
  assert.equal(s.presets()[0]?.quality, 90);
  s.dispose();
});
test("new App state restores only presets: no queues or switches", async () => {
  const { s } = setup();
  await s.ready;
  assert.equal(s.queues().size, 0);
  assert.equal(s.enabled().size, 0);
  assert.equal(s.processing(), false);
  s.dispose();
});

test("日期/时间片使用共享整组开关，焦点移动不破坏 issue 多选", async () => {
  const { s } = setup();
  await s.group([1, 2]);
  assert.equal(s.selection().ids.size, 6);
  s.focusIssue({ assetId: 2, variant: "latest" });
  assert.equal(
    s.selection().anchor,
    variantKey("repo", { assetId: 2, variant: "latest" }),
  );
  assert.equal(s.selection().ids.size, 6);
  await s.group([1]);
  assert.equal(s.selection().ids.size, 3);
  await s.group([1]);
  assert.equal(s.selection().ids.size, 6);
  await s.group([1, 2]);
  assert.equal(s.selection().ids.size, 0);
  s.dispose();
});
