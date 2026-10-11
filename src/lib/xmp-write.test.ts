/**
 * XMP 写出开关判定的单元测试（与 Rust `sidecar::write_enabled_value` 同一张值表）。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import { xmpWriteEnabled } from "./xmp-write.ts";

test("XMP 写出开关：缺失 / true / 读不懂都算开，只有明确的 false 才关", () => {
  // 保守的一侧：没设置过、读不到、值不合法 —— 都继续写。
  assert.equal(xmpWriteEnabled(null), true);
  assert.equal(xmpWriteEnabled("true"), true);
  assert.equal(xmpWriteEnabled("TRUE"), true);
  assert.equal(xmpWriteEnabled("  true  "), true);
  assert.equal(xmpWriteEnabled("1"), true);
  assert.equal(xmpWriteEnabled("garbage"), true);
  // 明确的关闭：大小写与空白无关。
  assert.equal(xmpWriteEnabled("false"), false);
  assert.equal(xmpWriteEnabled(" FALSE "), false);
});
