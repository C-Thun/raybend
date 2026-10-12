/**
 * 编辑左列的**布局数学**（唯一出处；画稿 `design/editor.md` §3.1 / §3.13）。
 *
 * 左列宽 280（紧凑档画布值，两个面板共用一份宽度 —— 不许各写一份），
 * 底部块的高度按「页签 + 预览（随宽度 3:2）+ 缩放行」算出来：
 *
 * ```text
 * 底部块（总览态）= 页签 30 + 间隙 8 + 预览（内容宽 / 1.5）+ 间隙 8 + 缩放行 24
 * 底部块（信息态）= 总览态 × 1.8   ← 崔总 2026-10-12：「高度升高大约 80%」
 * ```
 *
 * 用 CSS `calc` 而不是算好的像素值：密度档（`--panel-pad`）一变，
 * 预览的宽度跟着变，高度必须自己跟上。
 */

/** 编辑左列宽度（LUT 面板与定稿面板共用）。 */
export const EDITOR_LEFT_WIDTH = 280;

/** 底部块（总览态）高度。 */
export const EDITOR_LEFT_BOTTOM_VIEW =
  `calc((${EDITOR_LEFT_WIDTH}px - 2 * var(--panel-pad)) / 1.5 + 70px)`;

/** 底部块（信息态）高度：整块 ×1.8，挤占上方 LUT / 定稿列表的空间。 */
export const EDITOR_LEFT_BOTTOM_INFO = `calc((${EDITOR_LEFT_BOTTOM_VIEW}) * 1.8)`;
