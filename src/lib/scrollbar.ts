/**
 * 实测「经典滚动条」占掉的宽度（`offsetWidth - clientWidth`）。
 *
 * ## 为什么要实测、不读令牌
 *
 * `styles/scrollbar.css` 给 `*` 设了 `scrollbar-width: thin`；而 Chromium 121+ 一旦看到
 * `scrollbar-width` 不是 `auto`，就**忽略** `::-webkit-scrollbar { width: … }` 那一套。
 * 于是真正生效的是 Chromium 自己的 thin 宽度（本机实测 **10px**），
 * 而 `--scrollbar-w` 令牌（紧凑 8 / 宽松 10）**不是**它的宽度。
 *
 * 后果（2026-09-23 真机量到）：网格的「横向适合窗口」按令牌只留 8px，
 * 有滚动条时最后一列铺到滚动条底下 **2px** —— 正好被切一条边。
 *
 * 所以这里**量一次真的**：造一个强制出滚动条的离屏探针，量 `offsetWidth - clientWidth`。
 * 量不出（0）或没有 DOM（Node 单测）时退回 `fallback` —— **0 不能当宽度用**。
 */
export function measureScrollbarWidth(fallback = 8): number {
  const doc = (globalThis as { document?: Document }).document;
  if (doc === undefined || !doc.body) return fallback;
  const probe = doc.createElement("div");
  probe.style.cssText =
    "position:absolute;top:-9999px;left:0;width:100px;height:100px;overflow:scroll";
  doc.body.appendChild(probe);
  const width = probe.offsetWidth - probe.clientWidth;
  probe.remove();
  return width > 0 ? width : fallback;
}
