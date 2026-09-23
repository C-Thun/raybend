/**
 * 键位串解析 / 格式化 / 匹配的测试（`lib/key-chords.ts`）。
 *
 * 覆盖的都是**真会咬人**的地方：平台修饰键、`+`/`=` 这个键本身、字母大小写、
 * 命名键别名、Shift 造成的 `event.key` 变形、保留键、以及「只按修饰键」这类半成品输入。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  chordFromEvent,
  chordMatches,
  detectPlatform,
  eventKey,
  formatChord,
  isReservedChord,
  keyLabel,
  normalizeKey,
  parseChord,
  sameChord,
} from "./key-chords.ts";

test("normalizeKey：字母小写、数字原样、`+` 与 `=` 归一", () => {
  assert.equal(normalizeKey("K"), "k");
  assert.equal(normalizeKey("k"), "k");
  assert.equal(normalizeKey("5"), "5");
  assert.equal(normalizeKey("+"), "=");
  assert.equal(normalizeKey("="), "=");
  assert.equal(normalizeKey("F12"), "f12");
  assert.equal(normalizeKey(","), ",");
  assert.equal(normalizeKey(""), null);
  assert.equal(normalizeKey("nonsense"), null);
});

test("normalizeKey：命名键别名一套（Esc/Del/Return/方向键/空格）", () => {
  assert.equal(normalizeKey("Esc"), "escape");
  assert.equal(normalizeKey("escape"), "escape");
  assert.equal(normalizeKey("Del"), "delete");
  assert.equal(normalizeKey("Return"), "enter");
  assert.equal(normalizeKey("Space"), "space");
  assert.equal(normalizeKey("↑"), "arrowup");
  assert.equal(normalizeKey("ArrowLeft"), "arrowleft");
});

test("parseChord：Mod / 显式修饰键 / 单键", () => {
  const mod = parseChord("Mod+K");
  assert.equal(mod.ok, true);
  assert.deepEqual(mod.ok && mod.chord, { key: "k", mod: true, ctrl: false, meta: false, shift: false, alt: false });

  const combo = parseChord("Ctrl+Shift+Delete");
  assert.equal(combo.ok, true);
  assert.deepEqual(combo.ok && combo.chord, { key: "delete", mod: false, ctrl: true, meta: false, shift: true, alt: false });

  const single = parseChord("p");
  assert.equal(single.ok, true);
  assert.deepEqual(single.ok && single.chord, { key: "p", mod: false, ctrl: false, meta: false, shift: false, alt: false });
});

test("parseChord：`+` 这个键本身（`Mod++` 与 `Mod+=`）", () => {
  const plus = parseChord("Mod++");
  assert.equal(plus.ok, true);
  assert.equal(plus.ok && plus.chord.key, "=");
  assert.equal(plus.ok && plus.chord.mod, true);
  const equals = parseChord("Mod+=");
  assert.equal(equals.ok, true);
  assert.equal(equals.ok && equals.chord.key, "=");
  assert.ok(plus.ok && equals.ok && sameChord(plus.chord, equals.chord), "两种写法是同一条键");
});

test("parseChord：非法输入带原因（空 / 只有修饰键 / 认不出的键）", () => {
  assert.deepEqual(parseChord(""), { ok: false, reason: "empty", detail: "" });
  const modOnly = parseChord("Ctrl+Shift");
  assert.equal(modOnly.ok, false);
  assert.equal(!modOnly.ok && modOnly.reason, "modifier-only");
  const unknown = parseChord("Ctrl+Foo");
  assert.equal(unknown.ok, false);
  assert.equal(!unknown.ok && unknown.reason, "unknown-key");
  // 两个键（`Ctrl+K+P`）不猜：当成认不出来
  const twoKeys = parseChord("Ctrl+K+P");
  assert.equal(twoKeys.ok, false);
});

test("formatChord：Windows 写 Ctrl、macOS 写 ⌘（顺序固定）", () => {
  const parsed = parseChord("Mod+Shift+K");
  assert.equal(parsed.ok, true);
  if (!parsed.ok) return;
  assert.equal(formatChord(parsed.chord, "win"), "Ctrl+Shift+K");
  assert.equal(formatChord(parsed.chord, "mac"), "⌘+Shift+K");
  // 显式 Ctrl 在 mac 上不变成 ⌘
  const ctrl = parseChord("Ctrl+K");
  assert.equal(ctrl.ok && formatChord(ctrl.chord, "mac"), "Ctrl+K");
});

test("keyLabel：显示用名字（`=` 显示成 `+`、方向键用箭头）", () => {
  assert.equal(keyLabel("="), "+");
  assert.equal(keyLabel("escape"), "Esc");
  assert.equal(keyLabel("delete"), "Del");
  assert.equal(keyLabel("arrowleft"), "←");
  assert.equal(keyLabel("k"), "K");
});

test("chordMatches：修饰键必须精确匹配（多按了 Shift 不算）", () => {
  const chord = parseChord("Mod+K");
  assert.equal(chord.ok, true);
  if (!chord.ok) return;
  assert.equal(chordMatches(chord.chord, { key: "k", ctrlKey: true }, "win"), true);
  assert.equal(chordMatches(chord.chord, { key: "K", ctrlKey: true }, "win"), true, "大小写不敏感");
  assert.equal(chordMatches(chord.chord, { key: "k", ctrlKey: true, shiftKey: true }, "win"), false);
  assert.equal(chordMatches(chord.chord, { key: "k" }, "win"), false);
  assert.equal(chordMatches(chord.chord, { key: "j", ctrlKey: true }, "win"), false);
  // macOS：Mod = Cmd
  assert.equal(chordMatches(chord.chord, { key: "k", metaKey: true }, "mac"), true);
  assert.equal(chordMatches(chord.chord, { key: "k", ctrlKey: true }, "mac"), false);
});

test("chordMatches：`=` 键忽略 Shift（打出 + 必须按 Shift，用户不这么想）", () => {
  const chord = parseChord("Mod+=");
  assert.equal(chord.ok, true);
  if (!chord.ok) return;
  assert.equal(chordMatches(chord.chord, { key: "=", ctrlKey: true }, "win"), true);
  assert.equal(chordMatches(chord.chord, { key: "+", ctrlKey: true, shiftKey: true }, "win"), true);
  assert.equal(chordMatches(chord.chord, { key: "=" }, "win"), false, "Ctrl 还是得按");
});

test("chordFromEvent：捕获出一致的键位串；只按修饰键时不产出", () => {
  assert.equal(chordFromEvent({ key: "k", ctrlKey: true }, "win"), "Ctrl+K");
  assert.equal(chordFromEvent({ key: "P" }, "win"), "P");
  assert.equal(chordFromEvent({ key: "Delete", shiftKey: true }, "win"), "Shift+Del");
  assert.equal(chordFromEvent({ key: "+", ctrlKey: true, shiftKey: true }, "win"), "Ctrl++");
  assert.equal(chordFromEvent({ key: "Shift" }, "win"), null);
  assert.equal(chordFromEvent({ key: "Control" }, "win"), null);
});

test("保留键：Alt+F4 / Ctrl+Alt+Del / Ctrl+Shift+Esc 绑不了", () => {
  assert.equal(isReservedChord("Alt+F4"), true);
  assert.equal(isReservedChord("alt+f4"), true);
  assert.equal(isReservedChord("Ctrl+Alt+Delete"), true);
  assert.equal(isReservedChord("Ctrl+Shift+Esc"), true);
  assert.equal(isReservedChord("Ctrl+K"), false);
  assert.equal(isReservedChord("Mod+K"), false);
});

test("detectPlatform：Node 里没有 navigator 也不炸", () => {
  // Node 的 navigator 存在但没有 mac 字样 → win；关键是不抛错
  assert.ok(detectPlatform() === "win" || detectPlatform() === "mac");
});

test("eventKey：从事件取规范化键名", () => {
  assert.equal(eventKey({ key: "A" }), "a");
  assert.equal(eventKey({ key: "+" }), "=");
  assert.equal(eventKey({ key: "Nonsense" }), null);
});

test("keyLabel：F 键显示成大写（F11，不是 f11）", () => {
  // 人类 2026-09-23 定：全屏用 F11 —— 快捷键面板与命令面板都显示它，
  // 而 normalizeKey 存的是小写 `f11`，所以这里必须有一步大写。
  assert.equal(keyLabel("f11"), "F11");
  assert.equal(keyLabel("f1"), "F1");
  assert.equal(keyLabel("f12"), "F12");
  assert.equal(keyLabel("tab"), "Tab", "表里有的走表");
  assert.equal(keyLabel("p"), "P", "单字符键照旧大写");
});
