import assert from "node:assert/strict";
import test from "node:test";
import { enUS } from "./en-US.ts";
import { zhCN } from "./zh-CN.ts";
import { locationErrorKey, repositoryErrorKey } from "./repository-feedback.ts";

test("结构化位置错误走双语文案，不展示对象字符串或原始诊断", () => {
  for (const code of ["not_found", "identity_mismatch", "access_denied", "read_only", "catalog_invalid", "io_failure", "storage_full", "connection_lost", "schema_too_new", "migration_failed", "timeout", "unsupported_location", "busy", "invalid_path", "active_location"]) {
    const key = locationErrorKey({ code }); assert.equal(key, repositoryErrorKey(code)); assert.ok(enUS[key]); assert.ok(zhCN[key]);
  }
  for (const error of [null, "SQL ERROR", new Error("raw diagnostic"), { code: "__proto__" }, { code: 1 }])
    assert.equal(locationErrorKey(error), "repo.location_error.io_failure");
});
