/**
 * 看图态右栏「预览全图」那块**框**的几何（`BROWSE.md` §5.9、`DESIGN.md` §13.2）。
 *
 * ## 为什么框要有固定比例
 *
 * 以前框是按窗口高度写死 180px、里面的照片盒按原图比例 —— 于是**纵图**能占的高度
 * 由那个像素数决定，换面板宽度也不变。人类 2026-09-20 定：框改成**固定 4:3**
 * （不是 3:2，也不是跟着照片走）——
 *
 * > 「比例改成 4:3，不要 3:2，这样对纵图支持更好，同时我也是个 4/3 系统爱好者」
 *
 * 于是同一块面板宽度下：横图铺满宽度（4:3 比 3:2 高 ⇒ 预览整体更大），
 * 纵图铺满高度（高度 = 宽度 × 3/4 ⇒ 比原来的 180px 更宽松）。
 *
 * ## 装进去的办法：按哪条边对齐
 *
 * 框固定住之后，照片盒只需要**铺满其中一条边**，另一边按原图比例算 ——
 * 因为框的 4:3 是「夹在横竖之间」的比例，铺满的那条边必定装得下，不需要再夹取。
 */

/** 预览框的固定比例（人类 2026-09-20 口述定的 4:3，别改回 3:2）。 */
export const PREVIEW_FRAME_ASPECT = 4 / 3;

/**
 * 一张 `imgW × imgH` 的照片装进 `frameAspect` 的框里，该**铺满哪条边**。
 *
 * * `"width"`  = 照片不比框窄（横图 / 正方）⇒ 宽度铺满，高度按比例算；
 * * `"height"` = 照片比框更高窄（纵图）⇒ 高度铺满，宽度按比例算。
 *
 * 尺寸非法（`0` / 非有限值）时按 `"width"` —— 调用方本来就该在尺寸未知时
 * 不画照片盒（只出占位文案），这里只是不制造 `NaN`。
 */
export function fitAxisFor(imgW: number, imgH: number, frameAspect: number): "width" | "height" {
  if (
    !Number.isFinite(imgW) ||
    !Number.isFinite(imgH) ||
    !Number.isFinite(frameAspect) ||
    imgW <= 0 ||
    imgH <= 0 ||
    frameAspect <= 0
  ) {
    return "width";
  }
  // 等号归 width：恰姀 4:3 两边结果一样；比它窄的（含 1:1）都走 height
  return imgW / imgH >= frameAspect ? "width" : "height";
}
