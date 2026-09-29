/**
 * 库的**中央状态**（`memory/FUNCTION-REPOSITORY.md` §5）。
 *
 * ## 为什么要有它（人类 2026-09-16 的原话）
 *
 * > 「像库这种明显数据量是有限的资源，一律进此状态库，这样可以实现一个地方变更了状态，
 * > 所有挂在这套数据上的界面都会同步变更，不然你一个一个地方盯怎么可能做得好。」
 *
 * 之前是「谁发现谁自己记」：库设置弹窗去读 catalog、读不到，它自己知道这个库离线了，
 * 而外面那张库卡片**还显示在线** —— 一个状态被两个地方各存了一份，就必然漂。
 *
 * ## 规矩
 *
 * * **有限数据进这里**：库（以及以后的标签词典、设置项这类）。
 *   **无限增长的不要进**：照片列表这种按目录走各自的视图，进来只会变成一个巨型缓存。
 * * **所有改动都经由下面的方法**（`load` / `patch` / `markOffline` / `remount` / `setTemplate`），
 *   界面**只读**这里的信号。于是「一个地方变了，全都变」是结构上成立的，不靠自觉。
 * * `patch` 是**就地**的：不重新拉全表、不重建行对象，挂着的界面拿到的是同一个信号。
 */

import { revision } from "../../lib/revision.ts";
import { createSignal } from "solid-js";
import type { RepositoryView, RepositoryConnection } from "../../api/types.ts";
import type { RepositoryRemountError } from "../../components/ui/RepositoryCard.tsx";
import type { LoadStatus } from "../../lib/load-status.ts";

/**
 * 重挂载失败的原因。
 *
 * 为什么**不是一句拼好的中文**：文案归语言包，而这一层（状态模块）按纪律
 * 不引 i18n（用 `pnpm lint:arch` 盯着的分层跟「所有 store/state 都不引 i18n」一致）。
 * 所以这里只交出**事实**，句子由视图按当前语言渲染：
 *   * `not_found` → 卡片上写「没找到这个库（已试过 N 处）」（`repo.remount_failed`）；
 *   * `message` → 后端原话（已经是人话，直接展示）。
 */
export type RemountError = RepositoryRemountError;

/** 本模块用到的 `src/api/db.ts` 子集（注入以便测试） */
export interface RepositoryStateApi {
  listRepositories: () => Promise<RepositoryView[]>;
  remountRepository: (repositoryId: string, automatic?: boolean) => Promise<RepositoryView>;
  releaseRepository?: (repositoryId: string) => Promise<RepositoryView>;
  useRepositoryLocation?: (repositoryId: string, path: string) => Promise<RepositoryView>;
  addRepositoryLocation?: (repositoryId: string, path: string) => Promise<RepositoryView>;
  removeRepositoryLocation?: (repositoryId: string, path: string) => Promise<RepositoryView>;
  setRepositoryTemplate: (
    repositoryId: string,
    templateSource: string,
  ) => Promise<{ importTemplate: string }>;
}

export interface RepositoryStateStore {
  list: () => readonly RepositoryView[];
  status: () => LoadStatus;
  error: () => string | null;
  byId: (repositoryId: string) => RepositoryView | undefined;
  /** 仅用于保留当前展示路径，不能作为在线/写能力依据。 */
  lastVerifiedRoot: (repositoryId: string) => string | null;
  /** 重新拉全表（启动、建库后、用户手动重试） */
  load: () => Promise<void>;
  /** **就地打补丁**：只动给到的字段，别的行连对象都不换 */
  patch: (repositoryId: string, fields: Partial<RepositoryView>) => void;
  /** 有则就地改、没有则插进列表（重挂载/建库的返回值走这里） */
  upsert: (view: RepositoryView) => void;
  /**
   * 有地方发现它**其实读不到了**（例如库设置去读 `catalog.db` 失败）→ 立刻降级成离线。
   *
   * 这条就是「一个地方发现、所有界面同步」的入口：调用方不需要知道谁在显示这张卡片。
   */
  markOffline: (repositoryId: string) => void;
  remountingId: () => string | null;
  isRemounting: (repositoryId: string) => boolean;
  applyConnection: (connection: RepositoryConnection) => void;
  dispose: () => void;
  remountErrors: () => Readonly<Record<string, RemountError>>;
  /** 对所有登记路径重新查找一次（离线库的「插上盘再点我」） */
  remount: (repositoryId: string, automatic?: boolean) => Promise<void>;
  isChangingLocation: (repositoryId: string) => boolean;
  release: (repositoryId: string) => Promise<RepositoryView>;
  useLocation: (repositoryId: string, path: string) => Promise<RepositoryView>;
  addLocation: (repositoryId: string, path: string) => Promise<RepositoryView>;
  removeLocation: (repositoryId: string, path: string) => Promise<RepositoryView>;
  /** 改导入模版：写库成功后**就地把新模版同步进列表**，返回落定的模版 */
  setTemplate: (repositoryId: string, template: string) => Promise<string>;
}

export function createRepositoryState(deps: {
  api: RepositoryStateApi;
}): RepositoryStateStore {
  const [list, setList] = createSignal<readonly RepositoryView[]>([]);
  const [status, setStatus] = createSignal<LoadStatus>("idle");
  const [error, setError] = createSignal<string | null>(null);
  const [remountingIds, setRemountingIds] = createSignal<ReadonlySet<string>>(new Set());
  const [changingLocations, setChangingLocations] = createSignal<ReadonlySet<string>>(new Set());
  const inFlight = new Map<string, Promise<void>>();
  const versions = new Map<string, symbol>();
  const connectionVersions = new Map<string, symbol>();
  const connections = new Map<string, RepositoryConnection>();
  const verifiedRoots = new Map<string, string>();
  let loadTicket: symbol | null = null;
  let loading: Promise<void> | null = null;
  let disposed = false;
  const touch = (id: string): symbol => { const token = Symbol(); versions.set(id, token); return token; };
  const remountingId = (): string | null => remountingIds().values().next().value ?? null;
  const isRemounting = (id: string): boolean => remountingIds().has(id);
  function normalize(row: RepositoryView): RepositoryView {
    const cached = connections.get(row.id);
    const connection = cached && (!row.connection || revision(cached.revision) > revision(row.connection.revision)) ? cached : row.connection;
    if (!connection) { if (row.online && row.root) verifiedRoots.set(row.id, row.root); return row; }
    if (connection.state === "online" && connection.root) verifiedRoots.set(row.id, connection.root);
    connections.set(row.id, connection);
    return { ...row, connection, online: connection.state === "online", root: connection.state === "online" ? connection.root : null,
      displayPath: connection.state === "online" && connection.root ? connection.root : row.displayPath };
  }
  const [remountErrors, setRemountErrors] = createSignal<
    Record<string, RemountError>
  >({});

  const byId = (repositoryId: string): RepositoryView | undefined =>
    list().find((row) => row.id === repositoryId);

  function patch(repositoryId: string, fields: Partial<RepositoryView>): void {
    if (disposed) return;
    touch(repositoryId);
    setList((prev) => {
      let touched = false;
      const next = prev.map((row) => {
        if (row.id !== repositoryId) return row;
        touched = true;
        const next = normalize({ ...row, ...fields });
        if (next.online) clearRemountError(repositoryId);
        return next;
      });
      // 没这条就别造新数组：白换引用会让所有挂着列表的界面重渲染一遍
      return touched ? next : prev;
    });
  }

  function upsert(next: RepositoryView): void {
    if (disposed) return;
    touch(next.id);
    next = normalize(next);
    if (next.online) clearRemountError(next.id);
    setList((prev) => {
      const index = prev.findIndex((row) => row.id === next.id);
      if (index === -1) return [...prev, next];
      const copy = [...prev];
      copy[index] = next;
      return copy;
    });
  }

  function markOffline(repositoryId: string): void {
    const row = byId(repositoryId);
    if (row === undefined || !row.online) return;
    // 保留 displayPath（那是「上次已知路径」，离线时正要显示它），只收起在线专属的字段
    patch(repositoryId, { online: false, root: null, connection: row.connection ? { ...row.connection, state: "offline", root: null } : undefined });
  }

  function applyConnection(connection: RepositoryConnection): void {
    if (disposed) return;
    const current = connections.get(connection.repositoryId);
    if (current && revision(connection.revision) <= revision(current.revision)) return;
    connections.set(connection.repositoryId, connection);
    connectionVersions.set(connection.repositoryId, touch(connection.repositoryId));
    const row = byId(connection.repositoryId);
    if (row) {
      const next = normalize({ ...row, connection });
      if (next.online) clearRemountError(next.id);
      setList(rows => rows.map(item => item.id === next.id ? next : item));
    }
  }

  function load(): Promise<void> {
    if (disposed) return Promise.resolve();
    if (loading) return loading;
    const ticket = Symbol(); loadTicket = ticket;
    const before = new Map(versions);
    setStatus("loading");
    loading = (async () => {
      try {
        const rows = await deps.api.listRepositories();
        if (disposed || loadTicket !== ticket) return;
        const current = list();
        setList(rows.map(row => {
          const changed = versions.get(row.id) !== before.get(row.id);
          const existing = current.find(item => item.id === row.id);
          const next = normalize(changed && existing ? existing : row);
          if (next.online) clearRemountError(next.id);
          return next;
        }).concat(current.filter(row => !rows.some(item => item.id === row.id) && versions.get(row.id) !== before.get(row.id))));
        setError(null); setStatus("ready");
      } catch (caught) {
        if (disposed || loadTicket !== ticket) return;
        setError(caught instanceof Error ? caught.message : String(caught)); setStatus("error");
      } finally { loading = null; }
    })();
    return loading;
  }

  function setRemountError(repositoryId: string, error: RemountError): void {
    setRemountErrors((prev) => ({ ...prev, [repositoryId]: error }));
  }

  function clearRemountError(repositoryId: string): void {
    setRemountErrors((prev) => {
      if (!(repositoryId in prev)) return prev;
      const next = { ...prev };
      delete next[repositoryId];
      return next;
    });
  }

  function remount(repositoryId: string, automatic = false): Promise<void> {
    if (disposed || (automatic && ["released", "releasing"].includes(byId(repositoryId)?.connection?.state ?? ""))) return Promise.resolve();
    const existing = inFlight.get(repositoryId);
    if (existing) return existing;
    const ticket = touch(repositoryId);
    setRemountingIds(prev => new Set([...prev, repositoryId]));
    clearRemountError(repositoryId);
    const task = (async () => {
      try {
        const view = await deps.api.remountRepository(repositoryId, automatic);
        if (disposed) return;
        const current = connections.get(repositoryId);
        const observedOwnResult = versions.get(repositoryId) === connectionVersions.get(repositoryId)
          && view.connection && current && revision(view.connection.revision) >= revision(current.revision);
        if (versions.get(repositoryId) === ticket || observedOwnResult) {
          upsert(view);
          if (!byId(repositoryId)?.online && (!view.connection || view.connection.state === "offline"))
            setRemountError(repositoryId, { kind: "not_found", tried: view.triedPaths });
        }
      } catch (caught) {
        if (!disposed && versions.get(repositoryId) === ticket)
          setRemountError(repositoryId, { kind: "message", text: caught instanceof Error ? caught.message : String(caught) });
      } finally {
        inFlight.delete(repositoryId);
        if (!disposed) setRemountingIds(prev => { const next = new Set(prev); next.delete(repositoryId); return next; });
      }
    })();
    inFlight.set(repositoryId, task);
    return task;
  }

  async function setTemplate(
    repositoryId: string,
    template: string,
  ): Promise<string> {
    const saved = await deps.api.setRepositoryTemplate(repositoryId, template);
    patch(repositoryId, { importTemplate: saved.importTemplate });
    return saved.importTemplate;
  }

  async function changeLocation(id: string, path: string, remove: boolean): Promise<RepositoryView> {
    if (disposed || changingLocations().has(id)) throw { code: "busy" };
    const mutate = remove ? deps.api.removeRepositoryLocation : deps.api.addRepositoryLocation;
    if (!mutate) throw { code: "unsupported_location" };
    return mutateRepository(id, () => mutate(id, path));
  }
  async function mutateRepository(id: string, run: () => Promise<RepositoryView>): Promise<RepositoryView> {
    if (disposed || changingLocations().has(id)) throw { code: "busy" };
    const ticket = touch(id);
    setChangingLocations(prev => new Set([...prev, id]));
    try {
      const view = await run();
      const current = connections.get(id);
      const ownEvent = versions.get(id) === connectionVersions.get(id)
        && view.connection && current && revision(view.connection.revision) >= revision(current.revision);
      if (!disposed && (versions.get(id) === ticket || ownEvent)) upsert(view);
      return view;
    } finally {
      if (!disposed) setChangingLocations(prev => { const next = new Set(prev); next.delete(id); return next; });
    }
  }

  return {
    list,
    status,
    error,
    byId,
    lastVerifiedRoot: id => verifiedRoots.get(id) ?? null,
    load,
    patch,
    upsert,
    markOffline,
    remountingId,
    isRemounting,
    applyConnection,
    dispose: () => { disposed = true; loadTicket = null; },
    remountErrors,
    remount,
    isChangingLocation: id => changingLocations().has(id),
    release: id => mutateRepository(id, () => { if (!deps.api.releaseRepository) throw { code: "unsupported_location" }; return deps.api.releaseRepository(id); }),
    useLocation: (id, path) => mutateRepository(id, () => { if (!deps.api.useRepositoryLocation) throw { code: "unsupported_location" }; return deps.api.useRepositoryLocation(id, path); }),
    addLocation: (id, path) => changeLocation(id, path, false),
    removeLocation: (id, path) => changeLocation(id, path, true),
    setTemplate,
  };
}
