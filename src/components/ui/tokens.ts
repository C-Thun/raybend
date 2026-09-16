/**
 * 读 CSS 令牌的**像素值**（虚拟化要用数字算行高，而令牌是字符串）。
 *
 * 为什么需要「跟随重读」：密度开关改的是 `<html data-density>`，
 * 令牌随之变（`--row-h` 紧凑 26px / 宽松 32px）。虚拟化的行高如果只读一次，
 * 用户切到宽松档后行高就不对了 —— 表现为**每行都差几个像素**，
 * 累积起来就是「滚动到底部时最后几行露不出来」这种很难查的现象。
 *
 * DOM 相关的部分留在这里（`components/ui/` 允许碰 DOM；`lib/` 不允许）：
 * 字符串解析在 `lib/css-length.ts`，那一半是纯函数、有单测。
 */

import { createSignal, onCleanup, onMount, type Accessor } from "solid-js";
import { parseCssPx } from "../../lib/css-length.ts";

/** 读一次令牌（拿不到就返回 `fallback`） */
export function readTokenPx(name: string, fallback: number): number {
  if (typeof document === "undefined") return fallback;
  const raw = getComputedStyle(document.documentElement).getPropertyValue(name);
  return parseCssPx(raw, fallback);
}

/**
 * 令牌像素值（响应式）：初次读取，之后跟随 `data-density` / `data-theme`
 * 属性变化重读。
 */
export function createTokenPx(name: string, fallback: number): Accessor<number> {
  const [value, setValue] = createSignal(readTokenPx(name, fallback));

  onMount(() => {
    if (typeof MutationObserver === "undefined") return;
    const observer = new MutationObserver(() => {
      setValue(readTokenPx(name, fallback));
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-density", "data-theme"],
    });
    onCleanup(() => observer.disconnect());
  });

  return value;
}
