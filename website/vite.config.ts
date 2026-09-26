import tailwindcss from '@tailwindcss/vite';
import { fileRoutes } from 'filesystem-routing/vite';
import { defineConfig } from 'vitest/config';
import solid from '@solidjs/vite-plugin';

import { fetchReleaseInfo } from './scripts/release-info.ts';
import type { ReleaseInfo } from './src/data/release.ts';

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
