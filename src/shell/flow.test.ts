/**
 * 工作流模型与 toolsbar 装配表的单元测试。
 *
 * 这类「表驱动」的逻辑看着简单，但错一个格子就是「切到浏览后工具行没了」
 * 或者「批量排除在没选中时还能点」—— 两条都要靠测试钉住。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  DEFAULT_WORKFLOW,
  isWorkflow,
  isToolDisabled,
  normalizeWorkflow,
  toolsFor,
  WORKFLOW_LABEL_KEY,
  workflowIndex,
  WORKFLOWS,
} from "./flow.ts";

test("工作流顺序是固定的流水线：导入 → 浏览 → 编辑 → 导出", () => {
  assert.deepEqual([...WORKFLOWS], ["import", "browse", "edit", "export"]);
  assert.equal(DEFAULT_WORKFLOW, "import");
});

test("每个工作流都有文案 key，且与 i18n 的命名一致", () => {
  assert.deepEqual(WORKFLOW_LABEL_KEY, {
    import: "flow.import",
    browse: "flow.browse",
    edit: "flow.edit",
    export: "flow.export",
  });
});

test("isWorkflow：只认四个合法值", () => {
  for (const id of WORKFLOWS) assert.equal(isWorkflow(id), true);
  for (const bad of [
    "IMPORT",
    "",
    "import ",
    null,
    undefined,
    0,
    {},
    ["import"],
  ]) {
    assert.equal(isWorkflow(bad), false, `${String(bad)} 不该被当成工作流`);
  }
});

test("normalizeWorkflow：垃圾值回落到默认，合法值原样保留", () => {
  assert.equal(normalizeWorkflow("edit"), "edit");
  assert.equal(normalizeWorkflow("bogus"), DEFAULT_WORKFLOW);
  assert.equal(normalizeWorkflow(null), DEFAULT_WORKFLOW);
  assert.equal(normalizeWorkflow(undefined), DEFAULT_WORKFLOW);
});

test("workflowIndex：位置从 0 起，非法值 -1", () => {
  assert.equal(workflowIndex("import"), 0);
  assert.equal(workflowIndex("export"), 3);
  assert.equal(workflowIndex("nope"), -1);
  assert.equal(workflowIndex(null), -1);
});

test("toolsbar：只有「导入」有内容，其余三个工作流为空（整行隐藏）", () => {
  assert.deepEqual(
    toolsFor("import").map((tool) => tool.id),
    ["batch-exclude"],
  );
  for (const workflow of ["browse", "edit", "export"] as const) {
    assert.deepEqual(toolsFor(workflow), [], `${workflow} 下工具行应当是空的`);
  }
});

test("toolsbar：非法工作流按默认处理，而不是抛错或返回 undefined", () => {
  assert.deepEqual(
    toolsFor("bogus" as never).map((tool) => tool.id),
    ["batch-exclude"],
  );
});

test("工具规格：批量排除的文案 key 与「空选择即禁用」", () => {
  const [tool] = toolsFor("import");
  assert.equal(tool.labelKey, "tools.batch_exclude");
  assert.equal(tool.disabledWhenEmpty, true);
});

test("isToolDisabled：没有选中项时禁用，有选中项时可点", () => {
  const [tool] = toolsFor("import");
  assert.equal(isToolDisabled(tool, false), true);
  assert.equal(isToolDisabled(tool, true), false);
});

test("isToolDisabled：不声明该规则的工具永远可点", () => {
  const tool = { id: "x", labelKey: "flow.import" } as never;
  assert.equal(isToolDisabled(tool, false), false);
  assert.equal(isToolDisabled(tool, true), false);
});
