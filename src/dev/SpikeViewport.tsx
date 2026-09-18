/**
 * 渲染 spike 页（`?spike=1`）。
 *
 * ⚠️ 这是**开发期诊断页**（与 `src/dev/` 里的陈列室同一性质）：文案不进语言包
 * （i18n 守门脚本对本目录豁免），因为它只给开发者/维护者看，而且生命周期止于
 * 「A.2 结论落定」。真变成产品功能（比如查看器）时再搬进 `src/features/` 并补语言包。
 *
 * # 这一页在验证什么
 *
 * `PLAN.md` A.2 那张清单里，**人眼才能判**的部分：
 *
 * | 界面上的东西 | 对应 A.2 的哪一项 |
 * | --- | --- |
 * | 中间那块**透明区**（洞口） | 透明挖洞是否成立（看到的是桌面/下层窗口，还是黑底/白边） |
 * | 洞口里的照片 + 四角方位标记 | 方向/镜像有没有搞错；1:1 时白块是不是方方正正一个像素 |
 * | 左上「命中测试」两行数 | 坐标同步：鼠标放在白块上时图像坐标应当是图心 |
 * | 右栏帧统计表 | 性能基线（帧间隔 + 我们自己的 CPU 时长分开看） |
 * | 工具条上的档位/旋转/洞口开关 | DPI、多显示器、resize/最大化/全屏/最小化恢复 |
 *
 * # 两条交互纪律
 *
 * 1. **指针事件留在 webview 里，转成「意图」交给 Rust** —— 这是产品架构
 *    （`AGENTS.md` §6.1 红线 #1：前端只发意图，不做坐标数学）。所以洞口**不**用
 *    `pointer-events: none`：那样反而收不到滚轮与拖动。A.2 里提到的另一个方向
 *    （`set_ignore_cursor_events`）是「让原生层直接吃事件」的退路，本轮不走它。
 * 2. **洞口矩形由界面报给 Rust**：DOM 是布局权威（面板多宽是 CSS 说了算），
 *    Rust 是变换权威。两者在 `clip_rect` 上会合，谁都不许自己推另一方的数学。
 */

import { createSignal, For, onCleanup, onMount, Show } from "solid-js";

import {
  spikeClose,
  spikeCommand,
  spikeOpen,
  spikeSnapshot,
  spikeWriteReport,
  type HumanNotes,
  type SpikeSnapshot,
} from "../api/spike.ts";
import { isTauriRuntime } from "../api/tauri-env.ts";

/**
 * 洞口距中列四边的边距（CSS 像素）。
 *
 * **两处必须一致**：这里的布局（洞口矩形、四周那圈不透明底）与
 * `src-tauri/src/spike_viewport.rs` 的 `HOLE_MARGIN_CSS`（`setHole` 按边距推算时的退路）。
 * 正常运行以**界面上的报的**矩形为准（DOM 是布局权威，Rust 是变换权威），
 * 这个值只在两边都要各自“没收到对方消息”时兵分二路地落同一个数。
 */
const HOLE_MARGIN_PX = 0;

const EMPTY: SpikeSnapshot = {
  open: false,
  adapter: { backend: "", name: "", deviceType: "", driver: "", driverInfo: "" },
  format: "",
  alphaMode: "",
  surfaceSize: [0, 0],
  cssSize: [0, 0],
  monitorScale: 1,
  monitorName: "",
  maximized: false,
  fullscreen: false,
  decorated: false,
  transparent: false,
  uploadMs: 0,
  rendered: 0,
  viewport: {
    zoom: 1,
    panX: 0,
    panY: 0,
    rotation: 0,
    fitMode: "",
    holeCss: null,
    holePhysical: null,
    dpr: 1,
    imageWidth: 0,
    imageHeight: 0,
  },
  scenarios: [],
  deviceLost: [],
  coordMaxError: 0,
  lastHit: null,
  lastFrameMs: 0,
  lastCpuMs: 0,
  lastError: null,
  viewportProblems: [],
  clientOrigin: [0, 0],
  windowOrigin: [0, 0],
  webviewOrigin: null,
  inputOffset: null,
};

function fmt(value: number | null | undefined, digits = 2, suffix = ""): string {
  if (value === null || value === undefined) return "—";
  return `${value.toFixed(digits)}${suffix}`;
}

export default function SpikeViewport() {
  const [snap, setSnap] = createSignal<SpikeSnapshot>(EMPTY);
  const [busy, setBusy] = createSignal(false);
  const [message, setMessage] = createSignal<string>("");
  const [reportDir, setReportDir] = createSignal("C:\\rb-target\\spike-report");
  const [notes, setNotes] = createSignal<HumanNotes>({
    transparencyOk: null,
    oneToOneSharp: null,
    acrossMonitorsOk: null,
    feel: "",
    notes: "",
  });

  let hole: HTMLDivElement | undefined;
  let dragging = false;
  let lastPointer: { x: number; y: number } | null = null;
  let lastHitAt = 0;

  const apply = (next: SpikeSnapshot) => setSnap(next);

  async function run(fn: () => Promise<SpikeSnapshot>, note?: string): Promise<void> {
    setBusy(true);
    try {
      apply(await fn());
      if (note) setMessage(note);
    } catch (error) {
      setMessage(`命令失败：${String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  onMount(() => {
    // 这一页要能透出下层：整条祖先链的背景都必须透明，
    // 否则洞口看着「透明」，其实后面压着 body 的底色。
    const html = document.documentElement;
    const body = document.body;
    const prevHtml = html.style.background;
    const prevBody = body.style.background;
    html.style.background = "transparent";
    body.style.background = "transparent";
    onCleanup(() => {
      html.style.background = prevHtml;
      body.style.background = prevBody;
    });

    // 洞口矩形 → Rust（布局权威在 CSS 这边）
    if (hole) {
      const observer = new ResizeObserver(() => reportHole());
      observer.observe(hole);
      onCleanup(() => observer.disconnect());
      reportHole();
    }

    /*
     * webview 原点（屏幕 CSS 像素）→ Rust。
     *
     * 为什么需要它：输入的 `clientX/clientY` 是**相对 webview** 的，而 wgpu 表面挂在
     * 窗口句柄上；两者原点若不同（带边框的窗口就常差一个标题栏），就会出现
     * 「图看着是对的、鼠标读出的图像坐标却差一截」（人类 2026-09-18 报的）。
     * 这里把 webview 的原点也报上去，Rust 侧与 `inner_position()` 一比就知道差多少。
     */
    let lastOrigin = "";
    const reportOrigin = (): void => {
      if (!isTauriRuntime()) return;
      const key = `${window.screenX},${window.screenY},${window.devicePixelRatio}`;
      if (key === lastOrigin) return;
      lastOrigin = key;
      void spikeCommand({
        kind: "webviewOrigin",
        screenX: window.screenX,
        screenY: window.screenY,
        dpr: window.devicePixelRatio,
      }).then(apply).catch(() => {});
    };
    reportOrigin();
    window.addEventListener("resize", reportOrigin);

    // 轮询状态：渲染线程在跑，界面跟着看
    const timer = window.setInterval(() => {
      if (!isTauriRuntime()) return;
      void spikeSnapshot().then(apply).catch(() => {});
      reportOrigin();
    }, 300);
    onCleanup(() => window.clearInterval(timer));
    onCleanup(() => window.removeEventListener("resize", reportOrigin));
  });

  function reportHole(): void {
    if (!hole || !isTauriRuntime()) return;
    const rect = hole.getBoundingClientRect();
    void spikeCommand({
      kind: "holeRect",
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height,
    })
      .then(apply)
      .catch(() => {});
  }

  function onWheel(event: WheelEvent): void {
    event.preventDefault();
    const factor = event.deltaY < 0 ? 1.12 : 1 / 1.12;
    void run(() =>
      spikeCommand({ kind: "zoom", x: event.clientX, y: event.clientY, factor }),
    );
  }

  function onPointerMove(event: PointerEvent): void {
    if (!isTauriRuntime()) return;
    if (dragging && lastPointer) {
      const dx = event.clientX - lastPointer.x;
      const dy = event.clientY - lastPointer.y;
      lastPointer = { x: event.clientX, y: event.clientY };
      void run(() => spikeCommand({ kind: "pan", dx, dy }));
      return;
    }
    // 命中测试限流：每 60ms 一次，够看数又不会把 IPC 打满
    const now = performance.now();
    if (now - lastHitAt < 60) return;
    lastHitAt = now;
    void run(() =>
      spikeCommand({ kind: "hitTest", x: event.clientX, y: event.clientY }),
    );
  }

  const view = () => snap().viewport;
  const hit = () => snap().lastHit;

  return (
    <div class="flex h-screen w-screen flex-col overflow-hidden text-fs-2 text-fg-1">
      {/* ── 顶栏（不透明）──────────────────────────────── */}
      {/* 无边框窗口靠这一行拖（`data-tauri-drag-region` 由 Tauri 接管为原生拖动） */}
      <header
        data-tauri-drag-region
        class="flex shrink-0 items-center gap-2 border-b border-line-1 bg-surface-bar px-3 py-1.5"
      >
        <span class="font-600">渲染 spike（M2-W1 · PLAN.md A.2）</span>
        <span class="text-fg-3">
          {isTauriRuntime() ? "Tauri 环境" : "浏览器预览（没有后端，数据全空）"}
        </span>
        <span class="ml-auto flex items-center gap-2">
          <Show when={snap().lastError}>
            <span class="text-fg-2">⚠ {snap().lastError}</span>
          </Show>
          <button
            type="button"
            class="rounded-(--radius) bg-surface-track px-2 py-0.5 hover:bg-state-hover"
            disabled={busy()}
            onClick={() => void run(() => spikeOpen(), "窗口已打开")}
          >
            打开窗口
          </button>
          <button
            type="button"
            class="rounded-(--radius) bg-surface-track px-2 py-0.5 hover:bg-state-hover"
            disabled={busy()}
            onClick={() => void spikeClose().then(() => setMessage("窗口已关闭"))}
          >
            关闭窗口
          </button>
          <span class="text-fg-3">{message()}</span>
        </span>
      </header>

      <div class="flex min-h-0 flex-1">
        {/* ── 左：环境与坐标 ──────────────────────────── */}
        <aside class="w-[300px] shrink-0 overflow-y-auto border-r border-line-1 bg-surface-main p-2">
          <h2 class="mb-1 text-fs-1 tracking-wide text-fg-2 uppercase">适配器</h2>
          <dl class="mb-3 grid grid-cols-[auto_1fr] gap-x-2 gap-y-0.5">
            <dt class="text-fg-3">后端</dt>
            <dd>{snap().adapter.backend || "—"}</dd>
            <dt class="text-fg-3">设备类型</dt>
            <dd>{snap().adapter.deviceType || "—"}</dd>
            <dt class="text-fg-3">名称</dt>
            <dd class="break-all">{snap().adapter.name || "—"}</dd>
            <dt class="text-fg-3">驱动</dt>
            <dd class="break-all">
              {snap().adapter.driver} {snap().adapter.driverInfo}
            </dd>
            <dt class="text-fg-3">WGPU_BACKEND</dt>
            <dd>见报告（环境变量）</dd>
          </dl>

          <h2 class="mb-1 text-fs-1 tracking-wide text-fg-2 uppercase">表面 / 窗口</h2>
          <dl class="mb-3 grid grid-cols-[auto_1fr] gap-x-2 gap-y-0.5">
            <dt class="text-fg-3">格式</dt>
            <dd>{snap().format || "—"}</dd>
            <dt class="text-fg-3">alpha</dt>
            <dd>{snap().alphaMode || "—"}</dd>
            <dt class="text-fg-3">物理</dt>
            <dd>
              {snap().surfaceSize[0]}×{snap().surfaceSize[1]}
            </dd>
            <dt class="text-fg-3">CSS</dt>
            <dd>
              {snap().cssSize[0]}×{snap().cssSize[1]}
            </dd>
            <dt class="text-fg-3">dpr</dt>
            <dd>{view().dpr.toFixed(3)}</dd>
            <dt class="text-fg-3">显示器</dt>
            <dd>
              {snap().monitorName || "—"}（{snap().monitorScale.toFixed(2)}）
            </dd>
            <dt class="text-fg-3">窗口</dt>
            <dd>
              {snap().maximized ? "最大化 " : ""}
              {snap().fullscreen ? "全屏 " : ""}
              {snap().decorated ? "有边框" : "无边框"}
            </dd>
            {/*
              三个屏幕原点对照 —— 「图看着对、鼠标坐标却差一截」的判据就是最后一行。
              webview 与客户区重合（输入偏移 = 0,0）时，输入的 CSS 坐标与 wgpu
              表面用的是同一套坐标系；不重合就是差多少补多少。
            */}
            <dt class="text-fg-3">窗口原点</dt>
            <dd>
              {snap().windowOrigin[0]}, {snap().windowOrigin[1]}
            </dd>
            <dt class="text-fg-3">客户区原点</dt>
            <dd>
              {snap().clientOrigin[0]}, {snap().clientOrigin[1]}
            </dd>
            <dt class="text-fg-3">webview 原点</dt>
            <dd>
              {snap().webviewOrigin
                ? `${snap().webviewOrigin![0]}, ${snap().webviewOrigin![1]}`
                : "—"}
            </dd>
            <dt class="text-fg-3">输入偏移</dt>
            <dd class={snap().inputOffset ? "font-600 text-fg-1" : "text-fg-2"}>
              {snap().inputOffset
                ? `${snap().inputOffset![0]}, ${snap().inputOffset![1]}`
                : "—"}
              {snap().inputOffset &&
              snap().inputOffset![0] === 0 &&
              snap().inputOffset![1] === 0
                ? " ✓ 对齐"
                : ""}
            </dd>
          </dl>

          <h2 class="mb-1 text-fs-1 tracking-wide text-fg-2 uppercase">视口</h2>
          <dl class="mb-3 grid grid-cols-[auto_1fr] gap-x-2 gap-y-0.5">
            <dt class="text-fg-3">档位</dt>
            <dd>{view().fitMode || "—"}</dd>
            <dt class="text-fg-3">zoom</dt>
            <dd>{view().zoom.toFixed(4)}</dd>
            <dt class="text-fg-3">pan</dt>
            <dd>
              {view().panX.toFixed(1)}, {view().panY.toFixed(1)}
            </dd>
            <dt class="text-fg-3">旋转</dt>
            <dd>{view().rotation.toFixed(1)}°</dd>
            <dt class="text-fg-3">洞口(CSS)</dt>
            <dd>
              {view().holeCss
                ? view()
                    .holeCss!.map((v) => Math.round(v))
                    .join(", ")
                : "整窗"}
            </dd>
            {/*
              洞口的物理像素矩形。与上一行一比就知道单位错没错：1.25 缩放下
              (490,227,144,327) 应当对应 (612,284,180,409)。若这里仍是 144×327，
              就是漏了 dpr；若数值对、画面仍偏，那偏的是**表面原点**（wgpu 表面挂在
              窗口 HWND 上），而不是这里的数学。
            */}
            <dt class="text-fg-3">洞口(物理)</dt>
            <dd>
              {view().holePhysical
                ? view()
                    .holePhysical!.map((v) => Math.round(v))
                    .join(", ")
                : "整窗"}
            </dd>
            <dt class="text-fg-3">图像</dt>
            <dd>
              {view().imageWidth}×{view().imageHeight}
            </dd>
            <dt class="text-fg-3">上传耗时</dt>
            <dd>{fmt(snap().uploadMs, 1, " ms")}</dd>
          </dl>

          <h2 class="mb-1 text-fs-1 tracking-wide text-fg-2 uppercase">
            命中测试（把鼠标放到白块上）
          </h2>
          <div class="mb-1 rounded-(--radius) bg-surface-track p-2">
            <Show when={hit()} fallback={<span class="text-fg-3">把鼠标移到洞口里</span>}>
              <div>
                图像坐标 {fmt(hit()!.imageX, 1)}, {fmt(hit()!.imageY, 1)}
              </div>
              <div class={hit()!.inside ? "text-fg-2" : "text-fg-3"}>
                {hit()!.inside ? "在图像内" : "不在图像内（洞口外或图外）"}
              </div>
              <div class={hit()!.atCenter ? "font-600" : "text-fg-3"}>
                {hit()!.atCenter ? "✅ 命中图像中心（坐标同步正确）" : "（还没到中心）"}
              </div>
            </Show>
          </div>
          <div class="text-fg-3">
            程序化往返最大偏差：{snap().coordMaxError.toFixed(6)} 物理像素
          </div>
        </aside>

        {/* ── 中：洞口（**透明**，照片在这里透出来）────── */}
        <main class="relative min-h-0 min-w-0 flex-1">
          <div
            ref={hole}
            /*
             * 这一块**没有背景**，靠窗口透明透出 wgpu 画的内容；它同时是鼠标事件的
             * 接收面（滚轮缩放 / 拖动平移 / 位置上报）。
             *
             * ⚠️ 光标必须是**十字准星**而不是手形：本项判据是「鼠标压在哪一块像素上」，
             * 手形的图形会把瞄准点整个盖住（人类 2026-09-18 报「没法确定鼠标在哪」）。
             * 拖动时给 `grabbing` 作为「正在拖」的反馈，松开即回到准星。
             */
            class="absolute cursor-crosshair active:cursor-grabbing"
            style={{ "touch-action": "none", inset: `${HOLE_MARGIN_PX}px` }}
            onWheel={onWheel}
            onPointerDown={(event) => {
              dragging = true;
              lastPointer = { x: event.clientX, y: event.clientY };
              event.currentTarget.setPointerCapture(event.pointerId);
            }}
            onPointerUp={(event) => {
              dragging = false;
              lastPointer = null;
              event.currentTarget.releasePointerCapture(event.pointerId);
            }}
            onPointerMove={onPointerMove}
          />
          {/*
           * 洞口 = **整个中间列**（`HOLE_MARGIN_PX = 0`），这里一个字节都不画。
           *
           * ⚠️ **绝不能整块盖底色** —— 负 z-index 只决定页内堆叠顺序，**并不会**让页面
           * 像素变透明。早先这里是两块 `inset-0` 的底色，等于连洞口一起刷成不透明，
           * 「透明挖洞」根本没出现（人类 2026-09-17 看到中间一片暗色、wgpu 画的东西
           * 一点都看不见）。
           *
           * 后来改成「留 190px 的环」是**过度纠正**：左右面板本来就是分开的列，中间列
           * 再缩 190px 只剩 144×327 的一条竖缝 —— Fit 档下 6000×4000 被压成 144×96 的
           * 小条、四角标记缩到 3~4px 根本看不见、鼠标一挪就「不在图像内」。
           * 人类 2026-09-18 报的正是这一串。现在不缩：中间列多大，洞口就多大。
           * 两侧面板自带底色，遮光不靠这里的环。
           */}
          <div
            class="pointer-events-none absolute border border-dashed border-line-2"
            style={{ inset: `${HOLE_MARGIN_PX}px` }}
            aria-hidden="true"
          />
        </main>

        {/* ── 右：工具、场景、帧统计、人的填写 ─────────── */}
        <aside class="w-[340px] shrink-0 overflow-y-auto border-l border-line-1 bg-surface-main p-2">
          <h2 class="mb-1 text-fs-1 tracking-wide text-fg-2 uppercase">档位与视图</h2>
          <div class="mb-3 flex flex-wrap gap-1">
            <For
              each={[
                { label: "适配 Fit", cmd: { kind: "fit", mode: "fit" } as const },
                { label: "铺满 Fill", cmd: { kind: "fit", mode: "fill" } as const },
                { label: "1:1", cmd: { kind: "fit", mode: "oneToOne" } as const },
                { label: "旋转 0°", cmd: { kind: "rotate", degrees: 0 } as const },
                { label: "旋转 90°", cmd: { kind: "rotate", degrees: 90 } as const },
                { label: "复位", cmd: { kind: "reset" } as const },
              ]}
            >
              {(item) => (
                <button
                  type="button"
                  class="rounded-(--radius) bg-surface-track px-2 py-0.5 hover:bg-state-hover"
                  disabled={busy()}
                  onClick={() => void run(() => spikeCommand(item.cmd))}
                >
                  {item.label}
                </button>
              )}
            </For>
            <Show when={view().holeCss}>
              <button
                type="button"
                class="rounded-(--radius) bg-surface-track px-2 py-0.5 hover:bg-state-hover"
                onClick={() =>
                  void run(async () => {
                    const snapshot = await spikeCommand({ kind: "setHole", on: false });
                    // 洞口关掉是「整窗出图」的对照档；开回来时要把**真实洞口**重报一遍，
                    // 否则 Rust 侧会停在按边距推算的兜底矩形上（旧的 190px 那一版就这么错过）。
                    reportHole();
                    return snapshot;
                  })
                }
              >
                洞口：关（整窗出图）
              </button>
            </Show>
            <Show when={!view().holeCss}>
              <button
                type="button"
                class="rounded-(--radius) bg-surface-track px-2 py-0.5 hover:bg-state-hover"
                onClick={() =>
                  void run(async () => {
                    const snapshot = await spikeCommand({ kind: "setHole", on: true });
                    reportHole();
                    return snapshot;
                  })
                }
              >
                洞口：开
              </button>
            </Show>
          </div>

          <h2 class="mb-1 text-fs-1 tracking-wide text-fg-2 uppercase">场景采集</h2>
          <div class="mb-2 flex flex-wrap gap-1">
            <button
              type="button"
              class="rounded-(--radius) bg-surface-track px-2 py-0.5 hover:bg-state-hover"
              onClick={() =>
                void run(
                  () => spikeCommand({ kind: "scripted", name: "scripted-pan", seconds: 3 }),
                  "脚本化平移 3 秒（不经过 IPC）",
                )
              }
            >
              脚本平移 3s
            </button>
            <button
              type="button"
              class="rounded-(--radius) bg-surface-track px-2 py-0.5 hover:bg-state-hover"
              onClick={() =>
                void run(
                  () => spikeCommand({ kind: "scripted", name: "scripted-zoom", seconds: 3 }),
                  "脚本化缩放 3 秒（不经过 IPC）",
                )
              }
            >
              脚本缩放 3s
            </button>
            <button
              type="button"
              class="rounded-(--radius) bg-surface-track px-2 py-0.5 hover:bg-state-hover"
              onClick={() =>
                void run(
                  () => spikeCommand({ kind: "scenario", name: "manual" }),
                  "开始采集：现在请拖动/滚轮/缩放窗口",
                )
              }
            >
              开始采集（手操）
            </button>
            <button
              type="button"
              class="rounded-(--radius) bg-surface-track px-2 py-0.5 hover:bg-state-hover"
              onClick={() =>
                void run(() => spikeCommand({ kind: "scenario", name: null }), "已结束采集")
              }
            >
              结束采集
            </button>
          </div>

          <table class="mb-3 w-full text-left">
            <thead class="text-fg-3">
              <tr>
                <th class="font-400">场景</th>
                <th class="font-400">样本</th>
                <th class="font-400">p50</th>
                <th class="font-400">p95</th>
                <th class="font-400">fps</th>
                <th class="font-400">CPU</th>
              </tr>
            </thead>
            <tbody>
              <For each={snap().scenarios}>
                {(scenario) => (
                  <tr title={scenario.how}>
                    <td class="pr-1">{scenario.name}</td>
                    <td>{scenario.frames}</td>
                    <td>{fmt(scenario.p50Ms, 2)}</td>
                    <td>{fmt(scenario.p95Ms, 2)}</td>
                    <td>{fmt(scenario.fps ?? null, 1)}</td>
                    <td>{fmt(scenario.cpuP50Ms, 2)}</td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
          <div class="mb-3 text-fg-3">
            实时：帧间隔 {fmt(snap().lastFrameMs, 2, " ms")} · CPU{" "}
            {fmt(snap().lastCpuMs, 2, " ms")} · 已画 {snap().rendered} 帧
          </div>

          <h2 class="mb-1 text-fs-1 tracking-wide text-fg-2 uppercase">设备丢失</h2>
          <div class="mb-2 flex gap-1">
            <button
              type="button"
              class="rounded-(--radius) bg-surface-track px-2 py-0.5 hover:bg-state-hover"
              onClick={() =>
                void run(() => spikeCommand({ kind: "deviceLoss" }), "已演练 device.destroy()")
              }
            >
              演练丢失
            </button>
            <button
              type="button"
              class="rounded-(--radius) bg-surface-track px-2 py-0.5 hover:bg-state-hover"
              onClick={() =>
                void run(() => spikeCommand({ kind: "recover" }), "已尝试恢复")
              }
            >
              恢复
            </button>
          </div>
          <ul class="mb-3 list-inside list-disc text-fg-3">
            <For each={snap().deviceLost}>{(line) => <li>{line}</li>}</For>
          </ul>

          <Show when={snap().viewportProblems.length > 0}>
            <div class="mb-3 text-fg-2">
              ⚠ 视口自查：
              <ul class="list-inside list-disc">
                <For each={snap().viewportProblems}>{(p) => <li>{p}</li>}</For>
              </ul>
            </div>
          </Show>

          <h2 class="mb-1 text-fs-1 tracking-wide text-fg-2 uppercase">
            只有人能填的几项
          </h2>
          <div class="mb-2 flex flex-col gap-1">
            <For
              each={[
                { key: "transparencyOk" as const, label: "透明区是透明的（看到桌面/下层窗口，无黑底白边）" },
                { key: "oneToOneSharp" as const, label: "1:1 档位下白块像素级锐利（一个方块）" },
                { key: "acrossMonitorsOk" as const, label: "跨显示器拖动无错位/闪烁" },
              ]}
            >
              {(item) => (
                <label class="flex items-center gap-2">
                  <input
                    type="checkbox"
                    checked={notes()[item.key] === true}
                    onChange={(event) =>
                      setNotes({
                        ...notes(),
                        [item.key]: event.currentTarget.checked ? true : null,
                      })
                    }
                  />
                  <span>{item.label}</span>
                </label>
              )}
            </For>
            <label class="flex items-center gap-2">
              <span class="text-fg-3">主观帧率</span>
              <input
                class="min-w-0 flex-1 rounded-(--radius) bg-surface-track px-1"
                placeholder="快 / 一般 / 卡"
                value={notes().feel}
                onInput={(event) => setNotes({ ...notes(), feel: event.currentTarget.value })}
              />
            </label>
            <label class="flex flex-col gap-1">
              <span class="text-fg-3">备注</span>
              <textarea
                class="h-16 rounded-(--radius) bg-surface-track px-1"
                value={notes().notes}
                onInput={(event) => setNotes({ ...notes(), notes: event.currentTarget.value })}
              />
            </label>
          </div>

          <h2 class="mb-1 text-fs-1 tracking-wide text-fg-2 uppercase">落盘报告</h2>
          <div class="mb-1 flex gap-1">
            <input
              class="min-w-0 flex-1 rounded-(--radius) bg-surface-track px-1"
              value={reportDir()}
              onInput={(event) => setReportDir(event.currentTarget.value)}
            />
            <button
              type="button"
              class="rounded-(--radius) bg-brand px-2 py-0.5 text-fg-on-brand"
              disabled={busy()}
              onClick={() => {
                void spikeWriteReport(reportDir(), notes())
                  .then((paths) =>
                    setMessage(paths.length > 0 ? `已写入：${paths.join(" · ")}` : "浏览器里写不了"),
                  )
                  .catch((error) => setMessage(`写报告失败：${String(error)}`));
              }}
            >
              写报告
            </button>
          </div>
          <p class="text-fg-3">
            写好把 `spike-report.md` 发我；JSON 是给程序看的同一份数据。
          </p>
        </aside>
      </div>
    </div>
  );
}
