/**
 * 浏览模块的对外出口（`ARCHITECTURE.md` §2：别的层只能从这里拿东西）。
 */

/*
 * 网格**不从这里出**：全项目只有一个网格（`features/photo-grid/PhotoGrid.tsx`）。
 * 浏览侧只提供它的**数据源适配器**（`browseSource`）与水印文案（在工作区里）。
 */
export { browseSource, type BrowseSourceDeps } from "./grid-source.ts";
export { ViewerStatusBar, type ViewerStatusBarProps } from "./ViewerStatusBar.tsx";
export { BrowseToolbar, type BrowseToolbarProps } from "./BrowseToolbar.tsx";
export { TagDialog, type TagDialogProps } from "./TagDialog.tsx";
export {
  AssetInfo,
  BrowseLeftColumn,
  type AssetInfoProps,
  type BrowseLeftColumnProps,
} from "./BrowsePanels.tsx";
export {
  createBrowseStore,
  hasSelected,
  PAGE_SIZE,
  type BrowseApi,
  type BrowseDeps,
  type BrowseStore,
} from "./store.ts";
