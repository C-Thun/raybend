/**
 * 场景化冲突判定的测试（`specs/command-scenes.md` §5）。
 *
 * 钉三件事：跨场景同键合法、同场景同键仍拦、场景相交但作用域不相交也放行。
 * 另外对**真实注册表**做一次体检：有默认键位的命令必须标了场景；
 * 浏览删除与导出移出队列这对 `Delete` 必须并存。
 */

import assert from "node:assert/strict";
import test from "node:test";

import { detectConflicts, type CommandSpec } from "../../lib/commands.ts";
import { createCommandRegistry, type CommandDeps } from "./catalog.ts";

function cmd(id: string, patch: Partial<CommandSpec> = {}): CommandSpec {
  return { id, titleKey: `test.${id}`, group: "view", scope: "tiles", run: () => {}, ...patch };
}

test("场景：同一键位在不同场景各自生效（不算冲突，只是共享提示）", () => {
  const commands = [
    cmd("browse.delete", { scenes: ["browse"], defaultKey: "Delete" }),
    cmd("export.remove", { scenes: ["export"], defaultKey: "Delete" }),
  ];
  const issues = detectConflicts(commands, {});
  assert.deepEqual(issues.filter((issue) => issue.blocking), []);
  assert.equal(issues.find((issue) => issue.kind === "shared")?.commandIds.length, 2);
});

test("场景：同场景 + 同作用域仍拦（对照组）", () => {
  const commands = [
    cmd("a.one", { scenes: ["browse"], defaultKey: "Delete" }),
    cmd("b.two", { scenes: ["browse"], defaultKey: "Delete" }),
  ];
  assert.ok(
    detectConflicts(commands, {}).some((issue) => issue.blocking && issue.kind === "duplicate"),
  );
});

test("场景：场景相交但作用域不相交 = 放行（tiles 打星 vs 看图缩放）", () => {
  const commands = [
    cmd("tiles.zero", { scope: "tiles", scenes: ["browse"], defaultKey: "0" }),
    cmd("viewer.zero", { scope: "viewer", scenes: ["browse"], defaultKey: "0" }),
  ];
  assert.deepEqual(detectConflicts(commands, {}).filter((issue) => issue.blocking), []);
});

test("注册表：有默认键位的命令都标了场景；浏览删除与导出移出队列跨场景共存", () => {
  const deps = { flow: () => "browse" } as unknown as CommandDeps;
  const commands = createCommandRegistry(deps);
  for (const command of commands) {
    if (command.defaultKey !== undefined) {
      assert.ok((command.scenes ?? []).length > 0, `${command.id} 有默认键位却没标场景`);
    }
  }
  const del = commands.find((command) => command.id === "browse.delete");
  const remove = commands.find((command) => command.id === "export.remove");
  assert.deepEqual(del?.scenes, ["browse"]);
  assert.deepEqual(remove?.scenes, ["export"]);
  assert.equal(del?.defaultKey, "Delete");
  assert.equal(remove?.defaultKey, "Delete");
  // 这一对 `Delete` 在注册表层面零阻断冲突（其余的阻断冲突不属本测试范围）
  const blocking = detectConflicts(commands, {}).filter((issue) => issue.blocking);
  assert.equal(
    blocking.some(
      (issue) =>
        issue.commandIds.includes("browse.delete") || issue.commandIds.includes("export.remove"),
    ),
    false,
  );
});
