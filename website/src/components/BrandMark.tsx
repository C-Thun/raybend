import type { JSX } from '@solidjs/web';
import { asset } from '../lib/asset.ts';
import { locale, type Locale } from '../i18n/index.ts';

/**
 * 品牌抠图（名称 / 口号）。
 *
 * 素材是**白色墨迹 + 透明底**的 webp，所以这里用 CSS 遮罩渲染：颜色跟着
 * `currentColor` 走（Tailwind 的 `text-*` 类），同一张图能在琥珀底上染深墨、
 * 也能在深底上染米白 —— 换色不需要重新导出图片。
 */

type MarkKind = 'name' | 'slogan';

interface MarkSpec {
  file: string;
  width: number;
  height: number;
}

const MARKS = {
  name: {
    zh: { file: 'name-cn.webp', width: 724, height: 409 },
    en: { file: 'name-en.webp', width: 933, height: 389 },
  },
  slogan: {
    zh: { file: 'slogan-cn.webp', width: 1298, height: 265 },
    en: { file: 'slogan-en.webp', width: 1300, height: 309 },
  },
} satisfies Record<MarkKind, Record<Locale, MarkSpec>>;

export interface BrandMarkProps {
  kind: MarkKind;
  /** 宽高（高度按素材原始比例自动），例如 `w-[18rem]` */
  class?: string;
  /** 无障碍标签：抠图对读屏软件等于没有内容，必须给文字 */
  label: string;
}

export function BrandMark(props: BrandMarkProps) {
  const spec = () => MARKS[props.kind][locale()];

  return (
    <span
      class={['rb-mark', props.class]}
      // 内联样式是刻意的：遮罩图地址是按语言算出来的运行时值，且 `aspect-ratio` 与
      // 抠图原始比例绑定 —— 这两件事没有静态类名可写。
      // pi-lens-ignore: inline-styles
      style={
        {
          '--rb-mark-image': `url(${asset(spec().file)})`,
          'aspect-ratio': `${spec().width} / ${spec().height}`,
        } as JSX.CSSProperties
      }
      role="img"
      aria-label={props.label}
    />
  );
}
