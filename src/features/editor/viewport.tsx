/**
 * 编辑中列：**视口**（`design/editor.md` §3.2、M3-W1 的洞口 + M3-W2 的交互）。
 *
 * ```text
 * ┌──────────────────────────────┐
 * │            ↑                 │  ← 滚轮 = 以光标为锚缩放（意图发去 Rust）
 * │      照片由 wgpu 直绘          │  ← 拖动 = 平移；双击 = 适合窗口 ↔ 1:1
 * │            ↓                 │  ← 松手没动 = 命中测试（W5 的三个工具要用）
 * └──────────────────────────────┘
 * ```
 *
 * 四条纪律：
 *
 * 1. **只上报原始事实**（矩形 / DPR / CSS 视口 / 底色字符串），换算全在 Rust
 *    —— `AGENTS.md` §6.1 红线 2；
 * 2. 上报要**三处都触发 + 尾样本**：`ResizeObserver`、窗口 `resize`、DPR 变化各一路；
 * 3. **只发意图**：滚轮给「CSS 窗口坐标 + 倍数」，拖动给「CSS 位移」——
 *    一次乘法都不在前端做；
 * 4. **照片由 GPU 画在 DOM 底下**，所以这块 DOM 在出图时必须**没有底色**（`holeActive`）——
 *    底色由 wgpu 画（theme 值由上面第 1 条那条事实带过去）。
 *
 * ⚠️ 水印/提示是**印在洞口里的内容**，照片出来后它们就消失（不是浮在照片上的一层）。
 */

import { createEffect, onCleanup, onMount, Show, type JSX } from "solid-js";
import {
  IconAlertTriangle,
  IconAlbumOff,
  IconFolder,
  IconLoader2,
  IconPhoto,
  IconPhotoOff,
} from "@tabler/icons-solidjs";

import { StateWatermark } from "../../components/ui/StateWatermark.tsx";
import { createWheelZoom } from "../../components/ui/viewer/interaction.ts";
import { t } from "../../i18n/index.ts";
import { ViewportOverlay } from "./ViewportOverlay.tsx";
import type { MessageKey } from "../../i18n/index.ts";
import { createDragSession, createPanAccumulator } from "../../lib/editor-intent.ts";
import { createViewportReporter, type ViewportReporter } from "../../lib/editor-viewport.ts";
import { sendEditorViewportIntent, setEditorViewport } from "../../api/editor.ts";
import { isTauriRuntime } from "../../api/tauri-env.ts";
import type { EditorRenderState } from "../../api/types.ts";
import {
  editorEmptyIcon,
  editorEmptyOffersImport,
  editorViewportNotice,
  type EditorEmptyKind,
} from "./source.ts";

export interface EditorViewportProps {
  /** 空态（四态之一）；`null` = 有照片 */
  empty: EditorEmptyKind;
  /** 有没有选中照片（提示与水印的分工靠它） */
  hasPhoto: boolean;
  /** 渲染线程的状态快照（`null` = 拿不到：浏览器 / 还没 bind） */
  renderState: () => EditorRenderState | null;
  /** 「去导入」按钮（只有「没有库」那一态给） */
  onOpenImport?: () => void;
  /** 重新起渲染线程（出图失败时那颗「重试」） */
  onRetry?: () => void;
  /** 上报开关（开发页里可以关掉；默认开） */
  report?: boolean;
  class?: string;
}

/** 空态 → 图标（四态各一枚；与其它工作区用的图标同一套）。 */
const EMPTY_ICON = {
  library: IconAlbumOff,
  folder: IconFolder,
  photo: IconPhotoOff,
  select: IconPhoto,
} as const;

/** 空态 → 文案 key。 */
const EMPTY_TEXT = {
  "no-repository": "editor.empty.noRepository",
  "no-directory": "editor.empty.noDirectory",
  "no-photos": "editor.empty.noPhotos",
  "no-selection": "editor.empty.noSelection",
} as const satisfies Record<Exclude<EditorEmptyKind, null>, MessageKey>;

/** 键位/按钮的放大倍率（与看图件 `ViewerControls` 的 1.25 同一档）。 */
export const EDITOR_ZOOM_STEP = 1.25;

export function EditorViewport(props: EditorViewportProps): JSX.Element {
  let hole: HTMLDivElement | undefined;
  /**
   * 洞口底色的**稳定探针**（永远带着 `bg-surface-bar`）。
   *
   * 为什么不直接读洞口自己：出图时洞口会变成 `bg-transparent`，
   * 那时读到的是 `rgba(0, 0, 0, 0)` —— Rust 只能回退成**深色兜底**，
   * 浅色主题下洞口与缝就变成深灰（2026-09-23 修）。
   */
  let backdropProbe: HTMLSpanElement | undefined;
  /**
   * 洞口事实上报器（`onMount` 里建）。
   *
   * 挂在组件作用域上是因为**主题变化那条补报也要用它** —— 上报器自带去重，
   * 直接 `setEditorViewport` 会把「每 250ms 轮询一次状态」变成 4 次/秒的 IPC，
   * 而 Rust 那边每一条 `Viewport` 命令都会 `dirty = true` 画一帧
   * （设计目标是「空闲时一帧都不画」）。
   */
  let reporter: ViewportReporter | null = null;

  /** 一次意图：发出去，失败只记控制台（不该让交互把界面带崩）。 */
  const send = (intent: Parameters<typeof sendEditorViewportIntent>[0]): void => {
    void sendEditorViewportIntent(intent).catch((error: unknown) => {
      console.error("[editor] 视口意图发送失败", error); // i18n-exempt: 控制台诊断
    });
  };

  /**
   * 上报 + 交互：**只在真运行时挂**（浏览器预览里没有渲染线程与 `editor_*` 命令）。
   */
  onMount(() => {
    if (!isTauriRuntime()) return;
    if (hole === undefined) return;
    const element = hole;

    /* ── ① 洞口事实上报（W1）───────────────────────── */
    reporter = createViewportReporter({
      send: (payload) => {
        // 报错不吞：弹到控制台（终端诊断，不走语言包）——
        // 「一直没报上去」与「报错了」必须分得开
        void setEditorViewport(payload).catch((error: unknown) => {
          console.error("[editor] 视口上报失败", error); // i18n-exempt: 控制台诊断
        });
      },
    });

    const observe = (): void => {
      const rect = element.getBoundingClientRect();
      reporter?.observe({
        rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
        // ⚠️ **运行时** DPR：含显示器 DPI + 系统文字缩放 + 页面缩放（§7.9 铁律 3）
        dpr: window.devicePixelRatio,
        viewport: { width: window.innerWidth, height: window.innerHeight },
        // 洞口底色：**字符串原样上行**（解析在 Rust；主题一变它跟着变）
        backdrop: readComputedBackdrop(backdropProbe),
      });
    };

    /// DPR 变化时重新订阅（媒体查询串里带着旧 DPR，不重订就收不到第二次变化）
    let dprQuery: MediaQueryList | null = null;
    const onDprChange = (): void => {
      observe();
      dprQuery = subscribeDpr(onDprChange, dprQuery);
    };

    observe(); // 首帧就报一次（不等第一次尺寸变化）
    dprQuery = subscribeDpr(onDprChange, null);

    const observer = new ResizeObserver(observe);
    observer.observe(element);
    window.addEventListener("resize", observe);

    /* ── ② 滚轮：以光标为锚（复用看图件那一份实现）────── */
    const wheel = createWheelZoom({
      // 锚点是**原始事实**：视口内的 CSS 坐标（不做任何换算）
      resolveAnchor: (event) => ({ x: event.clientX, y: event.clientY }),
      apply: (factor, anchor) => {
        if (anchor === null) {
          // 拿不到锚点（理论上不会发生）→ 让 Rust 用洞口中心，别丢这一帧
          send({ kind: "zoomBy", factor });
          return;
        }
        send({ kind: "zoomAt", x: anchor.x, y: anchor.y, factor });
      },
    });

    /* ── ③ 拖动平移 + 点击命中 ────────────────────── */
    const drag = createDragSession();
    const pan = createPanAccumulator({
      send: (intent) => send(intent),
    });
    let pointerId: number | null = null;

    const onPointerDown = (event: PointerEvent): void => {
      if (event.button !== 0) return; // 只认左键（右键留给以后的上下文菜单）
      pointerId = event.pointerId;
      drag.start(event.clientX, event.clientY);
      try {
        element.setPointerCapture(event.pointerId);
      } catch {
        // 合成事件（冒烟脚本）拿不到指针捕获：照样能拖，别把交互搞崩
      }
    };

    const onPointerMove = (event: PointerEvent): void => {
      if (!drag.active()) return;
      const delta = drag.move(event.clientX, event.clientY);
      if (delta === null) return;
      pan.move(delta.dx, delta.dy);
    };

    const finishPointer = (event: PointerEvent): void => {
      if (pointerId !== null && pointerId !== event.pointerId) return;
      if (!drag.active()) return;
      pan.flush(); // 尾样本：松手那一段也要发出去
      const ended = drag.end();
      pointerId = null;
      try {
        element.releasePointerCapture(event.pointerId);
      } catch {
        // 没捕获过就没什么可释放的
      }
      if (ended.click) {
        // 没怎么动 = 点击：报一次命中测试（W5 的三个工具要用，现在只回填状态）
        send({ kind: "hitTest", x: event.clientX, y: event.clientY });
      }
    };

    const onDblClick = (): void => {
      send({ kind: "toggleFit" });
    };

    element.addEventListener("wheel", wheel.onWheel, { passive: false });
    element.addEventListener("pointerdown", onPointerDown);
    element.addEventListener("pointermove", onPointerMove);
    element.addEventListener("pointerup", finishPointer);
    element.addEventListener("pointercancel", finishPointer);
    element.addEventListener("dblclick", onDblClick);

    onCleanup(() => {
      // 卸载前把挂起的那一帧发出去（尾样本不能丢），再断开
      reporter?.flush();
      reporter?.dispose();
      reporter = null;
      wheel.dispose();
      pan.dispose();
      observer.disconnect();
      window.removeEventListener("resize", observe);
      dprQuery?.removeEventListener("change", onDprChange);
      element.removeEventListener("wheel", wheel.onWheel);
      element.removeEventListener("pointerdown", onPointerDown);
      element.removeEventListener("pointermove", onPointerMove);
      element.removeEventListener("pointerup", finishPointer);
      element.removeEventListener("pointercancel", finishPointer);
      element.removeEventListener("dblclick", onDblClick);
    });
  });

  /** 出图时洞口不该有底色（底下是 GPU 画的东西）；空态/失败时 DOM 自己画。 */
  const transparent = (): boolean => props.renderState()?.paintedPath != null || false;

  /*
   * 主题切换时底色会变 —— `ResizeObserver` 不会因此回调，DPR 也不会变，
   * 所以这里主动补一次上报（`data-theme` 变了就重报）。用 `createEffect` 读一个 DOM 属性
   * 不是响应式的，所以退一步：把重报挂在**每次渲染后**的微任务里，靠上报器的去重兜住开销。
   */
  createEffect(() => {
    // 读一下状态快照：它一变（换照片 / 换主题后 Rust 回读）就顺手重报一次洞口事实。
    // 这条不追求实时，兜的是「切主题之后洞口底还是旧色」这一种。
    void props.renderState();
    if (!isTauriRuntime() || hole === undefined) return;
    const element = hole;
    queueMicrotask(() => {
      if (!element.isConnected) return;
      const rect = element.getBoundingClientRect();
      // 走上报器（不是裸 `setEditorViewport`）：它自带去重 ——
      // 轮询带来的重复上报不会变成每秒四次「画一帧」
      reporter?.observe({
        rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
        dpr: window.devicePixelRatio,
        viewport: { width: window.innerWidth, height: window.innerHeight },
        backdrop: readComputedBackdrop(backdropProbe),
      });
    });
  });

  return (
    <div
      ref={hole}
      data-editor-viewport
      data-viewport-empty={props.empty ?? "photo"}
      data-viewport-painted={transparent() ? "on" : "off"}
      class={[
        "relative flex min-h-0 min-w-0 flex-1 items-center justify-center overflow-hidden",
        // 出图时透明（wgpu 画在下面），否则自己画底色
        transparent() ? "bg-transparent" : "bg-surface-bar",
        // 拖动时的手型与光标反馈（与看图件同一套观感）
        "cursor-grab active:cursor-grabbing",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      {/*
        洞口底色的**稳定探针**：0×0、不参与布局，只为读 `bg-surface-bar` 的计算值。
        出图时洞口自己是透明的，只有这个探针还能报出主题底色（见上面的说明）。
      */}
      <span
        ref={(element: HTMLSpanElement) => {
          backdropProbe = element;
        }}
        aria-hidden="true"
        class="pointer-events-none absolute h-0 w-0 bg-surface-bar"
      />
      {/*
        **覆盖层宿主**（M3-W3 定契约）：W5 的裁切 / 旋转 / 对比与将来的蒙版都插进这里。
        子元素一律用**图像像素**坐标书写，由它统一套上 Rust 给的仿射矩阵 ——
        视口数学只有一份，覆盖层不许自己乘 zoom / 减 pan / 补 DPR。
        W3 这一波还没有覆盖层内容（三个工具在 W5），所以这里是空的。
      */}
      <ViewportOverlay renderState={props.renderState()} />

      <Show when={props.empty} fallback={<ViewportMessage props={props} />}>
        {(kind) => {
          const Icon = EMPTY_ICON[editorEmptyIcon(kind())];
          return (
            <StateWatermark
              icon={<Icon size={64} stroke-width={1} />}
              text={t(EMPTY_TEXT[kind()])}
              action={
                editorEmptyOffersImport(kind()) && props.onOpenImport !== undefined
                  ? { label: t("editor.empty.goImport"), run: () => props.onOpenImport?.() }
                  : undefined
              }
            />
          );
        }}
      </Show>
    </div>
  );
}

/**
 * 洞口里那条「非空态」的说明（载入中 / 出错 / 浏览器里没有 GPU 视口）。
 *
 * 照片画出来之后它**不显示**（`editorViewportNotice` 已经算过优先级）——
 * 所以它不是浮在照片上的一层，而是「还没有照片可看」时的那块内容。
 */
function ViewportMessage(props: { props: EditorViewportProps }): JSX.Element {
  const kind = (): ReturnType<typeof editorViewportNotice> =>
    editorViewportNotice({
      empty: props.props.empty,
      hasPhoto: props.props.hasPhoto,
      state: props.props.renderState(),
    });

  const detail = (): string | null => {
    const state = props.props.renderState();
    if (kind() === "decode-error") return state?.decodeError ?? null;
    if (kind() === "render-error") return state?.lastError ?? null;
    return null;
  };

  return (
    <Show when={kind()}>
      {(which) => (
        <div class="flex max-w-96 flex-col items-center gap-2 text-center" data-viewport-notice={which()}>
          <Show when={detail()}>
            {(text) => (
              <p class="flex items-start gap-1.5 text-left text-fs-0 leading-snug text-fg-3">
                <IconAlertTriangle size={12} class="mt-px shrink-0" aria-hidden="true" />
                <span>
                  {t(
                    which() === "decode-error"
                      ? "editor.viewport.decodeError"
                      : "editor.viewport.renderError",
                  ).replace("{reason}", text())}
                </span>
              </p>
            )}
          </Show>

          <Show when={detail() === null}>
            {/*
              载入提示（`IMAGING.md` §5，人类 2026-09-24）：**半透毛玻璃**，
              与全屏模式那条观感一致（`FullscreenViewer` 的载入遮罩）。

              两条纪律：
              * **只遮 view** —— 这个容器就是洞口，胶片带 / 面板 / 状态栏照常可点；
              * `pointer-events-none` —— 连 view 里的拖动与缩放也不挡
                （「不要阻挡其他操作」，与全屏那条「铺满整屏且拦输入」相反）。
            */}
            <div
              class="pointer-events-none flex items-center gap-2 rounded-ui bg-surface-layer/70 px-3 py-1.5 backdrop-blur-md"
              data-viewport-loading="on"
            >
              <IconLoader2 size={14} class="animate-spin text-fg-2" aria-hidden="true" />
              <span class="text-fs-1 text-fg-1">
                {t(
                  which() === "browser"
                    ? "editor.viewport.browser"
                    : which() === "loading"
                      ? "editor.viewport.loading"
                      : "editor.viewport.init",
                )}
              </span>
            </div>
          </Show>

          <Show when={which() === "render-error" && props.props.onRetry !== undefined}>
            <button
              type="button"
              class="rounded-(--radius) bg-state-hover px-2 py-0.5 text-fs-1 text-fg-1"
              onClick={() => props.props.onRetry?.()}
            >
              {t("editor.viewport.retry")}
            </button>
          </Show>
        </div>
      )}
    </Show>
  );
}

/**
 * 读元素上算出来的底色（`getComputedStyle` 的字符串）。
 *
 * **不做解析**：`rgb(...)` / `rgba(...)` 的形态与「它怎么变成 GPU 清屏色」是同一件事，
 * 那件事在 Rust（`Srgb8::parse_css`）。这里只负责「把浏览器算出来的事实原样交上去」——
 * 读不到就返回 `null`，Rust 会用兜底色并在状态里标出来。
 *
 * ⚠️ 传进来的必须是**稳定探针**（`backdropProbe`），不是洞口自己：
 * 洞口出图时是透明的，拿它读会读到 `rgba(0, 0, 0, 0)`。
 */
function readComputedBackdrop(element: HTMLElement | undefined): string | null {
  if (element === undefined) return null;
  if (typeof window === "undefined" || typeof window.getComputedStyle !== "function") {
    return null;
  }
  const value = window.getComputedStyle(element).backgroundColor;
  return typeof value === "string" && value.trim() !== "" ? value : null;
}

/**
 * 订阅 DPR 变化。
 *
 * 媒体查询串里带着当前 DPR，所以每次变化后都要**重新订阅**（否则第二次变化收不到）——
 * 返回新的查询对象交给调用方保存。
 */
function subscribeDpr(
  onChange: () => void,
  previous: MediaQueryList | null,
): MediaQueryList | null {
  previous?.removeEventListener("change", onChange);
  if (typeof window.matchMedia !== "function") return null;
  const dpr = window.devicePixelRatio;
  if (!Number.isFinite(dpr) || dpr <= 0) return null;
  const query = window.matchMedia(`(resolution: ${dpr}dppx)`);
  query.addEventListener("change", onChange);
  return query;
}
