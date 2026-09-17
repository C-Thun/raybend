import { Title } from '@solidjs/meta';
import type { RouteDefinition } from '@solidjs/router';
import { httpStatus } from '@solidjs/web';
import { asset } from '../lib/asset.ts';
import { messages } from '../i18n/index.ts';

// The catch-all route. httpStatus() is a no-op in the browser and takes
// effect when SSR is enabled; it runs in preload so the status code is set
// before the response head flushes.
export const route = {
  preload: () => httpStatus(404),
} satisfies RouteDefinition;

export default function NotFound() {
  const t = () => messages().notFound;

  return (
    <section class="bg-accent px-6 py-24 text-ink sm:py-32">
      <div class="mx-auto flex w-full max-w-2xl flex-col items-start gap-5">
        <Title>{`${t().title} · ${messages().nav.brand}`}</Title>
        <p class="text-sm font-semibold tracking-widest text-ink/60">404</p>
        <h1 class="text-title font-semibold">{t().title}</h1>
        <p class="text-base/relaxed text-ink/75">{t().body}</p>
        {/* `asset('')` = 站点根（base 感知，预览子路径时也对） */}
        <a
          href={asset('')}
          class="mt-2 inline-flex items-center rounded-full bg-ink px-6 py-3 text-sm font-medium text-paper transition-colors hover:bg-ink-2"
        >
          {t().back}
        </a>
      </div>
    </section>
  );
}
