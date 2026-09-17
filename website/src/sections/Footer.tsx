import { For } from 'solid-js';
import { GithubMark } from '../components/GithubMark.tsx';
import { messages } from '../i18n/index.ts';
import { ISSUES_URL, LICENSE_NAME, LICENSE_URL, REPO_URL, RELEASES_URL } from '../data/site.ts';
import { asset } from '../lib/asset.ts';

/** 页脚：品牌 + 三列链接 + 版权（深墨底，给整页收个尾） */
export function Footer() {
  const t = () => messages().footer;

  const columns = () => [
    {
      title: t().product,
      links: [
        { label: t().source, href: REPO_URL },
        { label: t().releases, href: RELEASES_URL },
        { label: t().issues, href: ISSUES_URL },
      ],
    },
    {
      title: t().license,
      links: [
        { label: LICENSE_NAME, href: LICENSE_URL },
        { label: t().changelog, href: RELEASES_URL },
      ],
    },
  ];

  return (
    <footer class="bg-ink text-paper">
      <div class="mx-auto w-full max-w-6xl px-6 py-16">
        <div class="grid gap-10 sm:grid-cols-2 lg:grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)_minmax(0,1fr)]">
          <div>
            <div class="flex items-center gap-3">
              <img
                src={asset('logo-small.webp')}
                alt=""
                width={275}
                height={275}
                class="size-9 rounded-[0.7rem]"
              />
              <span class="text-base font-semibold">{messages().nav.brand}</span>
            </div>
            <p class="mt-4 max-w-xs text-sm/relaxed text-paper/60">{t().tagline}</p>
            <a
              href={REPO_URL}
              target="_blank"
              rel="noreferrer"
              class="mt-6 inline-flex items-center gap-2 text-sm text-paper/70 transition-colors hover:text-paper"
            >
              <GithubMark class="size-4" />
              {t().source}
            </a>
          </div>

          <For each={columns()}>
            {(column) => (
              <div>
                <h3 class="text-xs font-semibold tracking-wider text-paper/45 uppercase">
                  {column.title}
                </h3>
                <ul class="mt-4 space-y-2.5">
                  <For each={column.links}>
                    {(link) => (
                      <li>
                        <a
                          href={link.href}
                          target="_blank"
                          rel="noreferrer"
                          class="text-sm text-paper/75 transition-colors hover:text-paper"
                        >
                          {link.label}
                        </a>
                      </li>
                    )}
                  </For>
                </ul>
              </div>
            )}
          </For>
        </div>

        <div class="mt-12 flex flex-col gap-3 border-t border-paper/12 pt-6 text-xs text-paper/45 sm:flex-row sm:items-center sm:justify-between">
          <p>{t().rights}</p>
          <p>{t().builtWith}</p>
        </div>
      </div>
    </footer>
  );
}
