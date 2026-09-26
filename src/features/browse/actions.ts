/**
 * 浏览工作区的**动作槽**（`specs/M2-W3.md` §2.5 步骤 2）。
 *
 * ## 为什么需要它
 *
 * 有几件「主要操作」的状态住在**工作区**里、不在 store 里：
 *
 * | 动作 | 状态住哪 |
 * | --- | --- |
 * | 删除（回收站） | 工作区的确认弹窗（`pendingDelete` + 失败清单） |
 * | 进看图 | 工作区的 `prepareViewer`（四态复位、锚点） |
 * | `Tab` 四态循环 | 工作区的 `chrome` 信号 |
 *
 * 命令注册表要能调它们，但不能认识工作区内部 —— 所以工作区挂载时把这一组动作
 * **注册**进来，命令通过 `browseActions()` 取用（与 `viewer/actions.ts` 同一套做法）。
 *
 * ## 纪律
 *
 * * 卸载时**必须** `register(null)`（否则命令会打到已卸载的工作区上）；
 * * 槽里只有**动作**，没有状态：读状态一律走 `browseStore`（唯一事实来源）。
 */

import { createActionSlot } from "../../lib/action-slot.ts";
import type { FullscreenTarget } from "../../lib/fullscreen-target.ts";

export interface BrowseActions {
  /** 正在看图（单张或对比） */
  viewing: () => boolean;
  /** 处在对比态（选中 ≥ 2 张时的看图） */
  comparing: () => boolean;
  /** 看图态下胶片带可见（`infoKeyApplies` 要这一项） */
  filmVisible: () => boolean;
  /** 打开删除确认（工作区负责确认 → 回收站 → 失败清单） */
  requestDelete: () => void;
  /** 网格里移动「当前那张」（←/→）：与点击同一套选择语义 */
  moveFocus: (delta: -1 | 1) => void;
  /** 进看图（从锚点那张开始；多选时自然进对比） */
  openViewer: () => void;
  /** `Tab` 四态循环（左栏 / 右栏 / 胶片带） */
  cycleChrome: () => void;
  /** 回到默认四态（退出看图时用） */
  resetChrome: () => void;
  /** 对比态：胶片带只显示参与对比的图（再按一次回去） */
  toggleCompareStrip: () => void;
  /**
   * 全屏看图要的清单（当前显示序 + 锚点）；没有当前照片时为 `null`。
   *
   * 走动作槽而不是把清单存在组装层：**显示序是工作区的知识**
   * （它知道 `gridSource()` 与筛选），组装层只负责把它交给 `api/fullscreen.ts`。
   */
  fullscreenTarget: () => FullscreenTarget | null;
}

const slot = createActionSlot<BrowseActions>();

export function registerBrowseActions(actions: BrowseActions | null): void {
  slot.register(actions);
}

export function browseActions(): BrowseActions | null {
  return slot.read();
}
