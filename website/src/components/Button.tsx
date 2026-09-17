import type { JSX } from '@solidjs/web';

/**
 * 链接式按钮（站点的 CTA 都用它）。
 *
 * 三种面貌：`ink`（深墨实底，主操作）/ `brand`（青绿实底，次强调）/
 * `ghost`（描边，次要操作）。外链自动带 `target` 与 `rel`。
 */

export type ButtonVariant = 'ink' | 'brand' | 'ghost';
export type ButtonSize = 'md' | 'lg';

const VARIANTS = {
  ink: 'bg-ink text-paper hover:bg-ink-2',
  brand: 'bg-brand text-ink hover:bg-brand-dark',
  ghost: 'border border-ink/25 text-ink hover:border-ink/60 hover:bg-ink/5',
} satisfies Record<ButtonVariant, string>;

const SIZES = {
  md: 'px-5 py-2.5 text-sm',
  lg: 'px-6 py-3.5 text-base',
} satisfies Record<ButtonSize, string>;

export interface ButtonProps {
  href: string;
  variant?: ButtonVariant;
  size?: ButtonSize;
  class?: string;
  /** 按钮前面的图标（lucide 组件即可） */
  icon?: JSX.Element;
  children: JSX.Element;
}

export function Button(props: ButtonProps) {
  const isExternal = () => /^https?:/.test(props.href);

  return (
    <a
      href={props.href}
      target={isExternal() ? '_blank' : undefined}
      rel={isExternal() ? 'noreferrer' : undefined}
      class={[
        'inline-flex shrink-0 items-center justify-center gap-2 rounded-full font-medium whitespace-nowrap transition-colors',
        VARIANTS[props.variant ?? 'ink'],
        SIZES[props.size ?? 'md'],
        props.class,
      ]}
    >
      {props.icon}
      {props.children}
    </a>
  );
}
