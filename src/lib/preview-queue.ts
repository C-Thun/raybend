/**
 * 预览图刷新的**后台队列**（单飞 + 最新者优先）。
 *
 * 为什么需要它：一张 1920 预览要跑整条显影管线 + AVIF 编码 —— 普通照片几百毫秒，
 * 开了高质量降噪（BM3D）能到十几秒。它**不该押在任何用户动作的返回值上**：
 *
 * * 以前 `confirmEdit` 把它 await 在落库链里，于是「松手 → 落库 → 等十几秒」；
 *   连续调几根拉杆就排起一串重复渲染，定稿时 `await persistTail` 还要把它们等完
 *   （崔总 2026-10-10 报的「定稿生成近 1 分钟」的一半原因）；
 * * 现在：落库照旧（快，且必须等），预览**排队在后台**；同一时刻只渲染一张，
 *   排队中的请求只保留最后一次（中间状态没人会看）。
 *
 * `after` 是**落库闸门**：预览读的是库里的当前栈，必须等这一笔 commit 落完再渲染，
 * 否则会拿旧栈渲染出一张过期预览。闸门失败（落库失败）时**跳过渲染**并照常 resolve ——
 * 绝不能拿旧栈去覆盖。
 */
export interface PreviewQueueDeps {
  /** 真正的渲染（一般是 `refreshDevelopPreview`） */
  run: (path: string) => Promise<unknown>;
  /** 渲染失败（只报不抛：队列要继续跑后面的） */
  onError?: (error: unknown) => void;
}

export interface PreviewQueue {
  /**
   * 排队刷新一张预览。
   *
   * @param path 照片路径
   * @param after 落库闸门；解析后才渲染（失败则跳过）。缺省立即渲染。
   * @returns 在该次（或更新的）渲染完成后 resolve 的 promise ——
   *          需要「预览就绪后再做点什么」（如刷新总览图）时 await 它。
   */
  push: (path: string, after?: Promise<unknown>) => Promise<void>;
  /** 卸载：清掉排队项并放行等待者；在飞的那一次无法取消（结果会被丢弃） */
  dispose: () => void;
}

export function createPreviewQueue(deps: PreviewQueueDeps): PreviewQueue {
  interface Job {
    path: string;
    after: Promise<unknown>;
    waiters: (() => void)[];
  }

  let running = false;
  let disposed = false;
  let active: Job | null = null;
  let pending: Job | null = null;

  const pump = (): void => {
    if (disposed || running || pending === null) return;
    const job = pending;
    pending = null;
    active = job;
    running = true;
    void (async () => {
      try {
        await job.after;
      } catch {
        // 落库失败：不做渲染（旧栈渲染等于写一张过期预览）
        return;
      }
      if (disposed) return;
      try {
        await deps.run(job.path);
      } catch (error) {
        deps.onError?.(error);
      }
    })().finally(() => {
      running = false;
      active = null;
      for (const resolve of job.waiters) resolve();
      pump();
    });
  };

  return {
    push: (path, after) => {
      if (disposed) return Promise.resolve();
      const gate = after ?? Promise.resolve();
      const waiters = pending?.waiters ?? [];
      const promise = new Promise<void>((resolve) => { waiters.push(resolve); });
      // 最新者优先：排队中的请求被后来的替换，等待者合并（都等下一次真正跑完）
      pending = { path, after: gate, waiters };
      pump();
      return promise;
    },
    dispose: () => {
      disposed = true;
      pending = null;
      // 在飞那一次无法取消（Rust 侧渲染不接取消）——只放行等待者，结果丢弃
      for (const job of [active]) {
        if (job === null) continue;
        for (const resolve of job.waiters) resolve();
      }
    },
  };
}
