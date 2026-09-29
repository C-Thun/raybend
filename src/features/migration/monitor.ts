/** 先订阅再查询，完整快照解决晚监听和事件/查询响应乱序。 */
import type { MigrationSnapshot } from "../../api/types.ts";
import { applySnapshot, NO_MIGRATION_STATE, type MigrationMap } from "./notice.ts";

export function createMigrationMonitor(deps: {
  subscribe: (handler: (snapshot: MigrationSnapshot) => void) => Promise<() => void>;
  snapshot: () => Promise<MigrationSnapshot>;
  onChange: (notices: MigrationMap) => void;
  onSubscriptionError?: (error: unknown) => void;
  repeat?: (callback: () => void, ms: number) => () => void;
}) {
  let state = NO_MIGRATION_STATE;
  let disposed = false;
  let off: (() => void) | undefined;
  let started: Promise<void> | undefined;
  let stop: (() => void) | undefined;
  let refreshing: Promise<void> | undefined;
  function receive(snapshot: MigrationSnapshot): void {
    if (disposed) return;
    const next = applySnapshot(state, snapshot);
    if (next === state) return;
    state = next;
    deps.onChange(state.notices);
  }
  function refresh(): Promise<void> {
    if (disposed) return Promise.resolve();
    return refreshing ??= deps.snapshot().then(receive).finally(() => { refreshing = undefined; });
  }
  function start(): Promise<void> {
    return started ??= (async () => {
      try {
        off = await deps.subscribe(receive);
      } catch (error) {
        if (disposed) return;
        deps.onSubscriptionError?.(error);
        // 事件桥不可用时只查询原生内存状态，不重开数据库/探测磁盘。
        const repeat = deps.repeat ?? ((callback, ms) => {
          const timer = setInterval(callback, ms);
          return () => clearInterval(timer);
        });
        stop = repeat(() => {
          void refresh().catch(error => { if (!disposed) deps.onSubscriptionError?.(error); });
        }, 1_000);
      }
      if (disposed) { off?.(); off = undefined; stop?.(); stop = undefined; return; }
      // 查询失败也保留已建立的事件订阅，后续事件仍能恢复状态。
      await refresh();
    })();
  }
  function dispose(): void {
    disposed = true;
    off?.();
    off = undefined;
    stop?.();
    stop = undefined;
  }
  return { start, dispose };
}
