/**
 * 「这一次手势真的结束了」—— 与 Zag slider 的 `onValueChangeEnd` **不是**一回事。
 *
 * 为什么不能用 `onValueChangeEnd` 当「松手」：
 * Zag 的机器在**外部 `value` 变化**时也会派发 `invokeOnChangeEnd`
 * （`@zag-js/slider/slider.machine.mjs` 的 `SET_VALUE` 分支）。我们的滑杆是受控的，
 * `onValueChange` 每写一次 store，新的值就回灌一次，于是「松手才算」变成「每拖一格都算」:
 *
 * * 拖到 0% 的**那一格**就把该层关掉了（不是松手）；
 * * 层被重新打开后，任何一次回灌都会带着旧值再提交一次 —— 开关被反复改写，
 *   强度也会被「猜」成别的数（2026-10-31 崔总报的「点开关把效果抬到 100%」就是这一类）。
 *
 * 这里只认真的手势：指针在控件上按下 → 在**窗口任意位置**松开
 * （`pointerup` 可能落在控件外，所以挂在 window 的捕获阶段），或键盘 `keyup` 收尾。
 * 回灌与重新挂载都不产生手势，因此不会再触发任何「提交」逻辑。
 */

/** 指针监听的宿主（浏览器是 `window`；测试注入假对象）。 */
export interface GesturePointerEnv {
  addEventListener(type: string, listener: (event: Event) => void, capture?: boolean): void;
  removeEventListener(type: string, listener: (event: Event) => void, capture?: boolean): void;
}

export interface GestureEnd {
  /** 指针在控件上按下：开始一次手势（已在手势中则忽略）。 */
  start(event: { pointerId: number }): void;
  /** 组件卸载：摘掉挂在 window 上的监听（不留「松手就提交」的孤儿）。 */
  dispose(): void;
}

/** 收尾一次指针手势的监听器（`pointerup` / `pointercancel` 共用一个）。 */
export function createGestureEnd(done: () => void, env: GesturePointerEnv = window): GestureEnd {
  let pointerId: number | null = null;
  let listener: ((event: Event) => void) | null = null;
  const detach = (): void => {
    if (listener === null) return;
    env.removeEventListener("pointerup", listener, true);
    env.removeEventListener("pointercancel", listener, true);
    listener = null;
  };
  return {
    start(event) {
      if (pointerId !== null) return;
      pointerId = event.pointerId;
      listener = (value: Event) => {
        const id = (value as PointerEvent).pointerId;
        // 别的指针（多指 / 别的设备）不算这一次手势的结束
        if (pointerId === null || (typeof id === "number" && id !== pointerId)) return;
        pointerId = null;
        detach();
        done();
      };
      env.addEventListener("pointerup", listener, true);
      env.addEventListener("pointercancel", listener, true);
    },
    dispose() {
      pointerId = null;
      detach();
    },
  };
}

/** 滑杆认的键盘收尾键（方向键 / Home / End / PageUp / PageDown）。 */
export const SLIDER_KEYS: ReadonlySet<string> = new Set([
  "ArrowLeft",
  "ArrowRight",
  "ArrowUp",
  "ArrowDown",
  "Home",
  "End",
  "PageUp",
  "PageDown",
]);
