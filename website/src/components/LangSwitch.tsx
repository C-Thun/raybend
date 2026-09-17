import { For } from 'solid-js';
import { locale, LOCALES, messages, setLocale, type Locale } from '../i18n/index.ts';

const LABELS = { zh: '中', en: 'EN' } satisfies Record<Locale, string>;

/** 左上角的语言切换：「中 | EN」分段控件（当前语言用深墨底标出） */
export function LangSwitch(props: { class?: string }) {
  return (
    <div
      class={['inline-flex items-center gap-0.5 rounded-full bg-ink/10 p-0.5', props.class]}
      role="group"
      aria-label={messages().nav.language}
    >
      <For each={LOCALES}>
        {(item) => (
          <button
            type="button"
            onClick={() => setLocale(item)}
            aria-pressed={locale() === item ? 'true' : 'false'}
            class={[
              'min-w-9 cursor-pointer rounded-full px-2.5 py-1 text-xs font-semibold transition-colors',
              locale() === item ? 'bg-ink text-paper' : 'text-ink/70 hover:text-ink',
            ]}
          >
            {LABELS[item]}
          </button>
        )}
      </For>
    </div>
  );
}
