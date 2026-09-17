import tailwindcss from '@tailwindcss/vite';
import { fileRoutes } from 'filesystem-routing/vite';
import { defineConfig } from 'vitest/config';
import solid from '@solidjs/vite-plugin';

/** GitHub 发布信息（与 `src/data/release.ts` 的 `ReleaseInfo` 对应） */
interface ReleaseInfo {
  tag: string;
  url?: string;
  publishedAt?: string;
  prerelease?: boolean;
  assets?: { name: string; size?: number; url?: string }[];
}

const REPO = 'C-Thun/raybend';
const API = `https://api.github.com/repos/${REPO}/releases`;

/**
 * 取发布信息（**只在构建时做一次**，浏览器端不请求 GitHub）。
 *
 * 优先级：`RAYBEND_TAG`（发布流程从 `github.event.release.tag_name` 传进来）→ `releases/latest`。
 * 只在 CI 或显式 `RAYBEND_FETCH_RELEASE=1` 时才发请求；本地 `pnpm build` 不联网。
 * 取不到就注入 `null` —— 页面走「即将发布」占位态，构建不会因为网络问题失败。
 */
async function fetchReleaseInfo(): Promise<ReleaseInfo | null> {
  const tag = process.env.RAYBEND_TAG?.trim();
  const enabled = Boolean(tag) || process.env.CI === 'true' || process.env.RAYBEND_FETCH_RELEASE === '1';
  if (!enabled) return null;

  const endpoint = tag ? `${API}/tags/${encodeURIComponent(tag)}` : `${API}/latest`;
  const token = process.env.GITHUB_TOKEN ?? process.env.GH_TOKEN;

  try {
    const response = await fetch(endpoint, {
      headers: {
        accept: 'application/vnd.github+json',
        'user-agent': 'raybend-website-build',
        ...(token ? { authorization: `Bearer ${token}` } : {}),
      },
      signal: AbortSignal.timeout(8000),
    });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);

    const data = (await response.json()) as {
      tag_name?: string;
      html_url?: string;
      published_at?: string;
      prerelease?: boolean;
      assets?: { name?: string; size?: number; browser_download_url?: string }[];
    };
    if (!data.tag_name) throw new Error('响应里没有 tag_name');

    return {
      tag: data.tag_name,
      url: data.html_url,
      publishedAt: data.published_at,
      prerelease: data.prerelease,
      assets: (data.assets ?? [])
        .filter((asset) => asset.name)
        .map((asset) => ({
          name: asset.name as string,
          size: asset.size,
          url: asset.browser_download_url,
        })),
    };
  } catch (error) {
    // 还没有正式版时这里就会 404 —— 属于正常情况，页面会走「即将发布」占位态。
    // （vite 会为 client / ssr 环境各加载一次配置，所以这行可能重复打印几次。）
    console.warn(`⚠ 没取到 GitHub 发布信息（${endpoint}）：${String(error)} —— 还没有正式版时属正常`);
    // 有 tag 就退化用 tag（下载按钮指向发布页）；没有就当作还没发布
    return tag ? { tag } : null;
  }
}

/** 同一次构建里只解析一次（vite 会为 client / ssr 环境各加载一次配置，不缓存就会重复请求） */
let releaseInfoPromise: Promise<ReleaseInfo | null> | undefined;
function releaseInfo(): Promise<ReleaseInfo | null> {
  releaseInfoPromise ??= fetchReleaseInfo();
  return releaseInfoPromise;
}

export default defineConfig(async ({ command }) => ({
  // 部署在自定义域名（`raybend.cthun.com`）的根路径下，所以 base 是 `/`；
  // 域名生效前想用 `https://c-thun.github.io/raybend/` 预览时用 `RAYBEND_BASE=/raybend/` 构建。
  base: process.env.RAYBEND_BASE || '/',
  define: {
    // 构建期注入发布信息：dev 下不注入（页面走 pending 态），构建时查一次（见上）
    'import.meta.env.RB_RELEASE': JSON.stringify(command === 'build' ? await releaseInfo() : null),
  },
  // Turnkey client mode: no index.html and no mount file — the plugin
  // generates the entries around src/App.tsx, wrapped in src/Document.tsx
  // (or a built-in shell). `vite build` prerenders the shell into
  // dist/client/index.html and emits a purely static dist/client.
  plugins: [
    // `extensions` makes @solidjs/vite-plugin also compile the `?pick=` route
    // modules the fileRoutes plugin emits (their ids end in a query string).
    solid({ start: true, extensions: ['.jsx', '.tsx'], diagnostics: true }), // add `ssr: true` for streaming SSR
    fileRoutes({ types: true }),
    // Scans source files for class names and generates their CSS into the
    // stylesheet that imports tailwindcss (src/App.css).
    tailwindcss(),
  ],
  server: {
    port: 3000,
  },
  test: {
    environment: 'jsdom',
    globals: false,
    setupFiles: ['./vitest-setup.ts'],
    // if you have few tests, try commenting this
    // out to improve performance:
    isolate: false,
  },
  build: {
    target: 'esnext',
    // Keep images as asset files instead of inlining them into the JS bundle.
    assetsInlineLimit: 0,
  },
}));
