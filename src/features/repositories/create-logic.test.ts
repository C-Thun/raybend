/**
 * 建库弹窗判定逻辑的测试。
 *
 * 三种结局（新建 / 登记已有 / 拒绝）都要钉住，另外两条容易做错的：
 *   - 还在探测时**不能**把按钮禁用（否则像卡住了）；
 *   - 改了路径后，旧的探测结果必须作废。
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import type { RepositoryProbe } from "../../api/types.ts";
import { evaluateCreate, probeMatches } from "./create-logic.ts";

function probe(overrides: Partial<RepositoryProbe> = {}): RepositoryProbe {
  return {
    kind: "empty",
    name: null,
    repositoryId: null,
    registered: false,
    message: null,
    ...overrides,
  };
}

test("路径为空：不能提交，也不显示提示行", () => {
  const gate = evaluateCreate({ path: "   ", probe: null });
  assert.equal(gate.canSubmit, false);
  assert.equal(gate.hint, "none");
});

test("还在探测（probe 为 null）：按钮可点，先不提示", () => {
  const gate = evaluateCreate({ path: "D:\\Photos\\New", probe: null });
  assert.equal(gate.canSubmit, true, "Rust 侧建库前会再探一次，不必在这里拦住用户");
  assert.equal(gate.hint, "none");
});

test("空目录：直接新建，没有提示行", () => {
  const gate = evaluateCreate({
    path: "D:\\Photos\\New",
    probe: probe({ kind: "empty" }),
  });
  assert.equal(gate.canSubmit, true);
  assert.equal(gate.hint, "none");
});

test("已有库（没登记过）：提示「将登记为已有库」，按钮可点", () => {
  const gate = evaluateCreate({
    path: "/libs/studio",
    probe: probe({ kind: "existing", name: "Kowloon Studio", repositoryId: "abc" }),
  });
  assert.equal(gate.canSubmit, true);
  assert.equal(gate.hint, "existing");
  assert.equal(gate.existingName, "Kowloon Studio");
});

test("已有库（已登记过）：提示换成「登记一条新路径」", () => {
  const gate = evaluateCreate({
    path: "/libs/studio",
    probe: probe({
      kind: "existing",
      name: "Kowloon Studio",
      repositoryId: "abc",
      registered: true,
    }),
  });
  assert.equal(gate.hint, "existingRegistered");
  assert.equal(gate.canSubmit, true);
});

test("catalog.db 读不出来：拒绝提交，并把原因带出来", () => {
  const gate = evaluateCreate({
    path: "/libs/broken",
    probe: probe({ kind: "broken", message: "不是 raybend 的库" }),
  });
  assert.equal(gate.canSubmit, false, "绝不覆盖别人可能还在用的库文件");
  assert.equal(gate.hint, "broken");
  assert.equal(gate.brokenMessage, "不是 raybend 的库");
});

test("路径不是目录（例如指向一个文件）：拒绝提交", () => {
  const gate = evaluateCreate({
    path: "/tmp/note.txt",
    probe: probe({ kind: "notDirectory" }),
  });
  assert.equal(gate.canSubmit, false);
  assert.equal(gate.hint, "notDirectory");
});

test("probeMatches：换路径后旧结果作废；空路径永远不算匹配", () => {
  assert.equal(probeMatches("/a", "/a"), true);
  assert.equal(probeMatches("  /a  ", "/a"), true, "首尾空格不影响");
  assert.equal(probeMatches("/a", "/b"), false);
  assert.equal(probeMatches(null, "/a"), false, "还没探过");
  assert.equal(probeMatches("/a", "   "), false, "空路径要重新探");
});
