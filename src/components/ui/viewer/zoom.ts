/**
 * 「100%（1:1）」在**浏览器 CSS 坐标**里的倍率 —— 全项目唯一一份。
 *
 * ## 为什么要这个函数（崔总 2026-10-11 真机报的 bug）
 *
 * 看图的视图用 `<img>` + CSS `transform: scale(zoom)` 画（对比视图同理，画布也走 transform）。
 * 这里的 `zoom` 单位是「**CSS 像素 / 图像像素**」。如果按 `zoom = 1` 当 100%，那么在
 * DPR=2 的屏幕上，一张图会被画成「1 图像像素 = 2 设备像素」——用户看到的 100% 其实是 200%，
 * 而且浏览器把全尺寸图放大 2 倍 ⇒ 100% 看着发虚。
 *
 * 编辑器那边的原生视口按坐标契约（`AGENTS.md` §7.9）走的是**物理像素**：
 * `Viewport.zoom = 1.0` 恒为 1:1（1 图像像素 = 1 设备像素）。两边必须同口径，
 * 所以 DOM 侧的「100%」= `1 / devicePixelRatio`（CSS 缩放）。
 *
 * ## 纪律
 *
 * * **DPR 读运行时值**（本机 125% × 文字 110% = 1.375，写死必错 —— `memory/ARCHITECTURE.md` §7.9）；
 * * 换算**只在这里做一次**：视图与控件要 100%，一律调 [`oneToOneZoom`]；要显示百分比，
 *   一律调 [`zoomPercent`]。别在别处再写一个 `* 100` 或 `1 / dpr`。
 */

/**
 * 运行时 DPR（浏览器里就是 `window.devicePixelRatio`）。
 *
 * 用 `globalThis` 而不是 `window`：前端单测跑在裸 node 上（`node --test`，没有 DOM），
 * `globalThis.devicePixelRatio` 读不到就是 1 —— 不需要任何运行时能力探测。
 */
export function devicePixelRatioNow(): number {
  const dpr = (globalThis as { devicePixelRatio?: number }).devicePixelRatio ?? 1;
  return Number.isFinite(dpr) && dpr > 0 ? dpr : 1;
}

/**
 * 「100%」对应的 CSS 缩放（1 图像像素 = 1 设备像素）。
 *
 * `dpr` 只给单测注入用；生产代码不要传参（要读运行时值）。
 */
export function oneToOneZoom(dpr: number = devicePixelRatioNow()): number {
  return 1 / (Number.isFinite(dpr) && dpr > 0 ? dpr : 1);
}

/** CSS 缩放 → 用户看到的百分比（100 = 1:1）。 */
export function zoomPercent(zoom: number, dpr: number = devicePixelRatioNow()): number {
  return zoom * (Number.isFinite(dpr) && dpr > 0 ? dpr : 1) * 100;
}

/** 这一刻是不是「100%」（浮点比较，给 UI 判状态用）。 */
export function isOneToOne(zoom: number, dpr: number = devicePixelRatioNow()): boolean {
  return Math.abs(zoom - oneToOneZoom(dpr)) < 1e-6;
}
