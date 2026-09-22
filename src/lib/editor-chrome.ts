/**
 * 编辑工作区的**面板 × Tab 档位**状态机（`prompts/editor.pd` 2026-09-23 口述）。
 *
 * 编辑有两根正交的轴：
 *
 * ```text
 *   Tab 档位（lib/viewer-chrome.ts 的 editor 表）      面板开关（本文件）
 *   ① 左右 + 胶片带 + view        ← 用户用鼠标点 toolsbar left / 再点一次关掉
 *   ② 左右 + view
 *   ③ 仅 view
 * ```
 *
 * 两者只有**一处**交叉（人类明确要求，别自作主张扩展到别处）：
 *
 * * **只有切到 ③（仅 view）时才动 LUT 面板**：开着就关掉，并且**记住是我关的**；
 *   本来关着就什么都不做；
 * * **离开 ③ 时**，如果刚才是「我关的」就开回来；**空①② 两档不碰它**（保持用户的开关状态）；
 * * **任何档位下**，用户点 `toolsbar left` 都能把面板开回来 / 关掉 —— 用户操作永远优先，
 *   所以显式开关动作会**清掉**「我关的」这条记忆。
 *
 * 为什么写成纯函数：这三条规则要组合四种情形（本来关 / 本来开 / 在③里手动开回来 / 来回切两轮），
 * 靠肉眼在组件里验太贵；而它没有任何 UI 依赖，最适合做成可测的状态机
 * （与 `lib/viewer-chrome.ts` 同一套做法）。
 *
 * ⚠️ **持久化的是「用户意图」**（`lutOpen`），**不是**「这一刻可不可见」：
 * 用户在 ③ 里退出程序时不该把面板的偏好写成「关」——下次启动他并没见过「仅 view」那一屏。
 */

import {
  chromeAt,
  isLastChromeStep,
  maxChromeStep,
  normalizeChromeStep,
  type ChromeMode,
} from "./viewer-chrome.ts";

/** editor 这一套的档位表（本文件只服务它，避免调用方到处写字符串）。 */
const MODE: ChromeMode = "editor";

export interface EditorChromeState {
  /** `Tab` 档位下标（0 = 默认档） */
  step: number;
  /** LUT 面板的**用户意图**（要持久化的那一个） */
  lutOpen: boolean;
  /** 「进 仅view 时被我关掉了」的记忆（会话级） */
  lutHiddenForViewOnly: boolean;
}

/** 初始状态：档位在第一档，面板开关来自持久化偏好。 */
export function initialEditorChrome(lutOpen: boolean): EditorChromeState {
  return { step: 0, lutOpen, lutHiddenForViewOnly: false };
}

/** 用户点 `toolsbar left`：显式意图，覆盖「我关的」那条记忆（用户操作优先）。 */
export function setEditorLutOpen(
  state: EditorChromeState,
  open: boolean,
): EditorChromeState {
  return { ...state, lutOpen: open, lutHiddenForViewOnly: false };
}

/** 用户按 `Tab`：在 editor 的三档里循环，并处理 ③ 的进出。 */
export function stepEditorTab(state: EditorChromeState): EditorChromeState {
  const cycleLength = maxChromeStep(MODE) + 1;
  const current = normalizeChromeStep(MODE, state.step);
  const step = (current + 1) % cycleLength;

  const enteringViewOnly = isLastChromeStep(MODE, step) && !isLastChromeStep(MODE, current);
  const leavingViewOnly = !isLastChromeStep(MODE, step) && isLastChromeStep(MODE, current);

  if (enteringViewOnly) {
    // 开着才关（本来关着 ⇒ 无动作，也不留记忆）
    return state.lutOpen
      ? { ...state, step, lutOpen: false, lutHiddenForViewOnly: true }
      : { ...state, step };
  }
  if (leavingViewOnly && state.lutHiddenForViewOnly) {
    return { ...state, step, lutOpen: true, lutHiddenForViewOnly: false };
  }
  return { ...state, step };
}

/**
 * LUT 面板这一刻**可不可见**（= 档位允许左列 **且** 面板开着）。
 *
 * 这一个读数同时服务三处，不要为它们各写一个别名（那正是本项目踩过的坑）：
 *
 * * 左列渲染不渲染；
 * * `toolsbar left` 上那个开关**按不按下** —— 按**可见性**判而不是按意图：
 *   ③ 档把面板藏起来时按钮也该跟着弹起来，否则「按钮按下、面板却不在」看着像界面坏了；
 * * 挂空态 / 命令可用性判断。
 */
export function editorLutVisible(state: EditorChromeState): boolean {
  return chromeAt(MODE, state.step).left && state.lutOpen;
}

/** 右列这一刻在不在（与面板开关无关）。 */
export function editorShowsRight(state: EditorChromeState): boolean {
  return chromeAt(MODE, state.step).right;
}

/** 胶片带这一刻在不在。 */
export function editorShowsFilm(state: EditorChromeState): boolean {
  return chromeAt(MODE, state.step).film;
}

/** 档位名（`data-chrome` 属性用）。 */
export function editorChromeName(state: EditorChromeState): string {
  return chromeAt(MODE, state.step).name;
}

/** 复位到第一档（换库 / 换目录时用；面板开关不动 —— 那是用户的持久偏好）。 */
export function resetEditorChrome(state: EditorChromeState): EditorChromeState {
  return { ...state, step: 0, lutHiddenForViewOnly: false };
}
