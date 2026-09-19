import assert from "node:assert/strict";
import { test } from "node:test";

import { rebuildProgressMessage } from "./rebuild-progress.ts";

test("扫描进度保留 0、批次边界与大整数", () => {
  for (const done of [0, 1, 200, Number.MAX_SAFE_INTEGER]) {
    assert.deepEqual(
      rebuildProgressMessage({ repositoryId: "仓-一", phase: "scan", done, total: done }),
      { key: "repo.rebuild_progress_scan", params: { done } },
    );
  }
});

test("每个非扫描阶段映射到唯一文案", () => {
  assert.deepEqual(
    rebuildProgressMessage({ repositoryId: "r", phase: "apply", done: 8, total: 8 }),
    { key: "repo.rebuild_progress_apply" },
  );
  assert.deepEqual(
    rebuildProgressMessage({ repositoryId: "r", phase: "metadata", done: 3, total: 8 }),
    { key: "repo.rebuild_progress_metadata", params: { done: 3, total: 8 } },
  );
  assert.deepEqual(
    rebuildProgressMessage({ repositoryId: "r", phase: "counts", done: 8, total: 8 }),
    { key: "repo.rebuild_progress_counts" },
  );
  assert.deepEqual(
    rebuildProgressMessage({ repositoryId: "r", phase: "done", done: 8, total: 8 }),
    { key: "repo.rebuild_progress_finishing" },
  );
});
