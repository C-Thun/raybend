import { For, Show } from 'solid-js';
import { IconCheckCircle } from '../components/icons.tsx';
import { Section, SectionHeading } from '../components/Section.tsx';
import { Shot } from '../components/Shot.tsx';
import { WindowFrame } from '../components/WindowFrame.tsx';
import { featureRows, type FeatureRow } from '../data/features.ts';
import { messages } from '../i18n/index.ts';

/**
 * 功能细节：三种常规版式（左文右图 / 左图右文 / 整宽带），
 * 全部由 `featureRows` 驱动 —— 加一条数据就多一块，版式代码不动。
 */
export function Features() {
  const t = () => messages().features;

  return (
    <Section id="features">
      <SectionHeading title={t().title} subtitle={t().subtitle} />
      <div class="mt-14 space-y-20 sm:space-y-24">
        <For each={featureRows}>{(row) => <Row row={row} />}</For>
      </div>
    </Section>
  );
}

function Bullets(props: { items: readonly string[]; tone?: 'light' | 'dark' }) {
  return (
    <ul class="mt-7 space-y-3.5">
      <For each={props.items}>
        {(item) => (
          <li class="flex gap-3 text-sm/relaxed">
            <IconCheckCircle
              class={[
                'mt-0.5 size-4 shrink-0',
                props.tone === 'dark' ? 'text-accent' : 'text-brand-dark',
              ]}
              strokeWidth={2}
            />
            <span class={props.tone === 'dark' ? 'text-paper/80' : 'text-ink-soft'}>{item}</span>
          </li>
        )}
      </For>
    </ul>
  );
}

function Row(props: { row: FeatureRow }) {
  const copy = () => messages().features.items[props.row.id];
  const title = () => copy().title;
  const isBand = () => props.row.layout === 'band';
  const mediaFirst = () => props.row.layout === 'media-first';

  return (
    <Show
      when={isBand()}
      fallback={
        <div class="grid items-center gap-10 lg:grid-cols-2 lg:gap-16">
          <div class={mediaFirst() ? 'lg:order-2' : undefined}>
            <h3 class="text-2xl font-semibold text-balance sm:text-3xl">{title()}</h3>
            <p class="mt-4 text-base/relaxed text-ink-soft">{copy().body}</p>
            <Bullets items={copy().bullets} />
          </div>
          <div class={mediaFirst() ? 'lg:order-1' : undefined}>
            <WindowFrame title={`RayBend — ${props.row.id}`}>
              <Shot id={props.row.media} alt={title()} class="rounded-none" />
            </WindowFrame>
          </div>
        </div>
      }
    >
      <div class="rounded-3xl bg-ink px-7 py-10 text-paper sm:px-10 sm:py-12">
        <div class="grid items-center gap-10 lg:grid-cols-2">
          <div>
            <h3 class="text-2xl font-semibold text-balance sm:text-3xl">{title()}</h3>
            <p class="mt-4 text-sm/relaxed text-paper/70">{copy().body}</p>
            <Bullets items={copy().bullets} tone="dark" />
          </div>
          <WindowFrame class="border-paper/15" title="RayBend — command palette">
            <Shot id={props.row.media} alt={title()} class="rounded-none" />
          </WindowFrame>
        </div>
      </div>
    </Show>
  );
}
