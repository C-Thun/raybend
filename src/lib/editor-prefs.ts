/**
 * 编辑工作区的**设备级偏好**（`localStorage`）。
 *
 * 口径（`AGENTS.md` §2.16）：设备级偏好必须走**版本化 key + 显式一次性迁移**，
 * 迁移逻辑与测试就放在偏好自己的模块里。现在的版本是 `v3`：
 *
 * * `v1`：LUT 分类 + 面板开关（分类首次成功接入后导入 `app.db`）；
 * * `v2`：只保留面板开关（LUT 分类以数据库为真源）；
 * * `v3`（2026-10-12）：开关从「LUT 一个布尔」变成**左列面板**（`lut` / `issues` / `null`）——
 *   左列有 LUT 与定稿两个面板，同一时刻只开一个。
 *
 * # 为什么 LUT 分类是**设备级**而不是库级
 *
 * LUT 是**应用级资产**（用户导入的 `.cube` 文件），不是某个库的内容：
 * 换库之后同一个 LUT 仍然该能用（ON1 / Lightroom 也是这个口径）。
 * 所以分类表跟设备走，不进 `catalog.db`。
 *
 * # 存了什么
 *
 * * `leftPanel` —— 左列开着哪个面板（`lut` / `issues` / `null`；默认 `lut`，人类 2026-09-23 定「默认开」）；
 * * `lutCategories` —— 仅用于读取 `v1` 旧分类并迁入 `app.db`；新分类以数据库为真源。
 */

import {
  sanitizeLutCategories,
  LUT_NAME_MAX,
  type LutCategory,
} from "./lut-library.ts";
import type { EditorLeftPanel } from "./editor-chrome.ts";

export { LUT_NAME_MAX };

/** 当前存储键（版本化；以后改结构继续升版本并显式迁移）。 */
export const EDITOR_PREFS_KEY = "raybend.editor.v3";
export const LEGACY_EDITOR_PREFS_KEY = "raybend.editor.v1";
/** 上一版（只有 `lutOpen` 布尔的那个）—— 读出来做一次性迁移。 */
export const PREVIOUS_EDITOR_PREFS_KEY = "raybend.editor.v2";
const LUT_CATEGORY_IMPORT_KEY = "raybend.editor.lut-category-import.v1";

/**
 * 更早版本的键（**按新→旧排列**）。
 *
 * `v1` 的分类只读取一次，成功写入 `app.db` 后标记迁移完成。
 */
export const LEGACY_EDITOR_PREFS_KEYS: readonly string[] = [
  PREVIOUS_EDITOR_PREFS_KEY,
  LEGACY_EDITOR_PREFS_KEY,
];

export interface EditorPrefs {
  leftPanel: EditorLeftPanel | null;
  lutCategories: LutCategory[];
}

/** 默认值：**LUT 面板默认开**（人类 2026-09-23 定）。 */
export const DEFAULT_EDITOR_PREFS: EditorPrefs = {
  leftPanel: "lut",
  lutCategories: [],
};

/**
 * 左列面板字段的清洗：认 `lut` / `issues` / `null`；
 * 认不出来就回落到**旧版的 `lutOpen` 布尔**（`v2` 及更早的形状）。
 */
function sanitizeLeftPanel(panel: unknown, legacyLutOpen: unknown): EditorLeftPanel | null {
  if (panel === "lut" || panel === "issues") return panel;
  if (panel === null) return null;
  // 旧版只有「LUT 面板开着没有」这一个布尔；只有明确的 `false` 才算关着
  return legacyLutOpen === false ? null : "lut";
}

/** 任何输入 → 一份合法偏好（坏值一律丢弃，**不抛错**：存储里的垃圾不能让编辑器起不来）。 */
export function sanitizeEditorPrefs(raw: unknown): EditorPrefs {
  if (typeof raw !== "object" || raw === null) return { ...DEFAULT_EDITOR_PREFS };
  const record = raw as Record<string, unknown>;
  return {
    leftPanel: sanitizeLeftPanel(record.leftPanel, record.lutOpen),
    // 分类的清洗在 `lib/lut-library.ts`（那一份是唯一实现，这里不再抄一遍）
    lutCategories: sanitizeLutCategories(record.lutCategories),
  };
}

/**
 * 旧版本 → 当前版本的一次性迁移。
 *
 * `raw` 是 `parse` 前的原始字符串，来自当前版本、`v2` 或 `v1`。
 *
 * 返回值就是**当前版本的偏好形状**（不是 `unknown`）：调用方拿到即可直接用。
 * 解析失败一律回到默认值 —— 存储里的垃圾不该让编辑器起不来。
 */
export function migrateEditorPrefs(raw: string): EditorPrefs {
  try {
    return sanitizeEditorPrefs(JSON.parse(raw));
  } catch {
    return { ...DEFAULT_EDITOR_PREFS };
  }
}

export interface EditorPrefsStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

function storage(): EditorPrefsStorage | undefined {
  try {
    return globalThis.localStorage;
  } catch {
    return undefined;
  }
}

/** 读偏好（损坏、非法、缺字段都安全收敛到默认值；旧键读出来后写回当前键）。 */
export function readEditorPrefs(
  source: EditorPrefsStorage | undefined = storage(),
): EditorPrefs {
  if (!source) return { ...DEFAULT_EDITOR_PREFS };
  try {
    const raw = source.getItem(EDITOR_PREFS_KEY);
    if (raw !== null && raw !== "") return migrateEditorPrefs(raw);
    for (const key of LEGACY_EDITOR_PREFS_KEYS) {
      const old = source.getItem(key);
      if (old === null || old === "") continue;
      const migrated = migrateEditorPrefs(old);
      source.setItem(EDITOR_PREFS_KEY, JSON.stringify(migrated));
      return migrated;
    }
    return { ...DEFAULT_EDITOR_PREFS };
  } catch {
    return { ...DEFAULT_EDITOR_PREFS };
  }
}

/** 写偏好（写不进去也不抛 —— 隐私模式 / 配额满不该让面板开关崩掉）。 */
export function writeEditorPrefs(
  prefs: EditorPrefs,
  target: EditorPrefsStorage | undefined = storage(),
): void {
  if (!target) return;
  try {
    target.setItem(EDITOR_PREFS_KEY, JSON.stringify(prefs));
  } catch {
    // 静默：偏好是「锦上添花」，丢了不影响这次会话
  }
}

/** 旧分类仅首次成功接入 app.db 前上送；之后数据库就是唯一真源。 */
export function pendingLegacyLutCategories(
  source: EditorPrefsStorage | undefined = storage(),
): Array<{ id: string; name: string }> {
  if (!source) return [];
  try {
    if (source.getItem(LUT_CATEGORY_IMPORT_KEY) === "done") return [];
    const old = source.getItem(LEGACY_EDITOR_PREFS_KEY);
    if (!old) return [];
    return migrateEditorPrefs(old).lutCategories.map(({ id, name }) => ({ id, name }));
  } catch { return []; }
}

export function markLegacyLutCategoriesImported(
  target: EditorPrefsStorage | undefined = storage(),
): void {
  try { target?.setItem(LUT_CATEGORY_IMPORT_KEY, "done"); } catch { /* DB 已是唯一真源 */ }
}
