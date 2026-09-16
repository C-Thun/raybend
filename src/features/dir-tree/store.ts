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

export interface DirTreeDeps {
  /** 读一个目录的直接子目录（失败时抛错） */
  loadDirs: (path: string) => Promise<DirEntry[]>;
}

export interface DirTreeStore {
  isExpanded: (path: string) => boolean;
  /** 子目录；`undefined` = 还没读过 */
  childrenOf: (path: string) => readonly DirEntry[] | undefined;
  isLoading: (path: string) => boolean;
  /** 读失败的原因（没有错误时 `undefined`） */
  errorOf: (path: string) => string | undefined;
  /** 展开（没读过就加载；并发调用只会真的读一次） */
  expand: (path: string) => Promise<void>;
  /** 折叠（保留已读到的子目录：下次展开先秒现旧内容，同时重读那一级） */
  collapse: (path: string) => void;
  /** 切换展开 / 折叠 */
  toggle: (path: string) => Promise<void>;
  /** 重新读一个目录（行内「重试」用） */
  refresh: (path: string) => Promise<void>;
  /**
   * 重读**当前所有展开着的目录**。
   *
   * 这不是「刷新按钮」的后台（展开本来就会重读）—— 它给「窗口重新获得焦点」用：
   * 用户切走一会儿再切回来，磁盘/U 盘/网络盘上的东西可能已经变了，
   * 展开着的分支应当跟着对上现实（文件管理器都是这个行为）。
   */
  refreshAll: () => Promise<void>;
}

export function createDirTreeStore(deps: DirTreeDeps): DirTreeStore {
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
    /*
     * **展开就重读这一级**（只这一级的直接子目录，**不递归**）。
     *
     * 2026-09-16 人类定的原则：缓存是为了「展开时立刻有东西看、慢盘/网络盘也不卡手感」，
     * **不是**为了少读磁盘 —— 没有哪个文件管理器要求用户按「刷新」才看得到真实内容，
     * 那等于把正确性推给用户。一级 `readdir` 是整个设计里最便宜的一环
     * （对比：导入前的整库扫描要读成千上万个目录）。
     *
     * 重读期间**旧内容留着不动**（`load` 只在成功时覆盖）：熟悉的内容秒现，
     * 新数据到了悄悄替换；读失败也不会把已经看到的内容清空。
     */
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

  /**
   * 重新读所有**展开着**的目录。
   *
   * 为什么需要它（2026-09-16 人类反馈）：目录会在程序外面被创建 / 改名 / 删除，
   * 而我们**故意不做文件系统监听**（`FUTURE.md` 里 `notify` 是后续里程碑）——
   * 那就必须留一个「运行期间刷新」的入口，否则用户只能重启程序才看得到变化。
   *
   * 两个刻意的取舍：
   *   * **保留展开状态**：按刷新是为了看新内容，不是为了把树折叠回去；
   *   * **只刷展开着的**：没展开的分支用户看不到，刷它纯属白读磁盘。
   */
  async function refreshAll(): Promise<void> {
    const paths = Object.keys(expanded());
    await Promise.all(paths.map((path) => load(path)));
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
    refreshAll,
  };
}

/** 把任意异常转成一句话（与 `workspaces/import/store.ts` 的 `errorText` 同口径）。 */
function message(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return String(error);
}
