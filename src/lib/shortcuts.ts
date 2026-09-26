/**
 * 快捷键偏好（**设备级**）—— 覆盖表 + 最近使用（`specs/M2-W3.md` §2.3）。
 *
 * ## 为什么是 `localStorage` 而不是 `app.db`
 *
 * 键盘布局是**设备级**的（换台机器本来就该重来一次），而且菜单上的键位提示要在
 * **首次渲染前**就拿到 —— 异步读库会先按默认画一帧再跳变。与主题 / 密度 / 布局 /
 * 显示偏好同一条理由（`lib/display-prefs.ts` 的注释里写过同一段）。
 * 换机器靠**导入导出**（JSON 文件）。
 *
 * ## 形状
 *
 * ```jsonc
 * { "version": 1, "overrides": { "browse.flag.pick": "Ctrl+Alt+P", "view.tiles.info": null },
 *   "recent": ["view.filter.toggle", "edit.undo"] }
 * ```
 *
 * * `overrides[id] = null` = **显式解绑**（「我就是要这个功能没有键」）；
 * * `recent` 是命令面板「最近使用」的顺序（最新的在最前，去重、限长）。
 *
 * ## 纪律
 *
 * 坏值一律回落（`sanitizeShortcuts`），**读写都不抛错** —— 存储里的垃圾不能让界面起不来。
 */

import { createSignal, type Accessor } from "solid-js";

import { detectConflicts, type CommandSpec, type ShortcutOverrides } from "./commands.ts";
import { isReservedChord, parseChord } from "./key-chords.ts";

/** 存储键（`localStorage`） */
export const SHORTCUTS_STORAGE_KEY = "raybend.shortcuts.v1";

/** 导出文件的版本号（导入时严格比对，不猜） */
export const SHORTCUTS_FILE_VERSION = 1;

/** 最近使用的上限（命令面板空查询时最多显示这么多） */
export const RECENT_LIMIT = 8;

export interface ShortcutsState {
  overrides: Record<string, string | null>;
  recent: string[];
}

/** 存储的最小接口（便于测试注入；也兼容 localStorage 被禁用时的静默失败） */
export interface ShortcutsStorage {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
}

export const DEFAULT_SHORTCUTS: ShortcutsState = { overrides: {}, recent: [] };

function defaultStorage(): ShortcutsStorage | undefined {
  try {
    return globalThis.localStorage;
  } catch {
    // 隐私模式 / 禁用存储时访问 localStorage 会抛错，那时就当作没有存储
    return undefined;
  }
}

/** 解析存下来的值：任何非法输入都落到默认，**绝不抛错** */
export function sanitizeShortcuts(raw: unknown): ShortcutsState {
  if (typeof raw !== "object" || raw === null) return { overrides: {}, recent: [] };
  const record = raw as Record<string, unknown>;

  const overrides: Record<string, string | null> = {};
  const rawOverrides = record.overrides;
  if (typeof rawOverrides === "object" && rawOverrides !== null) {
    for (const [id, value] of Object.entries(rawOverrides as Record<string, unknown>)) {
      if (id === "") continue;
      if (value === null) {
        overrides[id] = null;
        continue;
      }
      if (typeof value !== "string") continue;
      const parsed = parseChord(value);
      if (!parsed.ok) continue; // 认不出的写法直接丢（`detectConflicts` 另有一路报给界面）
      if (isReservedChord(value)) continue;
      overrides[id] = parsed.text;
    }
  }

  const recent: string[] = [];
  if (Array.isArray(record.recent)) {
    for (const value of record.recent) {
      if (typeof value !== "string" || value === "") continue;
      if (recent.includes(value)) continue;
      recent.push(value);
      if (recent.length >= RECENT_LIMIT) break;
    }
  }

  return { overrides, recent };
}

export function readShortcuts(
  storage: ShortcutsStorage | undefined = defaultStorage(),
): ShortcutsState {
  if (!storage) return { overrides: {}, recent: [] };
  try {
    const raw = storage.getItem(SHORTCUTS_STORAGE_KEY);
    if (raw === null || raw === "") return { overrides: {}, recent: [] };
    return sanitizeShortcuts(JSON.parse(raw));
  } catch {
    return { overrides: {}, recent: [] };
  }
}

export function writeShortcuts(
  state: ShortcutsState,
  storage: ShortcutsStorage | undefined = defaultStorage(),
): void {
  if (!storage) return;
  try {
    storage.setItem(SHORTCUTS_STORAGE_KEY, JSON.stringify(state));
  } catch {
    // 存储不可用：静默（偏好丢了不影响用）
  }
}

/* ══════════════════════════════════════════════════════════════
 * 单例（与 `display-prefs` / `appearance` 同一套做法）
 * ══════════════════════════════════════════════════════════════ */

const [state, setState] = createSignal<ShortcutsState>(readShortcuts());

function patch(next: Partial<ShortcutsState>, persist: boolean): void {
  const current = state();
  const merged = sanitizeShortcuts({ ...current, ...next });
  setState(merged);
  if (persist) writeShortcuts(merged);
}

export const shortcuts: Accessor<ShortcutsState> = state;
export const shortcutOverrides: Accessor<ShortcutOverrides> = () => state().overrides;
export const recentCommands: Accessor<readonly string[]> = () => state().recent;

/** 改一条命令的键位（`null` = 解绑）；立刻落盘 */
export function setShortcutOverride(commandId: string, chord: string | null): void {
  patch({ overrides: { ...state().overrides, [commandId]: chord } }, true);
}

/** 恢复某条命令的默认键位（把覆盖项删掉） */
export function clearShortcutOverride(commandId: string): void {
  const next = { ...state().overrides };
  delete next[commandId];
  patch({ overrides: next }, true);
}

/** 全部恢复默认（覆盖表清空；最近使用保留 —— 那是使用习惯，不是配置） */
export function resetAllShortcuts(): void {
  patch({ overrides: {} }, true);
}

/** 记一次「用过这条命令」（命令面板的执行路径会调它） */
export function rememberCommand(commandId: string): void {
  if (commandId === "") return;
  const next = [commandId, ...state().recent.filter((id) => id !== commandId)].slice(
    0,
    RECENT_LIMIT,
  );
  patch({ recent: next }, true);
}

/** **仅测试用**：把单例状态复位 */
export function resetShortcutsForTests(next: ShortcutsState = DEFAULT_SHORTCUTS): void {
  setState(sanitizeShortcuts({ ...next }));
}

/* ══════════════════════════════════════════════════════════════
 * 导入 / 导出（JSON 文件，`specs/M2-W3.md` §4.3）
 * ══════════════════════════════════════════════════════════════ */

/** 导出成文件内容（只导出**改动**，不导出全部默认值 —— 文件小、diff 清楚） */
export function exportShortcuts(stateNow: ShortcutsState = state()): string {
  return `${JSON.stringify(
    { version: SHORTCUTS_FILE_VERSION, shortcuts: stateNow.overrides },
    null,
    2,
  )}\n`;
}

/**
 * 导入时的一条问题（**结构化**：文案归上层查语言包 —— `lib/` 不许 import i18n）。
 *
 * 渲染在 `features/commands/messages.ts`（`shortcuts.import.*` 那组键）。
 */
export type ImportProblem =
  | { kind: "notJson" }
  | { kind: "notObject" }
  | { kind: "version"; found: string; expected: number }
  | { kind: "missingShortcuts" }
  | { kind: "unknownCommand"; id: string }
  | { kind: "notString"; id: string }
  | { kind: "badChord"; id: string; value: string }
  | { kind: "reserved"; id: string; value: string }
  | { kind: "conflict"; chord: string; commandIds: readonly string[] };

export type ImportResult =
  | { ok: true; state: ShortcutsState }
  | { ok: false; problems: ImportProblem[] };

/**
 * 解析导入文件。
 *
 * **要么全过、要么不动**（`specs/M2-W3.md` §4.3）：任何一行有问题就整份拒绝，
 * 并把问题逐条列出来 —— 部分导入会让用户面对一个「一半新一半旧」的键位表，
 * 比导入失败更难收拾。
 *
 * 校验四件事：JSON 合法 / 版本匹配 / 命令 id 存在 / 键位合法且不冲突。
 */
export function parseShortcutsFile(
  text: string,
  commands: readonly CommandSpec[],
): ImportResult {
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    return { ok: false, problems: [{ kind: "notJson" }] };
  }
  if (typeof raw !== "object" || raw === null) {
    return { ok: false, problems: [{ kind: "notObject" }] };
  }
  const record = raw as Record<string, unknown>;
  if (record.version !== SHORTCUTS_FILE_VERSION) {
    return {
      ok: false,
      problems: [
        { kind: "version", found: String(record.version), expected: SHORTCUTS_FILE_VERSION },
      ],
    };
  }
  const shortcuts = record.shortcuts;
  if (typeof shortcuts !== "object" || shortcuts === null) {
    return { ok: false, problems: [{ kind: "missingShortcuts" }] };
  }

  const problems: ImportProblem[] = [];
  const overrides: Record<string, string | null> = {};
  const known = new Set(commands.map((command) => command.id));
  for (const [id, value] of Object.entries(shortcuts as Record<string, unknown>)) {
    if (!known.has(id)) {
      problems.push({ kind: "unknownCommand", id });
      continue;
    }
    if (value === null) {
      overrides[id] = null;
      continue;
    }
    if (typeof value !== "string") {
      problems.push({ kind: "notString", id });
      continue;
    }
    const parsed = parseChord(value);
    if (!parsed.ok) {
      problems.push({ kind: "badChord", id, value });
      continue;
    }
    if (isReservedChord(value)) {
      problems.push({ kind: "reserved", id, value });
      continue;
    }
    overrides[id] = parsed.text;
  }
  if (problems.length > 0) return { ok: false, problems };

  const conflicts = detectConflicts(commands, overrides).filter((issue) => issue.blocking);
  if (conflicts.length > 0) {
    return {
      ok: false,
      problems: conflicts.map((issue) => ({
        kind: "conflict" as const,
        chord: issue.chord,
        commandIds: issue.commandIds,
      })),
    };
  }

  return { ok: true, state: { overrides, recent: state().recent } };
}

/** 应用一份导入结果（落盘 + 立刻生效 —— 分发器读的就是这个信号） */
export function applyImportedShortcuts(imported: ShortcutsState): void {
  patch({ overrides: imported.overrides }, true);
}
