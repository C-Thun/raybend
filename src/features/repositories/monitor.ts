/** 应用级库观察：只枚举卷，按触发有界探测登记位置。 */
import type { RepositoryConnection, Volume } from "../../api/types.ts";
import { untrack } from "solid-js";
import type { RepositoryStateStore } from "./state.ts";
export const VOLUME_POLL_MS = 2_000;
export const PROBE_DEBOUNCE_MS = 200;
export const RETRY_MS = [1_000, 3_000, 10_000] as const;
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
  const attempts = new Map<string, number>();
  let disposed = false, subscribed = false, started: Promise<void> | null = null, stop: (() => void) | undefined, off: (() => void) | undefined;
  let volumes: Volume[] = [], signature: string | null = null, volumeFlight: Promise<Volume[]> | null = null;
  function observeConnection(status: RepositoryConnection): void {
    if (disposed) return;
    const previous = deps.repositories.byId(status.repositoryId)?.connection;
    deps.repositories.applyConnection(status);
    if (deps.repositories.byId(status.repositoryId)?.connection?.revision !== status.revision) return;
    if (status.state === "released" || status.state === "releasing") {
      attempts.delete(status.repositoryId); pending.get(status.repositoryId)?.(); pending.delete(status.repositoryId); return;
    }
    if (status.state === "online") {
      attempts.delete(status.repositoryId); pending.get(status.repositoryId)?.(); pending.delete(status.repositoryId);
      if (previous && (previous.state !== "online" || previous.generation !== status.generation)) deps.onRecovery?.(status.repositoryId);
    }
  }
  function schedule(id: string, delay = PROBE_DEBOUNCE_MS): void {
    if (disposed || pending.has(id)) return;
    pending.set(id, timers.after(() => { pending.delete(id); void probe(id); }, delay));
  }
  async function probe(id: string): Promise<void> {
    if (disposed) return;
    const previous = deps.repositories.byId(id)?.connection;
    await deps.repositories.remount(id, true);
    if (disposed) return;
    const row = deps.repositories.byId(id);
    if (!row || row.online) {
      attempts.delete(id);
      if (!subscribed && row?.connection && previous && (previous.state !== "online" || previous.generation !== row.connection.generation)) deps.onRecovery?.(id);
      return;
    }
    // 损坏/权限/版本/位置策略错误不当拔盘循环重试。
    if (row.connection?.state === "unavailable" && row.connection.reason !== "timeout") return;
    const attempt = attempts.get(id) ?? 0;
    if (attempt < RETRY_MS.length) { attempts.set(id, attempt + 1); schedule(id, RETRY_MS[attempt]); }
  }
  function request(ids?: string[]): void {
    // 触发是命令；不能把调用它的工作流 effect 订阅到库状态，造成探测回写再次触发探测。
    untrack(() => {
      for (const id of ids ?? deps.repositories.list().map(row => row.id)) {
        if (["released", "releasing"].includes(deps.repositories.byId(id)?.connection?.state ?? "")) continue;
        attempts.delete(id); schedule(id);
      }
    });
  }
  function listVolumes(): Promise<Volume[]> {
    if (volumeFlight) return volumeFlight;
    volumeFlight = deps.listVolumes().then(rows => {
      if (disposed) return volumes;
      const next = JSON.stringify(rows.map(row => [row.path, row.kind]).sort((a,b) => String(a[0]).localeCompare(String(b[0]))));
      volumes = rows; deps.onVolumes?.(rows);
      if (signature !== null && signature !== next) request();
      signature = next; return rows;
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
        else console.warn("[repository] Connection events unavailable; using bounded probes", error);
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
