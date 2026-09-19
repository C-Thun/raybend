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
  commitDisplayTileStep,
  displayByTime,
  displayTileStep,
  setDisplayByTime,
  setDisplayTileStep,
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
  TimeEntry,
} from "./types.ts";
import type { MetaFile, PhotoMeta } from "../../api/types.ts";
import { itemId } from "./rows.ts";
import type { LoadStatus } from "../../lib/load-status.ts";
import {
  applySelection,
  EMPTY_SELECTION,
  extendSelection,
  hasSelection as anySelected,
  selectAll as selectAllIds,
  selectionCount,
  type SelectionState,
} from "../../lib/selection.ts";
import { groupByTime, type TimeGrouping } from "../../lib/time-group.ts";
import { clampTileStepIndex } from "../../lib/tile-flow.ts";
import {
  createThumbQueue,
  type ThumbEntry,
  type ThumbQueue,
} from "../../components/ui/thumb-queue.ts";

/** 时间片阈值的默认值（分钟）——`DESIGN.md` §12.7 的「1 小时」 */
export const DEFAULT_GAP_MINUTES = 60;

/**
 * 等「文件头缓存」的时限（毫秒）。
 *
 * 网格现在**依赖** `dirMetaEnsure` 回来才铺 tile（见 `load`），所以它一旦不回来
 * （后端命令 panic 时 promise 永远不 settle，见 `lib/timeout.ts` 文件头），
 * 界面就会永远停在水印上。这道时限把那种情况变成「按占位比例照常铺照片」。
 *
 * 给得比普通命令（15s）宽：它扫的是整个目录的头，慢盘上本来就慢 ——
 * 时限防的是「卡死」，不是「慢」。
 */
export const DEFAULT_META_TIMEOUT_MS = 20_000;

/**
 * 仍在 `app.db` 里的网格设置键（与 `src/api/db.ts` 的 `SETTING_KEYS` 保持一致）。
 *
 * ⚠️ `tile_step` / `by_time` **已经搬走**（`lib/display-prefs.ts`，localStorage）：
 * 那两个是「怎么看」的设备级偏好，而且 import 与 browse 必须共用一份 ——
 * 以前只有导入侧读写它们，浏览侧每次进都重置（人类 2026-09-20 报的）。
 * 时间间隔阈值留在库里（它是**库内数据**的解释参数，不是显示偏好）。
 */
export const GRID_SETTING_KEYS = {
  gapMinutes: "grid.time_gap_minutes",
} as const;

export interface PhotoGridApi extends SettingsApi {
  scanSourceDir: (path: string) => Promise<SourceScan>;
  /** 一批文件的展示元信息（宽高 + 方向）。走 Rust 侧的会话级内存缓存，命中时几乎不花时间 */
  dirMetaEnsure: (dir: string, files: readonly MetaFile[]) => Promise<PhotoMeta[]>;
  readSourceTimes: (paths: string[]) => Promise<TimeEntry[]>;
  getThumbBytes: (path: string, size?: ThumbSize) => Promise<Uint8Array | null>;
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
  /** 全选一组（日 / 时间片）；`additive = false` 表示替换掉现有选择 */
  selectGroup: (ids: readonly string[], additive?: boolean) => void;
  selectAll: () => void;
  clearSelection: () => void;

  /* ── 缩略图 ───────────────────────────── */
  thumb: (path: string) => ThumbEntry;
  requestThumb: (path: string) => void;
}

export function createPhotoGridStore(deps: PhotoGridDeps): PhotoGridStore {
  const [dir, setDir] = createSignal<string | null>(null);
  const [items, setItems] = createSignal<readonly SourceItem[]>([]);
  const [status, setStatus] = createSignal<LoadStatus>("idle");
  const [error, setError] = createSignal<string | null>(null);
  const [problems, setProblems] = createSignal<readonly string[]>([]);

  /*
   * 格子尺寸与「按时间」走**共享的设备级偏好**（`lib/display-prefs.ts`）：
   * 两个工作区读同一份、且跨会话还原。梯度/分组的推导逻辑不变，只是值的来源换了地方。
   */
  const tileStep = displayTileStep;
  const byTime = displayByTime;
  const [gapMinutes, setGapMinutes] = createSignal(DEFAULT_GAP_MINUTES);
  const [loadingTimes, setLoadingTimes] = createSignal(false);

  const [selection, setSelection] = createSignal<SelectionState>(EMPTY_SELECTION);

  /** 换目录 / 重新扫描时推进它，迟到的结果直接丢掉 */
  let generation = 0;

  /** 等头部缓存的时限（依赖注入，测试里给小值） */
  const metaTimeout = deps.metaTimeoutMs ?? DEFAULT_META_TIMEOUT_MS;

  const thumbs: ThumbQueue = createThumbQueue({
    load: (path) => deps.api.getThumbBytes(path, "grid"),
    concurrency: deps.thumbConcurrency,
  });

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
    if (meta === undefined) return clampDisplayAspect(0, 0);
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

  /** 目录切换时把旧元信息丢掉（不同目录的文件名可能一样） */
  function clearPhotoMeta(): void {
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
    thumbs.clear();
  }

  async function load(next: string): Promise<void> {
    const token = generation;
    setStatus("loading");
    setError(null);
    try {
      const scan = await deps.api.scanSourceDir(next);
      if (token !== generation) return; // 用户又换了目录
      /*
       * 清单一到手就先把 items 摆上：计数（控制条）与「已选 N 张」这类东西
       * 不必等头部缓存 —— 它们在载入期间显示出来反而让人知道「目录读到了，
       * 正在补每张的宽高」。
       */
      setItems(scan.items);
      setProblems(scan.problems);
      /*
       * **等头部缓存铺完再铺 tile**（人类 2026-09-17 定）：照片的比例一次到位，
       * 不再先按 3:2 占位再各自「长大」。代价是首开大目录要多等这几秒 ——
       * 同目录二次打开命中会话级缓存，几乎是瞬时的。
       */
      await loadPhotoMeta(next, scan.items, token);
      if (token !== generation) return; // 等的时候用户又换了目录
      setStatus("ready");
      if (byTime()) void loadTimes();
    } catch (caught) {
      if (token !== generation) return;
      setError(message(caught));
      setStatus("error");
    }
  }

  /**
   * 补读真实拍摄时间（用户按下「按时间」时才调）。
   *
   * 只补**还没读到真相**的那些（`takenAtSource !== "exif"`）——
   * 换目录后重新读一遍时，已经准确的不用再读。
   */
  /**
   * 取这一批照片的展示元信息（宽高 + 方向）。
   *
   * 失败**不打扰用户**：比例退化成默认占位，网格照铺（读不到一个头不该让整个目录列不出来）。
   * 时限同理 —— 它防的是「后端卡死」，不是「慢盘」。
   *
   * ⚠️ 现在它是**阻塞 `ready`** 的一步（见 `load`），所以这里的每一条出路
   * （成功 / 失败 / 超时）都必须把控制权交回去。
   */
  async function loadPhotoMeta(
    dir: string,
    items: readonly SourceItem[],
    token: number,
  ): Promise<void> {
    if (items.length === 0) return;
    try {
      const metas = await withTimeout(
        deps.api.dirMetaEnsure(
          dir,
          items.map((item) => ({
            relative: item.fileName,
            fileSize: item.sizeBytes,
            mtimeMs: item.mtimeMs ?? 0,
          })),
        ),
        metaTimeout,
        timeoutMessage("grid.timeout.meta", metaTimeout),
      );
      if (token !== generation) return; // 用户又换了目录，迟到的结果丢掉
      /*
       * **按顺序一一对应**（后端契约：返回顺序与传入的 `files` 一致）。
       * 不用「目录 + 文件名」拼 key —— Windows 与 POSIX 的分隔符不同，
       * 拼出来的字符串一旦不一致就会静默查不到（比例永远是占位）。
       */
      const next = new Map<string, PhotoMeta>();
      items.forEach((item, index) => {
        const meta = metas[index];
        if (meta !== undefined) next.set(itemId(item), meta);
      });
      setPhotoMeta(next);
    } catch {
      // 读不到元信息不是错误：按默认比例显示就是了
    }
  }

  /**
   * 按需补读宽高并**合并**进现有元数据。
   *
   * 与 `loadPhotoMeta` 的区别很关键：那个是「换目录，整套重来」（`setPhotoMeta(next)` 覆盖），
   * 这里只补几张、**必须保留**已有的 —— 否则补读会把可见 tile 的比例清掉。
   */
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

    try {
      const metas = await deps.api.dirMetaEnsure(current, files);
      setPhotoMeta((prev) => {
        const merged = new Map(prev);
        missing.forEach((path, index) => {
          const meta = metas[index];
          if (meta !== undefined) merged.set(path, meta);
        });
        return merged;
      });
    } catch {
      // 读不到不是错误：按占位比例显示
    }
  }

  async function loadTimes(): Promise<void> {
    const token = generation;
    const pending = items()
      .filter((item) => item.takenAtSource !== "exif")
      .map(itemId);
    if (pending.length === 0) return;

    setLoadingTimes(true);
    try {
      const times = await deps.api.readSourceTimes(pending);
      if (token !== generation) return;
      const byPath = new Map(times.map((entry) => [entry.path, entry]));
      let merged = 0;
      let missed = 0;
      setItems((prev) =>
        prev.map((item) => {
          const entry = byPath.get(itemId(item));
          if (!entry) {
            // 只统计「请求过却没回来」的那些（其它条目本来就不需要合并）
            if (pending.includes(itemId(item))) missed += 1;
            return item;
          }
          merged += 1;
          return {
            ...item,
            takenAtMs: entry.takenAtMs,
            takenAtSource: entry.takenAtSource,
            takenAtOffsetMin: entry.takenAtOffsetMin,
          };
        }),
      );
      /*
       * 留一条控制台诊断：人类报过「按时间模式下同一天出现两个分组标题、一遍纯位图一遍纯 RAW」，
       * 而后端探针（`examples/source-times-probe.rs`）显示 300 个文件全都能从 EXIF 读到时间。
       * 到底是不是「补回来的没并上」（路径对不上 / 命令没回来），这行字直接给出答案 ——
       * 真的少的时候才打，平时不刷屏。
       */
      if (missed > 0) {
        console.warn(
          // i18n-exempt: 控制台诊断（不是界面文案），见 DESIGN.md §11.1 的豁免项
          `[photo-grid] 补读拍摄时间：合并 ${merged} 条，有 ${missed} 条没对上（按时间分组可能因此分层）`,
        );
      }
    } catch {
      // 读不到时间不该把网格变成错误态：退回「未知时间」组即可
    } finally {
      if (token === generation) setLoadingTimes(false);
    }
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
    setDisplayTileStep(clampTileStepIndex(step));
  };

  /** 把当前档位落盘（滑块拖拽结束时调一次） */
  const commitTileStep = (): void => {
    commitDisplayTileStep();
  };

  /** 「按时间」开关：立刻落盘（一次点击就是一个终值），并按需补读拍摄时间 */
  const setByTime = (value: boolean): void => {
    if (value === byTime()) return;
    setDisplayByTime(value);
    if (value) void loadTimes();
  };

  const grouping = (): TimeGrouping | undefined => {
    if (!byTime()) return undefined;
    const list = items();
    if (list.length === 0) return undefined;
    return groupByTime(
      list.map((item) => ({
        id: itemId(item),
        takenAtMs: item.takenAtMs,
        offsetMinutes: item.takenAtOffsetMin,
        // 片内按文件名自然序 —— 用路径（含目录）比只比文件名更确定
        name: item.path,
      })),
      { gapMinutes: gapMinutes() },
    );
  };

  /** 显示顺序：平铺 = 扫描顺序；按时间 = 日倒序 → 片升序 → 张升序 */
  const displayItems = (): readonly SourceItem[] => {
    const grouped = grouping();
    if (!grouped) return items();
    const byId = new Map(items().map((item) => [itemId(item), item]));
    const ordered: SourceItem[] = [];
    const push = (ids: readonly string[]): void => {
      for (const id of ids) {
        const item = byId.get(id);
        if (item) ordered.push(item);
      }
    };
    for (const day of grouped.days) {
      for (const slice of day.slices) push(slice.photoIds);
    }
    if (grouped.unknown) push(grouped.unknown.photoIds);
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
    if (byTime() && items().length > 0) void loadTimes();
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

  const selectGroup = (ids: readonly string[], additive = true): void => {
    if (ids.length === 0) return;
    setSelection((current) =>
      additive
        ? extendSelection(current, ids)
        : extendSelection(EMPTY_SELECTION, ids),
    );
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
    selectGroup,
    selectAll,
    clearSelection,

    thumb: (path) => thumbs.get(path),
    requestThumb: (path) => thumbs.request(path),
  };
}

/** 把任意异常转成一句话 */
function message(caught: unknown): string {
  if (caught instanceof Error) return caught.message;
  if (typeof caught === "string") return caught;
  return String(caught);
}
