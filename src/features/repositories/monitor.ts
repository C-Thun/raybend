/** 应用级库观察：只枚举卷，按**用户意图**探测登记位置。
 *
 * 探测的触发点（崔总 2026-10-11 定，**只有这两类**）：
 *   ① 启动时把登记过的库各探一次；
 *   ② 用户点了某个库（列表里换选中、卡片的「重新查找」、命令「重新连接」）。
 * 曾经的自动重试链（1s/3s/10s）、窗口回焦点、卷表变化这三类**都删掉**了 ——
 * 库离线是事实，后台反复探它只会刷屏日志与磁盘；插上盘之后用户点一次才是唯一入口
 * （`memory/FUNCTION-REPOSITORY.md` §2.3）。
 */
import type { RepositoryConnection, Volume } from "../../api/types.ts";
import { untrack } from "solid-js";
import type { RepositoryStateStore } from "./state.ts";
export const VOLUME_POLL_MS = 2_000;
export const PROBE_DEBOUNCE_MS = 200;
export interface RepositoryMonitorTimers {
  after: (fn: () => void, ms: number) => () => void;
  repeat: (fn: () => void, ms: number) => () => void;
}
export function createRepositoryMonitor(deps: {
  repositories: RepositoryStateStore;
  listVolumes: () => Promise<Volume[]>;
  subscribe: (handler: (status: RepositoryConnection) => void) => Promise<() => void>;
  onSubscriptionError?: (error: unknown) => void;
  onRecovery?: (id: string) => void;
  onVolumes?: (rows: Volume[]) => void;
  timers?: RepositoryMonitorTimers;
}) {
  const timers = deps.timers ?? {
    after: (fn: () => void, ms: number) => { const handle = setTimeout(fn, ms); return () => clearTimeout(handle); },
    repeat: (fn: () => void, ms: number) => { const handle = setInterval(fn, ms); return () => clearInterval(handle); },
  };
  const pending = new Map<string, () => void>();
  let disposed = false, subscribed = false, started: Promise<void> | null = null, stop: (() => void) | undefined, off: (() => void) | undefined;
  let volumes: Volume[] = [], volumeFlight: Promise<Volume[]> | null = null;
  function observeConnection(status: RepositoryConnection): void {
    if (disposed) return;
    const previous = deps.repositories.byId(status.repositoryId)?.connection;
    deps.repositories.applyConnection(status);
    if (deps.repositories.byId(status.repositoryId)?.connection?.revision !== status.revision) return;
    if (status.state === "released" || status.state === "releasing") {
      pending.get(status.repositoryId)?.(); pending.delete(status.repositoryId); return;
    }
    if (status.state === "online") {
      pending.get(status.repositoryId)?.(); pending.delete(status.repositoryId);
      if (previous && (previous.state !== "online" || previous.generation !== status.generation)) deps.onRecovery?.(status.repositoryId);
    }
  }
  function schedule(id: string, delay = PROBE_DEBOUNCE_MS): void {
    if (disposed || pending.has(id)) return;
    pending.set(id, timers.after(() => { pending.delete(id); void probe(id); }, delay));
  }
  /**
   * 探一次就完：**不自动重试**。
   *
   * 探完仍是离线就不再排下一轮 —— 重试入口是用户手里的那条（卡片上的「重新查找」/
   * 命令面板的「重新连接」），它们直接走 `repositories.remount`，不经这里。
   */
  async function probe(id: string): Promise<void> {
    if (disposed) return;
    const previous = deps.repositories.byId(id)?.connection;
    await deps.repositories.remount(id, true);
    if (disposed) return;
    const row = deps.repositories.byId(id);
    if (!subscribed && row?.connection && previous && (previous.state !== "online" || previous.generation !== row.connection.generation)) deps.onRecovery?.(id);
  }
  function request(ids?: string[]): void {
    // 触发是命令；不能把调用它的工作流 effect 订阅到库状态，造成探测回写再次触发探测。
    untrack(() => {
      for (const id of ids ?? deps.repositories.list().map(row => row.id)) {
        if (["released", "releasing"].includes(deps.repositories.byId(id)?.connection?.state ?? "")) continue;
        schedule(id);
      }
    });
  }
  function listVolumes(): Promise<Volume[]> {
    if (volumeFlight) return volumeFlight;
    volumeFlight = deps.listVolumes().then(rows => {
      if (disposed) return volumes;
      volumes = rows; deps.onVolumes?.(rows);
      return rows;
    }).finally(() => { volumeFlight = null; });
    return volumeFlight;
  }
  async function start(): Promise<void> {
    if (started) return started;
    started = (async () => {
      try { off = await deps.subscribe(observeConnection); subscribed = true; }
      catch (error) {
        if (disposed) return;
        if (deps.onSubscriptionError) deps.onSubscriptionError(error);
        else console.warn("[repository] Connection events unavailable; probing on demand", error);
      }
      if (disposed) { off?.(); return; }
      await deps.repositories.load();
      if (disposed) return;
      request();
      void listVolumes().catch(() => {});
      stop = timers.repeat(() => { void listVolumes().catch(() => {}); }, VOLUME_POLL_MS);
    })();
    return started;
  }
  return { start, request, listVolumes, dispose: () => {
    disposed = true; stop?.(); off?.(); for (const cancel of pending.values()) cancel(); pending.clear();
  } };
}
