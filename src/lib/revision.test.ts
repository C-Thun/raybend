import assert from "node:assert/strict";
import test from "node:test";
import { revision } from "./revision.ts";
test("原生大整数精确排序，零与前导零允许，非法文本不冒充新状态", () => {
  assert.equal(revision("0"), 0n);
  assert.equal(revision("001"), 1n);
  assert.equal(revision("18446744073709551615"), 18446744073709551615n);
  assert(revision("9007199254740993") > revision("9007199254740992"));
  for (const value of ["", "-1", "1.0", " 1", "1e3", "１２", "中文"]) assert.equal(revision(value), -1n);
});
