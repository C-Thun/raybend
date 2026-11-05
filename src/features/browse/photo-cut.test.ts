import assert from "node:assert/strict";
import test from "node:test";

import { flowHint, flowHintDefault, setFlowHintsEnabled } from "../../lib/flow-hint.ts";
import { beginPhotoCut, clearPhotoCut, cutIdsOf, photoCut } from "./photo-cut.ts";

test("剪切态：开始 / 取消，并写进 flowbar 默认槽", () => {
  clearPhotoCut();
  assert.equal(photoCut(), null);
  assert.equal(flowHintDefault(), "");

  beginPhotoCut("lib-a", ["1", "2"]);
  assert.deepEqual(photoCut(), { repositoryId: "lib-a", ids: ["1", "2"] });
  assert.deepEqual([...cutIdsOf("lib-a")].sort(), ["1", "2"]);
  assert.equal(cutIdsOf("lib-b").size, 0, "别的库看不到这份剪切");
  assert.equal(flowHint(), flowHintDefault());
  assert.ok(flowHintDefault().includes("2"), "默认槽要带上张数");

  clearPhotoCut();
  assert.equal(photoCut(), null);
  assert.equal(flowHintDefault(), "", "取消后默认槽回到空串");
});

test("剪切态：空集合 = 取消；幂等清理", () => {
  beginPhotoCut("lib-a", ["1"]);
  beginPhotoCut("lib-a", []);
  assert.equal(photoCut(), null);
  assert.equal(flowHintDefault(), "");
  clearPhotoCut(); // 不该炸
  assert.equal(flowHint(), null);
});

test("剪切状态不受「提示区开关」影响（那是状态，不是指向提示）", () => {
  setFlowHintsEnabled(false);
  beginPhotoCut("lib-a", ["7"]);
  assert.ok(flowHint() !== null, "关掉提示也要看得见剪切状态");
  clearPhotoCut();
  setFlowHintsEnabled(true);
});
