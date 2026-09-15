/**
 * 系统目录选择器（`@tauri-apps/plugin-dialog` 的前端封装，`src/api/` 是唯一碰命令的地方）。
 *
 * 两条纪律：
 *   1. **浏览器里返回 `null`**，不抛错 —— 「没有选择器」是可预期的开发环境状态，
 *      调用方（建库弹窗）退回「手打路径」即可（`plans/M1-5.md` §8 的退路）；
 *   2. 动态 import：只有真的点「浏览…」时才加载插件模块。
 */

import { isTauriRuntime } from "./tauri-env.ts";

let dialogModule: Promise<typeof import("@tauri-apps/plugin-dialog")> | undefined;

export interface PickDirectoryOptions {
  /** 对话框标题（不传则用系统默认） */
  title?: string;
}

/**
 * 选一个目录；用户取消或环境不支持时返回 `null`。
 */
export async function pickDirectory(
  options: PickDirectoryOptions = {},
): Promise<string | null> {
  if (!isTauriRuntime()) return null;
  dialogModule ??= import("@tauri-apps/plugin-dialog");
  const { open } = await dialogModule;
  const selected = await open({
    directory: true,
    multiple: false,
    ...(options.title === undefined ? {} : { title: options.title }),
  });
  // 插件在「多选」模式下返回数组；我们只要单选，但两种形状都认一下
  if (typeof selected === "string") return selected;
  if (Array.isArray(selected) && typeof selected[0] === "string") {
    return selected[0];
  }
  return null;
}
