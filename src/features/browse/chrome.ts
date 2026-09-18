/**
 * 看图时的三种显示状态（`BROWSE.md` §5.4 / §10 问题 3，人类 2026-09-18 定为**并列三态**）。
 *
 * ```text
 * Tab →
 *  ① 默认        [左][  图  ][右]
 *                [  胶片带   ]
 * Tab →
 *  ② 关左右      [    图     ]
 *                [  胶片带   ]
 * Tab →
 *  ③ 关胶片带    [左][  图  ][右]
 * Tab → 回到 ①
 * ```
 *
 * 两条纪律：
 *
 * 1. **并列**，不是叠加 —— ③ 不是「在 ② 的基础上再关胶片带」，而是「只关胶片带、左右回来」。
 *    这样从 ③ 再按 Tab 回到 ① 只需要一步，不会绕路。
 * 2. **退回 tiles 时左右栏必定回来**（`BROWSE.md` §5.4）——
 *    所以退出看图时状态重置为 `default`，不是记着上次那个。**tiles 里没有「最大化」这回事**。
 *
 * 抽成纯函数的原因：这是「按 Tab 三下必须回到原样」的循环，错了很难看出来；
 * 而它又没有任何 UI 依赖。
 */

/** 看图时的外壳状态。 */
export type ViewerChrome = "default" | "no-sides" | "no-film";

/** 循环顺序（`Tab` 走这个顺序）。 */
export const CHROME_CYCLE: readonly ViewerChrome[] = ["default", "no-sides", "no-film"];

/** 下一个状态（到末尾回到开头）。 */
export function nextChrome(current: ViewerChrome): ViewerChrome {
  const at = CHROME_CYCLE.indexOf(current);
  // 认不出来的值（将来加了新状态、老代码还在跑）就当默认态 —— 比抛错温和，且行为可预期
  if (at === -1) return "default";
  return CHROME_CYCLE[(at + 1) % CHROME_CYCLE.length] ?? "default";
}

/** 左右两列显示吗（①③ 显示，② 不显示）。 */
export function chromeShowsSides(chrome: ViewerChrome): boolean {
  return chrome !== "no-sides";
}

/** 胶片带显示吗（①② 显示，③ 不显示）。 */
export function chromeShowsFilm(chrome: ViewerChrome): boolean {
  return chrome !== "no-film";
}
