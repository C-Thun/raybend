/**
 * 看图时的四种显示状态（`BROWSE.md` §5.4，人类 2026-09-20 定）。
 *
 * ```text
 * Tab →
 *  ① 默认          [左][  图  ][右]
 *                  [  胶片带   ]
 * Tab →
 *  ② film + right  [  图  ][右]
 *                  [胶片带][右]
 * Tab →
 *  ③ film only     [    图     ]
 *                  [  胶片带   ]
 * Tab →
 *  ④ view only     [    图     ]
 * Tab → 回到 ①
 * ```
 *
 * 三条纪律：
 *
 * 1. 顺序固定：`film+左右` → `film+右` → `film only` → `view only`。
 *    第二档只移除左边的库 / 目录列，保留右边信息栏。
 * 2. 四个状态各自独立；从 ④ 再按 Tab 一步回到 ①。
 * 3. **退回 tiles 时左右栏必定回来**（`BROWSE.md` §5.4）——
 *    所以退出看图时状态重置为 `default`，不是记着上次那个。**tiles 里没有「最大化」这回事**。
 *
 * 抽成纯函数的原因：这是「按 Tab 四下必须回到原样」的循环，错了很难看出来；
 * 而它又没有任何 UI 依赖。
 */

/** 看图时的外壳状态。命名口径：说「显示成什么样」，不说「关了哪个」—— 后者一改就歧义。 */
export type ViewerChrome = "default" | "film-right" | "film-only" | "view-only";

/** 循环顺序（`Tab` 走这个顺序）。 */
export const CHROME_CYCLE: readonly ViewerChrome[] = [
  "default",
  "film-right",
  "film-only",
  "view-only",
];

/** 下一个状态（到末尾回到开头）。 */
export function nextChrome(current: ViewerChrome): ViewerChrome {
  const at = CHROME_CYCLE.indexOf(current);
  // 认不出来的值（将来加了新状态、老代码还在跑）就当默认态 —— 比抛错温和，且行为可预期
  if (at === -1) return "default";
  return CHROME_CYCLE[(at + 1) % CHROME_CYCLE.length] ?? "default";
}

/**
 * 左列只在默认态显示；第二档开始就收起。
 */
export function chromeShowsLeft(chrome: ViewerChrome): boolean {
  return chrome === "default" || !CHROME_CYCLE.includes(chrome);
}

/** 右列在前两档显示；`film-only` / `view-only` 收起。 */
export function chromeShowsRight(chrome: ViewerChrome): boolean {
  return chrome === "default" || chrome === "film-right" || !CHROME_CYCLE.includes(chrome);
}

/** 胶片带在前三档显示；④ `view-only` 收起。 */
export function chromeShowsFilm(chrome: ViewerChrome): boolean {
  return chrome !== "view-only" || !CHROME_CYCLE.includes(chrome);
}
