/**
 * 编辑器视口的**交互意图构造**（纯逻辑，不碰 DOM、不发 IPC）。
 *
 * 这一层做的事只有一件：把「用户拖了多少 / 点了一下没动」翻译成**意图**。
 * 坐标数学（CSS→物理、缩放锚点、命中测试）全在 Rust（`AGENTS.md` §6.1 红线 2），
 * 所以这里连一次乘法都不做 —— 只做**合并**与**判据**：
 *
 * | 关注点 | 为什么需要它 |
 * | --- | --- |
 * | 一帧最多发一次（`createPanAccumulator`） | 高刷屏的 `pointermove` 能到 200Hz；逐条发就是 200 次 IPC |
 * | **尾样本必发** | 停手那一次不能丢，否则 Rust 手里的 pan 会停在半路（§7.9「诊断红旗」第 5 条） |
 * | 点击 vs 拖动（`isClickGesture`） | 松手时要不要发命中测试（W5 的三个工具靠它区分） |
 *
 * 帧调度复用 `lib/editor-viewport.ts` 的 `createFrameScheduler`（一份实现，两处用）。
 */

import { createFrameScheduler, type FrameScheduler, type FrameTicket } from "./editor-viewport.ts";

/** 一次平移意图（CSS 位移，**不乘 DPR** —— 那是 Rust 的活）。 */
export interface PanIntent {
  kind: "pan";
  dx: number;
  dy: number;
}

/** 认作「没怎么动」的阈值（CSS 像素）—— 超过它就是拖动，不是点击。 */
export const CLICK_SLOP_PX = 3;

/**
 * 松手时算「点击」还是「拖动」。
 *
 * 用**总位移**而不是逐次位移：慢慢拖 20 像素再拖回来，逐次判会误判成点击。
 */
export function isClickGesture(totalDx: number, totalDy: number): boolean {
  if (!Number.isFinite(totalDx) || !Number.isFinite(totalDy)) return false;
  return Math.hypot(totalDx, totalDy) <= CLICK_SLOP_PX;
}

export interface PanAccumulatorDeps {
  /** 真正发出去（IPC / 测试里换成数组收集） */
  send: (intent: PanIntent) => void;
  /** 帧调度（默认真实 rAF；测试注入确定实现） */
  scheduler?: FrameScheduler;
}

export interface PanAccumulator {
  /** 记一段 CSS 位移（`pointermove` 里调，多密都行） */
  move: (dx: number, dy: number) => void;
  /** 立刻把挂起的那一帧发出去（松手 / 卸载前 —— 尾样本） */
  flush: () => void;
  /** 取消挂起的调度（卸载时），**不发** */
  dispose: () => void;
  /** 已经发出过几次（冒烟断言用） */
  sentCount: () => number;
}

/**
 * 把一堆小位移**合并成每帧一次**的平移意图。
 *
 * 三条规矩（都对着 `lib/editor-viewport.ts` 与看图件那份 `createWheelZoom` 的口径）：
 *
 * 1. 相邻两帧的位移**相加**，不是覆盖 —— 拖着走的时候每一段都要算数；
 * 2. 零位移不发（`move(0, 0)` 是噪声，不是事实）；
 * 3. `NaN` / `Infinity` **直接丢掉**（宁可不平移，也不给 Rust 一个要拒绝的值）。
 */
export function createPanAccumulator(deps: PanAccumulatorDeps): PanAccumulator {
  const scheduler = deps.scheduler ?? createFrameScheduler();

  let dx = 0;
  let dy = 0;
  let ticket: ReturnType<FrameScheduler["request"]> | null = null;
  let sent = 0;

  const emit = (): boolean => {
    const moveX = dx;
    const moveY = dy;
    dx = 0;
    dy = 0;
    if (moveX === 0 && moveY === 0) return false;
    sent += 1;
    deps.send({ kind: "pan", dx: moveX, dy: moveY });
    return true;
  };

  return {
    move: (moveX, moveY) => {
      if (!Number.isFinite(moveX) || !Number.isFinite(moveY)) return;
      if (moveX === 0 && moveY === 0) return;
      dx += moveX;
      dy += moveY;
      ticket ??= scheduler.request(() => {
        ticket = null;
        emit();
      });
    },
    flush: () => {
      if (ticket !== null) {
        scheduler.cancel(ticket);
        ticket = null;
      }
      emit();
    },
    dispose: () => {
      if (ticket !== null) {
        scheduler.cancel(ticket);
        ticket = null;
      }
      dx = 0;
      dy = 0;
    },
    sentCount: () => sent,
  };
}

/**
 * 一次拖动会话的**位置簿记**（纯数字进出，不碰 DOM 事件）。
 *
 * 为什么单独一个东西：`pan` 意图要的是**相邻两次采样的位移**，而「这是点击吗」
 * 要的是**从按下到松手的总位移** —— 两笔账，分开记最不容易混。
 */
export interface DragSession {
  /** 按下：记住起点 */
  start: (x: number, y: number) => void;
  /**
   * 移动到某个点：返回**相对上一次采样**的位移（第一次采样返回 `(0, 0)`，
   * 因为位移还没发生）；没在拖动时返回 `null`。
   */
  move: (x: number, y: number) => { dx: number; dy: number } | null;
  /** 松手：返回总位移与「算不算点击」 */
  end: () => { totalDx: number; totalDy: number; click: boolean };
  /** 正在拖动吗 */
  active: () => boolean;
}

export function createDragSession(): DragSession {
  let lastX = 0;
  let lastY = 0;
  let totalDx = 0;
  let totalDy = 0;
  let dragging = false;

  return {
    start: (x, y) => {
      lastX = x;
      lastY = y;
      totalDx = 0;
      totalDy = 0;
      dragging = true;
    },
    move: (x, y) => {
      if (!dragging) return null;
      const dx = x - lastX;
      const dy = y - lastY;
      lastX = x;
      lastY = y;
      if (Number.isFinite(dx) && Number.isFinite(dy)) {
        totalDx += dx;
        totalDy += dy;
        return { dx, dy };
      }
      return { dx: 0, dy: 0 };
    },
    end: () => {
      dragging = false;
      const click = isClickGesture(totalDx, totalDy);
      const result = { totalDx, totalDy, click };
      totalDx = 0;
      totalDy = 0;
      return result;
    },
    active: () => dragging,
  };
}

/**
 * **「最新值胜出」的帧合并器**（M3-W3 的参数通道）。
 *
 * 与 `createPanAccumulator` 的区别：平移要把位移**加起来**，而参数是**替换** ——
 * 拖动中来了十个值，只需把最后那个发出去。规矩与平移那条一样（`AGENTS.md` §7.9）：
 *
 * 1. **一帧最多发一次**（高刷屏的 `input` 事件能到 200Hz）；
 * 2. **尾样本必发** —— 停手那一次不能丢，否则 Rust 手里会停在半路的值上
 *    （用户看到的就是「松手后画面又弹回去一点」）；
 * 3. 值相同就不发（省掉一次 IPC + 一次管线）。
 *
 * 为什么不用防抖（debounce）：拖动过程中画面**必须**跟着变，防抖会让它一秒才动一次。
 */
export interface LatestCoalescer<T> {
  /** 记一个最新值（多密都行；同一帧内只发最后一个） */
  push: (value: T) => void;
  /** 立刻把挂起的那一个发出去（松手 / 卸载前 —— 尾样本） */
  flush: () => void;
  /** 取消挂起的调度（卸载时），**不发** */
  dispose: () => void;
  /** 已经发出过几次（冒烟断言用） */
  sentCount: () => number;
}

export interface LatestCoalescerDeps<T> {
  /** 真正发出去（IPC / 测试里换成数组收集） */
  send: (value: T) => void;
  /** 相等判定（默认为 `Object.is`） */
  equals?: (a: T, b: T) => boolean;
  /** 帧调度（默认真实 rAF；测试注入确定实现） */
  scheduler?: FrameScheduler;
}

export function createLatestCoalescer<T>(deps: LatestCoalescerDeps<T>): LatestCoalescer<T> {
  const scheduler = deps.scheduler ?? createFrameScheduler();
  const equals = deps.equals ?? Object.is;
  let pending: { value: T } | null = null;
  let ticket: FrameTicket | null = null;
  let last: { value: T } | null = null;
  let sent = 0;

  const emit = (value: T): void => {
    if (last !== null && equals(last.value, value)) return;
    last = { value };
    sent += 1;
    deps.send(value);
  };

  const flushPending = (): void => {
    ticket = null;
    if (pending === null) return;
    const value = pending.value;
    pending = null;
    emit(value);
  };

  return {
    push: (value) => {
      pending = { value };
      ticket ??= scheduler.request(flushPending);
    },
    flush: () => {
      if (ticket !== null) {
        scheduler.cancel(ticket);
        ticket = null;
      }
      flushPending();
    },
    dispose: () => {
      if (ticket !== null) {
        scheduler.cancel(ticket);
        ticket = null;
      }
      pending = null;
    },
    sentCount: () => sent,
  };
}
