/**
 * 导入侧的 **tiles 数据源**：把 `PhotoGridStore` 包成 `TilesSource`。
 *
 * 这一层只做「形状转换」，不含任何视图逻辑 —— 网格（`PhotoGrid`）两侧共用，
 * 差别全在这类适配器里（人类 2026-09-19 定的结构）。
 *
 * 导入侧的三个特点：
 * 1. **整目录一次性拿到**（没有分页）⇒ 不实现 `ensureRange`；
 * 2. **显示序 = 拿到的顺序** ⇒ `itemAt(i)` 就是第 i 个，分组也直接用
 *    `groupingToSlices(store.grouping())`；
 * 3. id 就是**路径** ⇒ `thumb(id)` 与 `store.thumb` 同一套键。
 */

import type { GridItem, GridStatus, TilesSource } from "../../components/ui/tiles/source.ts";
import { groupingToSlices, type RowSlice } from "../../components/ui/tiles/rows.ts";
import type { PhotoGridStore } from "./store.ts";
import { itemId } from "./rows.ts";
import type { TileInfoMode } from "../../lib/display-prefs.ts";

/** 把一张 `SourceItem` 转成网格要看的形状 */
function toGridItem(store: PhotoGridStore, item: ReturnType<PhotoGridStore["displayItems"]>[number]): GridItem {
  const id = itemId(item);
  return {
    id,
    path: item.path,
    fileName: item.fileName,
    ext: item.ext,
    aspect: store.aspectOf(id),
    // 展示的就是 RAW 才有角标；导入侧没有「位图 + RAW 复合」这回事
    isRaw: item.kind === "raw",
  };
}

export interface ImportSourceOptions {
  /** 这张是不是被排除了（状态不在网格里，见 `PhotoGrid` 的说明） */
  isExcluded?: (id: string) => boolean;
  infoMode?: () => TileInfoMode;
}

/** 包一个导入侧数据源。`isExcluded` 会一路带到 `Tile` 上（排除态的画法）。 */
export function importSource(
  store: PhotoGridStore,
  options: ImportSourceOptions = {},
): TilesSource & { isExcluded?: (id: string) => boolean } {
  return {
    isExcluded: options.isExcluded,
    count: () => store.displayItems().length,
    itemAt: (index) => {
      const item = store.displayItems()[index];
      return item === undefined ? null : toGridItem(store, item);
    },
    itemById: (id) => {
      const item = store.displayItems().find((entry) => itemId(entry) === id);
      return item === undefined ? null : toGridItem(store, item);
    },
    aspectOf: (id) => store.aspectOf(id),
    naturalOf: (id) => store.naturalOf(id),
    ensureNatural: (entries) => store.ensureNatural(entries.map((entry) => entry.path)),
    slices: (): readonly RowSlice[] | undefined => {
      const grouping = store.grouping();
      return grouping === undefined ? undefined : groupingToSlices(grouping);
    },
    status: () => store.status() as GridStatus,
    error: () => store.error(),
    reload: () => store.reload(),
    scopeKey: () => store.dir() ?? "",
    selection: () => store.selection(),
    select: (id, mode) => store.clickItem(id, mode),
    setAnchor: (id) => store.setAnchor(id),
    selectGroupRange: (start, count) => {
      const ids = store
        .displayItems()
        .slice(start, start + count)
        .map((item) => itemId(item));
      store.toggleGroup(ids);
    },
    clearSelection: () => store.clearSelection(),
    infoMode: () => options.infoMode?.() ?? "off",
    tileStep: () => store.tileStep(),
    setTileStep: (step) => store.setTileStep(step),
    commitTileStep: () => store.commitTileStep(),
    thumb: (path) => store.thumb(path),
    requestThumb: (path) => store.requestThumb(path),
  };
}
