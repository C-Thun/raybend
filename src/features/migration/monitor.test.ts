import assert from "node:assert/strict";
import test from "node:test";
import type { MigrationSnapshot } from "../../api/types.ts";
import { createMigrationMonitor } from "./monitor.ts";
const snapshot = (revision: string, running = true): MigrationSnapshot => ({
  revision, active: running ? [{ id: "1", kind: "catalog", label: "照片库", from: 1, to: 2, running: true }] : [],
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(r => { resolve = r; });
  return { promise, resolve };
}

test("先订阅后查询，较新的完成事件先到不被旧快照覆盖，start 单飞", async () => {
  const calls: string[] = [], states: number[] = [];
  let handler!: (value: MigrationSnapshot) => void;
  const query = deferred<MigrationSnapshot>();
  const monitor = createMigrationMonitor({
    subscribe: async fn => { calls.push("subscribe"); handler = fn; return () => { calls.push("off"); }; },
    snapshot: () => { calls.push("query"); return query.promise; },
    onChange: state => states.push(state.size),
  });
  const first = monitor.start();
  assert.equal(monitor.start(), first);
  await Promise.resolve();
  handler(snapshot("2", false));
  query.resolve(snapshot("1"));
  await first;
  assert.deepEqual(calls, ["subscribe", "query"]);
  assert.deepEqual(states, [0]);
  monitor.dispose(); handler(snapshot("3")); monitor.dispose();
  assert.deepEqual(states, [0]);
  assert.deepEqual(calls, ["subscribe", "query", "off"]);
});

test("晚监听快照恢复正在升级的 catalog", async () => {
  const states: number[] = [];
  const monitor = createMigrationMonitor({ subscribe: async () => () => {}, snapshot: async () => snapshot("2"), onChange: map => states.push(map.size) });
  await monitor.start(); assert.deepEqual(states, [1]); monitor.dispose();
});

test("订阅返回前销毁：立即清理迟到订阅，不再查询和回写", async () => {
  const listen = deferred<() => void>(); let off = 0, queries = 0;
  const monitor = createMigrationMonitor({ subscribe: () => listen.promise, snapshot: async () => { queries++; return snapshot("1"); }, onChange: () => assert.fail("销毁后不回写") });
  const starting = monitor.start(); monitor.dispose(); listen.resolve(() => { off++; });
  await starting; assert.equal(off, 1); assert.equal(queries, 0);
});

test("查询返回前销毁不回写，查询失败仍可从后续事件恢复", async () => {
  const query = deferred<MigrationSnapshot>();
  const monitor = createMigrationMonitor({ subscribe: async () => () => {}, snapshot: () => query.promise, onChange: () => assert.fail("销毁后不回写") });
  const starting = monitor.start(); await Promise.resolve(); monitor.dispose(); query.resolve(snapshot("1")); await starting;
  let handler!: (value: MigrationSnapshot) => void;
  const states: number[] = [];
  const failed = createMigrationMonitor({ subscribe: async fn => { handler = fn; return () => {}; }, snapshot: async () => { throw new Error("query failed"); }, onChange: map => states.push(map.size) });
  await assert.rejects(failed.start(), /query failed/);
  handler(snapshot("2")); handler(snapshot("3", false));
  assert.deepEqual(states, [1, 0]); failed.dispose();
});

test("事件订阅失败仍恢复快照，轮询内存状态到结束并在销毁时停止", async () => {
  let poll!: () => void, stopped = 0, failures = 0;
  let current = snapshot("2");
  const states: number[] = [];
  const monitor = createMigrationMonitor({
    subscribe: async () => { throw new Error("listen failed"); },
    snapshot: async () => current,
    onChange: map => states.push(map.size),
    onSubscriptionError: () => { failures++; },
    repeat: (fn, ms) => { assert.equal(ms, 1_000); poll = fn; return () => { stopped++; }; },
  });
  await monitor.start(); assert.deepEqual(states, [1]); assert.equal(failures, 1);
  current = snapshot("3", false); poll(); await Promise.resolve(); await Promise.resolve();
  assert.deepEqual(states, [1, 0]); monitor.dispose(); assert.equal(stopped, 1);
  current = snapshot("4"); poll(); await Promise.resolve(); assert.deepEqual(states, [1, 0]);
});

test("事件降级查询单飞，慢 IPC 不叠加请求，销毁后不再发查询", async () => {
  let poll!: () => void, calls = 0;
  const query = deferred<MigrationSnapshot>();
  const monitor = createMigrationMonitor({ subscribe: async () => { throw new Error("listen"); },
    snapshot: () => { calls++; return query.promise; }, onChange: () => {}, repeat: fn => { poll = fn; return () => {}; },
  });
  const starting = monitor.start(); await Promise.resolve();
  poll(); poll(); poll(); assert.equal(calls, 1);
  query.resolve(snapshot("1")); await starting;
  monitor.dispose(); poll(); assert.equal(calls, 1);
});
