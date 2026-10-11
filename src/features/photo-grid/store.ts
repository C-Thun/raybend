/**
 * 照片网格的状态（`features/photo-grid/` 自己的 store）。
 *
 * 它管三件事：**当前目录的照片清单**、**展示偏好**（档位 / 按时间）、
 * **选择**、以及**缩略图队列**。选择通过 store 的方法往上暴露给外壳的
 * `toolsbar`（由 `App.tsx` 接线），而不是塞进模块内部藏起来。
 *
 * ⚠️ **排除不在这里**（2026-09-16 修）：它曾经是这个 store 的一份 `Set`，
 * 于是换目录（`resetDirState`）时被一起清掉 —— 用户切一圈回来发现排除全丢了。
 * 排除是「跨目录、跨源」的事，现在住在导入工作区的 store 里（见 `workspaces/import/store.ts`），
 * 网格只负责**显示**（由 `PhotoGrid` 的 `isExcluded` 入参给）。
 *
 * 三条在实现里很容易做错、这里显式处理掉的：
 *
 * 1. **换目录要清干净**：选择与缩略图缓存都要重置 ——
 *    否则用户在新目录里会看到上一个目录的选中状态（也点不动）；
 * 2. **迟到的结果必须丢掉**：扫描慢、用户又换了目录时，旧结果不能覆盖新列表
 *    （用「代号」判断，与缩略图队列同一套做法）；
 * 3. **按时间要真读 EXIF**：列表阶段的时间只是文件名/mtime 兜底，
 *    用户按下「按时间」时才去读真相（否则进一个大目录要干等几秒）。
 *
 * ⚠️ 派生量一律写成**普通函数**，不用 `createMemo`：Node 里 `solid-js`
 * 走 SSR 构建，`createMemo` 只求值一次，测试会拿到永远不更新的假值
 * （见 `workspaces/import/store.ts` 里的同一段说明）。
 */

import { createSignal } from "solid-js";

import {
  commitImportDisplayTileStep,
  importDisplayByTime,
  importDisplayTileStep,
  setImportDisplayByTime,
  setImportDisplayTileStep,
} from "../../lib/display-prefs.ts";
import { withTimeout } from "../../lib/timeout.ts";
import { timeoutMessage } from "../../i18n/index.ts";
import {
  clampDisplayAspect,
} from "../../lib/tile-flow.ts";
import type {
  SettingsApi,
  SourceItem,
  SourceScan,
  ThumbSize,
} from "./types.ts";
import type { MetaFile, PhotoMeta, TakenAtSource } from "../../api/types.ts";
import { itemId } from "./rows.ts";
import type { LoadStatus } from "../../lib/load-status.ts";
import {
  applySelection,
  EMPTY_SELECTION,
  focusSelection,
  hasSelection as anySelected,
  selectAll as selectAllIds,
  selectionCount,
  toggleGroupSelection,
  type SelectionState,
} from "../../lib/selection.ts";
import { dayKeyOf, groupByTime, type TimeDay, type TimeGrouping } from "../../lib/time-group.ts";
import { clampTileStepIndex } from "../../lib/tile-flow.ts";
import {
  createThumbQueue,
  type ThumbEntry,
  type ThumbQueue,
} from "../../components/ui/thumb-queue.ts";

/** 时间片阈值的默认值（分钟）——`memory/DESIGN.md` §12.7 的「1 小时」 */
export const DEFAULT_GAP_MINUTES = 60;

/** view/compare 按需读原片尺寸的等待上限；tiles 不等待元数据。 */
export const DEFAULT_META_TIMEOUT_MS = 20_000;

/**
 * 仍在 `app.db` 里的网格设置键（与 `src/api/db.ts` 的 `SETTING_KEYS` 保持一致）。
 *
 * ⚠️ `tile_step` / `by_time` **已经搬走**（`lib/display-prefs.ts`，localStorage）：
 * 那两个是「怎么看」的设备级偏好；import / browse 共用一套持久化框架，
 * 但各自保存一份值。以前只有导入侧读写它们，浏览侧每次进都重置。
 * 时间间隔阈值留在库里（它是**库内数据**的解释参数，不是显示偏好）。
 */
export const GRID_SETTING_KEYS = {
  gapMinutes: "grid.time_gap_minutes",
} as const;

export interface PhotoGridApi extends SettingsApi {
  scanSourceDir: (path: string) => Promise<SourceScan>;
  dirMetaEnsure: (
    dir: string,
    files: readonly MetaFile[],
    cacheOnly?: boolean,
  ) => Promise<PhotoMeta[]>;
  getThumbBytes: (path: string, size?: ThumbSize) => Promise<Uint8Array | null>;
  /**
   * 忘掉一个目录的元信息缓存（拔盘 / 卸载时用）。
   *
   * 缓存**按目录分开存**；目录卸载（拔盘、或离开后重新挂载）之后就该当它没发生过 ——
   * 下次进来重新读一遍。不实现也照样工作（那是可选优化口子）。
   */
  dirMetaForget?: (dir: string) => Promise<void>;
}

export interface PhotoGridDeps {
  api: PhotoGridApi;
  /** 缩略图下载并发（默认 4，见 `thumbnails.ts`） */
  thumbConcurrency?: number;
  /** 等头部缓存的时限（默认 {@link DEFAULT_META_TIMEOUT_MS}；测试用小值） */
  metaTimeoutMs?: number;
}

export interface PhotoGridStore {
  /* ── 数据 ─────────────────────────────── */
  dir: () => string | null;
  items: () => readonly SourceItem[];
  /** 按**显示顺序**排好的照片（按时间模式下跟着分组走） */
  displayItems: () => readonly SourceItem[];
  /**
   * 某张照片的**展示用宽高比**（已应用方向、已按 3:1 夹取）。
   * 元信息还没到时返回默认占位比例 —— 界面据此决定照片在正方外框里长什么样。
   */
  aspectOf: (id: string) => number;
  /** 元数据里的已换算宽高（给看图用：尺寸未知时拖动会被夹死，见 viewer/store 的 clampPan） */
  naturalOf: (id: string) => { width: number; height: number } | null;
  /**
   * 按需补读**指定文件**的宽高（**合并**进现有元数据，不清空）。
   *
   * 人类 2026-09-19 报的「import 里一进对比就被拉伸、拖动也不对」就是缺这一步：
   * 网格只为**可见的** tile 读了元数据，视口外那些 `naturalOf` 是 `null`，
   * 于是对比的基准比例（`baselineAspect`）算不出来，画幅退回窗口比例；
   * 拖动也用错了尺寸。看图片先调一次它，尺寸就齐了。
   */
  ensureNatural: (paths: readonly string[]) => Promise<void>;
  status: () => LoadStatus;
  error: () => string | null;
  /** 扫描时读不了的位置（不致命） */
  problems: () => readonly string[];
  /** 切换到某个来源目录（`null` = 清空） */
  setSourceDir: (dir: string | null) => void;
  reload: () => void;
  refreshSource: (available: boolean) => void;

  /* ── 展示偏好 ─────────────────────────── */
  tileStep: () => number;
  setTileStep: (step: number) => void;
  /** 把当前 tile 档位落盘（拖拽结束时调一次） */
  commitTileStep: () => void;
  byTime: () => boolean;
  setByTime: (value: boolean) => void;
  gapMinutes: () => number;
  /** 分组结果（没按时间 / 还没读到时间时是 `undefined`） */
  grouping: () => TimeGrouping | undefined;
  /** 是否正在补读真实拍摄时间 */
  loadingTimes: () => boolean;
  /** 从设置里读回偏好（工作区挂载时调一次） */
  hydrate: () => Promise<void>;

  /* ── 选择 ─────────────────────────────── */
  selection: () => SelectionState;
  selectedIds: () => ReadonlySet<string>;
  hasSelection: () => boolean;
  /** 选中的张数（只算还在当前列表里的） */
  selectedCount: () => number;
  clickItem: (id: string, mode: "replace" | "toggle" | "range") => void;
  /** 只挪当前锚点，不改变多选集合。 */
  setAnchor: (id: string) => void;
  /**
   * 日 / 时间片那颗药丸的**整段开关**（全选中 → 全取消；否则 → 全选中）。
   * 语义与口径在 `lib/selection.ts::toggleGroupSelection`，这里只是转发。
   */
  toggleGroup: (ids: readonly string[]) => void;
  selectAll: () => void;
  clearSelection: () => void;

  /* ── 缩略图 ───────────────────────────── */
  /** 网格与胶片带共用的同一条队列。 */
  thumbQueue: ThumbQueue;
  thumb: (path: string) => ThumbEntry;
  requestThumb: (path: string, priority?: boolean) => void;
  /**
   * 窗口变了（网格滚停后调）：缩略图排队与**元信息补读**都按它排在最前。
   * 崔总 2026-10-11 的窗口优先口径（`AGENTS.md` §2.24）。
   */
  noteVisible: (ids: readonly string[]) => void;
}

export function createPhotoGridStore(deps: PhotoGridDeps): PhotoGridStore {
  const [dir, setDir] = createSignal<string | null>(null);
  const [items, setItems] = createSignal<readonly SourceItem[]>([]);
  const [status, setStatus] = createSignal<LoadStatus>("idle");
  const [error, setError] = createSignal<string | null>(null);
  const [problems, setProblems] = createSignal<readonly string[]>([]);

  /*
   * 格子尺寸与「按时间」走 import 自己的设备级偏好（`lib/display-prefs.ts`）：
   * 与 browse 复用同一套迁移/持久化实现，但值互相隔离并可跨会话还原。
   */
  const tileStep = importDisplayTileStep;
  const byTime = importDisplayByTime;
  const [gapMinutes, setGapMinutes] = createSignal(DEFAULT_GAP_MINUTES);
  const [loadingTimes, setLoadingTimes] = createSignal(false);

  const [selection, setSelection] = createSignal<SelectionState>(EMPTY_SELECTION);

  /** 换目录 / 重新扫描时推进它，迟到的结果直接丢掉 */
  let generation = 0;

  /** 等头部缓存的时限（依赖注入，测试里给小值） */
  const metaTimeout = deps.metaTimeoutMs ?? DEFAULT_META_TIMEOUT_MS;

  const thumbs: ThumbQueue = createThumbQueue({
    load: (path) => deps.api.getThumbBytes(path, "grid"),
    /*
     * **固定 2 个消费者**（崔总 2026-10-11）：瓶颈在磁盘不在计算；在飞多了只会
     * 把 WebView 主线程淹掉（一次 8 个响应一起回来时，滚动输入根本轮不到处理）。
     */
    concurrency: deps.thumbConcurrency ?? 2,
  });

  /* ══════════════════════════════════════════════════════════
   * 时间分组：按天维护、**局部增删**（崔总 2026-10-11）
   *
   * 一张照片的时间变化只会导致三件事：
   *   ① 需要就先**建一个新的日期分类**；
   *   ② 把它从**原来的分类里移除**（空了就把这个空分类删掉）；
   *   ③ 把它**加入新的分类**（按拍摄时间插到那一天的正确位置）。
   * 绝不再「收到一批 → 整表重排 → 整树重建」—— 那正是界面一卡一卡的来源。
   * ══════════════════════════════════════════════════════════ */

  /** 未知时间组的哨兵（日期键是 `YYYY-MM-DD`，不会撞上） */
  const UNKNOWN_DAY = "unknown";

  const [dayIds, setDayIds] = createSignal<Readonly<Record<string, readonly string[]>>>({});
  const [dayOrder, setDayOrder] = createSignal<readonly string[]>([]);
  const [unknownIds, setUnknownIds] = createSignal<readonly string[]>([]);
  /** 结构变了就 +1：分组与显示顺序的缓存据此重算（不是每读一次都重算） */
  const [groupingTick, setGroupingTick] = createSignal(0);
  const bumpGrouping = (): void => {
    setGroupingTick((n) => n + 1);
  };

  /** 每张现在落在哪一天、现在的真实时间（「只升不降」的判据在这儿，**不动 `items`**） */
  const dayOf = new Map<string, string>();
  const timeOf = new Map<string, { ms: number | null; source: TakenAtSource | null; offset: number | null }>();

  const dayKeyFor = (ms: number | null, offset: number | null | undefined): string =>
    dayKeyOf(ms, offset) ?? UNKNOWN_DAY;

  /** 按拍摄时间把 id 插进当天的列表（同一时刻按路径定序，与 `groupByTime` 的口径一致） */
  const insertByTime = (ids: readonly string[], id: string): string[] =>
    [...ids, id].sort((one, other) => {
      const a = timeOf.get(one)?.ms ?? 0;
      const b = timeOf.get(other)?.ms ?? 0;
      if (a !== b) return a - b;
      const byId = itemById();
      const an = byId.get(one)?.path ?? one;
      const bn = byId.get(other)?.path ?? other;
      return an < bn ? -1 : an > bn ? 1 : 0;
    });

  /** 换目录：从清单层重建结构（此刻是兜底时间；真相随后一张张补进来） */
  const resetGrouping = (list: readonly SourceItem[]): void => {
    dayOf.clear();
    timeOf.clear();
    const buckets = new Map<string, string[]>();
    for (const item of list) {
      const id = itemId(item);
      const key = dayKeyFor(item.takenAtMs, item.takenAtOffsetMin);
      timeOf.set(id, { ms: item.takenAtMs, source: item.takenAtSource, offset: item.takenAtOffsetMin });
      dayOf.set(id, key);
      const bucket = buckets.get(key) ?? [];
      bucket.push(id);
      buckets.set(key, bucket);
    }
    const days: Record<string, readonly string[]> = {};
    for (const [key, ids] of buckets) {
      if (key === UNKNOWN_DAY) continue;
      days[key] = [...ids].sort((one, other) => (timeOf.get(one)?.ms ?? 0) - (timeOf.get(other)?.ms ?? 0));
    }
    setUnknownIds(buckets.get(UNKNOWN_DAY) ?? []);
    setDayIds(days);
    setDayOrder(Object.keys(days).sort((a, b) => (a < b ? -1 : 1)));
    bumpGrouping();
  };

  /**
   * 一张的时间更新 → **只做那三件事**；没变化就什么都不做。
   * 返回 true 表示结构真的动了。
   */
  const placeTime = (
    id: string,
    ms: number | null,
    source: TakenAtSource | null,
    offset: number | null,
  ): boolean => {
    const key = dayKeyFor(ms, offset);
    const previous = dayOf.get(id);
    timeOf.set(id, { ms, source, offset });
    if (previous === key) return false;

    // ② 从旧分类移除（空了就删掉这个分类）
    if (previous !== undefined) {
      if (previous === UNKNOWN_DAY) {
        setUnknownIds((ids) => ids.filter((item) => item !== id));
      } else {
        const days = { ...dayIds() };
        const rest = (days[previous] ?? []).filter((item) => item !== id);
        if (rest.length === 0) {
          delete days[previous];
          setDayOrder((order) => order.filter((day) => day !== previous));
        } else {
          days[previous] = rest;
        }
        setDayIds(days);
      }
    }

    // ① 需要就建新分类（日期倒序插进去）　③ 加入
    if (key === UNKNOWN_DAY) {
      setUnknownIds((ids) => [...ids, id]);
    } else {
      const days = { ...dayIds() };
      days[key] = insertByTime(days[key] ?? [], id);
      setDayIds(days);
      if (!dayOrder().includes(key)) {
        // 新分类插进**升序**的位置（与清单层初次建树同一方向）
        setDayOrder((order) => [...order, key].sort((a, b) => (a < b ? -1 : 1)));
      }
    }
    dayOf.set(id, key);
    bumpGrouping();
    return true;
  };

  /* ══════════════════════════════════════════════════════════
   * 展示元信息（宽高 + 方向）
   *
   * 它决定每张照片在正方外框里长什么样。来源是 Rust 侧的**会话级内存缓存**
   * （`dir_meta_ensure`）—— 相机五花八门，M43 是 4:3、竖拍是 3:4，
   * 不读方向的话所有竖图都会躺着显示（2026-09-16 人类报的现象）。
   *
   * 拿不到不是错误：界面按默认占位比例显示，照片只是「长得不精确」而已。
   * ══════════════════════════════════════════════════════════ */

  const [photoMeta, setPhotoMeta] = createSignal<ReadonlyMap<string, PhotoMeta>>(
    new Map(),
  );

  /** 这条路径的展示比例（没读到元信息时给默认占位比例） */
  const aspectOf = (id: string): number => {
    const meta = photoMeta().get(id);
    if (meta === undefined) {
      const thumb = thumbs.get(id);
      return clampDisplayAspect(thumb.width ?? 0, thumb.height ?? 0);
    }
    return clampDisplayAspect(meta.width, meta.height);
  };

  /**
   * 元数据里的已换算宽高 —— 给看图用。
   *
   * 与 [`aspectOf`] 的区别：那个是**展示用**的（夹到 3:1 之内，只用于排版），
   * 这个是**真实尺寸**（拖动边界要拿它算，夹过就不准了）。
   * 拿不到（元信息还没到）就返回 `null`，让看图侧自己退到旧行为。
   */
  const naturalOf = (id: string): { width: number; height: number } | null => {
    const meta = photoMeta().get(id);
    if (meta === undefined || meta.width <= 0 || meta.height <= 0) return null;
    return { width: meta.width, height: meta.height };
  };

  /**
   * 上一次看到的**文件身份**（大小 + 修改时间），按路径记。
   *
   * 为什么要有它：缩略图队列按**路径**缓存 blob，而 import 面对的是临时目录 ——
   * 换了一张卡、或文件被重新导出覆盖之后，同一个路径下已经是**另一张照片**了。
   * 重新进目录时拿扫描带回的身份比一比，变了的就把那条作废（`thumbs.refresh`）：
   * 少了这一下，用户会看到上一张卡的照片（崔总 2026-10-11 点出的场景）。
   *
   * 只增不删（一条几十字节）、跟着进程活 —— 与缩略图缓存同一生命周期。
   */
  const seenStamps = new Map<string, string>();

  /**
   * 清单到手后：**身份变了的文件作废它的小图**（换卡 / 覆盖保存 / 重新导出）。
   *
   * 与 Rust 侧那份内存缓存的判据一致：**大小 + mtime**（`thumbnail::memory`）。
   * 队列里那条会回到 `idle`，可见的格子自己会重新要一次。
   */
  function invalidateChangedThumbs(items: readonly SourceItem[]): void {
    for (const item of items) {
      const stamp = `${item.sizeBytes}:${item.mtimeMs ?? 0}`;
      const previous = seenStamps.get(item.path);
      if (previous !== undefined && previous !== stamp) thumbs.refresh(item.path);
      seenStamps.set(item.path, stamp);
    }
  }

  /** 目录切换时把旧元信息丢掉（不同目录的文件名可能一样） */
  function clearPhotoMeta(): void {
    stopTimePump();
    setPhotoMeta(new Map());
  }

  /* ══════════════════════════════════════════════════════════
   * 数据加载
   * ══════════════════════════════════════════════════════════ */

  const orderedIds = (): string[] => displayItems().map(itemId);

  function resetDirState(next: string | null): void {
    generation += 1;
    setDir(next);
    setItems([]);
    clearPhotoMeta();
    setProblems([]);
    setError(null);
    setSelection(EMPTY_SELECTION);
    /*
     * ⚠️ **不再 `thumbs.clear()`**（崔总 2026-10-11）：
     * 换目录不该把已经出好的缩略图丢掉 —— 用户 A→B→A 来回切时，回到 A 应当是
     * **瞬间出图**（内存里就有 blob URL），而不是又叠一遍队列。
     * 腾内存靠队列自己的 LRU（`maxEntries`，600 条封顶）；
     * **真要失效的只有两处**：`reload()` 与 `refreshSource()`（磁盘事实变了）。
     */
  }

  async function load(next: string): Promise<void> {
    const token = generation;
    setStatus("loading");
    setError(null);
    try {
      const scan = await deps.api.scanSourceDir(next);
      if (token !== generation) return; // 用户又换了目录
      // 清单一到手就展示；首次小图携带方向和显示尺寸，不等全目录读头。
      setItems(scan.items);
      // 用**清单层的兜底时间**（文件名 → min(创建, 修改)）立刻把分组框架搭出来
      resetGrouping(scan.items);
      // 同一个路径换了内容（换卡 / 覆盖保存）→ 作废它的小图，别显示上一张
      invalidateChangedThumbs(scan.items);
      /*
       * 先用**缓存**把框架补到最真（零 I/O），再把**缓存里没有的**推进队列
       *（崔总 2026-10-11：「把未处理的推进队列，而不是全推」）：
       * 队列从此不停吃（固定 2 个消费者），滚屏只把窗口内的提到队首。
       * 每张出图时后端把这一张的信息（时间 + 整块 EXIF）顺手放回缓存，
       * 界面据此**局部**重排 —— 这就是「随构建实时重排」。
       */
      requestTimes(scan.items.map(itemId));
      startTimePump();
      setProblems(scan.problems);
      // 列目录不读取照片内容；小图带回显示比例，view/compare 才补原片尺寸。
      setStatus("ready");
      if (byTime()) requestTimes(items().map(itemId));
    } catch (caught) {
      if (token !== generation) return;
      setError(message(caught));
      setStatus("error");
    }
  }

  /** view/compare 按需补原片尺寸，不能用缩略图尺寸替代。 */
  async function ensureNatural(paths: readonly string[]): Promise<void> {
    const current = dir();
    if (current === null || paths.length === 0) return;
    const missing = paths.filter((path) => naturalOf(path) === null);
    if (missing.length === 0) return;

    const byPath = new Map(items().map((item) => [itemId(item), item]));
    const files = missing.flatMap((path) => {
      const item = byPath.get(path);
      return item === undefined
        ? []
        : [
            {
              relative: item.fileName,
              fileSize: item.sizeBytes,
              mtimeMs: item.mtimeMs ?? 0,
            },
          ];
    });
    if (files.length === 0) return;

    const token = generation;
    try {
      const metas = await withTimeout(deps.api.dirMetaEnsure(current, files), metaTimeout,
        timeoutMessage("grid.timeout.meta", metaTimeout));
      if (token !== generation) return;
      setPhotoMeta((prev) => {
        const merged = new Map(prev);
        files.forEach((file, index) => {
          const path = items().find(item => item.fileName === file.relative)?.path;
          if (path === undefined) return;
          const meta = metas[index];
          if (meta !== undefined) merged.set(path, meta);
        });
        return merged;
      });
    } catch {
      // 读不到不是错误：按占位比例显示
    }
  }

  /*
   * ── 拍摄时间：跟着「读文件信息」一起补，不再有单独的命令 ──
   *
   * 崔总 2026-10-11 口径（规格 `specs/source-time-inline.md`）：
   *   * 进目录先用**兜底时间**（文件名 / mtime，`scan` 就带回来了）建分组框架——瞬间完成；
   *   * 真实 EXIF 时间由 `dir_meta_ensure` **顺路带回**（读宽高/方向的那次 EXIF 读本来就拿着它），
   *     而且随文件身份（size+mtime）进 Rust 侧会话缓存——切回来直接命中；
   *   * 分块补、每块让出一帧：**任何一条命令都是短的**。
   *     以前是 `source_times` 一次读整目录（`TIMES_BATCH = 512`），真机 306 张要 6 秒，
   *     把那 6 秒里所有缩略图请求和整个界面都按住了（诊断：
   *     `implementations/2026-10-11_import-still-slow-diagnosis.md`）。
   */
  const META_CHUNK = 48;

  /**
   * 来源可信度分级：**只升不降**（与 Rust 的 `media::exif::source_rank` 同一套次序）。
   * 缓存没建完时先拿兜底顶上，真值到了再覆盖；反过来绝不允许。
   */
  const SOURCE_RANK: Record<TakenAtSource, number> = {
    exif: 4,
    sibling: 3,
    filename: 2,
    file_created: 1,
    file_mtime: 0,
  };
  const rankOf = (source: TakenAtSource | null): number =>
    source === null ? -1 : SOURCE_RANK[source] ?? -1;

  /** id → 条目（按需重建；几百条的量级，不值得常驻一张表） */
  const itemById = (): Map<string, SourceItem> =>
    new Map(items().map((item) => [itemId(item), item]));

  /**
   * 把一批「缓存里的信息」落进时间结构（**只做三件事**，见本节开头）。
   *
   * 规则：**只升不降**（EXIF > 姊妹继承 > 文件名 > 创建/mtime）。
   * 缓存里没有真值（`takenAtMs === null`）时什么都不做 —— 清单层的兜底时间就够好。
   */
  const applyTimes = (ids: readonly string[], metas: readonly PhotoMeta[]): void => {
    if (metas.length === 0) return;
    const byRelative = new Map(metas.map((meta) => [meta.relative, meta]));
    const byId = itemById();
    for (const id of ids) {
      const item = byId.get(id);
      const meta = item === undefined ? undefined : byRelative.get(item.fileName);
      if (meta === undefined || meta.takenAtMs === null) continue;
      const current = timeOf.get(id);
      if (current !== undefined && current.ms !== null) {
        if (rankOf(meta.takenAtSource) <= rankOf(current.source)) continue;
      }
      placeTime(id, meta.takenAtMs, meta.takenAtSource, meta.takenAtOffsetMin ?? null);
    }
  };

  /** 待取信息的 id（`requestTimes` 攒着，一个消费者串行地查缓存） */
  let timeQueue: string[] = [];
  const timeQueued = new Set<string>();
  let timeFlight: Promise<void> | null = null;

  /**
   * 向**缓存**要这些图的信息（零 I/O）：命中即回，没命中的如实报「不知道」。
   * 返回**缓存里没有**的那些 id（调用方拿它决定谁要进队列去出图）。
   */
  async function syncTimes(paths: readonly string[]): Promise<string[]> {
    const token = generation;
    const current = dir();
    const missing: string[] = [];
    if (current === null || paths.length === 0) return missing;

    setLoadingTimes(true);
    try {
      for (let start = 0; start < paths.length; ) {
        if (token !== generation || !byTime()) return missing;
        /*
         * 切块不许把一对 JPG/RAW 拆开：后端要在同一批里做「RAW 借同名位图的时间」
         *（配对继承，规则在 `media::pairing`）。
         */
        let end = Math.min(start + META_CHUNK, paths.length);
        while (end < paths.length && sameStem(paths[end - 1] ?? "", paths[end] ?? "")) end += 1;
        const slice = paths.slice(start, end);
        start = end;

        const byId = itemById();
        const files = slice.flatMap((id) => {
          const item = byId.get(id);
          return item === undefined
            ? []
            : [
                {
                  relative: item.fileName,
                  fileSize: item.sizeBytes,
                  mtimeMs: item.mtimeMs ?? 0,
                },
              ];
        });
        if (files.length === 0) continue;
        const metas = await deps.api
          .dirMetaEnsure(current, files, /* cacheOnly */ true)
          .catch(() => [] as PhotoMeta[]);
        if (token !== generation) return missing;
        applyTimes(slice, metas);
        const known = new Set(
          metas.filter((meta) => meta.takenAtMs !== null).map((meta) => meta.relative),
        );
        for (const id of slice) {
          const item = byId.get(id);
          if (item === undefined || !known.has(item.fileName)) missing.push(id);
        }
        // 让出一帧，别把主线程连着占住
        await new Promise<void>((resolve) => setTimeout(resolve, 0));
      }
    } finally {
      if (token === generation) setLoadingTimes(false);
    }
    return missing;
  }

  /** 请求「把这些图的信息取回来」（攒批、串行；同一张不重复排队） */
  function requestTimes(paths: readonly string[]): void {
    if (!byTime() || paths.length === 0) return;
    for (const path of paths) {
      // 已经是**真相**（EXIF）的不用再问；兜底的那些才值得查一次缓存
      if (timeOf.get(path)?.source === "exif") continue;
      if (timeQueued.has(path)) continue;
      timeQueued.add(path);
      timeQueue.push(path);
    }
    if (timeQueue.length === 0) return;
    if (timeFlight !== null) return;
    timeFlight = (async () => {
      while (timeQueue.length > 0) {
        // 关掉「按时间」就停：不再更新（出图照旧，缓存照旧在攒）
        if (!byTime()) {
          timeQueue = [];
          timeQueued.clear();
          break;
        }
        const batch = timeQueue;
        timeQueue = [];
        timeQueued.clear();
        const missing = await syncTimes(batch);
        // 缓存里没有的：交给缩略图队列去出图 —— 出图那一次读会顺手把信息放回缓存
        if (missing.length > 0) thumbs.enqueue(missing);
      }
    })().finally(() => {
      timeFlight = null;
    });
  }

  /**
   * 「随构建实时重排」的节拍：每 300ms 把刚出图那几张的信息收回来（**只查缓存，零 I/O**）。
   *
   * 只在浏览器里起（Node 测试没有 `window`；用例直接调 `collectFromRenders` 验行为）。
   */
  let timePump: ReturnType<typeof setInterval> | undefined;
  function startTimePump(): void {
    if (typeof window === "undefined" || timePump !== undefined) return;
    timePump = setInterval(collectFromRenders, 300);
  }
  function stopTimePump(): void {
    if (timePump === undefined) return;
    clearInterval(timePump);
    timePump = undefined;
  }

  /** 刚出图的那几张：后端已经把信息放进缓存了 → 取回来、局部重排（界面本身零 I/O） */
  function collectFromRenders(): void {
    if (!byTime()) return;
    const fresh = thumbs.drainReady();
    if (fresh.length > 0) requestTimes(fresh);
  }

  const setSourceDir = (next: string | null): void => {
    if (next === dir()) return;
    resetDirState(next);
    if (next === null) {
      setStatus("idle");
      return;
    }
    void load(next);
  };

  let refreshFlight: Promise<void> | null = null;
  let refreshAgain = false;
  const refreshSource = (available: boolean): void => {
    const current = dir();
    if (current === null) return;
    generation += 1;
    setLoadingTimes(false);
    setError(null);
    refreshAgain = available;
    if (!available) {
      // 拔盘 = 卸载：这个目录的缓存（时间/EXIF）不该再当真相用，忘掉它
      void deps.api.dirMetaForget?.(current);
      timeQueue = [];
      timeQueued.clear();
      setStatus(items().length ? "ready" : "idle");
      return;
    }
    if (refreshFlight) return;
    thumbs.clear();
    refreshFlight = (async () => {
      do { refreshAgain = false; await load(current); }
      while (refreshAgain && dir() === current);
    })().finally(() => { refreshFlight = null; });
  };

  const reload = (): void => {
    const current = dir();
    if (current === null) return;
    generation += 1;
    thumbs.clear();
    setSelection(EMPTY_SELECTION);
    void load(current);
  };

  /* ══════════════════════════════════════════════════════════
   * 展示偏好
   * ══════════════════════════════════════════════════════════ */

  /**
   * 改 tile 尺寸档位。**只改内存，不落盘** ——
   * 拖动滑块时每动一格都写一次存储，一拖就是几百次同步写，手感直接卡住
   *（2026-09-16 人类反馈「尺寸调节非常卡」）。落盘走 `commitTileStep`，
   * 由滑块在**拖拽结束**时调一次。
   */
  const setTileStep = (step: number): void => {
    setImportDisplayTileStep(clampTileStepIndex(step));
  };

  /** 把当前档位落盘（滑块拖拽结束时调一次） */
  const commitTileStep = (): void => {
    commitImportDisplayTileStep();
  };

  /**
   * 网格滚停后告诉我们「现在窗口里是哪几张」（`PhotoGrid` 的窗口优先 0.3s 防抖之后调）。
   * 缩略图排队用它，元信息补读也用它（`AGENTS.md` §2.24：窗口优先是第一优先）。
   */
  const noteVisible = (_ids: readonly string[]): void => {
    // 滚停之后：把刚出图的那些的信息收回来（零 I/O，只查缓存）—— 参数不用，
    // 因为查询不挑顺序、队列也不靠它驱动（队列一直在吃）。
    collectFromRenders();
  };

  /** 「按时间」开关：立刻落盘（一次点击就是一个终值），并按需补读拍摄时间 */
  const setByTime = (value: boolean): void => {
    if (value === byTime()) return;
    setImportDisplayByTime(value);
    if (value) {
      requestTimes(items().map(itemId));
      startTimePump();
    } else {
      bumpGrouping();
      setLoadingTimes(false);
    }
  };

  let groupingCache:
    | { tick: number; gap: number; value: TimeGrouping | undefined }
    | undefined;
  let displayCache: { tick: number; gap: number; value: readonly SourceItem[] } | undefined;

  /**
   * 分组结果（按时间模式下）。
   *
   * 结构是**增量维护**的（见本节开头），这里只负责按天拼装 + 缓存：
   * 结构没变就复用上一次的结果 —— 不再「每读一次就把 306 张重排一遍」。
   */
  const grouping = (): TimeGrouping | undefined => {
    if (!byTime()) return undefined;
    const tick = groupingTick();
    const gap = gapMinutes();
    if (groupingCache !== undefined && groupingCache.tick === tick && groupingCache.gap === gap) {
      return groupingCache.value;
    }

    const byId = itemById();
    const days: TimeDay[] = [];
    for (const key of dayOrder()) {
      const photos = (dayIds()[key] ?? []).flatMap((id) => {
        const item = byId.get(id);
        const time = timeOf.get(id);
        if (item === undefined || time === undefined || time.ms === null) return [];
        return [
          {
            id,
            takenAtMs: time.ms,
            offsetMinutes: time.offset,
            // 片内按文件名自然序 —— 用路径（含目录）比只比文件名更确定
            name: item.path,
          },
        ];
      });
      if (photos.length === 0) continue;
      // **一天一天地分组**（复用同一套规则）：只有这一天要算，别的一天不动
      days.push(...groupByTime(photos, { gapMinutes: gap }).days);
    }

    const unknownPhotos = unknownIds().flatMap((id) => {
      const item = byId.get(id);
      return item === undefined ? [] : [{ id, takenAtMs: null, offsetMinutes: null, name: item.path }];
    });
    const unknown =
      unknownPhotos.length === 0
        ? null
        : (groupByTime(unknownPhotos, { gapMinutes: gap }).unknown ?? null);

    const value: TimeGrouping = { days, unknown };
    groupingCache = { tick, gap, value };
    return value;
  };

  /** 显示顺序：平铺 = 扫描顺序；按时间 = 日倒序 → 片升序 → 张升序 */
  const displayItems = (): readonly SourceItem[] => {
    if (!byTime()) return items();
    const tick = groupingTick();
    const gap = gapMinutes();
    if (displayCache !== undefined && displayCache.tick === tick && displayCache.gap === gap) {
      return displayCache.value;
    }
    const grouped = grouping();
    const byId = itemById();
    const ordered: SourceItem[] = [];
    const push = (ids: readonly string[]): void => {
      for (const id of ids) {
        const item = byId.get(id);
        if (item) ordered.push(item);
      }
    };
    if (grouped !== undefined) {
      for (const day of grouped.days) {
        for (const slice of day.slices) push(slice.photoIds);
      }
      if (grouped.unknown) push(grouped.unknown.photoIds);
    }
    displayCache = { tick, gap, value: ordered };
    return ordered;
  };

  /**
   * 从 `app.db` 补齐**库内**设置（时间间隔阈值）。
   *
   * 格子尺寸与「按时间」不在这里：它们在 `lib/display-prefs.ts` 里**同步**读好
   *（`localStorage`），所以首次渲染就是对的，不会先画默认值再跳。
   */
  async function hydrate(): Promise<void> {
    const gap = await readNumber(GRID_SETTING_KEYS.gapMinutes, DEFAULT_GAP_MINUTES);
    if (Number.isFinite(gap) && gap > 0) setGapMinutes(gap);
    if (byTime() && items().length > 0) requestTimes(items().map(itemId));
  }

  async function readNumber(key: string, fallback: number): Promise<number> {
    try {
      const raw = await deps.api.getSetting(key);
      if (raw === null) return fallback;
      const value = Number(raw);
      return Number.isFinite(value) ? value : fallback;
    } catch {
      return fallback;
    }
  }

  /* ══════════════════════════════════════════════════════════
   * 选择
   *
   * （排除不在这里 —— 它住在导入工作区的 store，见文件头注释。）
   * ══════════════════════════════════════════════════════════ */

  const clickItem = (
    id: string,
    mode: "replace" | "toggle" | "range",
  ): void => {
    setSelection((current) => applySelection(current, orderedIds(), id, mode));
  };

  const toggleGroup = (ids: readonly string[]): void => {
    if (ids.length === 0) return;
    setSelection((current) => toggleGroupSelection(current, ids));
  };

  const setAnchor = (id: string): void => {
    setSelection((current) => focusSelection(current, id));
  };

  const selectAll = (): void => {
    setSelection(selectAllIds(orderedIds()));
  };

  const clearSelection = (): void => {
    setSelection(EMPTY_SELECTION);
  };

  /* ══════════════════════════════════════════════════════════
   * 缩略图
   * ══════════════════════════════════════════════════════════ */

  return {
    dir,
    items,
    displayItems,
    aspectOf,
    naturalOf,
    ensureNatural,
    status,
    error,
    problems,
    setSourceDir,
    reload,
    refreshSource,

    tileStep,
    setTileStep,
    commitTileStep,
    byTime,
    setByTime,
    gapMinutes,
    grouping,
    loadingTimes,
    hydrate,

    selection,
    selectedIds: () => selection().ids,
    hasSelection: () => anySelected(selection()),
    // 张数按**当前列表**算：换目录后旧选择可能还残留在集合里，不该算进去
    selectedCount: () => selectionCount(selection(), orderedIds()),
    clickItem,
    setAnchor,
    toggleGroup,
    selectAll,
    clearSelection,

    thumbQueue: thumbs,
    thumb: (path) => thumbs.get(path),
    requestThumb: (path, priority) => thumbs.request(path, priority),
    /** 窗口变了（滚停后）——缩略图与元信息补读都按它排在最前 */
    noteVisible,
  };
}

/**
 * 两张是不是「同一对」（同名不同扩展：`P1000019.JPG` 与 `P1000019.RW2`）。
 *
 * **只用来决定分块边界**（一对不许拆进两次请求）；「谁借谁的时间」这条规则
 * 归后端 `media::pairing`，前端不参与判断值。
 */
function sameStem(one: string, other: string): boolean {
  const stem = (name: string): string => name.replace(/\.[^.\\/]+$/, "").toLowerCase();
  return stem(one) === stem(other);
}

/** 把任意异常转成一句话 */
function message(caught: unknown): string {
  if (caught instanceof Error) return caught.message;
  if (typeof caught === "string") return caught;
  return String(caught);
}
