/** 胶片带上一项在横向内容坐标系里的几何。 */
export interface FilmStripItemMetric {
  id: string;
  start: number;
  size: number;
}

/** 参考项 + 它的左边缘离视口左边缘多少像素（可为负：项只露出后半截）。 */
export interface FilmStripAnchor {
  id: string;
  offsetPx: number;
}

function sane(value: number): number {
  return Number.isFinite(value) ? Math.max(0, value) : 0;
}

/** 抓住第一项仍有像素露在视口里的照片。 */
export function captureFilmStripAnchor(
  items: readonly FilmStripItemMetric[],
  scrollLeft: number,
): FilmStripAnchor | null {
  const scroll = sane(scrollLeft);
  for (const item of items) {
    const start = sane(item.start);
    const size = sane(item.size);
    if (start + size > scroll) return { id: item.id, offsetPx: start - scroll };
  }
  return null;
}

/**
 * 内容换过以后把参考项钉回原像素；参考项消失时退到当前照片。
 * 返回 `null` 表示两者都不在新列表里，调用方保持原位。
 */
export function restoreFilmStripScroll(
  items: readonly FilmStripItemMetric[],
  anchor: FilmStripAnchor,
  fallbackId: string | null,
): number | null {
  const target =
    items.find((item) => item.id === anchor.id) ??
    (fallbackId === null ? undefined : items.find((item) => item.id === fallbackId));
  if (target === undefined) return null;
  const offset = Number.isFinite(anchor.offsetPx) ? anchor.offsetPx : 0;
  return Math.max(0, sane(target.start) - offset);
}
