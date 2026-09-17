/// <reference types="vite/client" />
/// <reference types="../file-routes.d.ts" />

import type { ReleaseInfo } from './data/release.ts';

/**
 * `vite.config.ts` 在构建期注入的常量（`define` 到 `import.meta.env.*`）。
 *
 * 走 `import.meta.env` 而不是自造全局变量：这是 Vite 的常规通道，
 * 编辑器、`tsc` 与 `vite build` 对它的处理一致（自造全局变量会让部分工具看不到类型）。
 */
interface ImportMetaEnv {
  /** 构建期注入的发布信息（没有任何发布时为 `null`） */
  readonly RB_RELEASE?: ReleaseInfo | null;
}
