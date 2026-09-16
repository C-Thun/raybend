/**
 * 库的**中央状态**（`REPOSITORY.md` §5）。
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

import { createSignal } from "solid-js";
import type { RepositoryView } from "../../api/types.ts";
import type { LoadStatus } from "../../lib/load-status.ts";

/** 本模块用到的 `src/api/db.ts` 子集（注入以便测试） */
export interface RepositoryStateApi {
  listRepositories: () => Promise<RepositoryView[]>;
  remountRepository: (repositoryId: string) => Promise<RepositoryView>;
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
  remountErrors: () => Readonly<Record<string, string>>;
  /** 对所有登记路径重新查找一次（离线库的「插上盘再点我」） */
  remount: (repositoryId: string) => Promise<void>;
  /** 改导入模版：写库成功后**就地把新模版同步进列表**，返回落定的模版 */
  setTemplate: (repositoryId: string, template: string) => Promise<string>;
}

export function createRepositoryState(deps: {
  api: RepositoryStateApi;
}): RepositoryStateStore {
  const [list, setList] = createSignal<readonly RepositoryView[]>([]);
  const [status, setStatus] = createSignal<LoadStatus>("idle");
  const [error, setError] = createSignal<string | null>(null);
  const [remountingId, setRemountingId] = createSignal<string | null>(null);
  const [remountErrors, setRemountErrors] = createSignal<
    Record<string, string>
  >({});

  const byId = (repositoryId: string): RepositoryView | undefined =>
    list().find((row) => row.id === repositoryId);

  function patch(repositoryId: string, fields: Partial<RepositoryView>): void {
    setList((prev) => {
      let touched = false;
      const next = prev.map((row) => {
        if (row.id !== repositoryId) return row;
        touched = true;
        return { ...row, ...fields };
      });
      // 没这条就别造新数组：白换引用会让所有挂着列表的界面重渲染一遍
      return touched ? next : prev;
    });
  }

  function upsert(next: RepositoryView): void {
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
    patch(repositoryId, { online: false, root: null, photoCount: null });
  }

  async function load(): Promise<void> {
    setStatus("loading");
    try {
      const rows = await deps.api.listRepositories();
      setList(rows);
      setError(null);
      setStatus("ready");
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
      setStatus("error");
    }
  }

  function setRemountError(repositoryId: string, message: string): void {
    setRemountErrors((prev) => ({ ...prev, [repositoryId]: message }));
  }

  function clearRemountError(repositoryId: string): void {
    setRemountErrors((prev) => {
      if (!(repositoryId in prev)) return prev;
      const next = { ...prev };
      delete next[repositoryId];
      return next;
    });
  }

  async function remount(repositoryId: string): Promise<void> {
    if (remountingId() !== null) return;
    setRemountingId(repositoryId);
    try {
      const view = await deps.api.remountRepository(repositoryId);
      upsert(view);
      clearRemountError(repositoryId);
      // 没找到**不是错误**（`REPOSITORY.md` §2.3），但要给用户一句可读的话
      if (!view.online) {
        setRemountError(
          repositoryId,
          `未找到该库（已试过 ${view.triedPaths} 处已登记路径）`,
        );
      }
    } catch (caught) {
      setRemountError(
        repositoryId,
        caught instanceof Error ? caught.message : String(caught),
      );
    } finally {
      setRemountingId(null);
    }
  }

  async function setTemplate(
    repositoryId: string,
    template: string,
  ): Promise<string> {
    const saved = await deps.api.setRepositoryTemplate(repositoryId, template);
    patch(repositoryId, { importTemplate: saved.importTemplate });
    return saved.importTemplate;
  }

  return {
    list,
    status,
    error,
    byId,
    load,
    patch,
    upsert,
    markOffline,
    remountingId,
    remountErrors,
    remount,
    setTemplate,
  };
}
