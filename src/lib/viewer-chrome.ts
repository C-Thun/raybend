/**
 * 看图的**外壳档位**（`prompts/editor.pd` 2026-09-23 定；原 `BROWSE.md` §5.4 只有 browse 一套）。
 *
 * 一个「档位」= 这一屏显示哪几块：**左列 / 右列 / 胶片带**（view 永远在）。
 * `Tab` 在同一个 flow 的档位表里循环。
 *
 * ```text
 * browse（四档，**保持 M2 的现状不变**）
 *   ① default      [左][  图  ][右]     ③ film-only    [    图     ]
 *                  [  胶片带   ]                        [  胶片带   ]
 *   ② film-right   [  图  ][右]         ④ view-only    [    图     ]
 *                  [胶片带][右]
 *
 * import（三档，人类 2026-09-23 改口径：中间那档**先丢左右两列、留胶片带**）
 *   ① default      [左][  图  ][右]     ③ view-only    [    图     ]
 *                  [  胶片带   ]
 *   ② film-only    [    图     ]
 *                  [  胶片带   ]
 *
 * editor（三档，人类 2026-09-23 定：**先藏胶片带**，因为编辑时它比左右面板更常被让位）
 *   ① default      [左][  图  ][右]     ③ view-only    [    图     ]
 *                  [  胶片带   ]
 *   ② view         [左][  图  ][右]
 * ```
 *
 * 三条纪律（沿用 browse 那一版）：
 *
 * 1. 顺序固定，循环回到第一档；
 * 2. **退回 tiles 时左右栏必定回来** —— 退出看图走 `resetChrome()`，不是记着上次那个；
 * 3. 档位是**纯数据**：`data-chrome` 属性直接用它，冒烟脚本与以后接胶片带都读同一处。
 *
 * ⚠️ editor 的「第 ③ 档关掉 LUT 面板、离开时恢复」**不在这里**：
 * 那是「面板开关」与「档位」两个轴的交互，住在 `lib/editor-chrome.ts`（那一侧有自己的状态机与测试）。
 * 这里只管「这一档要不要给左列留位置」。
 */

/** 一档：这一屏显示哪几块。 */
export interface ChromeState {
  left: boolean;
  right: boolean;
  film: boolean;
  /** 档位名（`data-chrome` 用；冒烟脚本与历史文档都按它认） */
  name: string;
}

/** 哪一套档位表（`ARCHITECTURE.md` §3：由工作区决定，不是全局开关）。 */
export type ChromeMode = "browse" | "import" | "editor";

export const CHROME_MODES: readonly ChromeMode[] = ["browse", "import", "editor"];

/** 三套循环表。**顺序即语义**（`Tab` 按这个顺序走），不要重排。 */
export const CHROME_CYCLES: Record<ChromeMode, readonly ChromeState[]> = {
  browse: [
    { left: true, right: true, film: true, name: "default" },
    { left: false, right: true, film: true, name: "film-right" },
    { left: false, right: false, film: true, name: "film-only" },
    { left: false, right: false, film: false, name: "view-only" },
  ],
  import: [
    { left: true, right: true, film: true, name: "default" },
    { left: false, right: false, film: true, name: "film-only" },
    { left: false, right: false, film: false, name: "view-only" },
  ],
  editor: [
    { left: true, right: true, film: true, name: "default" },
    { left: true, right: true, film: false, name: "view" },
    { left: false, right: false, film: false, name: "view-only" },
  ],
};

/** 非法 mode（老代码/存储里的垃圾）一律按 browse —— 与以前 `nextChrome` 的宽容口径一致。 */
export function normalizeChromeMode(mode: unknown): ChromeMode {
  return typeof mode === "string" && (CHROME_MODES as readonly string[]).includes(mode)
    ? (mode as ChromeMode)
    : "browse";
}

export function chromeCycle(mode: unknown): readonly ChromeState[] {
  return CHROME_CYCLES[normalizeChromeMode(mode)];
}

/** 档位数是「循环长度 − 1」（第 0 档是默认；下标越界一律夹回来）。 */
export function maxChromeStep(mode: unknown): number {
  return chromeCycle(mode).length - 1;
}

/** 把任意输入夹成合法档位下标。不是数字/越界/NaN 都回到 0（默认档）。 */
export function normalizeChromeStep(mode: unknown, step: unknown): number {
  const max = maxChromeStep(mode);
  if (typeof step !== "number" || !Number.isFinite(step)) return 0;
  return Math.min(max, Math.max(0, Math.trunc(step)));
}

/** 取某一档（越界夹回，不会返回 `undefined`）。 */
export function chromeAt(mode: unknown, step: unknown): ChromeState {
  const cycle = chromeCycle(mode);
  return cycle[normalizeChromeStep(mode, step)] ?? cycle[0]!;
}

/** 下一档（到末尾回到第一档）。 */
export function nextChromeStep(mode: unknown, step: unknown): number {
  const cycle = chromeCycle(mode);
  return (normalizeChromeStep(mode, step) + 1) % cycle.length;
}

/** 这一档显示左列吗。 */
export function chromeShowsLeft(mode: unknown, step: unknown): boolean {
  return chromeAt(mode, step).left;
}

/** 这一档显示右列吗。 */
export function chromeShowsRight(mode: unknown, step: unknown): boolean {
  return chromeAt(mode, step).right;
}

/** 这一档显示胶片带吗。 */
export function chromeShowsFilm(mode: unknown, step: unknown): boolean {
  return chromeAt(mode, step).film;
}

/** 这一档的名字（`data-chrome` 属性用）。 */
export function chromeName(mode: unknown, step: unknown): string {
  return chromeAt(mode, step).name;
}

/** 最后一档（editor / import 的「仅 view」）：`lib/editor-chrome.ts` 判「进出仅 view」要用。 */
export function isLastChromeStep(mode: unknown, step: unknown): boolean {
  return normalizeChromeStep(mode, step) === maxChromeStep(mode);
}
