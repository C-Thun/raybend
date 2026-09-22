/**
 * 编辑视口的 IPC 封装（`src/` 里只有本目录可以直接 `invoke`，见 `ARCHITECTURE.md` §1.1）。
 *
 * 契约的两条纪律：
 *
 * 1. **前端只上报原始事实**（CSS 矩形 + 运行时 DPR + CSS 视口尺寸）——
 *    物理换算、缩放、命中测试全在 Rust（`AGENTS.md` §6.1 红线 2、§7.9 铁律 2）；
 * 2. **非法值当面报错**，不静默回退 —— Rust 侧会拒绝 NaN / 非正 DPR，
 *    这一层把错误往上抛给调用方（上报器有自己的 try 边界）。
 *
 * W1 只有「上报 + 回读」两条命令：回读是给诊断与冒烟用的（Rust 手里的洞口对不对，
 * 一比就知道前端有没有报错单位）。W2 的渲染线程会直接读同一份状态。
 */

import type { EditorViewportPayload } from "../lib/editor-viewport.ts";
import { isTauriRuntime } from "./tauri-env.ts";

/** 缓存的 `@tauri-apps/api/core` 模块（浏览器里根本不会加载它）。 */
let coreModule: Promise<typeof import("@tauri-apps/api/core")> | undefined;

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  coreModule ??= import("@tauri-apps/api/core");
  return coreModule.then((core) => core.invoke<T>(cmd, args));
}

/** Rust 侧存下的视口事实（含它自己算出来的**物理**像素洞口，用来核对单位）。 */
export interface EditorViewportState {
  /** 前端报上来的 CSS 像素洞口 */
  holeCss: { x: number; y: number; width: number; height: number } | null;
  /** Rust 按 DPR 换算出的物理像素洞口（与 CSS 值一比就知道 DPR 有没有对上） */
  holePhysical: { x: number; y: number; width: number; height: number } | null;
  dpr: number;
  viewportCss: { width: number; height: number };
  /** 已经收到过几次上报（诊断：一直不涨说明前端没报） */
  updates: number;
}

/**
 * 上报洞口（CSS 像素）+ DPR + 视口尺寸。
 *
 * 浏览器（无 Tauri）里返回 `null` —— 那里没有渲染线程可喂，不该报错。
 * 返回 `null` 也表示这条命令在当前环境不可用，调用方据此跳过后续动作。
 */
export async function setEditorViewport(
  payload: EditorViewportPayload,
): Promise<EditorViewportState | null> {
  if (!isTauriRuntime()) return null;
  return call<EditorViewportState>("editor_set_viewport", { ...payload });
}

/** 读回 Rust 手里的视口事实（诊断 / 冒烟）。 */
export async function getEditorViewportState(): Promise<EditorViewportState | null> {
  if (!isTauriRuntime()) return null;
  return call<EditorViewportState>("editor_viewport_state");
}
