/**
 * **全屏看图**（沉浸式，`?fullscreen=1`）—— 一扇独立窗口里的一张照片，**没有任何控件**。
 *
 * 人类 2026-09-23 的口径：
 *
 * | 操作 | 行为 |
 * | --- | --- |
 * | 打开 | 默认**适应窗口**；图是当前目录显示序里的那张（清单与网格同一份来源） |
 * | 双击 | 适应窗口 ↔ 100% |
 * | 滚轮 | 以鼠标位置为锚缩放 |
 * | 拖动 | 平移 |
 * | `Esc` / `Enter` | 退出（关掉这扇窗） |
 * | `←` / `→`、`PageUp` / `PageDown` | 上一张 / 下一张（**到头就停**，与主窗口看图一致） |
 *
 * 三处**刻意复用**（`AGENTS.md` §2.12：同一个能力只允许有一套实现）：
 *
 * 1. 状态与数学 → `components/ui/viewer/store.ts`（同一份 `createViewerStore`）；
 * 2. 滚轮手感 → `components/ui/viewer/interaction.ts::createWheelZoom`；
 * 3. 步进 → store 的 `next()` / `prev()`（它们本来就「到头停住 + 重置成适配」，不再写第二套）。
 *
 * 与主窗口看图的**唯一**差别：这里没有 chrome、没有胶片带、没有对比，
 * 且图片档位固定取 `screen`（与主窗口看图的 `loadScreen` 同一个 purpose —— 同一个东西
 * 不该在两条路上取不同的图）。所以 100% = **当前这张渲染图**的 1:1，
 * 大图的屏幕档（长边 1920）在 4K 上放大时不会再取更清的档 —— 那是「另一个 purpose」的事，
 * 不在本次范围（真要改就加 `original` 档并同步 Rust 的 `ImagePurpose`）。
 */

import { createSignal, onCleanup, onMount, Show } from "solid-js";

import { getViewImage } from "../../api/db.ts";
import {
  closeFullscreen,
  getFullscreenPayload,
  onFullscreenPayload,
} from "../../api/fullscreen.ts";
import type { FullscreenPayload } from "../../api/types.ts";
import { createViewerStore } from "../../components/ui/viewer/store.ts";
import { createWheelZoom } from "../../components/ui/viewer/interaction.ts";
import { t } from "../../i18n/index.ts";

export function FullscreenViewer() {
  let host: HTMLDivElement | undefined;
  const [dragging, setDragging] = createSignal(false);
  /** 清单还没到 / 清单为空：给一句话，别让人对着黑屏猜 */
  const [empty, setEmpty] = createSignal(false);

  const store = createViewerStore({
    loadScreen: (path) => getViewImage(path, "screen"),
  });

  const state = () => store.state();

  const applyPayload = (payload: FullscreenPayload | null): void => {
    if (payload === null || payload.items.length === 0) {
      setEmpty(true);
      return;
    }
    setEmpty(false);
    store.show(
      payload.items.map((item) => ({
        id: item.id,
        path: item.path,
        fileName: item.fileName,
      })),
      payload.index,
    );
  };

  onMount(() => {
    /*
     * 视口尺寸：`ResizeObserver` 通知（全屏窗口也会因为切屏 / DPI 变化而改尺寸）。
     * 必须在**取图之前**量到 —— 适配倍率是按视口算的，量晚了会先按 0 算一遍。
     */
    const measure = (): void => {
      if (host === undefined) return;
      store.setViewport({ width: host.clientWidth, height: host.clientHeight });
    };
    measure();
    const observer = new ResizeObserver(measure);
    if (host !== undefined) observer.observe(host);
    onCleanup(() => observer.disconnect());

    // 整扇窗就这一页：把焦点收上来，键盘才一定落到这里（窗口是 Rust 侧建好并聚焦的）
    host?.focus();

    // 挂载取一次清单（页面刷新也走这条）+ 订阅「窗口已开着时换图」
    void getFullscreenPayload()
      .then(applyPayload)
      .catch(() => {
        setEmpty(true);
      });
    let dispose: (() => void) | undefined;
    void onFullscreenPayload(applyPayload).then((off) => {
      dispose = off;
    });
    onCleanup(() => dispose?.());

    const onKey = (event: KeyboardEvent): void => {
      switch (event.key) {
        case "Escape":
        case "Enter":
          event.preventDefault();
          void closeFullscreen();
          return;
        case "ArrowLeft":
        case "PageUp":
          event.preventDefault();
          store.prev();
          return;
        case "ArrowRight":
        case "PageDown":
          event.preventDefault();
          store.next();
          return;
        default:
      }
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => window.removeEventListener("keydown", onKey));
  });

  /* 滚轮：与主窗口看图**同一份实现**（按帧合并，锚点是鼠标位置） */
  const wheel = createWheelZoom({
    resolveAnchor: (event) => {
      const rect = host?.getBoundingClientRect();
      return rect === undefined
        ? null
        : { x: event.clientX - rect.left, y: event.clientY - rect.top };
    },
    apply: (factor, anchor) => store.zoomBy(factor, anchor ?? undefined),
  });
  onCleanup(wheel.dispose);

  /* 拖动平移：指针捕获，拖出屏幕也不丢事件 */
  let dragFrom: { x: number; y: number } | null = null;

  function onPointerDown(event: PointerEvent): void {
    if (event.button !== 0) return;
    dragFrom = { x: event.clientX, y: event.clientY };
    setDragging(true);
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function onPointerMove(event: PointerEvent): void {
    if (dragFrom === null) return;
    const from = dragFrom;
    dragFrom = { x: event.clientX, y: event.clientY };
    store.panBy(event.clientX - from.x, event.clientY - from.y);
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
        // 沉浸式：整屏、没有 chrome、没有背景装饰 —— 最深的那级中性面当画布底
        "fixed inset-0 overflow-hidden bg-surface-track outline-none select-none",
        dragging() ? "cursor-grabbing" : "cursor-grab",
      ].join(" ")}
      tabindex="-1"
      data-fullscreen="open"
      onWheel={wheel.onWheel}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onDblClick={() => store.toggleFit()}
    >
      <Show when={!empty()} fallback={<Hint text={t("fullscreen.empty")} />}>
        <Show when={store.imageUrl()}>
          {(url) => (
            <img
              src={url()}
              alt={store.current()?.fileName ?? ""}
              draggable={false}
              onLoad={(event) =>
                store.setNatural({
                  width: event.currentTarget.naturalWidth,
                  height: event.currentTarget.naturalHeight,
                })
              }
              class="pointer-events-none absolute top-1/2 left-1/2 max-w-none select-none"
              style={{
                // 与主窗口看图同一套顺序：先居中，再平移，最后按元素中心缩放
                transform: `translate(-50%, -50%) translate3d(${state().pan.x}px, ${state().pan.y}px, 0) scale(${state().zoom})`,
                "will-change": "transform",
              }}
            />
          )}
        </Show>
        <Show when={store.imageStatus() === "error"}>
          <Hint text={t("fullscreen.failed")} />
        </Show>
      </Show>
    </div>
  );
}

/** 居中一句话（空态 / 读图失败）。没有按钮 —— 退出只有 `Esc` / `Enter` 两个键。 */
function Hint(props: { text: string }) {
  return (
    <p class="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 text-fs-2 text-fg-3">
      {props.text}
    </p>
  );
}

export default FullscreenViewer;
