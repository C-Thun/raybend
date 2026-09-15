/**
 * 目录树的**模块内部**状态（`ARCHITECTURE.md` §3：模块内部状态归该 feature）。
 *
 * 这里只有三件事：**展开集合**、**已读到的子目录**、**每个目录的加载/错误状态**。
 * 选中与勾选**不在这里** —— 它们是跨面板共享的（`workspaces/import/store.ts`），
 * 而且「选中不触发展开」这条规则（`DESIGN.md` §12.4.1）靠这个分工在结构上就成立：
 * 这个 store 里根本没有「选中」这个输入。
 *
 * 依赖 `loadDirs` 是注入的：测试不需要真文件系统。
 */

import { createSignal } from "solid-js";
import type { DirEntry } from "../../api/types.ts";

export interface SourceTreeDeps {
  /** 读一个目录的直接子目录（失败时抛错） */
  loadDirs: (path: string) => Promise<DirEntry[]>;
}

export interface SourceTreeStore {
  isExpanded: (path: string) => boolean;
  /** 子目录；`undefined` = 还没读过 */
  childrenOf: (path: string) => readonly DirEntry[] | undefined;
  isLoading: (path: string) => boolean;
  /** 读失败的原因（没有错误时 `undefined`） */
  errorOf: (path: string) => string | undefined;
  /** 展开（没读过就加载；并发调用只会真的读一次） */
  expand: (path: string) => Promise<void>;
  /** 折叠（**保留**已读到的子目录，下次展开是秒开） */
  collapse: (path: string) => void;
  /** 切换展开 / 折叠 */
  toggle: (path: string) => Promise<void>;
  /** 重新读（刷新按钮或外部变化时用） */
  refresh: (path: string) => Promise<void>;
}

export function createSourceTreeStore(deps: SourceTreeDeps): SourceTreeStore {
  const [expanded, setExpanded] = createSignal<Record<string, true>>({});
  const [children, setChildren] = createSignal<Record<string, DirEntry[]>>({});
  const [loading, setLoading] = createSignal<Record<string, true>>({});
  const [errors, setErrors] = createSignal<Record<string, string>>({});

  /** 正在飞的请求：合并同一个目录的并发展开（点两下不要再读一遍） */
  const inFlight = new Map<string, Promise<void>>();

  const isExpanded = (path: string): boolean => expanded()[path] === true;
  const childrenOf = (path: string): readonly DirEntry[] | undefined =>
    children()[path];
  const isLoading = (path: string): boolean => loading()[path] === true;
  const errorOf = (path: string): string | undefined => errors()[path];

  const setFlag = (
    setter: (updater: (prev: Record<string, true>) => Record<string, true>) => void,
    path: string,
    value: boolean,
  ): void => {
    setter((prev) => {
      if (value) return { ...prev, [path]: true };
      const next = { ...prev };
      delete next[path];
      return next;
    });
  };

  const clearError = (path: string): void => {
    setErrors((prev) => {
      if (!(path in prev)) return prev;
      const next = { ...prev };
      delete next[path];
      return next;
    });
  };

  async function load(path: string): Promise<void> {
    setFlag(setLoading, path, true);
    try {
      const list = await deps.loadDirs(path);
      setChildren((prev) => ({ ...prev, [path]: list }));
      clearError(path);
    } catch (error) {
      setErrors((prev) => ({ ...prev, [path]: message(error) }));
    } finally {
      setFlag(setLoading, path, false);
    }
  }

  async function expand(path: string): Promise<void> {
    setFlag(setExpanded, path, true);
    if (childrenOf(path) !== undefined) return; // 读过就不再读（折叠是秒开的）
    const existing = inFlight.get(path);
    if (existing) return existing;
    const task = load(path).finally(() => inFlight.delete(path));
    inFlight.set(path, task);
    return task;
  }

  function collapse(path: string): void {
    setFlag(setExpanded, path, false);
  }

  async function toggle(path: string): Promise<void> {
    if (isExpanded(path)) {
      collapse(path);
      return;
    }
    await expand(path);
  }

  async function refresh(path: string): Promise<void> {
    await load(path);
  }

  return {
    isExpanded,
    childrenOf,
    isLoading,
    errorOf,
    expand,
    collapse,
    toggle,
    refresh,
  };
}

/** 把任意异常转成一句话（与 `workspaces/import/store.ts` 的 `errorText` 同口径）。 */
function message(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return String(error);
}
