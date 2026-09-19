/**
 * 快捷键偏好（`lib/shortcuts.ts`）的测试。
 *
 * 四个重点：坏存储不炸、覆盖表语义（解绑）、最近使用去重限长、
 * 导入文件的**要么全过要么不动**（含冲突与未知 id）。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import type { CommandSpec } from "./commands.ts";
import {
  applyImportedShortcuts,
  clearShortcutOverride,
  exportShortcuts,
  parseShortcutsFile,
  readShortcuts,
  recentCommands,
  RECENT_LIMIT,
  rememberCommand,
  resetAllShortcuts,
  resetShortcutsForTests,
  sanitizeShortcuts,
  setShortcutOverride,
  shortcutOverrides,
  SHORTCUTS_FILE_VERSION,
  SHORTCUTS_STORAGE_KEY,
  writeShortcuts,
  type ShortcutsStorage,
} from "./shortcuts.ts";

function fakeStorage(seed: Record<string, string> = {}): ShortcutsStorage & { data: Map<string, string> } {
  const data = new Map(Object.entries(seed));
  return {
    data,
    getItem: (key) => data.get(key) ?? null,
    setItem: (key, value) => {
      data.set(key, value);
    },
  };
}

function cmd(id: string, patch: Partial<CommandSpec> = {}): CommandSpec {
  return { id, titleKey: `t.${id}`, group: "view", scope: "tiles", run: () => {}, ...patch };
}

const COMMANDS: CommandSpec[] = [
  cmd("view.filter", { defaultKey: "Mod+K" }),
  cmd("edit.undo", { defaultKey: "Mod+Z" }),
  cmd("browse.flag", { defaultKey: "P" }),
];

test("sanitizeShortcuts：垃圾输入一律回落，不抛错", () => {
  for (const raw of [null, undefined, 42, "x", []]) {
    assert.deepEqual(sanitizeShortcuts(raw), { overrides: {}, recent: [] });
  }
  // 认不出的键位 / 保留键 / 非字符串值：丢掉那一条，保留别的
  const state = sanitizeShortcuts({
    overrides: {
      a: "Mod+K",
      b: "Nonsense+Key",
      c: "Alt+F4",
      d: 42,
      e: null,
    },
    recent: ["x", "x", "", 7, "y"],
  });
  assert.deepEqual(state.overrides, { a: "Ctrl+K", e: null });
  assert.deepEqual(state.recent, ["x", "y"]);
});

test("readShortcuts：没有存储 / 坏 JSON / 空串都退回默认", () => {
  assert.deepEqual(readShortcuts(undefined), { overrides: {}, recent: [] });
  assert.deepEqual(readShortcuts(fakeStorage()), { overrides: {}, recent: [] });
  assert.deepEqual(readShortcuts(fakeStorage({ [SHORTCUTS_STORAGE_KEY]: "{oops" })), {
    overrides: {},
    recent: [],
  });
  assert.deepEqual(readShortcuts(fakeStorage({ [SHORTCUTS_STORAGE_KEY]: "" })), {
    overrides: {},
    recent: [],
  });
});

test("writeShortcuts / readShortcuts：来回一趟不丢字段", () => {
  const storage = fakeStorage();
  const state = { overrides: { "edit.undo": "Ctrl+Alt+Z" }, recent: ["edit.undo"] };
  writeShortcuts(state, storage);
  assert.deepEqual(readShortcuts(storage), state);
});

test("单例：改键立刻落盘、解绑保留在覆盖表里", () => {
  resetShortcutsForTests();
  setShortcutOverride("edit.undo", "Ctrl+Alt+Z");
  assert.equal(shortcutOverrides()["edit.undo"], "Ctrl+Alt+Z");
  setShortcutOverride("browse.flag", null);
  assert.equal(shortcutOverrides()["browse.flag"], null, "null = 显式解绑（不是删掉这一项）");
  clearShortcutOverride("edit.undo");
  assert.equal("edit.undo" in shortcutOverrides(), false, "恢复默认 = 删掉覆盖项");
  resetAllShortcuts();
  assert.deepEqual(shortcutOverrides(), {});
  resetShortcutsForTests();
});

test("最近使用：最新的在最前、去重、限长", () => {
  resetShortcutsForTests();
  rememberCommand("a");
  rememberCommand("b");
  rememberCommand("a");
  assert.deepEqual([...recentCommands()], ["a", "b"]);
  for (let i = 0; i < RECENT_LIMIT + 5; i += 1) rememberCommand(`c${i}`);
  assert.equal(recentCommands().length, RECENT_LIMIT);
  assert.equal(recentCommands()[0], `c${RECENT_LIMIT + 4}`);
  resetShortcutsForTests();
});

test("导出：只导出覆盖表 + 版本号", () => {
  const text = exportShortcuts({ overrides: { "edit.undo": "Ctrl+Alt+Z" }, recent: ["edit.undo"] });
  const parsed = JSON.parse(text) as { version: number; shortcuts: Record<string, string> };
  assert.equal(parsed.version, SHORTCUTS_FILE_VERSION);
  assert.deepEqual(parsed.shortcuts, { "edit.undo": "Ctrl+Alt+Z" });
  assert.equal(text.endsWith("\n"), true, "文件末尾留换行");
});

test("导入：合法文件全过", () => {
  const result = parseShortcutsFile(
    JSON.stringify({ version: 1, shortcuts: { "edit.undo": "Ctrl+Alt+Z", "browse.flag": null } }),
    COMMANDS,
  );
  assert.equal(result.ok, true);
  if (!result.ok) return;
  assert.deepEqual(result.state.overrides, { "edit.undo": "Ctrl+Alt+Z", "browse.flag": null });
});

test("导入：版本不匹配 / 不是 JSON / 缺字段都拒绝，并说清原因", () => {
  const bad = parseShortcutsFile("{oops", COMMANDS);
  assert.equal(bad.ok, false);
  assert.deepEqual(!bad.ok ? bad.problems : [], [{ kind: "notJson" }]);
  const wrongVersion = parseShortcutsFile(JSON.stringify({ version: 99, shortcuts: {} }), COMMANDS);
  assert.equal(wrongVersion.ok, false);
  assert.deepEqual(!wrongVersion.ok ? wrongVersion.problems : [], [
    { kind: "version", found: "99", expected: SHORTCUTS_FILE_VERSION },
  ]);
  const missing = parseShortcutsFile(JSON.stringify({ version: 1 }), COMMANDS);
  assert.equal(missing.ok, false);
  assert.deepEqual(!missing.ok ? missing.problems : [], [{ kind: "missingShortcuts" }]);
});

test("导入：未知 id / 坏键位 / 保留键 → 整份拒绝（要么全过、要么不动）", () => {
  const unknown = parseShortcutsFile(
    JSON.stringify({ version: 1, shortcuts: { "edit.undo": "Ctrl+Alt+Z", "no.such": "P" } }),
    COMMANDS,
  );
  assert.equal(unknown.ok, false);
  assert.deepEqual(!unknown.ok ? unknown.problems[0] : null, { kind: "unknownCommand", id: "no.such" });

  const badChord = parseShortcutsFile(
    JSON.stringify({ version: 1, shortcuts: { "edit.undo": "Nonsense+Key" } }),
    COMMANDS,
  );
  assert.equal(badChord.ok, false);

  const reserved = parseShortcutsFile(
    JSON.stringify({ version: 1, shortcuts: { "edit.undo": "Alt+F4" } }),
    COMMANDS,
  );
  assert.equal(reserved.ok, false);
  assert.deepEqual(!reserved.ok ? reserved.problems[0] : null, {
    kind: "reserved",
    id: "edit.undo",
    value: "Alt+F4",
  });
});

test("导入：与现有默认键冲突 → 整份拒绝并指出冲突", () => {
  // `view.filter` 默认就是 Mod+K；把 `edit.undo` 也绑成 Mod+K ⇒ 两条 grid 命令撞车
  const conflict = parseShortcutsFile(
    JSON.stringify({ version: 1, shortcuts: { "edit.undo": "Mod+K" } }),
    COMMANDS,
  );
  assert.equal(conflict.ok, false);
  const first = !conflict.ok ? conflict.problems[0] : null;
  assert.equal(first?.kind, "conflict");
  assert.equal(first?.kind === "conflict" ? first.chord : null, "Ctrl+K");
});

test("导入：作用域不同的同键（tiles vs viewer）允许导入", () => {
  const commands: CommandSpec[] = [
    cmd("grid.fit", { defaultKey: "0", scope: "tiles" }),
    cmd("viewer.fit", { defaultKey: "1", scope: "viewer" }),
  ];
  const result = parseShortcutsFile(
    JSON.stringify({ version: 1, shortcuts: { "grid.fit": "0", "viewer.fit": "0" } }),
    commands,
  );
  assert.equal(result.ok, true);
});

test("applyImportedShortcuts：落到单例并保留最近使用", () => {
  resetShortcutsForTests({ overrides: {}, recent: ["edit.undo"] });
  applyImportedShortcuts({ overrides: { "browse.flag": "Ctrl+B" }, recent: ["edit.undo"] });
  assert.equal(shortcutOverrides()["browse.flag"], "Ctrl+B");
  assert.deepEqual([...recentCommands()], ["edit.undo"]);
  resetShortcutsForTests();
});
