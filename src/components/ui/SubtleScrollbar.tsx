/**
 * 无感滚动条（人类 2026-09-20）。
 *
 * 一条 **2px 的横向进度指示**，贴在被观察容器的底边：左滚到头不显示（宽度 0）、
 * 右滚到头占满整个横向宽度；装得下（不可滚）时也看不见。
 *
 * ## 为什么不是原生滚动条
 *
 * 原生横向滚动条**会占高度**，而且「有时有有时无」：出现时把内容区挤矮，
 * 胶片带里的缩略图跟着变小 —— 同一屏照片的尺寸会随滚动条的有无而变化，
 * 视觉上就是「抖一下」（人类 2026-09-20 点名的问题）。
 * 所以容器侧用 `.scrollbar-none` 把原生滚动条整个隐藏（`styles/scrollbar.css`），
 * 位置信息改由这一条**不占布局空间**的指示来给。
 *
 * ## 为什么是「进度」而不是传统滑块
 *
 * 人类给的口径就是这一条：**左滚到头不显示、右滚到头占满全宽**。
 * 传统滑块（宽度 = 可视比例）在小图上会和这条口径打架，而进度条天然满足它。
 *
 * ## 用法
 *
 * ```tsx
 * <div class="relative">
 *   <div ref={scroller} class="overflow-x-auto scrollbar-none">…</div>
 *   <SubtleScrollbar target={() => scroller} />
 * </div>
 * ```
 *
 * `target` 是**取值函数**：`ref` 在挂载时才就位，传值会把 `undefined` 闭包进去。
 * 外层负责 `relative` 与定位；这一条自己只画 2px 的指示，不接管布局。
 */

import { createSignal, onCleanup, onMount, type JSX } from "solid-js";

export interface SubtleScrollbarProps {
  /** 要观察的滚动容器 */
  target: () => HTMLElement | undefined;
  /** 额外类（定位/颜色）；不传就是「贴底、占满宽、主色」 */
  class?: string;
}

export function SubtleScrollbar(props: SubtleScrollbarProps): JSX.Element {
  /** 滚动进度 0..1（不可滚时恒为 0） */
  const [progress, setProgress] = createSignal(0);

  const measure = (): void => {
    const el = props.target();
    if (el === undefined) return;
    const max = el.scrollWidth - el.clientWidth;
    // 1px 容差：亚像素布局下 max 可能是 0.5 这种值，不该当成「可滚」
    if (max <= 1) {
      setProgress(0);
      return;
    }
    setProgress(Math.min(1, Math.max(0, el.scrollLeft / max)));
  };

  onMount(() => {
    /*
     * 三处变化都要重算，且都**按帧合并**（滚动每帧都会来消息，直连状态会白算）：
     *   * 滚动 → `scroll`；
     *   * 容器尺寸变化（窗口缩放、胶片带高度调整）→ ResizeObserver(容器)；
     *   * 内容尺寸变化（照片增删、缩略图到位）→ ResizeObserver(内容那一层)。
     * 观察「第一个子元素」是刻意的：内容变宽变窄不一定改变容器尺寸，
     * 只盯容器会漏掉它。
     */
    let frame = 0;
    const schedule = (): void => {
      if (frame !== 0) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        measure();
      });
    };

    const el = props.target();
    el?.addEventListener("scroll", schedule, { passive: true });
    const observer = new ResizeObserver(schedule);
    if (el !== undefined) {
      observer.observe(el);
      const content = el.firstElementChild;
      if (content instanceof HTMLElement) observer.observe(content);
    }
    schedule();

    onCleanup(() => {
      el?.removeEventListener("scroll", schedule);
      observer.disconnect();
      if (frame !== 0) cancelAnimationFrame(frame);
    });
  });

  return (
    <div
      aria-hidden="true"
      data-subtle-scrollbar="open"
      class={[
        "pointer-events-none absolute bottom-0 left-0 h-0.5 w-full overflow-hidden",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      <div
        class="h-full bg-brand/70"
        style={{ width: `${progress() * 100}%` }}
      />
    </div>
  );
}
