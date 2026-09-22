/**
 * 编辑视口的**洞口上报**（`AGENTS.md` §6.1 红线 #2、§7.9 的坐标契约）。
 *
 * 放在 `lib/` 而不是 `features/editor/`：它是**纯逻辑**（载荷构造、去重、帧合并），
 * 只有 `send` 这一个出口由调用方注入 —— 于是不依赖 `api`、也不需要真窗口就能单测
 * （与 `lib/histogram.ts` 同一套做法）。IPC 那一步在 `src/api/editor.ts`。
 *
 * 前端在这一条链路上只做一件事：**把看到的原始事实报上去**。
 *
 * ```text
 *   getBoundingClientRect()  →  CSS 像素矩形（洞口）
 *   window.devicePixelRatio  →  **运行时**比例（含显示器 DPI + 系统文字缩放 + 页面缩放）
 *   window.innerWidth/Height →  WebView 的 CSS 视口（别拿 native 尺寸 ÷ native scale 冒充）
 *                    ↓  editor_set_viewport
 *          Rust 存下这份事实（W2 起渲染线程按它摆图 / 裁切 / 命中测试）
 * ```
 *
 * 三条纪律（都是 §7.9 真机血泪里写死的）：
 *
 * 1. **不许在前端做任何换算**：不乘系数、不除 scale、不猜容器偏移 —— 数学在 Rust；
 * 2. **DPR 读运行时值**，不许写死、不许用 Tauri 的 `scale_factor()` 顶替（它不含文字缩放）；
 * 3. **节流必须保留尾样本**：拖窗口结束时那一次也要发出去，否则 Rust 手里的洞口会停在
 *    中间某一帧上（`AGENTS.md` §7.9 的「诊断红旗」第 5 条）。
 *
 * 所以这里的节流是「**合并成每帧一次 + 只发最新值**」（帧级合并），而不是「N 毫秒丢弃」——
 * 后者会把最后一次丢掉。相同值不重发（`ResizeObserver` 会为不改变尺寸的挂载也回调一次）。
 *
 * 纯逻辑（载荷构造、合法性、去重）+ 可注入的帧调度 —— 于是能单测，不需要真窗口。
 */

/** 一次上报的载荷（**原始事实**，字段名与 Rust 侧 DTO 一一对应）。 */
export interface EditorViewportPayload {
  hole: { x: number; y: number; width: number; height: number };
  dpr: number;
  viewport: { width: number; height: number };
}

/** CSS 矩形（`getBoundingClientRect()` 的四个数）。 */
export interface CssRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/* ══════════════════════════════════════════════════════════════
 * 帧调度（可注入，测试里换成确定的假实现）
 * ══════════════════════════════════════════════════════════════ */

/**
 * 帧票号：**我们自己发的**不透明凭据。
 *
 * 为什么不把底层句柄直接露出来：浏览器 `requestAnimationFrame` 给 `number`，
 * 而 Node 的 `setTimeout` 给 `Timeout` 对象 —— 两种形状不同，露到接口上就得在两边都做断言。
 * 包一层之后接口只有一个形状，句柄留在实现内部的表里。
 */
export interface FrameTicket {
  readonly id: number;
}

export interface FrameScheduler {
  /** 安排一帧 */
  request: (run: () => void) => FrameTicket;
  /** 取消（票据无效时什么都不做） */
  cancel: (ticket: FrameTicket) => void;
}

/** 真实环境用的调度：优先 `requestAnimationFrame`，没有就退回 16ms 定时器（测试/SSR）。 */
export function createFrameScheduler(): FrameScheduler {
  let nextId = 1;
  const live = new Map<number, { handle: unknown; kind: "frame" | "timer" }>();

  return {
    request: (run) => {
      const id = nextId;
      nextId += 1;
      if (typeof requestAnimationFrame === "function") {
        const handle = requestAnimationFrame(() => {
          live.delete(id);
          run();
        });
        live.set(id, { handle, kind: "frame" });
      } else {
        const handle = globalThis.setTimeout(() => {
          live.delete(id);
          run();
        }, 16);
        live.set(id, { handle, kind: "timer" });
      }
      return { id };
    },
    cancel: (ticket) => {
      const entry = live.get(ticket.id);
      if (entry === undefined) return;
      live.delete(ticket.id);
      if (entry.kind === "frame" && typeof entry.handle === "number") {
        cancelAnimationFrame(entry.handle);
        return;
      }
      // SAFETY: 句柄来自上面的 `setTimeout` 分支（浏览器里是 number、Node 里是 Timeout），
      // `clearTimeout` 两种都收；TS 在 DOM 与 @types/node 混用时这两个签名不一致，只能在这里收窄。
      clearTimeout(entry.handle as Parameters<typeof clearTimeout>[0]);
    },
  };
}

/* ══════════════════════════════════════════════════════════════
 * 载荷
 * ══════════════════════════════════════════════════════════════ */

function finite(value: number): boolean {
  return typeof value === "number" && Number.isFinite(value);
}

/**
 * 构造上报载荷；**任何一项不是有限数就返回 `null`**（宁可不报，也不报一个错的洞口 ——
 * §7.9 的教训就是「静默回退」比报错可怕）。
 *
 * `dpr <= 0` 也算非法：后面所有的 CSS→物理换算都以它为分母。
 */
export function editorViewportPayload(input: {
  rect: CssRect;
  dpr: number;
  viewport: { width: number; height: number };
}): EditorViewportPayload | null {
  const { rect, dpr, viewport } = input;
  if (!finite(rect.x) || !finite(rect.y) || !finite(rect.width) || !finite(rect.height)) {
    return null;
  }
  if (!finite(dpr) || dpr <= 0) return null;
  if (!finite(viewport.width) || !finite(viewport.height)) return null;
  if (rect.width < 0 || rect.height < 0) return null;
  return {
    hole: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
    dpr,
    viewport: { width: viewport.width, height: viewport.height },
  };
}

/** 两次载荷「是不是同一件事」（相同就不重发）。 */
export function sameViewportPayload(
  a: EditorViewportPayload,
  b: EditorViewportPayload,
): boolean {
  return (
    a.dpr === b.dpr &&
    a.hole.x === b.hole.x &&
    a.hole.y === b.hole.y &&
    a.hole.width === b.hole.width &&
    a.hole.height === b.hole.height &&
    a.viewport.width === b.viewport.width &&
    a.viewport.height === b.viewport.height
  );
}

/* ══════════════════════════════════════════════════════════════
 * 上报器
 * ══════════════════════════════════════════════════════════════ */

export interface ViewportReporterDeps {
  /** 真正发出去（IPC / 日志 / 测试里换成数组收集） */
  send: (payload: EditorViewportPayload) => void;
  /** 帧调度（默认真实 rAF；测试注入确定实现） */
  scheduler?: FrameScheduler;
}

export interface ViewportReporter {
  /** 报一个新观察值（可在 ResizeObserver / resize / DPR 变化里随便调） */
  observe: (input: {
    rect: CssRect;
    dpr: number;
    viewport: { width: number; height: number };
  }) => void;
  /** 立刻把挂起的那一帧发出去（卸载前用，保证尾样本不丢） */
  flush: () => void;
  /** 取消挂起的调度（卸载时） */
  dispose: () => void;
  /** 已经成功发出过几次（诊断 / 冒烟断言用） */
  sentCount: () => number;
}

export function createViewportReporter(deps: ViewportReporterDeps): ViewportReporter {
  const scheduler = deps.scheduler ?? createFrameScheduler();

  let pending: EditorViewportPayload | null = null;
  let ticket: FrameTicket | null = null;
  let last: EditorViewportPayload | null = null;
  let sent = 0;

  const emit = (payload: EditorViewportPayload): boolean => {
    if (last !== null && sameViewportPayload(last, payload)) return false;
    last = payload;
    sent += 1;
    deps.send(payload);
    return true;
  };

  return {
    observe: (input) => {
      const payload = editorViewportPayload(input);
      // 非法值：**不报**（Rust 侧也会拒绝），但也不清掉上一次的好值 ——
      // 布局中间态（比如宽度 0 的那一帧）不该把洞口擦掉
      if (payload === null) return;
      if (last !== null && sameViewportPayload(last, payload) && pending === null) return;
      pending = payload;
      ticket ??= scheduler.request(() => {
        ticket = null;
        const next = pending;
        pending = null;
        if (next !== null) emit(next);
      });
    },
    flush: () => {
      if (ticket !== null) {
        scheduler.cancel(ticket);
        ticket = null;
      }
      const next = pending;
      pending = null;
      if (next !== null) emit(next);
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
