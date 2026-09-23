/**
 * 导入工作区的**动作槽**（与 `features/browse/actions.ts` 同一套做法，见那里的说明）。
 *
 * 导入侧的键位（`Tab` / `Esc` / `Ctrl+A` / `Enter` / `i`）与浏览侧**语义一致**，
 * 但状态住在导入工作区里（它的 `chrome` 信号与 `viewer` store）——
 * 所以它同样把自己那份动作注册进来，命令通过这里取用。
 */

import type { FullscreenTarget } from "../../lib/fullscreen-target.ts";

export interface ImportActions {
  /** 正在看图（film / view） */
  viewing: () => boolean;
  /** 看图态下胶片带可见（`infoKeyApplies` 要这一项） */
  filmVisible: () => boolean;
  /** 处在对比态。 */
  comparing: () => boolean;
  /** `Tab` 四态循环 */
  cycleChrome: () => void;
  /** 回到默认四态（退出看图） */
  resetChrome: () => void;
  /** 进看图（从锚点那张开始） */
  openViewer: () => void;
  /** 全选（`Ctrl+A`） */
  selectAll: () => void;
  /** 批量排除选中的照片 */
  excludeSelected: () => void;
  /** 对比态：胶片带在“仅对比集 / 全目录”之间切换。 */
  toggleCompareStrip: () => void;
  /** 全屏看图要的清单（当前显示序 + 锚点）；没有当前照片时为 `null`。 */
  fullscreenTarget: () => FullscreenTarget | null;
}

let current: ImportActions | null = null;

export function registerImportActions(actions: ImportActions | null): void {
  current = actions;
}

export function importActions(): ImportActions | null {
  return current;
}
