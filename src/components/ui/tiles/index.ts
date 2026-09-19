/**
 * `tiles` 一族（`components/ui/tiles/`）—— **所有 tiles 视图共用的外框与状态条**。
 *
 * 人类 2026-09-19：「我们沟通语境下的 tiles，就是包括下面状态条的，因为业务层面
 * 就是一个不可分割的整体」⇒ 两个工作区都用 `<TilesShell>` 拼中列，
 * 状态条的配置统一从 `bar` 传进去（同一个组件、同一份口径，差异只剩显式开关）。
 */

export { TilesShell, type TilesShellProps } from "./TilesShell.tsx";
export {
  PhotoStatusBar,
  TilesControlBar,
  type TilesControlBarProps,
  type TilesSortConfig,
  type TilesViewingInfo,
} from "./TilesControlBar.tsx";
