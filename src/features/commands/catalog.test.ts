import assert from "node:assert/strict";
import { detectConflicts } from "../../lib/commands.ts";
import test from "node:test";

import {
  createCommandRegistry,
  type CommandDeps,
  type CommandFlow,
} from "./catalog.ts";

test("compare Enter：按当前 flow 切各自的胶片带范围", () => {
  let flow: CommandFlow = "browse";
  const calls: string[] = [];
  const deps = {
    flow: () => flow,
    viewer: { comparing: () => true },
    browse: { toggleCompareStrip: () => calls.push("browse") },
    import: { toggleCompareStrip: () => calls.push("import") },
  } as unknown as CommandDeps;

  const command = createCommandRegistry(deps).find(
    (candidate) => candidate.id === "viewer.compareOnly",
  );
  assert.ok(command, "compare 胶片带切换必须登记进命令注册表");
  assert.equal(command.defaultKey, "Enter");
  assert.equal(command.when?.(), true);

  command.run();
  flow = "import";
  command.run();
  assert.deepEqual(calls, ["browse", "import"]);
});

test("compare Enter：非对比态不适用", () => {
  const deps = {
    flow: () => "browse" as const,
    viewer: { comparing: () => false },
  } as unknown as CommandDeps;
  const command = createCommandRegistry(deps).find(
    (candidate) => candidate.id === "viewer.compareOnly",
  );
  assert.equal(command?.when?.(), false);
});

test("SOOC/RAW 编辑源命令共用工具栏的选择动作且没有误触热键", () => {
  const calls: string[] = [];
  const deps = {
    editor: {
      active: () => true,
      hasPhoto: () => true,
      setBase: (base: string) => calls.push(base),
    },
  } as unknown as CommandDeps;
  const commands = createCommandRegistry(deps);
  const sooc = commands.find((candidate) => candidate.id === "editor.base.sooc");
  const raw = commands.find((candidate) => candidate.id === "editor.base.raw");
  assert.ok(sooc);
  assert.ok(raw);
  assert.equal(sooc.defaultKey, undefined);
  assert.equal(raw.defaultKey, undefined);
  assert.equal(sooc.when?.(), true);
  sooc.run();
  raw.run();
  assert.deepEqual(calls, ["sooc", "raw"]);
});

test("W5 三工具默认键位在命令注册表内且不冲突", () => {
  const deps = {
    editor: { active: () => true, hasPhoto: () => true, toggleTool: () => {} },
  } as unknown as CommandDeps;
  const commands = createCommandRegistry(deps);
  const expected = { "editor.tool.crop": "C", "editor.tool.rotate": "R", "editor.tool.compare": "B" };
  for (const [id, chord] of Object.entries(expected)) {
    assert.equal(commands.find((command) => command.id === id)?.defaultKey, chord);
  }
  const conflicts = detectConflicts(commands, {});
  assert.deepEqual(conflicts.filter((issue) => issue.blocking &&
    issue.commandIds.some((id) => id in expected)), []);
});

test('自动调整命令复用编辑动作且默认不占热键', () => {
  let applied = 0;
  const deps = {editor:{active:()=>true, hasPhoto:()=>true, autoAdjust:()=>{applied++;}}} as unknown as CommandDeps;
  const command = createCommandRegistry(deps).find(item=>item.id==='editor.develop.autoAdjust');
  assert.ok(command);
  assert.equal(command.defaultKey, undefined);
  assert.equal(command.menu, 'edit');
  command.run();
  assert.equal(applied, 1);
});
