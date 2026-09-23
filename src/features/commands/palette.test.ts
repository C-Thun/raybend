/**
 * 命令面板行模型的测试（`features/commands/palette.ts`）。
 *
 * 钉住两条 2026-09-23 人类报过的口径：
 *   1. **不可用的命令照样列出**（灰掉不消失）——「找不到命令」是最坏的表现；
 *   2. **键位可搜**（搜 `F11` 能直接找到绑了它的命令）。
 */

import assert from "node:assert/strict";
import test from "node:test";

import type { CommandSpec } from "../../lib/commands.ts";
import { buildPaletteRows } from "./palette.ts";

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

/** 假装配：标题带 id（便于断言），只有全屏那条绑了 F11 */
function sources(commands: readonly CommandSpec[], query = "", recent: string[] = []) {
  return {
    commands,
    query,
    recent,
    titleOf: (command: CommandSpec) => `标题:${command.id}`,
    groupOf: () => "视图",
    chordOf: (command: CommandSpec) =>
      command.id === "viewer.fullscreen" ? "F11" : null,
    rawChordOf: (command: CommandSpec) =>
      command.id === "viewer.fullscreen" ? "F11" : null,
  };
}

test("不可用的命令**照样列出**（灰掉不消失 —— 找不到命令是最坏的表现）", () => {
  const rows = buildPaletteRows(
    sources([cmd("viewer.fullscreen", { when: () => false }), cmd("edit.undo")]),
  );
  assert.equal(rows.length, 2, "when 为假也要在列表里");
  const fullscreen = rows.find((row) => row.command.id === "viewer.fullscreen");
  assert.ok(fullscreen !== undefined);
  assert.deepEqual(fullscreen.availability, { available: false, reason: "when" });
  assert.equal(
    rows.find((row) => row.command.id === "edit.undo")?.availability.available,
    true,
  );
});

test("搜键位能直接找到命令（F11）", () => {
  const rows = buildPaletteRows(
    sources([cmd("viewer.fullscreen"), cmd("edit.undo")], "F11"),
  );
  assert.deepEqual(
    rows.map((row) => row.command.id),
    ["viewer.fullscreen"],
  );
});

test("行上带着当前键位（显示用），没绑是 null", () => {
  const rows = buildPaletteRows(sources([cmd("viewer.fullscreen"), cmd("edit.undo")]));
  assert.equal(rows.find((row) => row.command.id === "viewer.fullscreen")?.chord, "F11");
  assert.equal(rows.find((row) => row.command.id === "edit.undo")?.chord, null);
});

test("空查询把最近用过的排最前", () => {
  const rows = buildPaletteRows(sources([cmd("a"), cmd("b"), cmd("c")], "", ["b"]));
  assert.equal(rows[0]?.command.id, "b");
});
