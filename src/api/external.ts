/**
 * 系统级外壳动作（`src/api/` 层）。
 *
 * 目前只有一件：**用系统默认浏览器打开外链**（右栏 GPS 行的「在地图中打开」）。
 *
 * 为什么不让前端直接 `window.open`：Tauri 的 WebView 里那会把导航接管进应用自身，
 * 而我们要的是「交给用户的浏览器」。命令在 Rust 侧只放行 `https`（见
 * `src-tauri/src/open_url.rs`），前端只发意图。
 */

import { isTauriRuntime } from "./tauri-env.ts";

let coreModule: Promise<typeof import("@tauri-apps/api/core")> | undefined;

/** 打开一个 https 外链；浏览器里（dev）什么都不做。 */
export async function openExternalUrl(url: string): Promise<void> {
  if (!isTauriRuntime()) return;
  coreModule ??= import("@tauri-apps/api/core");
  const core = await coreModule;
  await core.invoke<void>("open_external_url", { url });
}
