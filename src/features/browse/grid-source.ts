/**
 * 浏览侧的 **tiles 数据源**：把 `BrowseStore` 包成 `TilesSource`。
 *
 * 这一层只做「形状转换 + 三件浏览特有的事」，**不含任何视图逻辑** ——
 * 网格（`features/photo-grid/PhotoGrid.tsx`）两侧共用，差别全在这类适配器里。
 *
 * 浏览侧的三件特有的事，全都留在**这里**（视图层不许知道）：
 *
 * 1. **分页取数**：库可能很大，网格只按可见范围要数据（`ensureRange`）；
 *    没取回来的格子是 `null` —— 网格渲染成占位块。
 * 2. **显示序置换**：时间线按时间排，但**片内按文件名自然序**（`P1000019.JPG`
 *    与 `P1000019.RW2` 要挨着）。所以「显示序下标」与「数据序下标」是两套，
 *    这里做一次映射，出去之后只有显示序。
 * 3. **标记**：星标 / 色标 / 旗标 / 锁来自 store（导入侧没有这些）。
 *
 * 分组仍然只有一份规则：`lib/time-group.ts`（跨天断 + 间隔 > 1h 断 + 未知时间归末组）。
 */

import {
  clampDisplayAspect,
  DEFAULT_TILE_STEP_INDEX,
} from "../../lib/tile-flow.ts";
import { compareNatural } from "../../lib/natural-order.ts";
import { joinPath } from "../../lib/paths.ts";
import { groupByTime, type TimeGrouping } from "../../lib/time-group.ts";
import type { ThumbEntry } from "../../components/ui/thumb-queue.ts";
import type { RowSlice } from "../../components/ui/tiles/rows.ts";
import type { GridItem, GridStatus, TilesSource } from "../../components/ui/tiles/source.ts";
import type { BrowseStore } from "./store.ts";

/** 时间片阈值（分钟）：两侧同一个值 —— 见 `lib/time-group.ts` 的分组规则 */
export const DEFAULT_GAP_MINUTES = 60;

export interface BrowseSourceDeps {
  store: BrowseStore;
  /**
   * 库根目录（缩略图要绝对路径）；返回 `null` = 还没选库 / 库列表还没到。
   *
   * ⚠️ 必须是**取值函数**而不是值：库列表是异步来的，开工那一刻它还是 `null` ——
   * 传值等于把这个 `null` 闭包进去，之后永远拼不出绝对路径
   * （真机冒烟实测：格子永远停在占位、`thumb_get` 一次都不发）。
   */
  root: () => string | null;
  /**
   * 与胶片带**共用**的缩略图队列（工作区建、两边用）：
   * 网格里已缓存的照片，胶片带里立刻就有。
   */
  thumbs: { get: (path: string) => ThumbEntry; request: (path: string) => void; clear: () => void };
  /** 当前档位（受控：工作区落盘） */
  tileStep: () => number;
  setTileStep: (step: number) => void;
  commitTileStep: () => void;
  /** 按时间分组（受控） */
  grouped: () => boolean;
}

/** 显示序的一份快照（置换 + 分组） */
interface DisplayOrder {
  /** 显示序下的 id（长度 = 总数） */
  ids: readonly string[];
  /** id → **数据序**下标（`store.itemAt` 用的是这一套） */
  dataIndexOf: ReadonlyMap<string, number>;
  /** 分组（显示序空间）；平铺模式为 `undefined` */
  slices: readonly RowSlice[] | undefined;
}

export function browseSource(deps: BrowseSourceDeps): TilesSource {
  const { store } = deps;

  /**
   * 显示序：时间线 → 分组（片内按文件名自然序）→ 拍平。
   *
   * ⚠️ 每次调用都重算一遍**很廉价**（时间线是内存里的数组），但**必须稳定**：
   * 网格会反复读它（`itemAt` / `count` / 滚动的每一帧）。所以按**数组身份**缓存
   * —— 时间线换了（重新查询）才重算，与 store 的 `entries` 是同一个口径。
   */
  let cachedFor: readonly unknown[] | null = null;
  let cached: DisplayOrder = { ids: [], dataIndexOf: new Map(), slices: undefined };

  const order = (): DisplayOrder => {
    const timeline = store.timeline();
    if (timeline === cachedFor) return cached;

    const dataIndexOf = new Map<string, number>();
    timeline.forEach((entry, index) => dataIndexOf.set(String(entry.id), index));

    // 分组（只有「按时间」模式才算）：片内顺序由 `groupByTime` 按 name 排好
    const grouping: TimeGrouping | undefined = deps.grouped()
      ? groupByTime(
          timeline.map((entry) => {
            const item = store.itemById(entry.id);
            return {
              id: String(entry.id),
              takenAtMs: entry.takenAt,
              ...(entry.takenAt === null ? {} : {}),
              offset: item?.takenAtOffsetMin ?? null,
              name: entry.relPath,
            };
          }),
          // 时间片阈值：与导入侧同一个口径（1 小时），常量住在 `lib/time-group.ts`
          { gapMinutes: DEFAULT_GAP_MINUTES },
        )
      : undefined;

    if (grouping === undefined) {
      cachedFor = timeline;
      cached = {
        ids: timeline.map((entry) => String(entry.id)),
        dataIndexOf,
        slices: undefined,
      };
      return cached;
    }

    const ids: string[] = [];
    const slices: RowSlice[] = [];
    for (const day of grouping.days) {
      const dayStart = ids.length;
      const dayCount = day.slices.reduce((total, slice) => total + slice.photoIds.length, 0);
      for (const [index, slice] of day.slices.entries()) {
        if (slice.photoIds.length === 0) continue;
        slices.push({
          start: ids.length,
          count: slice.photoIds.length,
          dayId: day.id,
          dayStart,
          dayCount,
          startMs: slice.startMs,
          endMs: slice.endMs,
          offsetMinutes: slice.offsetMinutes,
          unknown: false,
          id: `${day.id}#${index}`,
        });
        ids.push(...slice.photoIds);
      }
    }
    if (grouping.unknown !== null && grouping.unknown.photoIds.length > 0) {
      const start = ids.length;
      slices.push({
        start,
        count: grouping.unknown.photoIds.length,
        dayId: "unknown",
        dayStart: start,
        dayCount: grouping.unknown.photoIds.length,
        startMs: null,
        endMs: null,
        offsetMinutes: null,
        unknown: true,
        id: "unknown",
      });
      ids.push(...grouping.unknown.photoIds);
    }

    // 兜底：分组理论上覆盖全部（`groupByTime` 的约定），真漏了也别让格子消失
    if (ids.length < timeline.length) {
      for (const entry of timeline) {
        const id = String(entry.id);
        if (!ids.includes(id)) ids.push(id);
      }
    }

    cachedFor = timeline;
    cached = { ids, dataIndexOf, slices };
    return cached;
  };

  const dataIndexAt = (displayIndex: number): number | undefined => {
    const id = order().ids[displayIndex];
    return id === undefined ? undefined : order().dataIndexOf.get(id);
  };

  const toGridItem = (dataIndex: number): GridItem | null => {
    const item = store.itemAt(dataIndex);
    if (item === null) return null;
    const root = deps.root();
    const path = root === null ? null : joinPath(root, item.relPath);
    if (path === null) return null;
    const natural = store.naturalOf(item.id);
    return {
      id: String(item.id),
      path,
      fileName: item.fileName,
      ext: item.ext,
      // 竖图按自己的比例居中：与导入侧**同一个夹取函数**（未知尺寸 → 占位比例）
      aspect: clampDisplayAspect(natural?.width ?? 0, natural?.height ?? 0),
      isRaw: item.isRaw,
      hasRaw: item.hasRaw,
      missing: item.missing,
      marks: {
        rating: item.rating,
        colorLabel: item.colorLabel,
        likeState: item.likeState,
        flag: store.picks().has(item.id)
          ? "pick"
          : store.rejects().has(item.id)
            ? "reject"
            : null,
        locked: item.lockLevel > 0,
      },
    };
  };

  return {
    count: () => order().ids.length,
    itemAt: (index) => {
      const dataIndex = dataIndexAt(index);
      return dataIndex === undefined ? null : toGridItem(dataIndex);
    },
    itemById: (id) => {
      const dataIndex = order().dataIndexOf.get(id);
      return dataIndex === undefined ? null : toGridItem(dataIndex);
    },
    ensureRange: (start, end) => {
      // 显示序 → 数据序：取一个**超集**区间即可（多取几张无所谓，页是按数据序取的）
      let lo = Number.POSITIVE_INFINITY;
      let hi = Number.NEGATIVE_INFINITY;
      for (let index = Math.max(0, start); index < end; index += 1) {
        const dataIndex = dataIndexAt(index);
        if (dataIndex === undefined) continue;
        if (dataIndex < lo) lo = dataIndex;
        if (dataIndex > hi) hi = dataIndex;
      }
      if (!Number.isFinite(lo)) return;
      return store.ensureRange(lo, hi + 1);
    },
    aspectOf: (id) => {
      const dataIndex = order().dataIndexOf.get(id);
      const item = dataIndex === undefined ? null : store.itemAt(dataIndex);
      if (item === null) return 1;
      const natural = store.naturalOf(item.id);
      return clampDisplayAspect(natural?.width ?? 0, natural?.height ?? 0);
    },
    naturalOf: (id) => {
      const dataIndex = order().dataIndexOf.get(id);
      return dataIndex === undefined ? null : store.naturalOf(Number(id));
    },
    ensureNatural: async (entries) => {
      await store.ensureNatural(entries.map((entry) => ({ id: Number(entry.id), path: entry.path })));
    },
    slices: () => order().slices,
    /*
     * 浏览侧没有「四态状态机」，只有 `loading` / `error` 两个信号 —— 在这里翻译成
     * 契约要的三态（导入侧是现成的状态机）。`loading` 只在**首次**（一张都没有）时
     * 显示加载态，翻页不置 loading（否则每次滚动都在闪水印）。
     */
    status: (): GridStatus => {
      if (store.error() !== null && store.total() === 0) return "error";
      if (store.loading() && store.total() === 0) return "loading";
      return "ready";
    },
    error: () => store.error(),
    reload: () => store.reload(),
    scopeKey: () => `${store.repositoryId() ?? ""}:${store.scopePath() ?? ""}`,
    selection: () => {
      const state = store.selection();
      return { ids: state.ids, anchor: state.anchor };
    },
    select: (id, mode) => store.select(Number(id), mode, [...order().ids]),
    selectGroupRange: (start, count, additive) => {
      const ids = order().ids.slice(start, start + count);
      store.selectAll(additive === true ? ids : ids);
    },
    clearSelection: () => store.clearSelection(),
    tileStep: () => deps.tileStep(),
    setTileStep: (step) => deps.setTileStep(step),
    commitTileStep: () => deps.commitTileStep(),
    thumb: (path) => deps.thumbs.get(path),
    requestThumb: (path) => deps.thumbs.request(path),
  };
}

/** tile 档位默认值（导出给调用方对齐） */
export const BROWSE_DEFAULT_TILE_STEP = DEFAULT_TILE_STEP_INDEX;

/** 片内按文件名自然序的比较器（导出给测试用） */
export { compareNatural };
