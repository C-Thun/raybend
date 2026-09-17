import { createMemo, For, Show } from 'solid-js';
import { IconDownload, IconMonitorDown } from '../components/icons.tsx';
import { Button } from '../components/Button.tsx';
import { Section, SectionHeading } from '../components/Section.tsx';
import { Shot } from '../components/Shot.tsx';
import { messages, locale } from '../i18n/index.ts';
import { currentRelease } from '../data/release.ts';
import { LICENSE_NAME, README_URL } from '../data/site.ts';

/**
 * 下载区。
 *
 * 版本号与按钮**全部来自构建期注入的发布信息**（`src/data/release.ts`）：
 * 还没有正式版时走 pending 态（版本显示「即将发布」、按钮指向发布列表页），
 * 接上第一个 release 之后自动变成真版本号 + 安装包直链。
 */
export function DownloadSection() {
  const t = () => messages().download;
  const release = () => currentRelease();
  /** 有正式版时的发布视图（pending 时为 `undefined`，类型上直接收窄） */
  const available = createMemo(() => {
    const view = release();
    return view.state === 'available' ? view : undefined;
  });
  /** 主按钮去处：有安装包直链用直链，否则去发布页 */
  const ctaHref = () => available()?.downloadUrl ?? release().releaseUrl;

  const versionText = () => {
    const view = available();
    return view ? `v${view.version}` : t().versionPending;
  };

  const publishedText = () => {
    const view = available();
    if (!view?.publishedAt) return undefined;
    const date = new Date(view.publishedAt);
    if (Number.isNaN(date.getTime())) return undefined;
    return new Intl.DateTimeFormat(locale() === 'zh' ? 'zh-CN' : 'en-US', {
      dateStyle: 'medium',
    }).format(date);
  };

  return (
    <Section id="download" tone="paper-2">
      <SectionHeading title={t().title} subtitle={t().subtitle} />

      <div class="mt-10 grid gap-10 rounded-3xl bg-card p-7 sm:p-10 lg:grid-cols-[minmax(0,1.15fr)_minmax(0,0.85fr)] lg:items-center">
        <div>
          <dl class="grid gap-x-8 gap-y-5 sm:grid-cols-2">
            <div>
              <dt class="text-xs text-muted">{t().versionLabel}</dt>
              <dd class="mt-1 text-lg font-semibold tabular-nums">{versionText()}</dd>
            </div>
            <div>
              <dt class="text-xs text-muted">{t().platformLabel}</dt>
              <dd class="mt-1 text-sm font-medium">{t().platform}</dd>
            </div>
            <div>
              <dt class="text-xs text-muted">{t().licenseLabel}</dt>
              <dd class="mt-1 text-sm font-medium">{LICENSE_NAME}</dd>
            </div>
            <Show when={available()}>
              {(view) => (
                <>
                  <Show when={view().sizeLabel}>
                    <div>
                      <dt class="text-xs text-muted">{t().sizeLabel}</dt>
                      <dd class="mt-1 text-sm font-medium tabular-nums">{view().sizeLabel}</dd>
                    </div>
                  </Show>
                  <Show when={publishedText()}>
                    <div>
                      <dt class="text-xs text-muted">{t().releaseDateLabel}</dt>
                      <dd class="mt-1 text-sm font-medium">{publishedText()}</dd>
                    </div>
                  </Show>
                </>
              )}
            </Show>
          </dl>

          <div class="mt-8 flex flex-wrap items-center gap-3">
            <Button
              href={ctaHref()}
              variant="ink"
              size="lg"
              icon={<IconDownload class="size-[1.1em]" strokeWidth={2} />}
            >
              {available() ? t().ctaDownload : t().ctaRelease}
            </Button>
          </div>

          <p class="mt-5 text-sm/relaxed text-ink-soft">
            {available() ? t().readyNote : t().pendingNote}
          </p>

          <div class="mt-7 border-t border-line pt-6">
            <h3 class="text-sm font-semibold">{t().requirementsTitle}</h3>
            <ul class="mt-3 space-y-1.5">
              <For each={t().requirements}>
                {(item) => <li class="text-sm text-ink-soft">{item}</li>}
              </For>
            </ul>
            <p class="mt-4 text-xs/relaxed text-muted">{t().smartscreenNote}</p>
            <p class="mt-3 text-xs/relaxed text-muted">
              {t().sourceNote}
              <a
                class="ml-1 font-medium text-ink underline decoration-brand decoration-2 underline-offset-2 hover:decoration-brand-dark"
                href={README_URL}
                target="_blank"
                rel="noreferrer"
              >
                {t().sourceLink}
              </a>{' '}
              {t().sourceSuffix}
            </p>
          </div>
        </div>

        <div class="flex items-center justify-center">
          <Shot id="splash" alt={messages().nav.brand} class="w-full max-w-md" />
        </div>
      </div>

      <p class="mt-8 flex items-center gap-2 text-xs text-muted">
        <IconMonitorDown class="size-4" strokeWidth={1.8} />
        {t().platform}
      </p>
    </Section>
  );
}
