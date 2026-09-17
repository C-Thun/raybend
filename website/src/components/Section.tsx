import type { JSX } from '@solidjs/web';

/**
 * 区块外壳：统一宽度、内边距与底色。
 * 页面的每一段都用它包一层，保证节奏一致（上下留白、左右对齐）。
 */

export type SectionTone = 'paper' | 'paper-2' | 'brand-soft' | 'ink';

const TONES = {
  paper: 'bg-paper text-ink',
  'paper-2': 'bg-paper-2 text-ink',
  'brand-soft': 'bg-brand-soft text-ink',
  ink: 'bg-ink text-paper',
} satisfies Record<SectionTone, string>;

const SIZES = {
  normal: 'py-20 sm:py-28',
  tight: 'py-16 sm:py-20',
} satisfies Record<'normal' | 'tight', string>;

export interface SectionProps {
  id?: string;
  tone?: SectionTone;
  /** 首段（hero 之后的第一段）可以收紧上边距 */
  size?: 'normal' | 'tight';
  class?: string;
  children: JSX.Element;
}

export function Section(props: SectionProps) {
  return (
    <section
      id={props.id}
      class={[TONES[props.tone ?? 'paper'], SIZES[props.size ?? 'normal'], props.class]}
    >
      <div class="mx-auto w-full max-w-6xl px-6">{props.children}</div>
    </section>
  );
}

/** 区块标题组（标题 + 一句副标题），左对齐、宽度受控 */
export function SectionHeading(props: { title: string; subtitle?: string; class?: string }) {
  return (
    <header class={['max-w-2xl', props.class]}>
      <h2 class="text-title font-semibold text-balance">{props.title}</h2>
      {props.subtitle ? (
        <p class="mt-4 text-base/relaxed text-ink-soft text-pretty">{props.subtitle}</p>
      ) : null}
    </header>
  );
}
