/**
 * Toast 的**状态机**（视图在 `Toast.tsx`；逻辑与视图分家是为了能单测 ——
 * `node --test` 不认 `.tsx`，与 `thumb-queue.ts` / `ThumbQueue` 同一个理由）。
 *
 * ```text
 * ┌──────────────────────────────┐   ← `titlebar` 正下方、右上角
 * │ ⚠ 2 张有锁，没有改动    [撤销] │      带边框：成功 `$brand` / 失败 `$danger`
 * └──────────────────────────────┘      底 `$surface-layer`，约 5 秒后自己走
 * ```
 *
 * ## 三条纪律
 *
 * 1. **不阻塞操作**：容器 `pointer-events-none`、只有卡片本身收事件 ——
 *    提示挡不住下面的按钮（这是设计稿里明确写的「不遮挡操作」）。
 * 2. **可撤销**：带「撤销」的动作把它自己挂上去（`action`）—— 人类对「误操作」的
 *    第一反应就是找撤销，别让他去找别处的按钮。
 * 3. **最多同时 3 条**：再多就顶掉最老的。提示是「刚发生了什么」，不是日志；
 *    真要看历史该去消息中心（`FUTURE.md` H5）。
 *
 * 状态机与视图分家（`createToastStore` 可单测，视图只管画）。
 */

import { createSignal } from "solid-js";

/** 一个提示 */
export interface ToastSpec {
  /** 语气：成功（边框 `$brand`）/ 失败（边框 `$danger`）/ 中性信息 */
  tone: "success" | "danger" | "info";
  message: string;
  /** 可选的动作（一般是「撤销」）；点完就消失 */
  action?: { label: string; onAction: () => void };
  /** 停留时间（毫秒）。缺省 5000 —— 够读完一句短话，又不至于赖着不走 */
  duration?: number;
}

export interface ToastItem extends ToastSpec {
  id: number;
}

export interface ToastStore {
  items: () => readonly ToastItem[];
  /** 推一条，返回它的 id（测试与「点撤销后关掉它自己」都要用） */
  show: (spec: ToastSpec) => number;
  dismiss: (id: number) => void;
  /** 全部清掉（换工作流等场景；测试里也用它收尾） */
  clear: () => void;
}

/** 一条默认停留 5 秒（约 5 秒是设计稿上写的） */
export const TOAST_DURATION_MS = 5000;
/** 同时最多几条（顶掉最老的） */
export const TOAST_MAX = 3;

/**
 * 建一个提示队列。
 *
 * `schedule` 可注入：测试里用假的定时器，不必真等 5 秒
 * （默认就是 `setTimeout` / `clearTimeout`）。
 */
export function createToastStore(deps?: {
  /**
   * 排一个定时器，返回一个**句柄**。
   *
   * 句柄类型写成 `unknown` 而不是 `number`：Node 返回 `Timeout` 对象、浏览器返回数字，
   * 两者都能当句柄传回去取消 —— 写成 `number` 就得靠 `as unknown as number` 硬转，
   * 那个断言会在真出问题的时候骗人。
   */
  schedule?: (fn: () => void, ms: number) => unknown;
  cancel?: (handle: unknown) => void;
  max?: number;
}): ToastStore {
  const schedule = deps?.schedule ?? ((fn, ms) => setTimeout(fn, ms));
  const cancel = deps?.cancel ?? ((handle) => clearTimeout(handle as ReturnType<typeof setTimeout>));
  const max = Math.max(1, Math.floor(deps?.max ?? TOAST_MAX));
  const [items, setItems] = createSignal<readonly ToastItem[]>([]);
  const timers = new Map<number, unknown>();
  let nextId = 1;

  const dismiss = (id: number): void => {
    const handle = timers.get(id);
    if (handle !== undefined) {
      cancel(handle);
      timers.delete(id);
    }
    setItems((current) => current.filter((item) => item.id !== id));
  };

  const clear = (): void => {
    for (const handle of timers.values()) cancel(handle);
    timers.clear();
    setItems([]);
  };

  return {
    items,
    show(spec) {
      const id = nextId;
      nextId += 1;
      // 新的放最上面（下面是更早的），超过上限就顶掉最老的
      setItems((current) => [{ ...spec, id }, ...current].slice(0, max));
      const handle = schedule(() => dismiss(id), spec.duration ?? TOAST_DURATION_MS);
      timers.set(id, handle);
      return id;
    },
    dismiss,
    clear,
  };
}

/**
 * 组件卸载时把挂着的定时器收掉（避免一会儿之后去 set 一个已经没了的 store）。
 *
 * 用法：宿主里 `onCleanup(toastDisposer(store))`。放成独立函数而不是 store 自带，
 * 是因为 store 的生命周期由宿主决定（组装层建、组装层收）。
 */
export function toastDisposer(store: ToastStore): () => void {
  return () => store.clear();
}
