/**
 * 编辑工作区的**设备级偏好**（`localStorage`）。
 *
 * 口径（`AGENTS.md` §2.16）：设备级偏好必须走**版本化 key + 显式一次性迁移**，
 * 迁移逻辑与测试就放在偏好自己的模块里。现在的版本是 `v1`，还没有旧版本要迁 ——
 * 但 `migrateEditorPrefs` 这条通道先立好：以后加 `v2` 时只在这一个文件里加一段。
 *
 * # 为什么 LUT 分类是**设备级**而不是库级
 *
 * LUT 是**应用级资产**（用户导入的 `.cube` 文件），不是某个库的内容：
 * 换库之后同一个 LUT 仍然该能用（ON1 / Lightroom 也是这个口径）。
 * 所以分类表跟设备走，不进 `catalog.db`。
 *
 * # 存了什么
 *
 * * `lutOpen` —— LUT 面板开着没有（`design/editor.md` §3.1：人类 2026-09-23 定「默认开 + 持久化」）；
 * * `lutCategories` —— 一级分类（**只有一级**，`.pd` 明确）+ 每个分类里的 LUT 条目。
 *   W1 只用到分类本身（LUT 文件导入在 W4），但结构先按最终形状定，免得 W4 再迁一次。
 */

import {
  sanitizeLutCategories,
  LUT_NAME_MAX,
  type LutCategory,
} from "./lut-library.ts";

export { LUT_NAME_MAX };

/** 当前存储键（**版本化**：以后加字段就升 v2 并写一段迁移）。 */
export const EDITOR_PREFS_KEY = "raybend.editor.v1";

/**
 * 更早版本的键（**按新→旧排列**）。
 *
 * 现在为空 —— 留着这条通道，是为了让「升版本时必须写迁移」这件事在代码里有个位置，
 * 而不是等到真要迁的时候才想起来（`lib/display-prefs.ts` 那边就是这样做的）。
 */
export const LEGACY_EDITOR_PREFS_KEYS: readonly string[] = [];

export interface EditorPrefs {
  lutOpen: boolean;
  lutCategories: LutCategory[];
}

/** 默认值：**面板默认开**（人类 2026-09-23 定）。 */
export const DEFAULT_EDITOR_PREFS: EditorPrefs = {
  lutOpen: true,
  lutCategories: [],
};

/** 任何输入 → 一份合法偏好（坏值一律丢弃，**不抛错**：存储里的垃圾不能让编辑器起不来）。 */
export function sanitizeEditorPrefs(raw: unknown): EditorPrefs {
  if (typeof raw !== "object" || raw === null) return { ...DEFAULT_EDITOR_PREFS };
  const record = raw as Record<string, unknown>;
  return {
    // 只有明确的 `false` 才算「关着」——缺字段 / 垃圾值都回到默认的「开」
    lutOpen: record.lutOpen !== false,
    // 分类的清洗在 `lib/lut-library.ts`（那一份是唯一实现，这里不再抄一遍）
    lutCategories: sanitizeLutCategories(record.lutCategories),
  };
}

/**
 * 旧版本 → 当前版本的一次性迁移。
 *
 * 现在没有旧版本可迁（通道已就位）；`raw` 是 `parse` 前的**原始字符串**，
 * 以后 `v2` 需要读旧键时在这里读 `storage`。
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

/** 读偏好（损坏、非法、缺字段都安全收敛到默认值）。 */
export function readEditorPrefs(
  source: EditorPrefsStorage | undefined = storage(),
): EditorPrefs {
  if (!source) return { ...DEFAULT_EDITOR_PREFS };
  try {
    const raw = source.getItem(EDITOR_PREFS_KEY);
    if (raw === null || raw === "") return { ...DEFAULT_EDITOR_PREFS };
    return migrateEditorPrefs(raw);
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
