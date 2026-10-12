/** 编辑偏好测试（版本化 key + 一次性迁移；边界：损坏 / 非法 / 缺字段 → 回默认）。 */
import assert from "node:assert/strict";
import test from "node:test";

import {
  DEFAULT_EDITOR_PREFS,
  EDITOR_PREFS_KEY,
  LEGACY_EDITOR_PREFS_KEY,
  PREVIOUS_EDITOR_PREFS_KEY,
  pendingLegacyLutCategories,
  markLegacyLutCategoriesImported,
  migrateEditorPrefs,
  readEditorPrefs,
  sanitizeEditorPrefs,
  writeEditorPrefs,
  type EditorPrefs,
  type EditorPrefsStorage,
} from "./editor-prefs.ts";

/** 一个**真的会记住**的假存储（`getItem` 读到的就是 `setItem` 写进去的）。 */
function fakeStorage(seed: Record<string, string> = {}): EditorPrefsStorage & {
  written: Record<string, string>;
} {
  const written: Record<string, string> = { ...seed };
  return {
    written,
    getItem: (key) => written[key] ?? null,
    setItem: (key, value) => {
      written[key] = value;
    },
  };
}

test("存储键是版本化的（AGENTS.md §2.16）", () => {
  assert.equal(EDITOR_PREFS_KEY, "raybend.editor.v3");
  assert.equal(PREVIOUS_EDITOR_PREFS_KEY, "raybend.editor.v2");
  assert.equal(LEGACY_EDITOR_PREFS_KEY, "raybend.editor.v1");
});

test("默认值：左列默认开 LUT 面板、没有分类", () => {
  assert.deepEqual(DEFAULT_EDITOR_PREFS, { leftPanel: "lut", lutCategories: [] });
  assert.deepEqual(readEditorPrefs(fakeStorage()), DEFAULT_EDITOR_PREFS);
});

test("leftPanel 只认 lut / issues / null，垃圾值回到默认的 lut", () => {
  assert.equal(sanitizeEditorPrefs({ leftPanel: "issues" }).leftPanel, "issues");
  assert.equal(sanitizeEditorPrefs({ leftPanel: "lut" }).leftPanel, "lut");
  assert.equal(sanitizeEditorPrefs({ leftPanel: null }).leftPanel, null);
  assert.equal(sanitizeEditorPrefs({ leftPanel: "nope" }).leftPanel, "lut");
  assert.equal(sanitizeEditorPrefs({}).leftPanel, "lut");
  assert.equal(sanitizeEditorPrefs(null).leftPanel, "lut");
});

test("v2 的 lutOpen 布尔迁到 v3 的 leftPanel（只有明确 false 才算关着）", () => {
  assert.equal(sanitizeEditorPrefs({ lutOpen: false }).leftPanel, null);
  assert.equal(sanitizeEditorPrefs({ lutOpen: true }).leftPanel, "lut");
  assert.equal(sanitizeEditorPrefs({ lutOpen: "no" }).leftPanel, "lut");
  // v3 的字段优先于旧的 lutOpen（升级后两个字段同时存在时不许旧值翻盘）
  assert.equal(sanitizeEditorPrefs({ leftPanel: "issues", lutOpen: false }).leftPanel, "issues");
});

test("损坏的 JSON / 非法结构都安全收敛（不抛）", () => {
  assert.deepEqual(migrateEditorPrefs("{ not json"), DEFAULT_EDITOR_PREFS);
  assert.deepEqual(migrateEditorPrefs("null"), DEFAULT_EDITOR_PREFS);
  assert.deepEqual(readEditorPrefs(fakeStorage({ [EDITOR_PREFS_KEY]: "(((" })), DEFAULT_EDITOR_PREFS);
});

test("读回写回一整轮：分类与左列面板都不丢", () => {
  const storage = fakeStorage();
  const prefs: EditorPrefs = { leftPanel: null, lutCategories: [{ id: "a", name: "旅行", entries: [] }] };
  writeEditorPrefs(prefs, storage);
  assert.deepEqual(readEditorPrefs(storage), prefs);
});

test("写失败不抛（隐私模式 / 配额满不该让面板开关崩掉）", () => {
  const broken: EditorPrefsStorage = {
    getItem: () => null,
    setItem: () => {
      throw new Error("quota");
    },
  };
  assert.doesNotThrow(() => writeEditorPrefs(DEFAULT_EDITOR_PREFS, broken));
});

test("v2 → v3：读旧键、迁移、写回新键", () => {
  const old = { lutOpen: false, lutCategories: [{ id: "film", name: "胶片", entries: [] }] };
  const storage = fakeStorage({ [PREVIOUS_EDITOR_PREFS_KEY]: JSON.stringify(old) });
  assert.deepEqual(readEditorPrefs(storage), {
    leftPanel: null,
    lutCategories: [{ id: "film", name: "胶片", entries: [] }],
  });
  assert.equal(
    storage.written[EDITOR_PREFS_KEY],
    JSON.stringify({ leftPanel: null, lutCategories: [{ id: "film", name: "胶片", entries: [] }] }),
  );
});

test("v1 分类升级后只上送一次 app.db，成功标记后不复活旧分类", () => {
  const old = { lutOpen: false, lutCategories: [{ id: "film", name: "胶片", entries: [] }] };
  const storage = fakeStorage({ [LEGACY_EDITOR_PREFS_KEY]: JSON.stringify(old) });
  assert.deepEqual(readEditorPrefs(storage).leftPanel, null);
  assert.deepEqual(pendingLegacyLutCategories(storage), [{ id: "film", name: "胶片" }]);
  markLegacyLutCategoriesImported(storage);
  assert.deepEqual(pendingLegacyLutCategories(storage), []);
});
