import { For, Show } from 'solid-js';
import { IconPlayCircle, IconVideo } from '../components/icons.tsx';
import { Section, SectionHeading } from '../components/Section.tsx';
import { messages } from '../i18n/index.ts';
import { tutorials } from '../data/tutorials.ts';

/**
 * 教程区（B 站）。
 *
 * 现在是**占位态**：视频还没录，版式先立起来（用户 2026-09-17 的选择）。
 * 录完往 `src/data/tutorials.ts` 的数组里加条目，这里自动变成视频卡片列表。
 */
export function Tutorials() {
  const t = () => messages().tutorials;

  return (
    <Section id="tutorials">
      <SectionHeading title={t().title} subtitle={t().subtitle} />

      <Show
        when={tutorials.length > 0}
        fallback={
          <div class="mt-10 flex flex-col items-center gap-4 rounded-3xl border border-dashed border-brand/40 bg-brand-soft/60 px-6 py-14 text-center">
            <IconPlayCircle class="size-8 text-brand-dark" strokeWidth={1.6} />
            <p class="text-base font-semibold">{t().comingSoonTitle}</p>
            <p class="max-w-sm text-sm/relaxed text-ink-soft">{t().comingSoonBody}</p>
          </div>
        }
      >
        <ul class="mt-10 grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
          <For each={tutorials}>
            {(item) => (
              <li>
                <a
                  href={item.url}
                  target="_blank"
                  rel="noreferrer"
                  class="flex h-full flex-col gap-3 rounded-2xl bg-card p-6 transition-colors hover:bg-brand-soft"
                >
                  <IconVideo class="size-5 text-brand-dark" strokeWidth={1.9} />
                  <span class="font-medium">{item.title}</span>
                </a>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </Section>
  );
}
