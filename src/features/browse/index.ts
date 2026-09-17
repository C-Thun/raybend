/**
 * 浏览模块的对外出口（`ARCHITECTURE.md` §2：别的层只能从这里拿东西）。
 */

export { BrowseGrid, type BrowseGridProps } from "./BrowseGrid.tsx";
export { BrowseToolbar, type BrowseToolbarProps } from "./BrowseToolbar.tsx";
export {
  AssetInfo,
  BrowseLeftColumn,
  type AssetInfoProps,
  type BrowseLeftColumnProps,
} from "./BrowsePanels.tsx";
export {
  buildBrowseRows,
  browseGroups,
  DEFAULT_GAP_MINUTES,
  GROUP_ROW_HEIGHT,
  type BrowseGroupBoundary,
  type BrowseGroupRow,
  type BrowseRowModel,
  type BrowseTileRow,
} from "./rows.ts";
export {
  createBrowseStore,
  hasSelected,
  PAGE_SIZE,
  type BrowseApi,
  type BrowseDeps,
  type BrowseStore,
} from "./store.ts";
