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

import type { FullscreenTarget } from "../../lib/fullscreen-target.ts";

export interface EditorActions {
  /** 正在看一张照片（编辑器里 = 有选中照片且渲染线程接管了画面） */
  viewing: () => boolean;
  /** 胶片带可见（`infoKeyApplies` 要这一项） */
  filmVisible: () => boolean;
  /** 处在对比态（编辑器这一波还没有对比 —— W5） */
  comparing: () => boolean;
  /** `Tab` 三档循环 */
  cycleChrome: () => void;
  /** 回到默认档（离开编辑器 / 换库换目录时用） */
  resetChrome: () => void;
  /** 有没有可编辑的照片（空态下工具与控制块一律禁用） */
  hasPhoto: () => boolean;
  /** 全屏看图要的清单（编辑侧同样是「当前目录显示序 + 锚点」）；没有照片时为 `null` */
  fullscreenTarget: () => FullscreenTarget | null;
}

let current: EditorActions | null = null;

/** 工作区挂载时注册自己那一份；卸载时传 `null` 清空 */
export function registerEditorActions(actions: EditorActions | null): void {
  current = actions;
}

/** 当前生效的那一份（不在编辑工作流里时为 `null`） */
export function editorActions(): EditorActions | null {
  return current;
}
