/**
 * browse/editor 共用的全图总览几何。
 * 外框随照片比例变化，但宽图最多 3:1、长图最多 3:4；超出后保持框高，
 * 将整张图等比缩进框内。尺寸未知时先用旧的 4:3 占位。
 */
export const PREVIEW_FRAME_MIN_ASPECT = 3 / 4;
export const PREVIEW_FRAME_MAX_ASPECT = 3;
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
