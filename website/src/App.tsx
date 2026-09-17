import { Meta, Title } from '@solidjs/meta';
import { For, Loading, onSettled } from 'solid-js';
import './App.css';
import { IconDownload } from './components/icons.tsx';
import { Button } from './components/Button.tsx';
import { GithubMark } from './components/GithubMark.tsx';
import { LangSwitch } from './components/LangSwitch.tsx';
import { Footer } from './sections/Footer.tsx';
import { messages, syncDocumentLanguage } from './i18n/index.ts';
import { currentRelease } from './data/release.ts';
import { REPO_URL } from './data/site.ts';
import { asset } from './lib/asset.ts';
import { Router } from './router.ts';

/**
 * 站点外壳：吸顶导航 + 页面内容 + 页脚。
 *
 * 导航**没有自己的底色条** —— 它和 hero 共用同一块琥珀底，滚动时才吸顶，
 * 这样首屏看起来是「一整块平面」，不是「顶栏 + 内容」。
 */
export default function App() {
  onSettled(() => syncDocumentLanguage());

  return (
    <Router>
      {(props) => (
        <>
          <Title>{messages().meta.title}</Title>
          <Meta name="description" content={messages().meta.description} />

          <Nav />

          <main>
            {/*
             * 路由组件是异步加载的（文件式路由默认 code splitting），必须有 Loading 边界：
             * 没有它，Solid 会把整个根挂载推迟到异步就绪，首屏会出现一段空白。
             * fallback 用 hero 的琥珀底做一个同色占位，避免闪白。
             */}
            <Loading
              fallback={
                <div class="flex min-h-[60vh] items-center justify-center bg-accent px-6 py-24">
                  <span class="size-8 animate-pulse rounded-full bg-ink/20" />
                </div>
              }
            >
              {props.children}
            </Loading>
          </main>

          <Footer />
        </>
      )}
    </Router>
  );
}

function Nav() {
  const t = () => messages().nav;
  const release = () => currentRelease();

  const links = () => [
    { href: '#features', label: t().features },
    { href: '#download', label: t().download },
    { href: '#tutorials', label: t().tutorials },
  ];

  return (
    <header class="sticky top-0 z-40 bg-accent text-ink">
      <div class="mx-auto flex w-full max-w-6xl items-center gap-3 px-6 py-3">
        <a href="#top" class="flex shrink-0 items-center gap-2.5">
          <img
            src={asset('logo-small.webp')}
            alt=""
            width={275}
            height={275}
            class="size-8 rounded-[0.6rem]"
          />
          {/* 定宽（按**最长的那种语言**给足）：名字中英长度差很多（「光伴」两字 vs RayBend），
              不定宽的话一切语言、右边的切换钮就跟着位移。短的内容靠左、右边留白。
              改名字或字号时记得同步这个宽度（不够时文字会溢出盒但不会被裁，安全）。 */}
          <span class="w-[4.5rem] text-sm font-semibold tracking-wide">{t().brand}</span>
        </a>

        <LangSwitch />

        <div class="ml-auto flex items-center gap-2">
          <nav class="hidden items-center gap-0.5 md:flex" aria-label={t().features}>
            <For each={links()}>
              {(link) => (
                <a
                  href={link.href}
                  class="rounded-full px-3 py-2 text-sm font-medium text-ink/75 transition-colors hover:bg-ink/10 hover:text-ink"
                >
                  {link.label}
                </a>
              )}
            </For>
          </nav>

          <a
            href={REPO_URL}
            target="_blank"
            rel="noreferrer"
            aria-label="GitHub"
            class="hidden size-9 items-center justify-center rounded-full text-ink/75 transition-colors hover:bg-ink/10 hover:text-ink md:inline-flex"
          >
            <GithubMark class="size-[1.15rem]" />
          </a>

          <Button
            href={release().releaseUrl}
            variant="ink"
            icon={<IconDownload class="size-4" strokeWidth={2} />}
          >
            {t().downloadCta}
          </Button>
        </div>
      </div>
    </header>
  );
}
