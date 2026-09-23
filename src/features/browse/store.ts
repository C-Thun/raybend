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
 * 另外两件事分得很清（人类 2026-09-22 报的「导入完了回浏览看不到新照片」）：
 * **查询变了**用 `reload`（清空重来、选择作废）；**查询没变但数据可能变了**
 * （每次进浏览）用 `refresh`（保留内容与选择，只把数据重读一遍）——
 * 判断依据是「这次重读是为了换查询，还是为了看最新的库」。
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
  EditableTextField,
  MarkAction,
  MarkResult,
  MarkingItem,
  MetaFile,
  PhotoMeta,
  Tag,
  TimelineEntry,
} from "../../api/types.ts";
import {
  applySelection,
  clearSelection,
  EMPTY_SELECTION,
  focusSelection,
  hasSelection,
  pruneSelection,
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
  /** 标签词典（右栏显示标签名用；标签弹窗自己另按搜索拉一份更长的） */
  tagList(query: string, limit: number): Promise<Tag[]>;
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
  /**
   * 补读几个文件的展示元信息（宽高）—— **只在库里没这两列时才用**。
   *
   * 为什么需要它：`assets.width/height` 是**导入时**写进去的，而 2026-09-18 之前的导入
   * 根本不写 EXIF（见 `store/backfill.rs` 的文件头），老库那批资产这两列是 NULL。
   * 于是浏览网格拿不到比例（tile 显示不对）、对比画幅报「还没读到这张的尺寸」——
   * 人类 2026-09-19 报的就是这个。补读是**兜底**（真正的修复是回填，见 `repository_backfill`）。
   */
  metaEnsure?: (dir: string, files: readonly MetaFile[]) => Promise<PhotoMeta[]>;
}

/** 撤销/重做按钮要的几个值（来自后端每次动作返回的 `MarkResult`） */
export interface UndoState {
  canUndo: boolean;
  canRedo: boolean;
  /** 「撤销：<label>」里的 label（后端给的可读动作名） */
  undoLabel: string | null;
  redoLabel: string | null;
}

/** 标签词典一次拉多少条（够右栏显示用；标签弹窗自己按搜索拉）。 */
const TAG_DICTIONARY_LIMIT = 200;

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
  /**
   * 按**资产 id** 取已加载的那一项（`null` = 还没加载到）。
   *
   * 看图/对比那几条路手上只有 id（缩略图队列、选择集合都是 id），
   * 要库内相对路径（拼绝对路径去读元信息）就得反查一次。
   */
  itemById(id: number): AssetItem | null;
  timeline(): readonly TimelineEntry[];
  facets(): BrowseFacets | null;
  loading(): boolean;
  error(): string | null;
  /** 重新加载（换库/换筛选后自动调；也可以手动调）。 */
  reload(): Promise<void>;
  /**
   * **同一个查询重读一遍**（`reload` 的「不清空」版）。
   *
   * 用在哪：**每次进浏览**（`BrowseWorkspace` 挂载时调）。数据可能被别的流程改掉了
   * —— 导入、重建、在程序外面换了文件 —— 而查询一个字都没变，`reload` 永远不会被触发。
   *
   * 它与 `reload` 的两条区别都是刻意的：
   *
   * 1. **不清空**已有数据：刷新期间界面继续显示上一份内容，直到新的一页回来
   *    （换查询才必须清空 —— 旧数据在新查询下是错的；同一个查询下它只是「可能过时」，
   *    比整片空白好）；选择同理，**刷新不该把用户的选中弄丢**（不在列表里的会被剔掉）；
   * 2. **第一页之外的页作废**：新照片会把后面的下标整体推移，留着旧页会在滚动时
   *    显示错人；它们会在用户滚到时按需重取。
   */
  refresh(): Promise<void>;
  /**
   * 标签词典（id → 名字）。**库里只存 tag id**（标记来自 catalog，名字来自 app.db），
   * 所以界面要显示标签名就得有一份词典 —— 收在这里，右栏与标签弹窗共用同一份，
   * 新建的标签当场就能在这两处显示。
   */
  tags(): readonly Tag[];
  /** 拉一次词典（幂等：已经拉到过就不再拉） */
  loadTags(): Promise<void>;
  /** 记一个刚建出来的标签（弹窗里「回车建新标签」之后调，免得右栏要等下次刷新） */
  rememberTag(tag: Tag): void;
  /**
   * 一张照片的**真实宽高**（tile 比例、看图缩放边界都要它）。
   *
   * 取法：先看补读缓存（老库那批），再退回数据库清单里的 `width/height`；
   * 都没有就是 `null`（界面按占位比例显示，不报错）。
   *
   * 注意它**不是**响应式的读取（内部按数组身份缓存索引）：调用方要拿到「补读之后」
   * 的新值，得等 `ensureNatural` 的 promise 结算、并且重渲染一次（信号 `extraNatural`
   * 会触发重渲染 —— 组件里读它的地方自然就更新了）。
   */
  naturalOf(id: number): { width: number; height: number } | null;
  /**
   * 按需补读这几张的宽高（**要绝对路径**，调用方知道库根）。
   *
   * 与导入侧 `PhotoGridStore.ensureNatural` 是同一件事、同一套纪律：
   * 只补缺的、**合并**进缓存（不清空已有的）、失败静默（读不到就按占位比例显示）。
   */
  ensureNatural(entries: readonly { id: number; path: string }[]): Promise<void>;
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
  /**
   * **撤销 / 重做发生过几次**（M3-W3）。
   *
   * 编辑器要靠它知道「库里的编辑栈可能被改回去了」—— 撤销会动 `develop_*` 表
   * （显影参数也是撤销栈里的一步），所以编辑器必须重新读一遍。
   * 只认「撤销 / 重做」这一件事，不跟着普通标记动作抖。
   */
  undoTick(): number;
  /** 删掉选中的照片（回收站）。 */
  removeSelected(): Promise<DeleteResult | null>;
  /**
   * 改右栏里可编辑的文字字段（作者 / 描述 / 地理四项）。
   *
   * 作用对象是**当前选中的那些照片**（批量也能改 —— 与打标同一条规矩）。
   * `value` 传空串 = 清空该字段。走的是 `browse_mark`，所以**可以撤销**。
   */
  setText(field: EditableTextField, value: string): Promise<MarkResult | null>;

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
  const [undoTick, setUndoTick] = createSignal(0);

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

  /**
   * 补读回来的真实宽高（按资产 id）。老库那批 `assets.width/height` 是 NULL，
   * 网格与对比都靠它兜底 —— 见 `BrowseDeps.metaEnsure` 的说明。
   */
  const [extraNatural, setExtraNatural] = createSignal<
    ReadonlyMap<number, { width: number; height: number }>
  >(new Map());

  /*
   * 已加载项按 id 索引（`naturalOf` 要按 id 取数据库那份宽高）。
   *
   * ⚠️ **故意不用 `createMemo`**：Node 里 `solid-js` 走的是 SSR 构建（没有响应式），
   * `createMemo` 只算一次 —— 那样索引会永远停在「空表」，测试里表现为
   * 「DB 里明明有宽高，`naturalOf` 却总返回 null」。
   * 改成**按数组身份**缓存的惰性索引：`setEntries` 每次都换新数组，所以身份一变就重建。
   */
  let indexedFrom: readonly (AssetItem | null)[] | undefined;
  let indexedItems = new Map<number, AssetItem>();
  const itemsById = (): Map<number, AssetItem> => {
    const list = entries();
    if (list !== indexedFrom) {
      const map = new Map<number, AssetItem>();
      for (const item of list) if (item !== null) map.set(item.id, item);
      indexedFrom = list;
      indexedItems = map;
    }
    return indexedItems;
  };

  const naturalOf = (id: number): { width: number; height: number } | null => {
    const extra = extraNatural().get(id);
    if (extra !== undefined) return extra;
    const item = itemsById().get(id);
    if (item === undefined || item.width === null || item.height === null) return null;
    if (item.width <= 0 || item.height <= 0) return null;
    return { width: item.width, height: item.height };
  };

  async function ensureNatural(entries: readonly { id: number; path: string }[]): Promise<void> {
    const load = deps.metaEnsure;
    if (load === undefined) return;
    const missing = entries.filter((entry) => naturalOf(entry.id) === null);
    if (missing.length === 0) return;

    // 按目录分组（同一个目录一次 IPC，与导入侧同一套做法）
    const byDir = new Map<string, { id: number; name: string }[]>();
    for (const entry of missing) {
      const cut = Math.max(entry.path.lastIndexOf("/"), entry.path.lastIndexOf("\\"));
      if (cut <= 0) continue;
      const dir = entry.path.slice(0, cut);
      const name = entry.path.slice(cut + 1);
      const list = byDir.get(dir);
      if (list === undefined) byDir.set(dir, [{ id: entry.id, name }]);
      else list.push({ id: entry.id, name });
    }
    if (byDir.size === 0) return;

    const found: [number, { width: number; height: number }][] = [];
    for (const [dir, wanted] of byDir) {
      const token = generation;
      try {
        const metas = await load(
          dir,
          wanted.map((item) => ({ relative: item.name, fileSize: 0, mtimeMs: 0 })),
        );
        if (token !== generation) return; // 换查询了，迟到的结果丢掉
        wanted.forEach((item, index) => {
          const meta = metas[index];
          if (meta !== undefined && meta.width > 0 && meta.height > 0) {
            found.push([item.id, { width: meta.width, height: meta.height }]);
          }
        });
      } catch {
        // 读不到不是错误：按占位比例显示（与导入侧同一条纪律）
      }
    }
    if (found.length === 0) return;
    setExtraNatural((prev) => {
      const merged = new Map(prev);
      for (const [id, size] of found) merged.set(id, size);
      return merged;
    });
  }

  const [selection, setSelection] = createSignal<SelectionState>(EMPTY_SELECTION);
  const [markings, setMarkings] = createSignal<ReadonlyMap<number, MarkingItem>>(new Map());
  /** 标签词典（`id → 名字`）。右栏与标签弹窗共用，见接口说明。 */
  const [tags, setTags] = createSignal<readonly Tag[]>([]);
  /** 词典拉过没有（避免每次渲染都发一趟 IPC） */
  let tagsLoaded = false;
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
      filter: withFlagIds(filter()),
      sort: sort(),
    };
  };

  /**
   * 把旗标筛选里的 `ids` 填上（界面只给 `mode`）。
   *
   * 为什么在这里填：旗标是**内存集合**（`picks` / `rejects`），只有 store 同时看得见
   * 「条件」与「集合」；让工具条自己去拼，就得把两边的同步责任散到界面上。
   */
  const withFlagIds = (current: BrowseFilter): BrowseFilter => {
    const mode = current.flag?.mode;
    if (mode === undefined || mode === null) return current;
    const ids =
      mode === "pick"
        ? [...picks()]
        : mode === "reject"
          ? [...rejects()]
          : // 「无旗标」要排除的是**有旗标的全部**（pick 与 reject 都算）
            [...new Set([...picks(), ...rejects()])];
    return { ...current, flag: { mode, ids } };
  };

  async function loadTags(): Promise<void> {
    if (tagsLoaded) return;
    tagsLoaded = true;
    try {
      const list = await api.tagList("", TAG_DICTIONARY_LIMIT);
      setTags(list);
    } catch {
      // 读不到词典不是错误：界面把标签名退回「#id」显示
      tagsLoaded = false;
    }
  }

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

  /**
   * 丢掉第一页之后的每一格（`refresh` 用）。
   *
   * 刷新后不能留旧页：新照片插进来会把后面的下标整体推移，旧页会在滚动时
   * 显示成另一张照片（比空着更糟）。它们会在用户滚到时由 `ensureRange` 按需重取 ——
   * 进浏览那一刻滚动条就在顶部，所以页 0 以外的内容本来也不在屏幕上。
   */
  const dropPagesAfterFirst = (): void => {
    setEntries((prev) => {
      if (prev.length <= PAGE_SIZE) return prev;
      return [
        ...prev.slice(0, PAGE_SIZE),
        ...new Array<AssetItem | null>(prev.length - PAGE_SIZE).fill(null),
      ];
    });
  };

  /**
   * 取「进目录首屏」的三件事：第一页 + 时间线 + 分面。
   *
   * `reload` 与 `refresh` 共用 —— 这一条路上就这么多事，两者只差**读之前/之后
   * 怎么处理已有数据**（清空 vs 保留），取数本身不该有两份。
   * 出错时只记错误并交回 `null`；代号对不上（有更新的请求在飞）也交回 `null`。
   */
  const loadFirstScreen = async (
    q: BrowseQuery,
    mine: number,
  ): Promise<readonly TimelineEntry[] | null> => {
    try {
      const [window, line, faces] = await Promise.all([
        api.page(q, 0, PAGE_SIZE),
        api.timeline(q, 0),
        api.facets(q),
      ]);
      if (mine !== generation) return null;
      applyPage(window.offset, window.items, window.total);
      setTimeline(line.entries);
      setFacets(faces);
      return line.entries;
    } catch (e) {
      if (mine === generation) setError(e instanceof Error ? e.message : String(e));
      return null;
    }
  };

  /**
   * **换查询**：清空重来（数据、选择、分面全部作废）。
   *
   * 只在库/目录/筛选/排序变了时用。数据可能变了但查询没变时用 [`refresh`]——
   * 那个不清空、也不弄丢选择。
   */
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
    await loadFirstScreen(q, mine);
    if (mine === generation) setLoading(false);
  };

  /**
   * **同一个查询重读一遍**（见接口里的 `refresh` 说明）。
   *
   * 结构上就是「不清空 + 把已有的页作废」：能这么短，正是因为不去猜「库变了没有」——
   * 猜法的代价（一套要维护的「什么算更新」判定）比多读这一趟贵得多。
   */
  const refresh = async (): Promise<void> => {
    const q = query();
    if (q === null) {
      // 没选库/没选目录时没什么可刷的：只把可能挂着的「加载中」收掉
      setLoading(false);
      return;
    }
    // 作废在飞的请求（它们属于这次刷新之前的同一份数据），但**不动**已经铺出来的内容
    generation += 1;
    inFlight = new Set();
    const mine = generation;
    setLoading(true);
    setError(null);

    const line = await loadFirstScreen(q, mine);
    if (mine !== generation) return;
    dropPagesAfterFirst();
    if (line !== null) {
      /*
       * 照片可能已经不在了（程序外面删的、别处导入后重排的）：把选择收敛到新时间线上。
       * 这是**刷新与重载的分界** —— 重载清空选择，刷新只剔掉真的不见了的那几张。
       */
      setSelection((current) =>
        pruneSelection(current, line.map((entry) => String(entry.id))),
      );
      // 选中那些照片的标记也重读一次（工具条的三态控件读的就是它）
      void refreshMarkings();
    }
    setLoading(false);
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
    undoTick,
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
    itemById: (id) => itemsById().get(id) ?? null,
    timeline,
    facets,
    loading,
    error,
    reload,
    refresh,
    ensureRange,
    tags,
    loadTags,
    rememberTag: (tag) => {
      setTags((prev) => (prev.some((item) => item.id === tag.id) ? prev : [...prev, tag]));
    },
    naturalOf,
    ensureNatural,

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
      setSelection((current) => focusSelection(current, String(id)));
    },
    selectAll(order) {
      /*
       * 全选 = 当前**范围**里的全部照片，**不只是已加载的那几页**
       *（人类 2026-09-20：「即使未显示的部分也要设置选中状态」）。
       *
       * 列表是**按页取**的（`PAGE_SIZE = 256`），所以 `entries()` 可能只有一部分；
       * 而 `timeline()` 是这个 scope 的**全量 id 清单**（首屏就取了），正好拿来用 ——
       * 不用为此再发一轮「把所有页取回来」的请求（十万张照片要四百次）。
       * 时间线还没到时退回已加载的顺序（至少不是空的）。
       */
      const fromTimeline = timeline().map((entry) => String(entry.id));
      const source = order ?? (fromTimeline.length > 0 ? fromTimeline : orderedIds());
      setSelection(selectAllIds(source));
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
      // 撤销可能动的是**编辑栈**（显影参数也是撤销栈里的一步）—— 通知编辑器重读
      setUndoTick((current) => current + 1);
      return rememberUndo(result);
    },
    async redo() {
      const id = repositoryId();
      if (id === null) return null;
      const result = await api.redo(id);
      await reload();
      setUndoTick((current) => current + 1);
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

    async setText(field, value) {
      return mark({ kind: "setText", field, value });
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
