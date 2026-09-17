import { For, Show } from 'solid-js';
import { IconDownload } from '../components/icons.tsx';
import { BrandMark } from '../components/BrandMark.tsx';
import { Button } from '../components/Button.tsx';
import { GithubMark } from '../components/GithubMark.tsx';
import { Shot } from '../components/Shot.tsx';
import { WindowFrame } from '../components/WindowFrame.tsx';
import { locale, messages } from '../i18n/index.ts';
import { currentRelease } from '../data/release.ts';
import { REPO_URL } from '../data/site.ts';

/** 胶片孔数量：够铺满最宽的屏幕（多出来的被 `overflow-hidden` 裁掉） */
const FILM_HOLES = Array.from({ length: 64 }, (_, index) => index);

/**
 * 产品名的宽度**按语言分开定** —— 因为两种字形的比例差得很远：
 *
 * ```text
 * 中文「光伴」    `marks/name-zh.svg` 的 viewBox 2172×1227 ⇒ 高/宽 ≈ 0.565
 * 英文「RayBend」 `marks/name-en.svg` 的 viewBox 2799×1167 ⇒ 高/宽 ≈ 0.417
 * ```
 *
 * 用**同一个宽度**，英文名会整整矮掉四分之一（实测 15rem 时中文高 ≈ 8.5rem、
 * 英文只有 ≈ 6.3rem），而人看的是**高度**是否一致。所以要分开给的是宽度：
 *
 * ```text
 * 中文 15rem → 高 ≈ 8.5rem ｜ 英文 19rem → 高 ≈ 7.9rem
 * ```
 *
 * 英文略矮一点是刻意的（它本来就长得多）。**要调大小就改下面这两个 `w-[…]`**，
 * 高度会自动跟着各自的比例走；改完 `pnpm dev` 看一眼。
 */
const NAME_WIDTH_CLASS: Record<string, string> = {
  zh: 'w-[min(15rem,58vw)]',
  en: 'w-[min(19rem,72vw)]',
};

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
        {/* 左下角：低浓度的品牌色圆角方块「切进来」（半透明感，不抢主体）——
            胶片孔带压在它上面，看起来就是「打孔」。 */}
        <div class="absolute -bottom-24 -left-16 size-72 rotate-12 rounded-[3rem] bg-brand/20" />
        {/* 右上：与左下同色系的同心圆环（线条细，浓度略抬一点才看得出） */}
        <svg
          class="rb-hero-rings absolute -top-32 -right-24 size-[30rem] text-brand/35 sm:-top-40 sm:-right-16"
          viewBox="0 0 200 200"
          fill="none"
        >
          <circle cx="100" cy="100" r="98" stroke="currentColor" />
          <circle cx="100" cy="100" r="72" stroke="currentColor" />
          <circle cx="100" cy="100" r="46" stroke="currentColor" />
        </svg>
        {/*
          底边一条白色「胶片孔」带：过琥珀处像镂空，过绿块处像打孔。

          **起点用负值**：`-left-4` 让第一个孔从屏幕外就开始，看起来才像一条真正
          穿过去的胶片带。留 24px 起头会出现一段突兀的空白（人类点过名）。
          多出来的孔被装饰层的 `overflow-hidden` 裁掉，不影响右侧布局。
        */}
        <div class="absolute bottom-0 -left-4 flex w-max gap-3 pb-3.5">
          <For each={FILM_HOLES}>{() => <span class="h-3 w-6 shrink-0 rounded-[3px] bg-paper" />}</For>
        </div>
      </div>

      <div class="relative mx-auto w-full max-w-6xl px-6 pt-8 pb-16 sm:pt-12 sm:pb-20">
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
              class={`mt-5 ${NAME_WIDTH_CLASS[locale()] ?? NAME_WIDTH_CLASS.zh}`}
              outline={10}
            />
            <BrandMark
              kind="slogan"
              label={t().tagline}
              class="mt-2.5 w-[min(21rem,72vw)]"
              outline={8}
            />

            <p class="mt-5 max-w-xl text-base/relaxed text-ink/80">{t().intro}</p>

            <div class="mt-6 flex flex-wrap items-center gap-3">
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

            <ul class="mt-5 flex flex-wrap items-center gap-x-3 gap-y-2 text-sm text-ink/70">
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

            <p class="mt-5 max-w-xl text-xs/relaxed text-ink/60">{t().earlyNote}</p>
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
