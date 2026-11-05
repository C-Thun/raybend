/**
 * 覆盖层的**指针 ↔ 图像像素**换算（`AGENTS.md` §6.1 红线 2 / §7.9）。
 *
 * # 这里为什么不算「视口数学」
 *
 * 缩放、平移、DPR、旋转**全部在 Rust**（`Viewport::css_overlay_transform`），
 * 前端拿到的就是那一个仿射矩阵。本模块只做**逆变换**：
 * 「指针落在图像的第几个像素」与「指针移动多少 CSS 像素 = 图像移动多少像素」。
 * 前端**不**自己乘 zoom、不自己减 pan、不自己补 DPR —— 那才是被禁止的那件事。
 *
 * 控制点标记的圆点 / 引线 / handle 尺寸是**屏幕像素固定**的，所以还要用
 * Rust 报的 `overlayScale`（= zoom / dpr）与 `overlayRotation` 反向抵消。
 */

/** 2D 仿射矩阵（与 CSS `matrix(a, b, c, d, e, f)`、Rust 的 `[f32; 6]` 同一个顺序）。 */
export type OverlayMatrix = readonly [number, number, number, number, number, number];

export interface OverlayPoint {
  x: number;
  y: number;
}

/**
 * CSS 点 → **归一化图像坐标**（0..1）；矩阵退化（缩放为 0）时返回 `null`。
 *
 * 调用方传的必须是**洞口内**的 CSS 坐标（`Viewport::css_overlay_transform` 的口径：
 * `clip_rect` 的原点已经减过）——不是窗口 / 屏幕的绝对坐标。
 *
 * `width` / `height` 是**图像**（显示画幅）的像素尺寸 —— 来自渲染状态的 `image`。
 */
export function normalizeFromCss(
  matrix: OverlayMatrix,
  width: number,
  height: number,
  css: OverlayPoint,
): OverlayPoint | null {
  if (!(width > 0) || !(height > 0)) return null;
  const [a, b, c, d, e, f] = matrix;
  const det = a * d - b * c;
  if (!Number.isFinite(det) || Math.abs(det) < 1e-9) return null;
  const dx = css.x - e;
  const dy = css.y - f;
  const px = (d * dx - c * dy) / det;
  const py = (-b * dx + a * dy) / det;
  if (!Number.isFinite(px) || !Number.isFinite(py)) return null;
  return { x: px / width, y: py / height };
}

/**
 * CSS 位移 → **图像像素**位移（拖动时用；`scale` / `rotation` 都来自 Rust）。
 *
 * 退化比例（≤0）时返回零位移（不猜）。
 */
export function imageDeltaFromCss(
  scale: number,
  rotationDegrees: number,
  dx: number,
  dy: number,
): OverlayPoint {
  if (!Number.isFinite(scale) || scale <= 1e-9) return { x: 0, y: 0 };
  const radians = ((Number.isFinite(rotationDegrees) ? rotationDegrees : 0) * Math.PI) / 180;
  const cos = Math.cos(radians);
  const sin = Math.sin(radians);
  // 反向旋转再除以比例：矩阵的线性部分就是 [scale·cos, scale·sin; −scale·sin, scale·cos]
  return {
    x: (cos * dx + sin * dy) / scale,
    y: (-sin * dx + cos * dy) / scale,
  };
}

/** 归一化坐标 → **图像像素**（覆盖层子元素写图像像素）。 */
export function cssOfNormalized(
  width: number,
  height: number,
  point: OverlayPoint,
): OverlayPoint {
  return { x: point.x * width, y: point.y * height };
}
