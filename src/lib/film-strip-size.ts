/**
 * 胶片带缩略 tile 的 17 档尺寸。
 *
 * 这里的数字是 **tile 高度**，不是胶片带总高度：总高度始终由
 * `tile height + 上下固定边距` 推导，避免再次出现「只把条加高，照片没变大」。
 * 宽度沿用胶片带原来的 6:5 容器比例；照片本身仍由 `object-contain` 完整显示。
 */
export const FILM_STRIP_TILE_HEIGHT_STEPS = Object.freeze(
  Array.from({ length: 17 }, (_, index) => 96 + index * 9),
);

/** 96 + 4 × 9 = 132px：偏小、仍落在用户指定的 130–140px 默认范围。 */
export const DEFAULT_FILM_STRIP_STEP = 4;

/** 上、下各 8px；细滚动指示条绝对定位，不额外占高度。 */
export const FILM_STRIP_PADDING_Y = 8;

export interface FilmStripMetric {
  step: number;
  tileHeight: number;
  tileWidth: number;
  stripHeight: number;
}

export function clampFilmStripStep(step: number): number {
  if (!Number.isFinite(step)) return DEFAULT_FILM_STRIP_STEP;
  return Math.min(
    FILM_STRIP_TILE_HEIGHT_STEPS.length - 1,
    Math.max(0, Math.round(step)),
  );
}

export function filmStripMetric(step: number): FilmStripMetric {
  const at = clampFilmStripStep(step);
  const tileHeight = FILM_STRIP_TILE_HEIGHT_STEPS[at] ?? 132;
  return {
    step: at,
    tileHeight,
    tileWidth: Math.round(tileHeight * 1.2),
    stripHeight: tileHeight + FILM_STRIP_PADDING_Y * 2,
  };
}

/** `direction > 0` 放大，`direction < 0` 缩小；一次滚轮只走一档。 */
export function nextFilmStripStep(step: number, direction: number): number {
  if (direction === 0 || !Number.isFinite(direction)) return clampFilmStripStep(step);
  return clampFilmStripStep(step + (direction > 0 ? 1 : -1));
}
