import { For } from 'solid-js';
import { IconBolt, IconHardDrive, IconTree } from '../components/icons.tsx';
import { Section, SectionHeading } from '../components/Section.tsx';
import { highlights } from '../data/features.ts';
import { messages } from '../i18n/index.ts';

/** 三个卖点小卡（本地优先 / 快 / 开源） */
const ICONS = { local: IconHardDrive, fast: IconBolt, open: IconTree } as const;

export function Highlights() {
  const t = () => messages().highlights;

  return (
    <Section id="why" size="tight">
      <SectionHeading title={t().title} />
      <ul class="mt-12 grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
        <For each={highlights}>
          {(id) => {
            const Icon = ICONS[id];
            const item = () => t().items[id];
            return (
              <li class="rounded-2xl bg-card p-7">
                <span class="flex size-11 items-center justify-center rounded-xl bg-brand-soft text-brand-dark">
                  <Icon class="size-5" strokeWidth={1.9} />
                </span>
                <h3 class="mt-5 text-lg font-semibold">{item().title}</h3>
                <p class="mt-2.5 text-sm/relaxed text-ink-soft">{item().body}</p>
              </li>
            );
          }}
        </For>
      </ul>
    </Section>
  );
}
