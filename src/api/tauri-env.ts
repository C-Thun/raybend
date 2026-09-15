/**
 * 运行环境判定（`ARCHITECTURE.md` §1.1：`src/api/` 是唯一的数据出入口）。
 *
 * 为什么要判定：「窗口控制」这类能力**只在 Tauri 里存在**。而我们的日常开发有一半时间
 * 是在普通浏览器里跑 `pnpm dev`（看陈列室、调样式、跑 `pnpm smoke:ui`）。
 * 不做判定的话，浏览器里点一下最小化就会抛异常 —— 那是最廉价也最烦人的一类崩溃。
 *
 * 实现上是纯函数：不读模块级状态、不碰 DOM，scope 由参数注入，
 * 所以可以用普通的 Node 单元测试把各种「假 Tauri 环境」都演一遍。
 *
 * 标记说明：Tauri 2 注入 `window.__TAURI_INTERNALS__`；Tauri 1 是 `window.__TAURI__`。
 * 官方 `@tauri-apps/api` 的 `isTauri()` 做的也是这件事，但这里只需要一个布尔量，
 * 为它引一个包不划算，而且自己写才能测。
 */

/** Tauri 注入的全局标记（新→旧） */
export const TAURI_MARKERS = ["__TAURI_INTERNALS__", "__TAURI__"] as const;

export type RuntimeKind = "tauri" | "browser";

export interface RuntimeScope {
 [key: string]: unknown;
}

/**
 * 判定当前是否跑在 Tauri 的 WebView 里。
 *
 * 只认**存在且非空**的标记：`{ __TAURI_INTERNALS__: undefined }` 这类「有键无值」的
 * 情况按浏览器处理 —— 否则一旦注入顺序变化（标记先挂上、内容后填），
 * 界面会以为自己能用窗口 API，然后在真正调用时才炸。
 */
export function detectRuntime(
 scope: RuntimeScope | undefined | null,
): RuntimeKind {
 if (!scope) return "browser";
 for (const marker of TAURI_MARKERS) {
  const value = scope[marker];
  if (value !== undefined && value !== null) return "tauri";
 }
 return "browser";
}

/** 便利包装：直接判定当前全局环境 */
export function isTauriRuntime(): boolean {
 return detectRuntime(globalThis as RuntimeScope) === "tauri";
}
