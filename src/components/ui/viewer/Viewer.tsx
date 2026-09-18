/**
 * 看图的视图（`design/main.md` §3.2）—— **纯前端**，变换只走 CSS transform。
 *
 * 为什么用 `<img>` + `transform` 而不是 canvas：这一版的目标是「基础可用」，
 * 而 DOM 版在**不变换布局**的前提下就是合成器在干活（拖动/缩放不掉帧）。
 * 真正的 wgpu 查看器属于 M2-W3；这里把**状态**（`store.ts`）与视图分开，
 * 换渲染层时状态与交互都不用重写。
 *
 * 交互（设计稿里写明的 + 一点必要的补充）：
 *
 * | 操作 | 行为 |
 * | --- | --- |
 * | `Esc` | 返回网格 |
 * | 左上角返回按钮 | 同 `Esc`（设计稿：浮在左上角） |
 * | 双击 | **适配窗口 ↔ 100%** 之间切换 |
 * | 滚轮 | 以**鼠标位置为锚**平滑缩放（按帧合并，避免滚轮风暴） |
 * | 拖动 | 平移（图比窗口小时锁在中间） |
 * | `←` / `→` | 上一张 / 下一张（胶片带是后续里程碑，先给键盘） |
 * | `+` / `-` / `0` / `1` | 放大 / 缩小 / 适配 / 100% |
 */

import { IconArrowLeft, IconMinus, IconPlus } from "@tabler/icons-solidjs";
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { t } from "../../../i18n/index.ts";
import type { ViewerStore } from "./store.ts";

export interface ViewerProps {
  store: ViewerStore;
  /** 关闭时通知外面（把焦点还给网格之类） */
  onClose?: () => void;
  class?: string;
}

export function Viewer(props: ViewerProps) {
  let host: HTMLDivElement | undefined;
  const [dragging, setDragging] = createSignal(false);

  const state = () => props.store.state();

  /*
   * 两个悬浮控件都**只在鼠标靠近各自的角时才出现**（人类 2026-09-16）：
   *   左上角 → 返回按钮；右下角 → 缩放指示。
   * 判据是「离角落多少像素以内」，不是「有没有悬停在按钮上」—— 按钮本身很小时，
   * 要求精确悬停等于找不到。键盘用户靠 `:focus-within` 兜住（Tab 能到）。
   */
  const [cursor, setCursor] = createSignal<{ x: number; y: number } | null>(null);
  const CORNER_BACK = 120;
  const CORNER_ZOOM = 200;
  const nearTopLeft = (): boolean => {
    const at = cursor();
    return at !== null && at.x <= CORNER_BACK && at.y <= CORNER_BACK;
  };
  const nearBottomRight = (): boolean => {
    const at = cursor();
    if (at === null) return false;
    const width = host?.clientWidth ?? 0;
    const height = host?.clientHeight ?? 0;
    return width - at.x <= CORNER_ZOOM && height - at.y <= CORNER_ZOOM;
  };

  /*
   * 显示哪个数字，这里有个坑（人类 2026-09-16 报的「打开瞬间比例很大、有时卡住」）：
   * 大图没到之前，画面用的是网格小图（长边 384），以它为基准算出来的比例会是个很大的数
   * （1200px 视口 ≈ 312%），大图一到又变成正常值 —— 看着就是「先闪一个错数」。
   * 所以：**适配状态只说「适配」**（不报数字），大图到了再补上百分比；
   * 用户自己缩放之后报的就是「相对当前显示这张图」的比例，始终是真的。
   */
  const zoomLabel = (): string => {
    const zoom = state().zoom;
    if (!state().fit) return `${Math.round(zoom * 100)}%`;
    const fitText = t("viewer.fit_label");
    return props.store.sharp() ? `${fitText} · ${Math.round(zoom * 100)}%` : fitText;
  };

  function trackCursor(event: PointerEvent): void {
    const rect = host?.getBoundingClientRect();
    if (rect === undefined) return;
    setCursor({ x: event.clientX - rect.left, y: event.clientY - rect.top });
  }

  /** 视口尺寸：`ResizeObserver` 通知（store 里已去重，尺寸没变不会重渲染） */
  onMount(() => {
    if (host === undefined) return;
    const measure = (): void => {
      if (host === undefined) return;
      props.store.setViewport({
        width: host.clientWidth,
        height: host.clientHeight,
      });
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(host);
    onCleanup(() => observer.disconnect());
  });

  const close = (): void => {
    props.store.close();
    props.onClose?.();
  };

  /** 键盘：Esc 返回、方向键翻页、+/-/0/1 缩放 */
  onMount(() => {
    const onKey = (event: KeyboardEvent): void => {
      if (!state().active) return;
      /*
       * 已经被别人用掉的事件不处理（`preventDefault` 是个通用信号）：
       * 实测事故 —— 网格里按回车打开看图时，**同一个仍在冒泡的事件**会接着到达这里，
       * 被当成「回车＝退出」把刚开的看图又关掉（2026-09-18 冒烟：active 同一个 tick 变回 false）。
       * 网格那边现在也 `stopPropagation` 了，但这里再加一道 —— 以后别的入口（胶片带、命令面板）
       * 用回车打开看图时不会重踩。
       */
      if (event.defaultPrevented) return;
      switch (event.key) {
        case "Escape":
          event.preventDefault();
          close();
          break;
        case "Enter":
          // 设计稿 §3.2：选中后回车进来，**再按一次回车出去**（进胶片带/对比模式后再另说）
          event.preventDefault();
          close();
          break;
        case "ArrowRight":
          event.preventDefault();
          props.store.next();
          break;
        case "ArrowLeft":
          event.preventDefault();
          props.store.prev();
          break;
        case "+":
        case "=":
          event.preventDefault();
          props.store.zoomBy(1.25);
          break;
        case "-":
          event.preventDefault();
          props.store.zoomBy(1 / 1.25);
          break;
        case "0":
          event.preventDefault();
          if (!state().fit) props.store.toggleFit();
          break;
        case "1":
          event.preventDefault();
          if (state().fit) props.store.toggleFit();
          break;
        default:
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => window.removeEventListener("keydown", onKey));
  });

  /*
   * 滚轮：**按帧合并**。滚轮事件在高精度触控板上可以到 100+ Hz，
   * 每次都改状态 = 每帧重算多次变换；合并成「一帧一次」手感一样但开销固定。
   */
  let wheelDelta = 0;
  let wheelAnchor: { x: number; y: number } | null = null;
  let wheelFrame = 0;

  function onWheel(event: WheelEvent): void {
    event.preventDefault();
    const rect = host?.getBoundingClientRect();
    if (rect !== undefined) {
      wheelAnchor = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    }
    wheelDelta += event.deltaY;
    if (wheelFrame !== 0) return;
    wheelFrame = requestAnimationFrame(() => {
      wheelFrame = 0;
      const delta = wheelDelta;
      const anchor = wheelAnchor;
      wheelDelta = 0;
      if (delta === 0) return;
      // 指数映射：无论快慢滚，视觉上的缩放速度都一致
      props.store.zoomBy(
        Math.exp(-delta * 0.0015),
        anchor === null ? undefined : anchor,
      );
    });
  }
  onCleanup(() => {
    if (wheelFrame !== 0) cancelAnimationFrame(wheelFrame);
  });

  /* 拖动平移：用指针捕获，拖出窗口也不丢事件 */
  let dragFrom: { x: number; y: number } | null = null;

  function onPointerDown(event: PointerEvent): void {
    if (event.button !== 0) return;
    /*
     * 点在按钮之类的控件上时**不要**开始拖动：`setPointerCapture` 会把后续的 click
     * 从按钮手里抢走 —— 人类实测「左上角返回按钮点了没反应」就是这个原因
     * （放大/缩小按钮同理）。判据用 `closest`，这样按钮里的图标也算控件。
     */
    const target = event.target;
    if (target instanceof Element && target.closest("button, a, input, [role='button']") !== null) {
      return;
    }
    dragFrom = { x: event.clientX, y: event.clientY };
    setDragging(true);
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function onPointerMove(event: PointerEvent): void {
    if (dragFrom === null) return;
    const from = dragFrom;
    dragFrom = { x: event.clientX, y: event.clientY };
    props.store.panBy(event.clientX - from.x, event.clientY - from.y);
  }

  function endDrag(): void {
    dragFrom = null;
    setDragging(false);
  }

  return (
    <div
      ref={(element: HTMLDivElement) => {
        host = element;
      }}
      class={[
        // 覆盖层原生：网格保持挂载（滚动位置才留得住），看图盖在它上面。
        // 这里**不能**写 `relative` —— 它与 `absolute` 同属 Tailwind 的一个工具组，
        // 编译产物里 `relative` 在后，`relative` 会赢，于是 Viewer 退化成列里的普通
        // flex 子项、只占住下半屏（2026-09-17 实测 bug）。定位交给调用方传 class。
        "absolute inset-0 overflow-hidden bg-surface-bar",
        dragging() ? "cursor-grabbing" : "cursor-grab",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
      data-viewer="open"
      onWheel={onWheel}
      onPointerDown={onPointerDown}
      onPointerMove={(event) => {
        onPointerMove(event);
        trackCursor(event);
      }}
      onPointerLeave={() => setCursor(null)}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onDblClick={() => props.store.toggleFit()}
    >
      <Show when={props.store.imageUrl()}>
        {(url) => (
          <img
            src={url()}
            alt={props.store.current()?.fileName ?? ""}
            draggable={false}
            onLoad={(event) =>
              props.store.setNatural({
                width: event.currentTarget.naturalWidth,
                height: event.currentTarget.naturalHeight,
              })
            }
            class="pointer-events-none absolute top-1/2 left-1/2 max-w-none select-none"
            style={{
              // 顺序要紧：先居中，再平移，最后按**元素中心**缩放
              //（store 里的锚点数学就是按「未变换时居中」推的）
              transform: `translate(-50%, -50%) translate3d(${state().pan.x}px, ${state().pan.y}px, 0) scale(${state().zoom})`,
              "will-change": "transform",
            }}
          />
        )}
      </Show>

      {/* 左上角返回（设计稿：浮在左上角；悬停才浮出是后续细节） */}
      <button
        type="button"
        class={[
          "absolute start-3 top-3 flex cursor-pointer items-center gap-1 rounded-ui bg-surface-layer px-2 py-1 text-fs-1 text-fg-1",
          "transition-opacity",
          "opacity-0 focus-visible:opacity-100 focus-within:opacity-100",
          nearTopLeft() ? "opacity-100" : "",
        ].join(" ")}
        onClick={close}
        aria-label={t("viewer.back")}
      >
        <IconArrowLeft size={14} aria-hidden="true" />
        {t("viewer.back")}
      </button>

      {/* 右下角：缩放控件 + 百分比（基础版：给鼠标用户一个不靠滚轮的入口） */}
      <div
        class={[
          "absolute end-3 bottom-3 flex items-center gap-1 rounded-ui bg-surface-layer px-1.5 py-1",
          "transition-opacity",
          "opacity-0 focus-within:opacity-100",
          nearBottomRight() ? "opacity-100" : "",
        ].join(" ")}
      >
        <button
          type="button"
          class="flex size-6 cursor-pointer items-center justify-center rounded-ui text-fg-2 hover:bg-state-hover hover:text-fg-1"
          aria-label={t("viewer.zoom_out")}
          onClick={() => props.store.zoomBy(1 / 1.25)}
        >
          <IconMinus size={14} aria-hidden="true" />
        </button>
        <button
          type="button"
          class="min-w-16 cursor-pointer rounded-ui px-1 text-center text-fs-1 text-fg-2 hover:bg-state-hover hover:text-fg-1 tnum"
          aria-label={t("viewer.fit")}
          onClick={() => props.store.toggleFit()}
        >
          {zoomLabel()}
        </button>
        <button
          type="button"
          class="flex size-6 cursor-pointer items-center justify-center rounded-ui text-fg-2 hover:bg-state-hover hover:text-fg-1"
          aria-label={t("viewer.zoom_in")}
          onClick={() => props.store.zoomBy(1.25)}
        >
          <IconPlus size={14} aria-hidden="true" />
        </button>
        <span class="max-w-64 truncate ps-1 text-fs-1 text-fg-3">
          {props.store.current()?.fileName ?? ""}
        </span>
      </div>
    </div>
  );
}
