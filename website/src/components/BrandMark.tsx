import type { JSX } from '@solidjs/web';
import { locale } from '../i18n/index.ts';
import { MARK_PATHS, type MarkId } from './mark-paths.ts';

/**
 * 品牌书法字（产品名 / 口号）。
 *
 * 素材是「白色墨迹 + 透明底」的位图，这里用 `scripts/generate-marks.mjs` 描摹出的
 * **矢量路径**渲染（理由见那个脚本的注释）：位图只能给墨迹本身上色，做不到「向外扩的白边」；
 * 矢量路径可以 —— `fill` 染绿、`stroke` 描白、`paint-order="stroke"` 把描边压到填充下面，
 * 于是只剩**向外那一半**，就是外扩边。
 */
export interface BrandMarkProps {
  kind: 'name' | 'slogan';
  /** 宽度（如 `w-[16rem]`）；高度按生成物的 viewBox 比例自动 */
  class?: string;
  /** 白色外扩边的宽度，单位是**屏幕像素**（描边居中，可见部分 ≈ 一半）；0/省略 = 不描边 */
  outline?: number;
  /** 无障碍标签：字形对读屏软件等于没有内容，必须给文字 */
  label: string;
}

export function BrandMark(props: BrandMarkProps) {
  const mark = () => MARK_PATHS[`${props.kind}-${locale()}` as MarkId];
  const ratio = () => {
    const [, , width, height] = mark().viewBox.split(' ');
    return `${width} / ${height}`;
  };

  return (
    <svg
      viewBox={mark().viewBox}
      role="img"
      aria-label={props.label}
      class={['block h-auto', props.class]}
      // 宽高比来自生成物的 viewBox（数据驱动），没有静态类名可写
      // pi-lens-ignore: inline-styles
      style={{ 'aspect-ratio': ratio() } as JSX.CSSProperties}
    >
      <path
        d={mark().path}
        class="rb-mark-path"
        fill="var(--color-brand)"
        stroke={props.outline ? 'var(--color-white)' : 'none'}
        // 外扩边宽度（可见值，屏幕像素）—— 传给 CSS，见 App.css 的 `.rb-mark-path`。
        // **必须带 `px`**：这个值最终进 `calc(...)`，纯数字不是合法长度，
        // 整条 `stroke-width` 会被丢掉、退回初始值 1（实测白边就剩细细一条）。
        // pi-lens-ignore: inline-styles
        style={{ '--rb-mark-outline': `${props.outline ?? 0}px` } as JSX.CSSProperties}
      />
    </svg>
  );
}
