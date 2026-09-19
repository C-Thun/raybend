/**
 * 命令与快捷键纯逻辑的测试（`lib/commands.ts`）。
 *
 * 重点是**冲突判据**：相交作用域的同键必须拦、不相交的必须放行、
 * 保留键必须拦、危险单键只警告 —— 这几条决定了「改键」到底能不能改坏。
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  blockingIssues,
  chordOf,
  detectConflicts,
  effectiveBindings,
  parsedChordOf,
  scopeOverlaps,
  type CommandSpec,
} from "./commands.ts";

/** 造一条命令（只填测试关心的字段） */
function cmd(id: string, patch: Partial<CommandSpec> = {}): CommandSpec {
  return {
    id,
    titleKey: `test.${id}`,
    group: "view",
    scope: "tiles",
    run: () => {},
    ...patch,
  };
}

test("scopeOverlaps：global 与谁都相交，tiles / viewer 互不相交", () => {
  assert.equal(scopeOverlaps("global", "tiles"), true);
  assert.equal(scopeOverlaps("tiles", "global"), true);
  assert.equal(scopeOverlaps("global", "viewer"), true);
  assert.equal(scopeOverlaps("tiles", "tiles"), true);
  assert.equal(scopeOverlaps("tiles", "viewer"), false);
  assert.equal(scopeOverlaps("viewer", "tiles"), false);
  assert.equal(scopeOverlaps("viewer", "viewer"), true);
});

test("chordOf：默认键、覆盖、显式解绑、坏写法", () => {
  const a = cmd("a", { defaultKey: "Mod+K" });
  assert.equal(chordOf(a, {}), "Ctrl+K");
  assert.equal(chordOf(a, { a: "Ctrl+Alt+P" }), "Ctrl+Alt+P");
  assert.equal(chordOf(a, { a: null }), null, "null = 显式解绑");
  assert.equal(chordOf(a, { a: "Nonsense+Key" }), null, "认不出的写法当没绑");
  const b = cmd("b");
  assert.equal(chordOf(b, {}), null, "没有默认键就是没绑");
});

test("parsedChordOf：能拿到结构化键位（供分发器匹配）", () => {
  const a = cmd("a", { defaultKey: "Mod+K" });
  const chord = parsedChordOf(a, {});
  assert.ok(chord !== null);
  assert.equal(chord?.key, "k");
  assert.equal(chord?.mod, true);
});

test("effectiveBindings：只收绑了键的命令", () => {
  const commands = [cmd("a", { defaultKey: "Mod+K" }), cmd("b"), cmd("c", { defaultKey: "P" })];
  const bindings = effectiveBindings(commands, { b: "Ctrl+B" });
  assert.equal(bindings.get("a"), "Ctrl+K");
  assert.equal(bindings.get("b"), "Ctrl+B");
  assert.equal(bindings.get("c"), "P");
  assert.equal(bindings.size, 3);
});

test("冲突：同一作用域的同一条键 = 拦（duplicate）", () => {
  const commands = [
    cmd("a", { defaultKey: "Mod+K", scope: "global" }),
    cmd("b", { defaultKey: "Mod+K", scope: "tiles" }),
  ];
  const issues = detectConflicts(commands, {});
  const dup = issues.find((issue) => issue.kind === "duplicate");
  assert.ok(dup, `应当报 duplicate：${JSON.stringify(issues)}`);
  assert.equal(dup?.blocking, true);
  assert.deepEqual([...(dup?.commandIds ?? [])].sort(), ["a", "b"]);
  assert.equal(blockingIssues(issues).length, 1);
});

test("冲突：作用域不相交的同一条键 = 只提示（shared，放行）", () => {
  const commands = [
    cmd("star.0", { defaultKey: "0", scope: "tiles" }),
    cmd("viewer.fit", { defaultKey: "0", scope: "viewer" }),
  ];
  const issues = detectConflicts(commands, {});
  assert.equal(issues.length, 1);
  assert.equal(issues[0].kind, "shared");
  assert.equal(issues[0].blocking, false);
  assert.equal(blockingIssues(issues).length, 0, "现状（0/1 两种含义）不该被拦");
});

test("冲突：三条命令两条相交 → 报一条 duplicate", () => {
  const commands = [
    cmd("a", { defaultKey: "P", scope: "tiles" }),
    cmd("b", { defaultKey: "P", scope: "tiles" }),
    cmd("c", { defaultKey: "P", scope: "viewer" }),
  ];
  const issues = detectConflicts(commands, {});
  assert.equal(issues.length, 1);
  assert.equal(issues[0].kind, "duplicate");
  assert.equal(issues[0].commandIds.length, 3);
});

test("冲突：系统保留键直接拦（reserved）", () => {
  const commands = [cmd("quit", { defaultKey: "Alt+F4" })];
  const issues = detectConflicts(commands, {});
  assert.equal(issues.length, 1);
  assert.equal(issues[0].kind, "reserved");
  assert.equal(issues[0].blocking, true);
});

test("冲突：破坏性命令绑危险单键只警告（risky，不拦）", () => {
  const commands = [
    cmd("edit.delete", { defaultKey: "Escape", dangerous: true }),
    cmd("edit.exit", { defaultKey: "Space", dangerous: true }),
    cmd("view.close", { defaultKey: "Escape", scope: "viewer" }),
  ];
  const issues = detectConflicts(commands, {});
  const risky = issues.filter((issue) => issue.kind === "risky");
  assert.equal(risky.length, 2, `两条危险命令各一条警告：${JSON.stringify(issues)}`);
  assert.equal(blockingIssues(issues).length, 0);
  assert.equal(
    issues.some((issue) => issue.kind === "duplicate"),
    false,
    "Escape 分别属于 grid / viewer（不相交）—— 不是冲突，只该是 shared",
  );
  assert.equal(issues.some((issue) => issue.kind === "shared"), true);
});

test("冲突：覆盖表里的坏写法报 invalid（可拦），且不参与分组", () => {
  const commands = [cmd("a", { defaultKey: "Mod+K" })];
  const issues = detectConflicts(commands, { a: "Nonsense+Key" });
  assert.equal(issues.length, 1);
  assert.equal(issues[0].kind, "invalid");
  assert.equal(issues[0].blocking, true);
});

test("冲突：解绑（null）不产生任何报告", () => {
  const commands = [cmd("a", { defaultKey: "Mod+K" }), cmd("b", { defaultKey: "Mod+K" })];
  const issues = detectConflicts(commands, { a: null, b: null });
  assert.deepEqual(issues, []);
});
