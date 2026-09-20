/**
 * 性能脚本的**页内探针**（`perf-browse` 与 `perf-win` 共用）。
 *
 * 这些字符串会被 `Runtime.evaluate` 注进页面执行 —— 它们必须**自洽**
 * （不能引用模块作用域的任何东西，`perf-browse` 里 `T0` 漏网过一次）。
 *
 * 为什么抽出来：滚动采样这一段在两个脚本里**逐字相同**（`AGENTS.md` §2.12
 * 「同一个能力只允许有一套实现」）—— 无头量的是 JS 预算，真机量的是 GPU 合成，
 * 采法必须一模一样，数字才可比。
 */

/**
 * 程序化滚动 `durationMs` 毫秒，采 `requestAnimationFrame` 间隔。
 *
 * 返回：`{ frames, p50, p95, worst, scrolledTo, tiles }`（毫秒，保留两位）。
 * 找不到虚拟滚动容器时返回 `{ error }`。
 */
export const SCROLL_PROBE = `
(async (durationMs) => {
  const scroller = document.querySelector("main [data-virtual-scroller]");
  if (!scroller) return { error: "找不到虚拟滚动容器" };
  const deltas = [];
  let last = performance.now();
  let stop = false;
  const tick = () => {
    const now = performance.now();
    deltas.push(now - last);
    last = now;
    if (!stop) requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
  const started = performance.now();
  let y = scroller.scrollTop;
  while (performance.now() - started < durationMs) {
    y += 600;
    scroller.scrollTop = y;
    await new Promise((resolve) => requestAnimationFrame(resolve));
  }
  stop = true;
  await new Promise((resolve) => setTimeout(resolve, 100));
  const sorted = deltas.slice(2).sort((a, b) => a - b);
  const at = (q) => (sorted.length === 0 ? 0 : sorted[Math.min(sorted.length - 1, Math.round(q * (sorted.length - 1)))]);
  return {
    frames: sorted.length,
    p50: Math.round(at(0.5) * 100) / 100,
    p95: Math.round(at(0.95) * 100) / 100,
    worst: sorted.length === 0 ? 0 : Math.round(sorted[sorted.length - 1] * 100) / 100,
    scrolledTo: Math.round(scroller.scrollTop),
    tiles: document.querySelectorAll('main [data-virtual-scroller] [role="option"]').length,
  };
})`;

/** 可见 tile 数（网格挂没挂、换没换内容，都看它）。 */
export const TILES_PROBE = `(() => ({
  count: document.querySelectorAll('main [data-virtual-scroller] [role="option"]').length,
  first: document.querySelector('main [data-virtual-scroller] [role="option"]')?.textContent?.slice(0, 20) ?? null,
}))()`;

/** 真机才有的环境事实（视口 / DPR / 缩放 —— 报告里必带，`AGENTS.md` §7.9 的口径）。 */
export const VIEWPORT_PROBE = `(() => ({
  inner: [window.innerWidth, window.innerHeight],
  outer: [window.outerWidth, window.outerHeight],
  dpr: window.devicePixelRatio,
  screen: [screen.width, screen.height],
  lang: document.documentElement.lang || null,
  title: document.title,
  href: location.href,
}))()`;
