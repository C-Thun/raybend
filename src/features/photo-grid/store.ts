/**
 * 照片网格的状态（`features/photo-grid/` 自己的 store）。
 *
 * 它管四件事：**当前目录的照片清单**、**展示偏好**（档位 / 按时间）、
 * **选择与排除**、**缩略图队列**。跨面板要用的那部分（选择与排除）
 * 通过 store 的方法往上暴露给外壳的 `toolsbar`（由 `App.tsx` 接线），
 * 而不是塞进模块内部藏起来。
 *
 * 三条在实现里很容易做错、这里显式处理掉的：
 *
 * 1. **换目录要清干净**：选择、排除、缩略图缓存都要重置 ——
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
import type {
  SettingsApi,
  SourceItem,
  SourceScan,
  ThumbSize,
  TimeEntry,
} from "./types.ts";
import { itemId } from "./rows.ts";
import type { LoadStatus } from "../../lib/load-status.ts";
import {
  applySelection,
  EMPTY_SELECTION,
  extendSelection,
  hasSelection as anySelected,
  invertSet,
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
} from "./thumbnails.ts";

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
  byTime: () => boolean;
  setByTime: (value: boolean) => void;
  gapMinutes: () => number;
  /** 分组结果（没按时间 / 还没读到时间时是 `undefined`） */
  grouping: () => TimeGrouping | undefined;
  /** 是否正在补读真实拍摄时间 */
  loadingTimes: () => boolean;
  /** 从设置里读回偏好（工作区挂载时调一次） */
  hydrate: () => Promise<void>;

  /* ── 选择与排除 ───────────────────────── */
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

  excluded: () => ReadonlySet<string>;
  excludedCount: () => number;
  /** `toolsbar` 的「批量排除」：对**选中项**做反转（`DESIGN.md` §12.2） */
  toggleExcludedSelected: () => void;

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
  const [excluded, setExcluded] = createSignal<ReadonlySet<string>>(new Set());

  /** 换目录 / 重新扫描时推进它，迟到的结果直接丢掉 */
  let generation = 0;

  const thumbs: ThumbQueue = createThumbQueue({
    load: (path) => deps.api.getThumbBytes(path, "grid"),
    concurrency: deps.thumbConcurrency,
  });

  /* ══════════════════════════════════════════════════════════
   * 数据加载
   * ══════════════════════════════════════════════════════════ */

  const orderedIds = (): string[] => displayItems().map(itemId);

  function resetDirState(next: string | null): void {
    generation += 1;
    setDir(next);
    setItems([]);
    setProblems([]);
    setError(null);
    setSelection(EMPTY_SELECTION);
    setExcluded(new Set<string>());
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
    setExcluded(new Set<string>());
    void load(current);
  };

  /* ══════════════════════════════════════════════════════════
   * 展示偏好
   * ══════════════════════════════════════════════════════════ */

  const setTileStep = (step: number): void => {
    const next = clampTileStepIndex(step);
    if (next === tileStep()) return;
    setTileStepSignal(next);
    void writeSetting(GRID_SETTING_KEYS.tileStep, String(next));
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
   * 选择与排除
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

  const toggleExcludedSelected = (): void => {
    const selected = selection().ids;
    if (selected.size === 0) return;
    setExcluded((prev) => invertSet(prev, selected));
  };

  /* ══════════════════════════════════════════════════════════
   * 缩略图
   * ══════════════════════════════════════════════════════════ */

  return {
    dir,
    items,
    displayItems,
    status,
    error,
    problems,
    setSourceDir,
    reload,

    tileStep,
    setTileStep,
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

    excluded,
    excludedCount: () => excluded().size,
    toggleExcludedSelected,

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
