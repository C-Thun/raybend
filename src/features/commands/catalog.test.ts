import assert from "node:assert/strict";
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
