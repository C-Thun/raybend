import assert from "node:assert/strict";
import test from "node:test";
import { displayColorCopy, displayColorMessages, displayDetectionKey } from "./display-color.ts";
import { zhCN } from "./zh-CN.ts";
import { enUS } from "./en-US.ts";

test("所有实际显示状态和诊断原因均有中英反馈",()=> {
  for(const mode of Object.keys(displayColorMessages) as (keyof typeof displayColorMessages)[]) {
    const copy=displayColorCopy(mode);
    assert.ok(zhCN[copy.title]);assert.ok(zhCN[copy.detail]);assert.ok(enUS[copy.title]);assert.ok(enUS[copy.detail]);
    assert.equal(copy.warning,["error","systemSrgb","fallback","preparationFailed","unavailable"].includes(mode));
  }
  assert.equal(displayColorCopy("preparationFailed").detail,"color.display.preparationFailedDetail");
});

test("系统检测的措辞独立于画布实际呈现",()=> {
  assert.equal(displayDetectionKey(undefined),"settings.display.unavailable");
  assert.equal(displayDetectionKey("icc"),"settings.display.icc");
  assert.equal(displayDetectionKey("systemManaged"),"settings.display.systemManaged");
  assert.equal(displayDetectionKey("srgbFallback"),"settings.display.fallback");
  assert.equal(displayDetectionKey("unavailable"),"settings.display.unavailable");
});
