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
import {
  clampTileStepIndex,
  DEFAULT_TILE_STEP_INDEX,
} from "../../lib/tile-flow.ts";
import {
  createThumbQueue,
  type ThumbEntry,
  type ThumbQueue,
} from "../../components/ui/thumb-queue.ts";

/** 时间片阈值的默认值（分钟）——`DESIGN.md` §12.7 的「1 小时」 */
export const DEFAULT_GAP_MINUTES = 60;

/** 设置键（与 `src/api/db.ts` 的 `SETTING_KEYS` 保持一致） */
export const GRID_SETTING_KEYS = {
  tileStep: "grid.tile_step",
  byTime: "grid.by_time",
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

  const [tileStep, setTileStepSignal] = createSignal(DEFAULT_TILE_STEP_INDEX);
  const [byTime, setByTimeSignal] = createSignal(false);
  const [gapMinutes, setGapMinutes] = createSignal(DEFAULT_GAP_MINUTES);
  const [loadingTimes, setLoadingTimes] = createSignal(false);

  const [selection, setSelection] = createSignal<SelectionState>(EMPTY_SELECTION);

  /** 换目录 / 重新扫描时推进它，迟到的结果直接丢掉 */
  let generation = 0;

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
      setItems(scan.items);
      setProblems(scan.problems);
      setStatus("ready");
      /*
       * 元信息**不阻塞**出网格：先按默认比例把照片铺出来，宽高到了再各自修正
       * （外框是正方，所以迟到不会重排布局，只是照片在框里长大）。
       */
      void loadPhotoMeta(next, scan.items, token);
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
   * 取这一批照片的展示元信息（宽高 + 方向）。失败**不打扰用户** —— 只是比例退化成默认占位。
   */
  async function loadPhotoMeta(
    dir: string,
    items: readonly SourceItem[],
    token: number,
  ): Promise<void> {
    if (items.length === 0) return;
    try {
      const metas = await deps.api.dirMetaEnsure(
        dir,
        items.map((item) => ({
          relative: item.fileName,
          fileSize: item.sizeBytes,
          mtimeMs: item.mtimeMs ?? 0,
        })),
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
      setItems((prev) =>
        prev.map((item) => {
          const entry = byPath.get(itemId(item));
          if (!entry) return item;
          return {
            ...item,
            takenAtMs: entry.takenAtMs,
            takenAtSource: entry.takenAtSource,
            takenAtOffsetMin: entry.takenAtOffsetMin,
          };
        }),
      );
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
   * 改 tile 尺寸档位。**只改状态，不写设置** ——
   * 拖动滑块时每动一格都写一次设置，一拖就是几百次 IPC 加几百次数据库写，
   * 手感直接卡住（2026-09-16 人类反馈「尺寸调节非常卡」）。落盘走 `commitTileStep`，
   * 由滑块在**拖拽结束**时调一次。
   */
  const setTileStep = (step: number): void => {
    const next = clampTileStepIndex(step);
    if (next === tileStep()) return;
    setTileStepSignal(next);
  };

  /** 把当前档位写进设置（滑块拖拽结束时调一次） */
  const commitTileStep = (): void => {
    void writeSetting(GRID_SETTING_KEYS.tileStep, String(tileStep()));
  };

  const setByTime = (value: boolean): void => {
    if (value === byTime()) return;
    setByTimeSignal(value);
    void writeSetting(GRID_SETTING_KEYS.byTime, value ? "1" : "0");
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

  async function hydrate(): Promise<void> {
    const [step, time, gap] = await Promise.all([
      readNumber(GRID_SETTING_KEYS.tileStep, DEFAULT_TILE_STEP_INDEX),
      readBoolean(GRID_SETTING_KEYS.byTime, false),
      readNumber(GRID_SETTING_KEYS.gapMinutes, DEFAULT_GAP_MINUTES),
    ]);
    setTileStepSignal(clampTileStepIndex(step));
    if (Number.isFinite(gap) && gap > 0) setGapMinutes(gap);
    if (time && !byTime()) {
      setByTimeSignal(true);
      if (items().length > 0) void loadTimes();
    }
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

  async function readBoolean(key: string, fallback: boolean): Promise<boolean> {
    try {
      const raw = await deps.api.getSetting(key);
      if (raw === null) return fallback;
      return raw === "1" || raw.toLowerCase() === "true";
    } catch {
      return fallback;
    }
  }

  async function writeSetting(key: string, value: string): Promise<void> {
    try {
      await deps.api.setSetting(key, value);
    } catch {
      // 存不下偏好不影响本次使用
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
