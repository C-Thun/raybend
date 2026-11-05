/**
 * 右栏**可编辑文字字段**的键名，以及「最近输入」的设备级记忆。
 *
 * 为什么在 `lib` 里也有一份键名：`src/lib` 不许 import `src/api`（`lint:arch` 的分层规则），
 * 而这里既要不依赖 API、又要跟后端的 `marking::TextField` 对齐。对齐全靠
 * `features/browse/text-edit.ts` 里那条**编译期断言**（两边类型必须完全相同）。
 *
 * 存储是 **localStorage、版本化 key**（`AGENTS.md` §2.16 对设备级偏好的口径）：
 *   * 这是「这台电脑上这个人的输入习惯」，不属于某个库，不进 catalog、不进 XMP；
 *   * 读坏/读不到一律当作空历史（它不是数据，丢了不影响任何照片）。
 *
 * 与「库内已有值」（`browse_text_values`）是**两个源**，界面合并去重后一起给：
 * 最近输入跨库、跟着人走；库内已有值跟着库走。第一次在新库写作者时，前者才是救命的。
 */

import { createSignal, type Accessor } from "solid-js";

/** 与 `src/api/types.ts::EditableTextField`、Rust `marking::TextField` 一一对应。 */
export type TextFieldKey =
  | "author"
  | "description"
  | "country"
  | "provinceState"
  | "city"
  | "sublocation";

export const TEXT_FIELDS: readonly TextFieldKey[] = [
  "author",
  "description",
  "country",
  "provinceState",
  "city",
  "sublocation",
];

/**
 * 每个字段保留多少条最近输入。
 *
 * 与 Rust 的 `marking::TEXT_SUGGEST_LIMIT` 同值（那边给「库内已有值」封顶，这边给最近输入封顶），
 * 登记在 `memory/LIMITS.md`。20 = 下拉 8 行可见、最多滚两屏，够覆盖一次旅行的常用地点。
 */
export const TEXT_SUGGEST_LIMIT = 20;

/**
 * 文字字段的长度上限（与 Rust `marking::TEXT_FIELD_MAX_CHARS` 同值）。
 *
 * 前端用它给输入框的 `maxLength`：打到顶就输不进去了，比「存的时候报错」好得多。
 */
export const TEXT_FIELD_MAX_CHARS = 2000;

export type TextHistory = Partial<Record<TextFieldKey, readonly string[]>>;

export interface TextHistoryStorage {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
}

export const TEXT_HISTORY_STORAGE_KEY = "raybend.textHistory.v1";

function defaultStorage(): TextHistoryStorage | undefined {
  try {
    return globalThis.localStorage;
  } catch {
    return undefined;
  }
}

function isFieldKey(value: string): value is TextFieldKey {
  return (TEXT_FIELDS as readonly string[]).includes(value);
}

/** 把一条输入规整成可存的样子：去掉首尾空白，空串不算一条。 */
export function normalizeRecentValue(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

/** `value` 挪到队首；重复的（按 trim 后逐字比较）只留一条；超出 `limit` 的丢掉。 */
export function pushRecentValue(
  recent: readonly string[],
  value: string,
  limit: number = TEXT_SUGGEST_LIMIT,
): readonly string[] {
  const next = normalizeRecentValue(value);
  if (next === null || limit <= 0) return recent;
  const rest = recent.filter((item) => item !== next);
  return [next, ...rest].slice(0, limit);
}

/**
 * 读存储里的历史并**消毒**：未知字段、非字符串、超长数组、空白项一律丢掉。
 * 任何异常都退回空历史（偏好不可读不影响主流程）。
 */
export function sanitizeTextHistory(raw: unknown, limit: number = TEXT_SUGGEST_LIMIT): TextHistory {
  if (typeof raw !== "object" || raw === null) return {};
  const record = raw as Record<string, unknown>;
  // 版本化 key 的第一道闸：结构对不上的（比如来自旧版本）直接当空。
  if (record.version !== 1) return {};
  const source = typeof record.fields === "object" && record.fields !== null
    ? (record.fields as Record<string, unknown>)
    : {};
  const out: TextHistory = {};
  for (const [key, value] of Object.entries(source)) {
    if (!isFieldKey(key) || !Array.isArray(value)) continue;
    const seen = new Set<string>();
    const items: string[] = [];
    for (const item of value) {
      if (typeof item !== "string") continue;
      const normalized = normalizeRecentValue(item);
      if (normalized === null || seen.has(normalized)) continue;
      seen.add(normalized);
      items.push(normalized);
      if (items.length >= limit) break;
    }
    if (items.length > 0) out[key] = items;
  }
  return out;
}

export function readTextHistory(
  storage: TextHistoryStorage | undefined = defaultStorage(),
): TextHistory {
  if (!storage) return {};
  try {
    const raw = storage.getItem(TEXT_HISTORY_STORAGE_KEY);
    if (raw === null || raw === "") return {};
    return sanitizeTextHistory(JSON.parse(raw));
  } catch {
    return {};
  }
}

export function writeTextHistory(
  history: TextHistory,
  storage: TextHistoryStorage | undefined = defaultStorage(),
): void {
  if (!storage) return;
  try {
    storage.setItem(
      TEXT_HISTORY_STORAGE_KEY,
      JSON.stringify({ version: 1, fields: history }),
    );
  } catch {
    // 存不进去不影响输入本身：下次少一条候选而已。
  }
}

const [history, setHistory] = createSignal<TextHistory>(readTextHistory());

/** 全部字段的最近输入（只读）。 */
export const textHistory: Accessor<TextHistory> = history;

/** 某个字段的最近输入（队首 = 最近用过）。 */
export function recentTextValues(field: TextFieldKey): readonly string[] {
  return history()[field] ?? [];
}

/**
 * 成功写入库之后调用：把值记进该字段的历史。
 *
 * 只在**落库成功**后记（失败的值不该变成下次的候选 —— 那会把打错的名字留下来）。
 */
export function rememberTextValue(field: TextFieldKey, value: string): void {
  const current = history();
  const next = pushRecentValue(current[field] ?? [], value);
  if (next === current[field]) return;
  const merged: TextHistory = { ...current, [field]: next };
  setHistory(merged);
  writeTextHistory(merged);
}

/** 仅测试用：回到「从存储里重新读一遍」的初始状态。 */
export function resetTextHistoryForTests(storage?: TextHistoryStorage): void {
  setHistory(readTextHistory(storage));
}
