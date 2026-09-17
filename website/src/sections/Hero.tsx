import { For, Show } from 'solid-js';
import { IconDownload } from '../components/icons.tsx';
import { BrandMark } from '../components/BrandMark.tsx';
import { Button } from '../components/Button.tsx';
import { GithubMark } from '../components/GithubMark.tsx';
import { Shot } from '../components/Shot.tsx';
import { WindowFrame } from '../components/WindowFrame.tsx';
import { messages } from '../i18n/index.ts';
import { currentRelease } from '../data/release.ts';
import { REPO_URL } from '../data/site.ts';

/** 胶片孔数量：够铺满最宽的屏幕（多出来的被 `overflow-hidden` 裁掉） */
const FILM_HOLES = Array.from({ length: 64 }, (_, index) => index);

/**
 * Hero：**贴顶**、辅色（琥珀）满铺、平面化。
 *
 * 顶部没有额外的白色/深色条 —— 导航就浮在同一块琥珀底上，滚动时才吸顶。
 * 右侧是应用窗口截图（还没截，先占位）。
 */
export function Hero() {
  const t = () => messages().hero;
  const release = () => currentRelease();

  return (
    <section id="top" class="relative overflow-hidden bg-accent text-ink">
      {/* 平面几何装饰（不含阴影/毛玻璃） */}
      <div aria-hidden="true" class="pointer-events-none absolute inset-0 overflow-hidden">
        <svg
          class="absolute -top-32 -right-24 size-[30rem] text-brand/30 sm:-top-40 sm:-right-16"
          viewBox="0 0 200 200"
          fill="none"
        >
          <circle cx="100" cy="100" r="98" stroke="currentColor" stroke-width="1.5" />
          <circle cx="100" cy="100" r="72" stroke="currentColor" stroke-width="1.5" />
          <circle cx="100" cy="100" r="46" stroke="currentColor" stroke-width="1.5" />
        </svg>
        <div class="absolute top-1/3 left-4 size-2.5 rounded-full bg-ink/20" />
        <div class="absolute top-2/5 right-6 size-3 rounded-full bg-brand/70" />
        {/* 底边一条「胶片孔」带：平面化的摄影暗示，也让 hero 有个收口 */}
        <div class="absolute bottom-0 left-0 flex w-full gap-3 px-6 pb-3.5">
          <For each={FILM_HOLES}>{() => <span class="h-3 w-6 shrink-0 rounded-[3px] bg-brand/55" />}</For>
        </div>
      </div>

      <div class="relative mx-auto w-full max-w-6xl px-6 pt-10 pb-20 sm:pt-16 sm:pb-24">
        <div class="grid items-center gap-12 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.05fr)] lg:gap-16">
          <div>
            <span
              class="inline-flex items-center gap-2 rounded-full bg-ink/10 px-3 py-1 text-xs font-medium text-ink"
            >
              <span class="size-1.5 rounded-full bg-ink" />
              {t().earlyBadge}
            </span>

            <BrandMark
              kind="name"
              label={messages().nav.brand}
              class="mt-7 w-[min(16rem,62vw)] text-ink"
            />
            <BrandMark
              kind="slogan"
              label={t().tagline}
              class="mt-3 w-[min(23rem,78vw)] text-ink/85"
            />

            <p class="mt-7 max-w-xl text-base/relaxed text-ink/80">{t().intro}</p>

            <div class="mt-8 flex flex-wrap items-center gap-3">
              <Button
                href={release().releaseUrl}
                variant="ink"
                size="lg"
                icon={<IconDownload class="size-[1.1em]" strokeWidth={2} />}
              >
                {t().ctaPrimary}
              </Button>
              <Button
                href={REPO_URL}
                variant="ghost"
                size="lg"
                icon={<GithubMark />}
                class="border-ink/30"
              >
                {t().ctaSecondary}
              </Button>
            </div>

            <ul class="mt-7 flex flex-wrap items-center gap-x-3 gap-y-2 text-sm text-ink/70">
              <For each={t().facts}>
                {(fact, index) => (
                  <>
                    <Show when={index() > 0}>
                      <li aria-hidden="true" class="text-ink/30">
                        ·
                      </li>
                    </Show>
                    <li>{fact}</li>
                  </>
                )}
              </For>
            </ul>

            <p class="mt-6 max-w-xl text-xs/relaxed text-ink/60">{t().earlyNote}</p>
          </div>

          <div class="relative">
            <WindowFrame title="RayBend — browse">
              <Shot
                id="heroApp"
                eager
                alt={t().shotCaption}
                class="rounded-none"
              />
            </WindowFrame>
          </div>
        </div>
      </div>
    </section>
  );
}
