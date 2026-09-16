/**
 * CSS 长度解析（纯字符串，不碰 DOM —— 所以在 `lib/` 里，可单测）。
 *
 * 用途：虚拟化要的是**像素数**，而令牌（`--row-h` 之类）在运行时是
 * `"26px"` 这样的字符串。读值那一步在组件里做（`getComputedStyle`），
 * 解析这一步放这里。
 */

/**
 * 把 CSS 长度解析成像素数。
 *
 * * `"26px"` → 26；`"26"` → 26（无单位按 px）；`"26.5px"` → 26.5
 * * `"0"` → 0（合法）
 * * 认不出来（`"1.5rem"`、`"auto"`、空串、NaN）→ `fallback`
 *
 * 只认 px 是**有意的**：令牌里全是 px，支持 rem/em 就得依赖根字号，
 * 而虚拟化算错一点就会让滚动位置漂 —— 认不出来退回一个确定的默认值更安全。
 */
export function parseCssPx(value: string | null | undefined, fallback: number): number {
  if (value == null) return fallback;
  const text = value.trim();
  if (text === "") return fallback;
  const match = /^(-?\d+(?:\.\d+)?)(px)?$/.exec(text);
  if (!match) return fallback;
  const parsed = Number(match[1]);
  if (!Number.isFinite(parsed)) return fallback;
  return parsed;
}
