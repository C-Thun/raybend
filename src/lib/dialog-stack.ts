/**
 * 弹窗叠放栈（`memory/DESIGN.md` §8.5 / §10.1 #9）。
 *
 * 只回答两个问题：
 *   1. 我是第几层？（`depth`，1 起；关闭时 0）
 *   2. 我该不该画压暗遮罩？（`paintsScrim`，只有最底层为真）
 *
 * 为什么要有它：弹窗叠弹窗时，每一层都默认画 `bg-scrim` 就会「半透叠半透」，
 * 背景越叠越黑；而让调用方手写 `scrim={false}` 迟早会漏。把这件事收进栈里，
 * 无脑调用 `<Dialog open>` 也只有一层遮罩。
 *
 * 另一个用途是给 `index.css` 的弹窗 z 阶梯提供深度：
 * `--dialog-layer` 由 `DialogFrame` 写进 backdrop / positioner，公式见那里。
 *
 * 注意：**不负责点击拦截**。下层不可点是 Ark/Zag 的 modal 行为
 * （body `pointer-events: none` + 下层 content 内联 `none` + 焦点陷阱按栈暂停），
 * 这里不重复实现。
 */

import { createComputed, createSignal, onCleanup } from "solid-js";

/** 当前打开的弹窗（按打开顺序；最后一个 = 最上层） */
const [layers, setLayers] = createSignal<readonly symbol[]>([]);

export interface DialogLayer {
  /** 1 起；未打开 / 已卸载时为 0 */
  depth: () => number;
  /** 只有最底层负责 `bg-scrim`，其余层透明 */
  paintsScrim: () => boolean;
}

/**
 * 注册一个弹窗层。在组件里调用；`open` 变成 true 时入栈（追加到末尾），
 * 变 false / 组件卸载时出栈。重复打开会移到末尾（= 最上层）。
 */
export function createDialogLayer(open: () => boolean): DialogLayer {
  const token = Symbol("dialog-layer");

  /**
   * 入栈/出栈。**无变化时必须返回原数组**：Solid 的信号按引用比较，
   * 每次都返回新数组会让所有已打开的弹窗重渲染 backdrop 与 z（全应用 50 多个
   * `Dialog` 在启动时都要跑一次 `open() === false`）。
   */
  const setActive = (active: boolean): void => {
    setLayers((current) => {
      const without = current.filter((entry) => entry !== token);
      if (!active) return without.length === current.length ? current : without;
      const alreadyTop = without.length === current.length && current[current.length - 1] === token;
      return alreadyTop ? current : [...without, token];
    });
  };

  // 用 createComputed 而不是 createEffect：注册与首帧同步，避免第一帧没遮罩。
  // 它只依赖 open()，不会因为 layers 变化而重跑，因此不存在自激。
  createComputed(() => setActive(open()));

  onCleanup(() => setActive(false));

  const index = (): number => layers().indexOf(token);

  return {
    depth: () => index() + 1,
    paintsScrim: () => index() === 0,
  };
}
