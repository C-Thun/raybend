import { For } from 'solid-js';
import { IconChip, IconLock, IconShieldCheck } from '../components/icons.tsx';
import { Button } from '../components/Button.tsx';
import { GithubMark } from '../components/GithubMark.tsx';
import { Section, SectionHeading } from '../components/Section.tsx';
import { messages } from '../i18n/index.ts';
import { REPO_URL } from '../data/site.ts';

/** 「开源与技术」整宽带：许可 / 技术栈 / 隐私，右侧一个去 GitHub 的入口 */
const ITEMS = [
  { id: 'license', Icon: IconShieldCheck },
  { id: 'stack', Icon: IconChip },
  { id: 'privacy', Icon: IconLock },
] as const;

export function OpenSource() {
  const t = () => messages().oss;

  return (
    <Section id="open-source" tone="brand-soft">
      <div class="grid gap-12 lg:grid-cols-[minmax(0,0.9fr)_minmax(0,1.3fr)] lg:items-center lg:gap-16">
        <div>
          <SectionHeading title={t().title} />
          <p class="mt-5 max-w-md text-sm/relaxed text-ink-soft">{t().body}</p>
          <Button
            href={REPO_URL}
            variant="ink"
            size="lg"
            class="mt-8"
            icon={<GithubMark />}
          >
            {t().cta}
          </Button>
        </div>

        <ul class="grid gap-5 sm:grid-cols-3">
          <For each={ITEMS}>
            {(item) => (
              <li class="rounded-2xl bg-card/80 p-6">
                <item.Icon class="size-5 text-brand-dark" strokeWidth={1.9} />
                <h3 class="mt-4 text-base font-semibold">{t().items[item.id].title}</h3>
                <p class="mt-2 text-sm/relaxed text-ink-soft">{t().items[item.id].body}</p>
              </li>
            )}
          </For>
        </ul>
      </div>
    </Section>
  );
}
