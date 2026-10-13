/**
 * 编辑左列的**布局数学**（唯一出处；画稿 `design/editor.md` §3.1 / §3.11）。
 *
 * 左列宽 280（LUT 与定稿两个面板共用一份宽度），底部块的高度按「页签 + 预览 + 缩放行」算：
 *
 * ```text
 * 底部块（总览态）= 页签条 + 8 + 预览（内容宽 / 当前比例）+ 8 + 缩放行 24 + 上下内边距
 *                   ↑ 预览比例来自 PreviewFrame（夹在 3:2～2:3），由 `--preview-aspect` 传进来
 * 底部块（信息态）= 总览态 × 1.8（崔总 2026-10-12「升高大约 80%」），
 *                   但**最多占左列 60%**（保证上面的 LUT / 定稿列表还有地方），
 *                   且**绝不比总览态矮**（纵向照片的总览本身就可能很高）
 * ```
 *
 * 常数与实物一一对应（**组件改版时这里要跟着核**）：
 *
 * ```text
 * 页签条 = --seg-chip-h + 2 × --seg-track-pad     （SegmentedControl 凹槽 + 胶囊）
 * 缩放行 = 24px                                    （Input 的 `h-6`，与密度无关）
 * 上下内边距 = 8px（pt-2）+ --panel-pad（pb-panel-pad）
 * 两个间隙 = 8px × 2
 * ```
 *
 * 用 CSS `calc` 而不是算好的像素值：密度档一变、照片比例一变，高度必须自己跟上。
 */

/** 编辑左列宽度（LUT 面板与定稿面板共用）。 */
export const EDITOR_LEFT_WIDTH = 280;

/** 内容宽（扣掉两侧面板内边距）—— 预览与底部块的共同基准。 */
const LEFT_CONTENT_WIDTH = `(${EDITOR_LEFT_WIDTH}px - 2 * var(--panel-pad))`;

/** 底部块里**除预览之外**的固定高度（页签条 + 两个间隙 + 缩放行 + 上下内边距）。 */
const LEFT_BOTTOM_CHROME =
  "var(--seg-chip-h) + 2 * var(--seg-track-pad) + var(--panel-pad) + 48px";

/**
 * 底部块（总览态）高度。
 * `--preview-aspect` 由 `left-panel.tsx` 按当前照片写进来（缺省 3:2 = 最矮那一档）。
 */
export const EDITOR_LEFT_BOTTOM_VIEW =
  `calc(${LEFT_CONTENT_WIDTH} / var(--preview-aspect, 1.5) + ${LEFT_BOTTOM_CHROME})`;

/** 底部块（信息态）高度：升高 80%，但最多占左列 60%，且不比总览矮。 */
export const EDITOR_LEFT_BOTTOM_INFO =
  `max(${EDITOR_LEFT_BOTTOM_VIEW}, min(calc(${EDITOR_LEFT_BOTTOM_VIEW} * 1.8), 60%))`;
