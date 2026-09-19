/**
 * 看图时的三种显示状态（`BROWSE.md` §5.4 / §10 问题 3，人类 2026-09-18 定为**并列三态**）。
 *
 * ```text
 * Tab →
 *  ① 默认          [左][  图  ][右]
 *                  [  胶片带   ]
 * Tab →
 *  ② film only     [    图     ]
 *                  [  胶片带   ]
 * Tab →
 *  ③ view only     [    图     ]
 * Tab → 回到 ①
 * ```
 *
 * 三条纪律：
 *
 * 1. **三项就是这三项**（人类 2026-09-19 纠正）：`film+左右` → `film only` → `view only`。
 *    **没有** `view+左右` 这种组合 —— ③ 是「只看图」：左右栏与胶片带**一起收起**，
 *    把整个中列让给照片。
 * 2. **并列**，不是叠加 —— 三个状态各自独立，不是「在②的基础上再关一层」。
 *    从 ③ 再按 Tab 一步回到 ①，不绕路。
 * 3. **退回 tiles 时左右栏必定回来**（`BROWSE.md` §5.4）——
 *    所以退出看图时状态重置为 `default`，不是记着上次那个。**tiles 里没有「最大化」这回事**。
 *
 * 抽成纯函数的原因：这是「按 Tab 三下必须回到原样」的循环，错了很难看出来；
 * 而它又没有任何 UI 依赖。
 */

/** 看图时的外壳状态。命名口径：说「显示成什么样」，不说「关了哪个」—— 后者一改就歧义。 */
export type ViewerChrome = "default" | "film-only" | "view-only";

/** 循环顺序（`Tab` 走这个顺序）。 */
export const CHROME_CYCLE: readonly ViewerChrome[] = ["default", "film-only", "view-only"];

/** 下一个状态（到末尾回到开头）。 */
export function nextChrome(current: ViewerChrome): ViewerChrome {
  const at = CHROME_CYCLE.indexOf(current);
  // 认不出来的值（将来加了新状态、老代码还在跑）就当默认态 —— 比抛错温和，且行为可预期
  if (at === -1) return "default";
  return CHROME_CYCLE[(at + 1) % CHROME_CYCLE.length] ?? "default";
}

/**
 * 左右两列显示吗（① 显示；②③ 都收起 —— ③ 是「只看图」，连左右一起收）。
 *
 * 认不出来的值按**默认态**处理（与 `nextChrome` 同一口径）：
 * 宁可多显示一点，也不要因为一个陌生字符串把界面凭空收没了。
 */
export function chromeShowsSides(chrome: ViewerChrome): boolean {
  return chrome !== "film-only" && chrome !== "view-only";
}

/** 胶片带显示吗（①② 显示；③ 是「只看图」，胶片带也收起来）。 */
export function chromeShowsFilm(chrome: ViewerChrome): boolean {
  return chrome !== "view-only";
}
