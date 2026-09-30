import assert from "node:assert/strict";
import { test } from "node:test";
import { createOrganizationWorker } from "./organization-worker.ts";

test("reconcile drains bounded batches and a concurrent wake never starts a second writer", async () => {
  let calls = 0;
  let active = 0;
  let maxActive = 0;
  const worker = createOrganizationWorker({
    repositories: () => ["A"],
    reconcile: async () => {
      calls += 1;
      active += 1;
      maxActive = Math.max(maxActive, active);
      if (calls === 1) worker.wake();
      await Promise.resolve();
      active -= 1;
      return { processedBatches: calls < 3 ? 1 : 0, failedRepositories: [] };
    },
  });
  worker.wake();
  await new Promise<void>((resolve) => setTimeout(resolve, 30));
  assert.equal(calls, 3);
  assert.equal(maxActive, 1);
  worker.dispose();
});

test("partial library failures surface once and reappear after recovery", async () => {
  const reports: string[][] = [];
  const outcomes = [["A"], ["A"], [], ["A"]];
  const worker = createOrganizationWorker({
    repositories: () => ["A", "B"],
    reconcile: async () => ({ processedBatches: 0, failedRepositories: outcomes.shift() ?? [] }),
    onPartialFailure: (ids) => reports.push(ids),
  });
  for (let index = 0; index < 4; index += 1) {
    worker.wake();
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
  assert.deepEqual(reports, [["A"], ["A"]]);
  worker.dispose();
});
