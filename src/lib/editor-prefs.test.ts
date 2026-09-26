/** 编辑偏好测试（版本化 key + 一次性迁移；边界：损坏 / 非法 / 缺字段 → 回默认）。 */
import assert from "node:assert/strict";
import test from "node:test";

import {
  DEFAULT_EDITOR_PREFS,
  EDITOR_PREFS_KEY,
  LEGACY_EDITOR_PREFS_KEY,
  pendingLegacyLutCategories,
  markLegacyLutCategoriesImported,
  migrateEditorPrefs,
  readEditorPrefs,
  sanitizeEditorPrefs,
  writeEditorPrefs,
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
  assert.equal(EDITOR_PREFS_KEY, "raybend.editor.v2");
});

test("默认值：面板默认开、没有分类", () => {
  assert.deepEqual(DEFAULT_EDITOR_PREFS, { lutOpen: true, lutCategories: [] });
  assert.deepEqual(readEditorPrefs(fakeStorage()), DEFAULT_EDITOR_PREFS);
});

test("只有明确的 false 才算「关着」，垃圾值回到默认的开", () => {
  assert.equal(sanitizeEditorPrefs({ lutOpen: false }).lutOpen, false);
  assert.equal(sanitizeEditorPrefs({}).lutOpen, true);
  assert.equal(sanitizeEditorPrefs({ lutOpen: "no" }).lutOpen, true);
  assert.equal(sanitizeEditorPrefs(null).lutOpen, true);
});

test("损坏的 JSON / 非法结构都安全收敛（不抛）", () => {
  assert.deepEqual(migrateEditorPrefs("{ not json"), DEFAULT_EDITOR_PREFS);
  assert.deepEqual(migrateEditorPrefs("null"), DEFAULT_EDITOR_PREFS);
  assert.deepEqual(readEditorPrefs(fakeStorage({ [EDITOR_PREFS_KEY]: "(((" })), DEFAULT_EDITOR_PREFS);
});

test("读回写回一整轮：分类与开关都不丢", () => {
  const storage = fakeStorage();
  const prefs = { lutOpen: false, lutCategories: [{ id: "a", name: "旅行", entries: [] }] };
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


test("v1 分类升级 v2 后只上送一次 app.db，成功标记后不复活旧分类", () => {
  const old = { lutOpen: false, lutCategories: [{ id: "film", name: "胶片", entries: [] }] };
  const storage = fakeStorage({ [LEGACY_EDITOR_PREFS_KEY]: JSON.stringify(old) });
  assert.deepEqual(readEditorPrefs(storage), old);
  assert.equal(storage.written[EDITOR_PREFS_KEY], JSON.stringify(old));
  assert.deepEqual(pendingLegacyLutCategories(storage), [{ id: "film", name: "胶片" }]);
  markLegacyLutCategoriesImported(storage);
  assert.deepEqual(pendingLegacyLutCategories(storage), []);
});
