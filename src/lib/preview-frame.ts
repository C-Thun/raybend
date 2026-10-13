/**
 * browse/editor 共用的全图总览几何。
 *
 * 组件是**统一**的一份：宽度由所在栏（browse 右栏 / editor 左列）给足，
 * **只有高度**跟着照片比例走 —— 所谓「比例变化」就是改高度（宽度不可调，也不必可调）。
 *
 * 高度范围（崔总 2026-10-13 重新约定）：
 *
 * ```text
 *   最矮 = 3:2   ← 标准相机画幅；比 3:2 更宽的图不再压矮，左右留白
 *   最高 = 2:3   ← 比 2:3 更长的图不再拉高，上下留白
 * ```
 *
 * 也就是框高 ∈ `[宽 / 1.5, 宽 × 1.5]`；超出范围的图**保持框高**、整张等比缩进框内居中。
 * 尺寸未知时用 4:3 占位（落在范围内）。
 *
 * 将来若允许栏宽可调，这里不需要另做处理：宽度一变，高度按同一个比例跟着变
 * （实测靠 ResizeObserver 量内容盒，见 `components/ui/PreviewFrame.tsx`）。
 */
export const PREVIEW_FRAME_MIN_ASPECT = 2 / 3;
export const PREVIEW_FRAME_MAX_ASPECT = 3 / 2;
export const PREVIEW_FRAME_FALLBACK_ASPECT = 4 / 3;

export function previewFrameAspect(width: number | undefined, height: number | undefined): number {
  if (width === undefined || height === undefined || !Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    return PREVIEW_FRAME_FALLBACK_ASPECT;
  }
  return Math.max(PREVIEW_FRAME_MIN_ASPECT, Math.min(PREVIEW_FRAME_MAX_ASPECT, width / height));
}

/** 用实际内容盒大小拟合图片，视野框据此贴住图片而不是贴住含留白的外框。 */
export function fitPreviewSize(imageWidth: number, imageHeight: number, boxWidth: number, boxHeight: number): { width: number; height: number } | null {
  if (![imageWidth, imageHeight, boxWidth, boxHeight].every((value) => Number.isFinite(value) && value > 0)) return null;
  const scale = Math.min(boxWidth / imageWidth, boxHeight / imageHeight);
  return { width: imageWidth * scale, height: imageHeight * scale };
}
