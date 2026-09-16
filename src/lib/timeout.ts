/**
 * 给「可能永远不回来的异步调用」加一道时限。
 *
 * ## 为什么需要它（2026-09-16 真机踩坑，值得记住）
 *
 * 后端命令如果 **panic**，前端那个 promise **永远不会 settle** —— 不是 reject、不是
 * resolve，是静悄悄地悬在那里。后果比「报错」严重得多：
 *
 *   * 界面卡在忙碌态：进度条不来（因为快照一直没到）、暂停/取消全禁用；
 *   * 连「取消」都发不出去（它同样卡在同一条死路上），用户**没有任何逃生的按钮**；
 *   * 看起来像「软件死了但窗口还在」。
 *
 * 有这道时限之后，最坏情况会落到**错误态**：用户看到一句人话、能关掉弹窗、能重试。
 * 也就是说：**时限的作用不是让慢操作变快，而是让「卡死」变成「报错」**。
 *
 * 用法：`await withTimeout(deps.api.start(...), timeoutMs, "启动导入")`
 */

/** 超时错误：`instanceof` 可辨，消息已是可以直接给用户看的一句话 */
export class TimeoutError extends Error {
  /** 被限时的那个动作（中文，用于拼消息） */
  readonly what: string;
  /** 时限（毫秒） */
  readonly timeoutMs: number;

  constructor(what: string, timeoutMs: number) {
    super(`${what}没有在 ${(timeoutMs / 1000).toFixed(0)} 秒内回应（后端可能已经挂了）`);
    this.name = "TimeoutError";
    // ⚠️ 字段在这里**逐个赋值**，不用构造函数参数属性：项目的测试跑在 Node 的
    // strip-only TS 模式下（`--experimental-strip-types`），**不支持**参数属性
    // （`constructor(readonly x: T)`）—— 那样会整个测试文件语法报错。
    this.what = what;
    this.timeoutMs = timeoutMs;
  }
}

/**
 * 给 `task` 加时限；超时就抛 {@link TimeoutError}（原始结果与错误照常透传）。
 *
 * 时限到了之后**不再关心** task 的最终结果（它可能永远不来）——
 * 但要挂一个空 `catch` 兜住「超时之后它又失败了」的未处理拒绝，免得控制台报噪音。
 */
export function withTimeout<T>(
  task: Promise<T>,
  timeoutMs: number,
  what: string,
): Promise<T> {
  // 时限非正数 = 不设限（测试与「用户自己关掉了保护」都用得上）
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) return task;

  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => {
      task.catch(() => {
        // 超时之后它才失败：与我们无关了，吞掉以免出现 unhandled rejection
      });
      reject(new TimeoutError(what, timeoutMs));
    }, timeoutMs);

    task.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (error: unknown) => {
        clearTimeout(timer);
        reject(error instanceof Error ? error : new Error(String(error)));
      },
    );
  });
}
