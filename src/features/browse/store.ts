/**
 * 浏览工作流的状态（`features/browse/` 自己的 store）。
 *
 * 它管五件事：
 *
 * 1. **查询**：库 / 目录范围 / 筛选 / 排序 —— 任何一项变了就重新加载；
 * 2. **窗口数据**：照片是按页取的（虚拟网格只看得见几十行，没必要一次拿十万条），
 *    所以内部是一张**稀疏表** `(AssetItem | null)[]`，`null` = 这一格还没到；
 * 3. **选择**：完全复用 `lib/selection.ts`（`BROWSE.md` §5.2 的点击/Shift/Ctrl 语义
 *    在那里实现并有测试），这里只负责把「可见顺序」喂给它；
 * 4. **标记**：把选中集合交给后端，并维护一份「这些照片现在是什么状态」的缓存
 *    （`toolsbar` 的三态控件要用）；
 * 5. **旗标**：内存态、跨库（`BROWSE.md` §3.2），所以在本地维护 pick/reject 集合。
 *
 * # 三条容易写错的地方（都有测试盯着）
 *
 * 1. **迟到的结果必须丢掉**：换库/换目录/换筛选之后，旧请求回来不能覆盖新数据
 *    （照片网格在 M1 踩过这个坑，这里用同一套「代号」办法）。
 * 2. **同一页不能重复请求**：滚动会反复问「这个范围有没有数据」，
 *    没有 in-flight 记录的话一屏能打出几十个重复请求。
 * 3. **换查询要清选择**：换了库/目录/筛选之后，原来选中的照片可能根本不在列表里了，
 *    留着它们会让「批量操作」作用在看不见的照片上。
 *
 * ⚠️ 派生量一律写成普通函数（不用 `createMemo`）：Node 里 `solid-js` 走 SSR 构建，
 * `createMemo` 只求值一次，测试会拿到永远不更新的假值（与既有 store 同一套说明）。
 */

import { createSignal } from "solid-js";

import type {
  AssetItem,
  BrowseFacets,
  BrowseFilter,
  BrowseQuery,
  BrowseSort,
  DeleteResult,
  MarkAction,
  MarkResult,
  MarkingItem,
  TimelineEntry,
} from "../../api/types.ts";
import {
  applySelection,
  clearSelection,
  EMPTY_SELECTION,
  hasSelection,
  selectAll as selectAllIds,
  selectionCount,
  type SelectionState,
} from "../../lib/selection.ts";
import { clearMarkFilters, conditionCount } from "./filter.ts";

/** 一页取多少张。256 与网格的 overscan 一起够撑一屏还有富余。 */
export const PAGE_SIZE = 256;

/** 后端接口（测试里换成假实现）。 */
export interface BrowseApi {
  page(
    query: BrowseQuery,
    offset: number,
    limit: number,
  ): Promise<{ total: number; offset: number; items: AssetItem[] }>;
  timeline(query: BrowseQuery, limit?: number): Promise<{
    total: number;
    entries: TimelineEntry[];
  }>;
  facets(query: BrowseQuery): Promise<BrowseFacets>;
  markings(repositoryId: string, ids: readonly number[]): Promise<MarkingItem[]>;
  mark(
    repositoryId: string,
    ids: readonly number[],
    action: MarkAction,
  ): Promise<MarkResult>;
  undo(repositoryId: string): Promise<MarkResult>;
  redo(repositoryId: string): Promise<MarkResult>;
  remove(repositoryId: string, ids: readonly number[]): Promise<DeleteResult>;
  flagsGet(repositoryId: string): Promise<{
    total: number;
    picks: number[];
    rejects: number[];
  }>;
  flagsSet(
    repositoryId: string,
    ids: readonly number[],
    flag: "pick" | "reject" | null,
  ): Promise<{ total: number; picks: number[]; rejects: number[] }>;
  flagsClear(repositoryId: string): Promise<{
    total: number;
    picks: number[];
    rejects: number[];
  }>;
}

export interface BrowseDeps {
  api: BrowseApi;
}

/** 撤销/重做按钮要的几个值（来自后端每次动作返回的 `MarkResult`） */
export interface UndoState {
  canUndo: boolean;
  canRedo: boolean;
  /** 「撤销：<label>」里的 label（后端给的可读动作名） */
  undoLabel: string | null;
  redoLabel: string | null;
}

export const EMPTY_UNDO_STATE: UndoState = {
  canUndo: false,
  canRedo: false,
  undoLabel: null,
  redoLabel: null,
};

export interface BrowseStore {
  // ── 查询 ──
  repositoryId(): string | null;
  /** 查询范围（**库内相对路径**，如 `photos/2026-08-15`）。
   *  `null` = 还没选目录 —— 网格是空态、**不发查询**（口径见 `query` 的说明）。 */
  scopePath(): string | null;
  filter(): BrowseFilter;
  sort(): BrowseSort;
  /**
   * 筛选模式（`BROWSE.md` §3.1）：开着时 toolsbar 的标记控件是**筛选条件**，
   * 而不是「给选中的照片设值」。关掉时会把四组标记条件清干净 ——
   * 否则界面看起来「没筛」却还少着照片（原先这件事在工具条里做，现在收到这里，
   * 因为工具条与结果区（chips）两边都要读它）。
   */
  filterMode(): boolean;
  setFilterMode(on: boolean): void;
  /** 当前查询（没有库时是 `null`）。 */
  query(): BrowseQuery | null;
  setRepository(id: string | null): void;
  setScope(path: string | null): void;
  setFilter(filter: BrowseFilter): void;
  patchFilter(patch: BrowseFilter): void;
  setSort(sort: BrowseSort): void;

  // ── 数据 ──
  total(): number;
  /** 第 `index` 张；还没加载到就是 `null`。 */
  itemAt(index: number): AssetItem | null;
  timeline(): readonly TimelineEntry[];
  facets(): BrowseFacets | null;
  loading(): boolean;
  error(): string | null;
  /** 重新加载（换库/换筛选后自动调；也可以手动调）。 */
  reload(): Promise<void>;
  /** 保证 `[start, end)` 这一段的数据都在（缺哪页补哪页）。 */
  ensureRange(start: number, end: number): Promise<void>;

  // ── 选择 ──
  selection(): SelectionState;
  selectedIds(): number[];
  selectedCount(order?: readonly string[]): number;
  selectedItems(): AssetItem[];
  anchorId(): number | null;
  /**
   * 锚点那张（多选时右栏与 flowinfo 用它）；一张没选就是 `null`。
   *
   * 规则只此一处（人类 2026-09-19）：以前 browse 工作区与组装层各写一份，
   * flowbar 的信息区（flowinfo）再抄就是第三份，所以下沉到 store。
   */
  anchorItem(): AssetItem | null;
  select(id: number, mode: "replace" | "toggle" | "range", order?: readonly string[]): void;
  /**
   * **只把「当前那张」（锚点）挪过去**，不动选择集合。
   *
   * 对比视图里点某一幅画幅就是这件事（`BROWSE.md` §5.7：点哪张图就是当前实际选中的图，
   * 决定右栏与状态栏显示谁）—— 那里**不能**走 `select()`：那会把选择改成只剩这一张，
   * 对比当场散掉。
   *
   * 纪律：锚点**必须在选择集合里**（`§1.12`：锚点就是「选了一堆里的那一张」）。
   * 不在就不动 —— 静默把锚点挪到没被选中的图上，会让右栏显示一张「没被选中」的照片。
   */
  setAnchor(id: number): void;
  /**
   * 「可见顺序」由**显示层**给：区间选择（Shift）与「全选本片」都要按用户看到的顺序走。
   *
   * 为什么不让 store 自己算：分组、片内排序是**显示规则**（人类 2026-09-18 定），
   * 数据层不该知道它们。不传就退回查询顺序 —— 那样只有未分组时才是对的。
   */
  selectAll(order?: readonly string[]): void;
  clearSelection(): void;

  // ── 标记 ──
  markings(): ReadonlyMap<number, MarkingItem>;
  /** 读当前选中照片的标记（三态控件的输入）。 */
  refreshMarkings(): Promise<void>;
  /** 给选中的照片打标记；返回后端的结果。 */
  mark(action: MarkAction): Promise<MarkResult | null>;
  undo(): Promise<MarkResult | null>;
  redo(): Promise<MarkResult | null>;
  /**
   * **撤销栈的当前状态**（按钮的可用性与文案）。
   *
   * 后端每次动作都会回 `canUndo/canRedo` 与两个可读标签（「标 3 星」），
   * 前端只负责记住最后一次看到的快照 —— 不去猜栈里有什么。
   * 刚进库、还没做过任何动作时是全 false（此时栈里确实可能是空的，
   * 也可能是上次会话留下的，但我们不猜）。
   */
  undoState(): UndoState;
  /** 删掉选中的照片（回收站）。 */
  removeSelected(): Promise<DeleteResult | null>;

  // ── 旗标 ──
  picks(): ReadonlySet<number>;
  rejects(): ReadonlySet<number>;
  refreshFlags(): Promise<void>;
  setFlag(ids: readonly number[], flag: "pick" | "reject" | null): Promise<void>;
  clearFlags(): Promise<void>;
}


export function createBrowseStore(deps: BrowseDeps): BrowseStore {
  const { api } = deps;

  const [repositoryId, setRepositoryIdSignal] = createSignal<string | null>(null);
  const [scopePath, setScopePathSignal] = createSignal<string | null>(null);
  const [filter, setFilterSignal] = createSignal<BrowseFilter>({});
  const [filterMode, setFilterModeSignal] = createSignal(false);
  const [undoState, setUndoState] = createSignal<UndoState>(EMPTY_UNDO_STATE);

  /** 每次动作之后把撤销栈状态收下来（marking/undo/redo 三处都走它） */
  const rememberUndo = (result: MarkResult | null): MarkResult | null => {
    if (result !== null) {
      setUndoState({
        canUndo: result.canUndo,
        canRedo: result.canRedo,
        undoLabel: result.undoLabel,
        redoLabel: result.redoLabel,
      });
    }
    return result;
  };
  const [sort, setSortSignal] = createSignal<BrowseSort>({ key: "takenAt", desc: true });

  const [total, setTotal] = createSignal(0);
  const [entries, setEntries] = createSignal<readonly (AssetItem | null)[]>([]);
  const [timeline, setTimeline] = createSignal<readonly TimelineEntry[]>([]);
  const [facets, setFacets] = createSignal<BrowseFacets | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const [selection, setSelection] = createSignal<SelectionState>(EMPTY_SELECTION);
  const [markings, setMarkings] = createSignal<ReadonlyMap<number, MarkingItem>>(new Map());
  const [picks, setPicks] = createSignal<ReadonlySet<number>>(new Set<number>());
  const [rejects, setRejects] = createSignal<ReadonlySet<number>>(new Set<number>());

  /**
   * 代号：每换一次查询 +1。**迟到的响应靠它作废** ——
   * 拿到结果时代号对不上就直接扔掉（而不是去比对请求内容）。
   */
  let generation = 0;
  /** 正在飞的页（按代数分开记，换查询后自然失效）。 */
  let inFlight = new Set<number>();

  const query = (): BrowseQuery | null => {
    const id = repositoryId();
    if (id === null) return null;
    const scope = scopePath();
    // **没选目录就不查**（人类 2026-09-18 定的口径）：浏览的范围必须是库下的一个目录，
    // 点库本身不铺出整库照片 —— 后端把 `scopePath: null` 当「整个库」，所以这里根本不能发。
    if (scope === null) return null;
    return {
      repositoryId: id,
      scopePath: scope,
      filter: filter(),
      sort: sort(),
    };
  };

  /** 换查询之后的统一处理：作废在飞结果、清空数据与选择、重新加载。 */
  const resetForNewQuery = (): void => {
    generation += 1;
    inFlight = new Set();
    setEntries([]);
    setTotal(0);
    setTimeline([]);
    setFacets(null);
    setSelection(EMPTY_SELECTION);
    setMarkings(new Map());
  };

  const applyPage = (
    offset: number,
    items: readonly AssetItem[],
    totalCount: number,
  ): void => {
    setEntries((prev) => {
      // 长度可能变（筛选后总数变了）——按新总数重建，已加载的部分搬过去
      const next: (AssetItem | null)[] = new Array(totalCount).fill(null);
      for (let i = 0; i < prev.length && i < totalCount; i += 1) {
        next[i] = prev[i] ?? null;
      }
      for (let i = 0; i < items.length; i += 1) {
        const at = offset + i;
        if (at < totalCount) next[at] = items[i] ?? null;
      }
      return next;
    });
    setTotal(totalCount);
  };

  const reload = async (): Promise<void> => {
    const q = query();
    if (q === null) {
      resetForNewQuery();
      // 没选库/没选目录时别把「加载中」留在屏幕上（空态自己会说话）
      setLoading(false);
      setError(null);
      return;
    }
    resetForNewQuery();
    const mine = generation;
    setLoading(true);
    setError(null);
    try {
      // 首屏：第一页 + 时间线 + 分面。三者一起发，谁先回来谁先画。
      const [window, line, faces] = await Promise.all([
        api.page(q, 0, PAGE_SIZE),
        api.timeline(q, 0),
        api.facets(q),
      ]);
      if (mine !== generation) return; // 迟到的结果，丢掉
      applyPage(window.offset, window.items, window.total);
      setTimeline(line.entries);
      setFacets(faces);
    } catch (e) {
      if (mine !== generation) return;
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      if (mine === generation) setLoading(false);
    }
  };

  const ensureRange = async (start: number, end: number): Promise<void> => {
    const q = query();
    if (q === null || end <= start) return;
    const firstPage = Math.max(0, Math.floor(start / PAGE_SIZE));
    const lastPage = Math.max(0, Math.floor((end - 1) / PAGE_SIZE));
    const mine = generation;

    const want: number[] = [];
    for (let page = firstPage; page <= lastPage; page += 1) {
      const key = page;
      if (inFlight.has(key)) continue;
      const offset = page * PAGE_SIZE;
      const loaded = entries()[offset];
      if (loaded !== undefined && loaded !== null) continue;
      inFlight.add(key);
      want.push(page);
    }
    if (want.length === 0) return;

    // 一屏最多覆盖两三页，串行取反而更稳（也不会把读池打满）
    for (const page of want) {
      try {
        const window = await api.page(q, page * PAGE_SIZE, PAGE_SIZE);
        if (mine !== generation) return;
        applyPage(window.offset, window.items, window.total);
      } catch (e) {
        if (mine !== generation) return;
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        if (mine === generation) inFlight.delete(page);
      }
    }
  };

  /**
   * 选中/导航用的顺序：**调用方（显示层）给的优先**，不传就用查询顺序。
   *
   * 顺序本身是显示规则（分组 + 片内排序），所以 store 不自己算 —— 见接口里 `selectAll` 的说明。
   */
  const orderedIds = (order?: readonly string[]): string[] => {
    if (order !== undefined) return [...order];
    const ids: string[] = [];
    for (const item of entries()) {
      if (item !== null) ids.push(String(item.id));
    }
    return ids;
  };

  const selectedIds = (): number[] =>
    [...selection().ids].map((id) => Number(id)).filter((n) => Number.isFinite(n));

  /** 选中了哪些照片的**完整条目**（右栏与信息条要用；没加载到的跳过）。 */
  const selectedItems = (): AssetItem[] => {
    const ids = selection().ids;
    const out: AssetItem[] = [];
    for (const item of entries()) {
      if (item !== null && ids.has(String(item.id))) out.push(item);
    }
    return out;
  };

  const refreshMarkings = async (): Promise<void> => {
    const id = repositoryId();
    const ids = selectedIds();
    if (id === null || ids.length === 0) {
      setMarkings(new Map());
      return;
    }
    try {
      const mine = generation;
      const items = await api.markings(id, ids);
      if (mine !== generation) return;
      setMarkings(new Map(items.map((m) => [m.id, m])));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const mark = async (action: MarkAction): Promise<MarkResult | null> => {
    const id = repositoryId();
    const ids = selectedIds();
    if (id === null || ids.length === 0) return null;
    const result = await api.mark(id, ids, action);
    rememberUndo(result);
    // 打完标要把这批照片的新状态读回来（三态控件显示的就是它）
    await refreshMarkings();
    // 本地条目里的标记值也要跟着变（网格上的星点/色标就是这些字段）
    const map = markings();
    setEntries((prev) =>
      prev.map((item) => {
        if (item === null) return null;
        const fresh = map.get(item.id);
        if (fresh === undefined) return item;
        return {
          ...item,
          rating: fresh.rating,
          colorLabel: fresh.colorLabel,
          likeState: fresh.likeState,
          lockLevel: fresh.lockLevel,
        };
      }),
    );
    return result;
  };

  const refreshFlags = async (): Promise<void> => {
    const id = repositoryId();
    if (id === null) {
      setPicks(new Set<number>());
      setRejects(new Set<number>());
      return;
    }
    try {
      const view = await api.flagsGet(id);
      setPicks(new Set<number>(view.picks));
      setRejects(new Set<number>(view.rejects));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return {
    repositoryId,
    scopePath,
    filter,
    sort,
    filterMode,
    undoState,
    query,

    setRepository(id) {
      if (repositoryId() === id) return;
      setRepositoryIdSignal(id);
      // 换库：范围回到「还没选目录」—— 浏览范围一定是库下的某个目录（见 `query`）
      setScopePathSignal(null);
      void reload().then(() => refreshFlags());
    },
    setScope(path) {
      if (scopePath() === path) return;
      setScopePathSignal(path);
      void reload();
    },
    setFilter(next) {
      setFilterSignal(next);
      void reload();
    },
    patchFilter(patch) {
      setFilterSignal({ ...filter(), ...patch });
      void reload();
    },
    setSort(next) {
      setSortSignal(next);
      void reload();
    },
    setFilterMode(on) {
      setFilterModeSignal(on);
      if (on) return;
      // 关掉筛选：四组标记条件清干净；只有真的要清才重查（省一次往返）
      const had = conditionCount(filter()) > 0;
      setFilterSignal(clearMarkFilters(filter()));
      if (had) void reload();
    },

    total,
    itemAt(index) {
      return entries()[index] ?? null;
    },
    timeline,
    facets,
    loading,
    error,
    reload,
    ensureRange,

    selection,
    selectedIds,
    selectedCount: (order) => selectionCount(selection(), orderedIds(order)),
    selectedItems,
    anchorId: () => {
      const anchor = selection().anchor;
      return anchor === null ? null : Number(anchor);
    },
    /*
     * 锚点那张（多选时右栏显示它）—— **规则只此一处**：
     * 有锚点就用锚点，锚点不在选中集里（或没有锚点）就退回第一张；一张没选就是 null。
     *
     * 人类 2026-09-19：这条规则以前同时写在 browse 工作区与组装层，很容易走偏；
     * flowbar 的信息区（flowinfo）也要用它，所以下沉到这里 —— 两处都读同一个方法。
     */
    anchorItem: () => {
      const selected = selectedItems();
      if (selected.length === 0) return null;
      const anchor = selection().anchor;
      return selected.find((item) => String(item.id) === anchor) ?? selected[0] ?? null;
    },
    select(id, mode, order) {
      setSelection(
        applySelection(selection(), orderedIds(order), String(id), mode),
      );
      void refreshMarkings();
    },
    setAnchor(id) {
      const key = String(id);
      setSelection((current) =>
        current.ids.has(key) ? { ...current, anchor: key } : current,
      );
    },
    selectAll(order) {
      setSelection(selectAllIds(orderedIds(order)));
      void refreshMarkings();
    },
    clearSelection() {
      setSelection(clearSelection());
      void refreshMarkings();
    },

    markings,
    refreshMarkings,
    async mark(action) {
      return rememberUndo(await mark(action));
    },
    async undo() {
      const id = repositoryId();
      if (id === null) return null;
      const result = await api.undo(id);
      await reload(); // 撤销改的是库里的值，最稳的是重新取一遍
      return rememberUndo(result);
    },
    async redo() {
      const id = repositoryId();
      if (id === null) return null;
      const result = await api.redo(id);
      await reload();
      return rememberUndo(result);
    },
    async removeSelected() {
      const id = repositoryId();
      const ids = selectedIds();
      if (id === null || ids.length === 0) return null;
      const result = await api.remove(id, ids);
      setSelection(clearSelection());
      await reload();
      return result;
    },

    picks,
    rejects,
    refreshFlags,
    async setFlag(ids, flag) {
      const id = repositoryId();
      if (id === null) return;
      const view = await api.flagsSet(id, ids, flag);
      setPicks(new Set<number>(view.picks));
      setRejects(new Set<number>(view.rejects));
    },
    async clearFlags() {
      const id = repositoryId();
      if (id === null) return;
      const view = await api.flagsClear(id);
      setPicks(new Set<number>(view.picks));
      setRejects(new Set<number>(view.rejects));
    },
  };
}

/** 选择里有东西吗（外壳要据此启用/禁用工具按钮）。 */
export function hasSelected(store: BrowseStore): boolean {
  return hasSelection(store.selection());
}
