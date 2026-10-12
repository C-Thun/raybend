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
 * * **只有切到 ③（仅 view）时才动左列面板**：开着就关掉，并且**记住是哪一个、是我关的**；
 *   本来关着就什么都不做；
 * * **离开 ③ 时**，如果刚才是「我关的」就把那一个开回来；**空①② 两档不碰它**（保持用户的开关状态）；
 * * **任何档位下**，用户点 `toolsbar left` 都能把面板开回来 / 关掉 —— 用户操作永远优先，
 *   所以显式开关动作会**清掉**「我关的」这条记忆；
 * * ⤴️ 补一条（人类 2026-09-23 晚）：「用户操作优先」要**真的看得到效果** ——
 *   在 ③ 里点开关，面板就得在 ③ 里开出来（而不是只把意图记下来、等出了 ③ 才显形）。
 *   所以 ③ 里显式开启会亮一枚临时通行证（`leftForcedInViewOnly`），
 *   它**只在本次 ③ 期间有效**，进出 ③ 都会被清掉。
 *
 * 左列有**两个面板**（2026-10-12 改版：LUT 与定稿），同一时刻只开一个 ——
 * 点另一个开关就是切过去（再点同一个才关）。面板的「用户意图」存在 `leftPanel` 里，
 * `null` = 左列整个关着（连底部总览 / 信息一起消失）。
 *
 * 为什么写成纯函数：这些规则要组合四种情形（本来关 / 本来开 / 在③里手动开回来 / 来回切两轮），
 * 靠肉眼在组件里验太贵；而它没有任何 UI 依赖，最适合做成可测的状态机
 * （与 `lib/viewer-chrome.ts` 同一套做法）。
 *
 * ⚠️ **持久化的是「用户意图」**（`leftPanel`），**不是**「这一刻可不可见」：
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

/** 左列的两个面板（同一时刻只开一个）；`null` = 左列整个关着。 */
export type EditorLeftPanel = "lut" | "issues";

export interface EditorChromeState {
  /** `Tab` 档位下标（0 = 默认档） */
  step: number;
  /** 左列面板的**用户意图**（要持久化的那一个）；`null` = 关着 */
  leftPanel: EditorLeftPanel | null;
  /** 「进 仅view 时被我关掉了」的记忆（会话级）：记住被关掉的是哪一个 */
  leftHiddenForViewOnly: EditorLeftPanel | null;
  /**
   * 【仅在 ③（仅 view）里有意义】用户在 ③ 里显式把左列面板开回来了。
   *
   * 为何需要它：③ 档的档位表说「左列位置不给」，而如果可见性只由档位决定，
   * 用户在 ③ 里点开关就会「按了没反应」（人类 2026-09-23 报的就是这个）。
   * 这一位是**临时通行证**：只在当前这次 ③ 期间有效，进出 ③ 都清掉。
   *
   * 名字带 `Left` 而不是 `Lut`：左列有多个面板（LUT / 定稿），
   * 人类要的是「**不锁任何左列面板开关**」，所以这枚通行证属于整个左列。
   */
  leftForcedInViewOnly: boolean;
}

/** 初始状态：档位在第一档，面板意图来自持久化偏好。 */
export function initialEditorChrome(leftPanel: EditorLeftPanel | null): EditorChromeState {
  return { step: 0, leftPanel, leftHiddenForViewOnly: null, leftForcedInViewOnly: false };
}

/**
 * 用户点 `toolsbar left` 的某个开关：
 *
 * * 该面板**已经可见** → 关掉整列（`null`）；
 * * 另一个面板可见 → **切过去**（同一时刻只开一个）；
 * * 左列没开 → 开这个。
 *
 * 显式意图覆盖「我关的」那条记忆（用户操作优先）；在 ③ 里显式开启亮临时通行证。
 */
export function toggleEditorLeftPanel(
  state: EditorChromeState,
  panel: EditorLeftPanel,
): EditorChromeState {
  const visible = editorLeftPanelVisible(state) ? state.leftPanel : null;
  return setEditorLeftPanel(state, visible === panel ? null : panel);
}

/** 直接设置左列面板（`null` = 全关）。 */
export function setEditorLeftPanel(
  state: EditorChromeState,
  panel: EditorLeftPanel | null,
): EditorChromeState {
  return {
    ...state,
    leftPanel: panel,
    leftHiddenForViewOnly: null,
    // 在 ③ 里显式开启 = 临时越过档位（否则「点了没反应」）；其余情况一律清掉
    leftForcedInViewOnly: panel !== null && isLastChromeStep(MODE, state.step),
  };
}

/** 用户按 `Tab`：在 editor 的三档里循环，并处理 ③ 的进出。 */
export function stepEditorTab(state: EditorChromeState): EditorChromeState {
  const cycleLength = maxChromeStep(MODE) + 1;
  const current = normalizeChromeStep(MODE, state.step);
  const step = (current + 1) % cycleLength;

  const enteringViewOnly = isLastChromeStep(MODE, step) && !isLastChromeStep(MODE, current);
  const leavingViewOnly = !isLastChromeStep(MODE, step) && isLastChromeStep(MODE, current);

  if (enteringViewOnly) {
    // 进 ③：关掉（开着的话）+ 记住关的是哪一个；同样清掉上一次 ③ 留下的通行证
    const next = { ...state, step, leftForcedInViewOnly: false };
    return state.leftPanel !== null
      ? { ...next, leftPanel: null, leftHiddenForViewOnly: state.leftPanel }
      : next;
  }
  if (leavingViewOnly && state.leftHiddenForViewOnly !== null) {
    return {
      ...state,
      step,
      leftPanel: state.leftHiddenForViewOnly,
      leftHiddenForViewOnly: null,
      leftForcedInViewOnly: false,
    };
  }
  return { ...state, step, leftForcedInViewOnly: false };
}

/**
 * 左列这一刻**可不可见**（= （档位允许左列 **或** ③ 里的临时通行证）**且** 有面板开着）。
 *
 * 这一个读数同时服务三处，不要为它们各写一个别名（那正是本项目踩过的坑）：
 *
 * * 左列渲染不渲染（连底部总览 / 信息一起）；
 * * `toolsbar left` 上那两个开关**按不按下** —— 按**可见性**判而不是按意图：
 *   ③ 档把面板藏起来时按钮也该跟着弹起来，否则「按钮按下、面板却不在」看着像界面坏了；
 * * 挂空态 / 命令可用性判断。
 */
export function editorLeftPanelVisible(state: EditorChromeState): boolean {
  return (chromeAt(MODE, state.step).left || state.leftForcedInViewOnly) && state.leftPanel !== null;
}

/** 这一刻开着的是不是 LUT 面板（按钮的按下态用它）。 */
export function editorLutVisible(state: EditorChromeState): boolean {
  return editorLeftPanelVisible(state) && state.leftPanel === "lut";
}

/** 这一刻开着的是不是定稿面板（按钮的按下态用它）。 */
export function editorIssuesVisible(state: EditorChromeState): boolean {
  return editorLeftPanelVisible(state) && state.leftPanel === "issues";
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

/** 复位到第一档（换库 / 换目录时用；面板意图不动 —— 那是用户的持久偏好）。 */
export function resetEditorChrome(state: EditorChromeState): EditorChromeState {
  return { ...state, step: 0, leftHiddenForViewOnly: null, leftForcedInViewOnly: false };
}
