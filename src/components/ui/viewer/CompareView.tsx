/**
 * 对比视图（`BROWSE.md` §5.7、`plans/M2-W2.md` 2.2–2.3）。
 *
 * 第一幅只定义共同画幅比例；绿色描边表示**当前照片**，点任意画幅会切换它，
 * 但不改变参与对比的选择集合。每幅图都通过同一个 ViewerStore 取得自己的 URL。
 */

import {
  createEffect,
  createSignal,
  For,
  onCleanup,
  onMount,
  Show,
  type JSX,
} from "solid-js";

import { t } from "../../../i18n/index.ts";
import {
  baselineAspect,
  COMPARE_MAX,
  compareFrames,
  compareLayout,
  fitAspectWithin,
} from "../../../lib/viewer-compare.ts";
import type { ViewerPhoto, ViewerStore } from "./index.ts";
import {
  isViewerControlTarget,
  viewerControlsVisible,
} from "./interaction.ts";
import { ViewerControls } from "./ViewerControls.tsx";

export interface CompareViewProps {
  /** 参与对比的照片（已按显示顺序、已截到上限，顺序里第一个是画幅比例基准） */
  photos: readonly ViewerPhoto[];
  /** 选中总数；大于 4 时说明界面只显示最近选择的 4 张。 */
  selectedCount?: number;
  /** 点哪幅就把哪幅设为当前照片；不改变选择集合。 */
  onFocus?: (photo: ViewerPhoto) => void;
  /** 倍率、位移、当前照片与多图 URL 的唯一来源。 */
  store: ViewerStore;
  /** 返回 tiles（左上角按钮）。 */
  onClose?: () => void;
  class?: string;
}

export function CompareView(props: CompareViewProps): JSX.Element {
  let host: HTMLDivElement | undefined;
  const [dragging, setDragging] = createSignal<{ x: number; y: number } | null>(null);
  const [cursor, setCursor] = createSignal<{ x: number; y: number } | null>(null);
  const [frameSize, setFrameSize] = createSignal({ width: 0, height: 0 });

  const frames = () => compareFrames(props.photos);
  const aspect = () => baselineAspect(props.photos) ?? 1;
  const layout = () => compareLayout(props.photos.length);

  /**
   * 对比图片的基础 CSS 尺寸已经是「适配画框」，所以 store 的绝对像素倍率不能再直接乘一次。
   * 记下进入这组对比时的适配倍率，画面只使用 `当前倍率 / 适配倍率` 这一个相对倍数。
   */
  const [fitZoom, setFitZoom] = createSignal(Math.max(0.0001, props.store.state().zoom));
  const compareScale = (): number => props.store.state().zoom / fitZoom();

  let photoKey = "";
  createEffect(() => {
    const nextKey = props.photos.map((photo) => `${photo.id}\u0000${photo.path}`).join("\u0001");
    if (nextKey === photoKey) return;
    photoKey = nextKey;

    // 同一套 store 负责取图；这里仅表达「这几张现在都需要」，不另写第二套加载器。
    for (const photo of props.photos) void props.store.ensureImage(photo);

    const currentId = props.store.current()?.id;
    if (!props.photos.some((photo) => photo.id === currentId)) {
      const first = props.photos[0];
      if (first !== undefined) props.onFocus?.(first);
    }

    const zoom = Math.max(0.0001, props.store.state().zoom);
    setFitZoom(zoom);
    props.store.resetFit(zoom);
  });

  /**
   * 直接算出每个网格格子里最大的等比例画框。这样窗口变窄/变高时宽高一起变化，
   * 不依赖 `height + max-width` 的约束转移，杜绝空盒比例被打破后把图片拉伸。
   */
  const measureFrames = (): void => {
    if (host === undefined) return;
    const currentLayout = layout();
    const gap = 8;
    const padding = 16;
    const cell = {
      width: Math.max(
        0,
        (host.clientWidth - padding - gap * (currentLayout.columns - 1)) /
          currentLayout.columns,
      ),
      height: Math.max(
        0,
        (host.clientHeight - padding - gap * (currentLayout.rows - 1)) /
          currentLayout.rows,
      ),
    };
    const next = fitAspectWithin(cell, aspect());
    setFrameSize((previous) =>
      previous.width === next.width && previous.height === next.height ? previous : next,
    );
  };

  onMount(() => {
    if (host === undefined) return;
    measureFrames();
    const observer = new ResizeObserver(measureFrames);
    observer.observe(host);
    onCleanup(() => observer.disconnect());
  });
  createEffect(() => {
    // 布局列数、行数或基准比例变了，host 尺寸未必变；显式补量一次。
    layout();
    aspect();
    measureFrames();
  });
  createEffect(() => {
    // 窗口缩放或倍率变化后，把已有 pan 收回新的等比例画框边界。
    const size = frameSize();
    props.store.panWithinFrame(0, 0, size, fitZoom());
  });

  const controlsVisible = (): boolean =>
    viewerControlsVisible(cursor(), {
      width: host?.clientWidth ?? 0,
      height: host?.clientHeight ?? 0,
    });

  const close = (): void => {
    props.store.close();
    props.onClose?.();
  };

  const onWheel = (event: WheelEvent): void => {
    event.preventDefault();
    props.store.zoomWithinFrame(
      event.deltaY < 0 ? 1.1 : 1 / 1.1,
      frameSize(),
      fitZoom(),
    );
  };

  const focusFromTarget = (target: EventTarget | null): void => {
    if (!(target instanceof Element)) return;
    const frame = target.closest<HTMLElement>("[data-compare-photo-id]");
    const id = frame?.dataset.comparePhotoId;
    if (id === undefined) return;
    const photo = props.photos.find((candidate) => candidate.id === id);
    if (photo !== undefined) props.onFocus?.(photo);
  };

  const onPointerDown = (event: PointerEvent): void => {
    if (event.button !== 0 || isViewerControlTarget(event.target)) return;
    // pointer capture 会改变后续 click 的目标，所以在捕获前就确定当前画幅。
    focusFromTarget(event.target);
    setDragging({ x: event.clientX, y: event.clientY });
    host?.setPointerCapture(event.pointerId);
  };

  const trackCursor = (event: PointerEvent): void => {
    const rect = host?.getBoundingClientRect();
    if (rect !== undefined) {
      setCursor({ x: event.clientX - rect.left, y: event.clientY - rect.top });
    }
  };

  const onPointerMove = (event: PointerEvent): void => {
    trackCursor(event);
    const from = dragging();
    if (from === null) return;
    const dx = event.clientX - from.x;
    const dy = event.clientY - from.y;
    // pan 的单位就是 CSS px：鼠标走 80px，画面也走 80px，不再除以原图像素尺寸。
    props.store.panWithinFrame(dx, dy, frameSize(), fitZoom());
    setDragging({ x: event.clientX, y: event.clientY });
  };

  const endDrag = (event: PointerEvent): void => {
    if (dragging() === null) return;
    setDragging(null);
    if (host?.hasPointerCapture(event.pointerId)) host.releasePointerCapture(event.pointerId);
  };

  return (
    <div
      ref={host}
      data-compare="open"
      data-compare-count={props.photos.length}
      data-compare-cols={layout().columns}
      data-compare-rows={layout().rows}
      class={[
        "absolute inset-0 z-10 grid overflow-hidden bg-surface-bar p-2",
        dragging() === null ? "cursor-grab" : "cursor-grabbing",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
      style={{
        gap: "8px",
        "grid-template-columns": `repeat(${layout().columns}, minmax(0, 1fr))`,
        "grid-template-rows": `repeat(${layout().rows}, minmax(0, 1fr))`,
      }}
      onWheel={onWheel}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerLeave={() => setCursor(null)}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
    >
      <Show when={(props.selectedCount ?? 0) > COMPARE_MAX}>
        <span
          data-compare-note="open"
          class="absolute bottom-1 left-1/2 z-10 -translate-x-1/2 rounded-ui bg-surface-layer px-2 py-1 text-fs-0 text-fg-3"
        >
          {t("browse.compareLimit").replace("{n}", String(props.selectedCount ?? 0))}
        </span>
      </Show>

      <For each={frames()}>
        {(frame, at) => (
          <div
            data-compare-frame={at()}
            data-compare-photo-id={frame.photo.id}
            data-baseline={at() === 0 ? "true" : undefined}
            data-current={props.store.current()?.id === frame.photo.id ? "true" : undefined}
            aria-label={frame.photo.fileName}
            class="flex h-full min-w-0 cursor-pointer items-center justify-center overflow-hidden"
            /* 保留 click 入口给键盘/自动化；真实指针在 pointerdown 已先切焦点。 */
            onClick={() => props.onFocus?.(frame.photo)}
          >
            <div
              data-compare-canvas
              class={[
                "relative shrink-0 overflow-hidden rounded-ui bg-surface-main",
                props.store.current()?.id === frame.photo.id
                  ? "border border-brand"
                  : "border border-transparent",
              ].join(" ")}
              style={{
                width: `${frameSize().width}px`,
                height: `${frameSize().height}px`,
              }}
            >
              <Show
                when={
                  frame.photo.natural !== undefined &&
                  frame.photo.natural.width > 0 &&
                  frame.photo.natural.height > 0
                }
                fallback={
                  <span class="flex h-full items-center justify-center text-fs-2 text-fg-3">
                    {t("browse.compareNoSize")}
                  </span>
                }
              >
                <Show when={props.store.imageUrlFor(frame.photo)}>
                  {(url) => (
                    <img
                      class="pointer-events-none absolute max-w-none select-none"
                      src={url()}
                      alt=""
                      draggable={false}
                      style={{
                        width: `${(frame.photo.natural!.width / Math.max(1, frame.crop.width)) * 100}%`,
                        height: `${(frame.photo.natural!.height / Math.max(1, frame.crop.height)) * 100}%`,
                        left: `${-(frame.crop.x / Math.max(1, frame.crop.width)) * 100}%`,
                        top: `${-(frame.crop.y / Math.max(1, frame.crop.height)) * 100}%`,
                        transform: `translate3d(${props.store.state().pan.x}px, ${
                          props.store.state().pan.y
                        }px, 0) scale(${compareScale()})`,
                        "transform-origin": "center",
                        "will-change": "transform",
                      }}
                    />
                  )}
                </Show>
              </Show>
            </div>
          </div>
        )}
      </For>

      <ViewerControls
        store={props.store}
        onClose={close}
        visible={controlsVisible()}
        zoomLabel={() =>
          props.store.state().fit
            ? t("viewer.fit")
            : `${Math.round(compareScale() * 100)}%`
        }
        onZoomOut={() => props.store.zoomWithinFrame(1 / 1.25, frameSize(), fitZoom())}
        onZoomIn={() => props.store.zoomWithinFrame(1.25, frameSize(), fitZoom())}
        onFit={() => props.store.resetFit(fitZoom())}
      />
    </div>
  );
}
