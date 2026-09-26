/**
 * 编辑工作区的**动作槽**（与 `features/browse/actions.ts` / `import/actions.ts` 同一套做法）。
 *
 * 编辑侧的键盘语义与看图那套**共用同一批命令**（`viewer.zoomIn` / `fit` / `actual` /
 * `prev` / `next`）—— 命令只有一条，谁挂载谁把自己那份实现注册进去
 * （缩放与适配走 `components/ui/viewer/actions.ts` 的 `ViewerActions` 槽，
 * 而 `viewing()` / `filmVisible()` 这两个「当前这一屏是什么状态」的读数走这里）。
 *
 * 为什么不把 `hasPhoto` 与 `viewing` 合成一个：`viewing` 会被看图命令当 `when` 用，
 * 而「有没有照片」还会被工具栏 / 控制块的可用性用 —— 两个问题，两个读数，
 * 合成一个就会有人开始猜「viewing 在编辑里到底什么意思」。
 */

import { createActionSlot } from "../../lib/action-slot.ts";
import type { FullscreenTarget } from "../../lib/fullscreen-target.ts";

export interface EditorActions {
  /** 正在看一张照片（编辑器里 = 有选中照片且渲染线程接管了画面） */
  viewing: () => boolean;
  /** 胶片带可见（`infoKeyApplies` 要这一项） */
  filmVisible: () => boolean;
  /** 处在对比态；状态栏显示参考源与当前结果。 */
  comparing: () => boolean;
  /** `Tab` 三档循环 */
  cycleChrome: () => void;
  /** 回到默认档（离开编辑器 / 换库换目录时用） */
  resetChrome: () => void;
  /** 有没有可编辑的照片（空态下工具与控制块一律禁用） */
  hasPhoto: () => boolean;
  /** 两层重置：先确认清除手动调整，再直接清除自动调整；无默认热键，避免误触。 */
  canReset: () => boolean;
  resetDevelop: () => void;
  /** RAW、位图、机型信息齐全且当前可编辑。 */
  canAutoAdjust: () => boolean;
  /** 拟合基础曲线，保守调整影调色彩并匹配镜头。 */
  autoAdjust: () => void;
  canFinalize: () => boolean;
  finalize: () => void;
  /** 切换编辑源后保存 latest 来源。 */
  commitDevelop: () => void;
  /** 全屏看图要的清单（编辑侧同样是「当前目录显示序 + 锚点」）；没有照片时为 `null` */
  fullscreenTarget: () => FullscreenTarget | null;
}

const slot = createActionSlot<EditorActions>();

/** 工作区挂载时注册自己那一份；卸载时传 `null` 清空 */
export function registerEditorActions(actions: EditorActions | null): void {
  slot.register(actions);
}

/** 当前生效的那一份（不在编辑工作流里时为 `null`） */
export function editorActions(): EditorActions | null {
  return slot.read();
}
