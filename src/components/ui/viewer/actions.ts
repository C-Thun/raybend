/**
 * 看图动作的**当前挂载槽**（`plans/M2-W3.md` §2.5 步骤 3）。
 *
 * ## 为什么需要它
 *
 * 「放大 / 缩小 / 适配 / 100% / 上一张 / 下一张」在**单张看图**与**对比**里各有实现
 * （对比的缩放是它自己的局部状态：每幅画幅要按百分比同步，与单张那套不是同一份数学）。
 * 但**命令只有一条**（`viewer.zoomIn` 等）—— 命令不该知道现在是哪种形态，
 * 所以由**当前挂载的那个视图**把自己那份实现注册进来，命令通过这里取「当前生效的那一份」。
 *
 * ## 为什么是模块级单例
 *
 * 与 `components/ui/tile-info.ts` / `lib/display-prefs.ts` 同一套做法：
 * 同一时刻只有一个看图视图挂载（单张与对比互斥），注册表就是一个「当前是谁」的槽。
 * 卸载时**必须清空**（`register(null)`），否则命令会打到已经卸载的视图上。
 */

/** 看图动作（两种形态各自实现同一套语义） */
export interface ViewerActions {
  /** 放大一档（锚点由实现自己决定：单张居中、对比按当前画幅） */
  zoomIn: () => void;
  zoomOut: () => void;
  /** 适配窗口 ↔ 100%（再按一次回去） */
  toggleFit: () => void;
  /** 100% ↔ 适配（`1` 键的语义：与 `toggleFit` 是同一条命令的两种入口） */
  actual: () => void;
  next: () => void;
  prev: () => void;
  /** 退出看图（对比态下也一样） */
  close: () => void;
}

let current: ViewerActions | null = null;

/** 视图挂载时注册自己那份实现；卸载时传 `null` 清空 */
export function registerViewerActions(actions: ViewerActions | null): void {
  current = actions;
}

/** 当前生效的那一份（没挂载看图视图时为 `null`） */
export function viewerActions(): ViewerActions | null {
  return current;
}
