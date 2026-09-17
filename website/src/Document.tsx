import type { ParentProps } from 'solid-js';
import { HydrationScript } from '@solidjs/web';
import { asset } from './lib/asset.ts';

/**
 * 文档外壳 —— 「新的 index.html」。
 *
 * 它只进**预渲染的静态外壳**，不下发任何客户端 JS；页面内容是浏览器端渲染的
 * （本站是纯静态客户端模式，不走 SSR，见 `website/AGENTS.md`）。
 *
 * 因此**静态 head 是爬虫唯一能直接读到的东西** —— SEO 相关的 title / description /
 * og / JSON-LD 都写在这里，运行时再由 `@solidjs/meta` 按语言覆盖标题与描述。
 */

const SITE_URL = 'https://raybend.cthun.com/';

const TITLE = '光伴 RayBend — 开源相片管理软件｜导入 · 浏览 · 整理 · 导出';

const DESCRIPTION =
  '光伴（RayBend）是一款本地优先的开源相片管理软件：把导入、浏览、评级、筛选、整理、导出做成一条顺手的流水线。照片只留在你自己的硬盘上。免费开源（AGPL-3.0），Windows 桌面版开发中。';

const KEYWORDS = [
  '光伴',
  'RayBend',
  '相片管理',
  '照片管理软件',
  'RAW',
  '摄影工作流',
  '开源软件',
  'AGPL',
  'Lightroom 替代',
  'ON1 Photo RAW 替代',
].join(',');

/** 结构化数据：让搜索引擎知道这是一个免费的桌面软件（而不是一篇文章） */
const STRUCTURED_DATA = {
  '@context': 'https://schema.org',
  '@type': 'SoftwareApplication',
  name: 'RayBend',
  alternateName: ['光伴'],
  description: DESCRIPTION,
  applicationCategory: 'MultimediaApplication',
  applicationSubCategory: 'Photo Management',
  operatingSystem: 'Windows 10, Windows 11',
  softwareRequirements: 'Windows 10/11（64 位）',
  license: 'https://www.gnu.org/licenses/agpl-3.0.html',
  isAccessibleForFree: true,
  inLanguage: ['zh-CN', 'en'],
  url: SITE_URL,
  codeRepository: 'https://github.com/C-Thun/raybend',
  offers: { '@type': 'Offer', price: '0', priceCurrency: 'CNY' },
};

export default function Document(props: ParentProps) {
  return (
    <html lang="zh-CN">
      <head>
        <meta charset="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />

        <title>{TITLE}</title>
        <meta name="description" content={DESCRIPTION} />
        <meta name="keywords" content={KEYWORDS} />
        <meta name="robots" content="index, follow" />
        <meta name="author" content="RayBend" />
        <link rel="canonical" href={SITE_URL} />

        {/* 分享卡片 */}
        <meta property="og:type" content="website" />
        <meta property="og:site_name" content="光伴 RayBend" />
        <meta property="og:title" content={TITLE} />
        <meta property="og:description" content={DESCRIPTION} />
        <meta property="og:url" content={SITE_URL} />
        <meta property="og:image" content={`${SITE_URL}logo.webp`} />
        <meta property="og:locale" content="zh_CN" />
        <meta property="og:locale:alternate" content="en_US" />
        <meta name="twitter:card" content="summary_large_image" />
        <meta name="twitter:title" content={TITLE} />
        <meta name="twitter:description" content={DESCRIPTION} />
        <meta name="twitter:image" content={`${SITE_URL}logo.webp`} />

        {/* 浏览器界面配色（跟随 hero 的琥珀底） */}
        <meta name="theme-color" content="#f0b033" />

        <link rel="icon" href={asset('favicon.ico')} sizes="any" />
        <link rel="apple-touch-icon" href={asset('logo-small.webp')} />

        <script type="application/ld+json">{JSON.stringify(STRUCTURED_DATA)}</script>

        <HydrationScript />
      </head>
      <body>
        {/*
         * 无 JS 降级：这段只在预渲染的静态外壳里，**必须自带样式** ——
         * 站点样式表是由客户端入口加载的，禁 JS 时还没有样式可用。
         */}
        <noscript>
          <p
            style={{
              margin: '0',
              padding: '1rem 1.5rem',
              background: '#f0b033',
              color: '#202226',
              'font-family': 'sans-serif',
            }}
          >
            本站的交互部分需要 JavaScript。没有它也能看：软件源码与发布包都在
            <a href="https://github.com/C-Thun/raybend" style={{ color: '#202226' }}>
              GitHub
            </a>
            上。
          </p>
        </noscript>
        {props.children}
      </body>
    </html>
  );
}
