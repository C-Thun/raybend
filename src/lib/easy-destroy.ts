/**
 * `easy destroy` 范式（DESIGN.md §12.2）的**逻辑层**。
 *
 * 规则：所有「移除 / 排除」类操作默认弹确认框；**按住 `Shift` 再点可跳过确认**。
 *
 * 为什么单独成一个文件而不是写在组件里：
 *   1. 判定（要不要弹）与呈现（弹什么）分开，判定才能被单元测试覆盖 ——
 *      「Shift 跳过」这种规则一旦写错，人会在**真的删掉东西**之后才发现
 *   2. M1-5 有多个应用点（已选目录横条、Recent 行、Tile 排除徽标），逻辑必须共用一份
 *
 * 语义红线（AGENTS.md §11.3）：**移除/排除不是删除到磁盘**，
 * 第一阶段没有「删除」这个动作，所以这里连带提一句：文案与图标都不要用破坏性语汇。
 */

import { createSignal } from "solid-js";

/** 只依赖 `shiftKey`，这样测试里可以传一个字面量，不需要真的造 MouseEvent */
export interface ShiftLikeEvent {
  shiftKey?: boolean;
}

/** 是否跳过确认（按住 Shift 即为是） */
export function shouldSkipConfirm(event?: ShiftLikeEvent | null): boolean {
  return Boolean(event?.shiftKey);
}

export interface PendingDestroy {
  /** 已经是**翻译后**的文案（i18n key 由调用方取好） */
  message: string;
  /** 确认框的标题（可空） */
  title?: string;
}

export interface EasyDestroyRequest extends PendingDestroy {
  run: () => void | Promise<void>;
}

export type RequestOutcome = "ran" | "asked";

export interface EasyDestroyController {
  /** 当前待确认的请求；`null` 表示没有弹窗 */
  pending: () => PendingDestroy | null;
  /**
   * 发起一次移除操作。
   *
   * - 事件带 `shiftKey` → **立即执行**，返回 `"ran"`
   * - 否则 → 挂起并返回 `"asked"`，由调用方渲染确认框，再调 `confirm()` / `cancel()`
   */
  request: (
    message: string,
    run: () => void | Promise<void>,
    event?: ShiftLikeEvent | null,
    title?: string,
  ) => RequestOutcome;
  confirm: () => void;
  cancel: () => void;
}

/**
 * 创建一个移除确认控制器。
 *
 * 注意 `request` 返回的是**同步**结论：`"ran"` 只代表「已经开始执行」，
 * 若 `run` 是异步的，其完成与否由调用方自己跟踪（确认框此时已经关掉了）。
 * `run` 抛错会被捕获并打到控制台 —— 状态不会脏，但**不会**自动重试或回滚。
 */
export function createEasyDestroy(): EasyDestroyController {
  const [pending, setPending] = createSignal<EasyDestroyRequest | null>(null);

  const execute = (request: EasyDestroyRequest): void => {
    // 先关弹窗再执行：run 抛错时不能把弹窗留在「已确认但没生效」的状态
    setPending(null);
    try {
      const result = request.run();
      if (result instanceof Promise) {
        // 不吞错也不放它变成 unhandled rejection（那会直接崩掉 Node 进程）
        result.catch((error: unknown) => {
          console.error("[easy-destroy] 移除操作失败", error);
        });
      }
    } catch (error) {
      // 同步抛错同理：调用方负责提示，这里只保证状态干净
      console.error("[easy-destroy] 移除操作失败", error);
    }
  };

  return {
    pending: () => {
      const current = pending();
      return current
        ? { message: current.message, title: current.title }
        : null;
    },

    request: (message, run, event, title) => {
      const request: EasyDestroyRequest = { message, run, title };
      if (shouldSkipConfirm(event)) {
        execute(request);
        return "ran";
      }
      // 连续发起第二次时**覆盖**前一个：同一时刻只该有一个确认框，
      // 否则用户会对着一句已经过期的文案点「确定」。
      setPending(request);
      return "asked";
    },

    confirm: () => {
      const current = pending();
      if (!current) return;
      execute(current);
    },

    cancel: () => setPending(null),
  };
}
