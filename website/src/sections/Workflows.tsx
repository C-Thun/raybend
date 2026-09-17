import { For } from 'solid-js';
import { IconFolderInput, IconGrid, IconTags, IconUpload } from '../components/icons.tsx';
import { Section, SectionHeading } from '../components/Section.tsx';
import { workflowStages } from '../data/features.ts';
import { messages } from '../i18n/index.ts';

/** 工作流总览：四个阶段（顺序即叙事顺序，卡片由数据驱动） */
const ICONS = {
  import: IconFolderInput,
  browse: IconGrid,
  organize: IconTags,
  export: IconUpload,
} as const;

export function Workflows() {
  const t = () => messages().workflows;

  return (
    <Section id="workflow" tone="paper-2">
      <SectionHeading title={t().title} subtitle={t().subtitle} />
      <ol class="mt-12 grid gap-6 sm:grid-cols-2 lg:grid-cols-4">
        <For each={workflowStages}>
          {(id, index) => {
            const Icon = ICONS[id];
            const item = () => t().items[id];
            return (
              <li class="rounded-2xl bg-card p-6">
                <div class="flex items-center justify-between">
                  <span class="text-xs font-semibold text-brand-dark tabular-nums">
                    {String(index() + 1).padStart(2, '0')}
                  </span>
                  <Icon class="size-5 text-ink/35" strokeWidth={1.75} />
                </div>
                <h3 class="mt-4 text-lg font-semibold">{item().title}</h3>
                <p class="mt-2 text-sm/relaxed text-ink-soft">{item().body}</p>
              </li>
            );
          }}
        </For>
      </ol>
    </Section>
  );
}
