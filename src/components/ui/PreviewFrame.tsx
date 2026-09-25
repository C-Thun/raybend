/**
 * browse/editor 共用的全图总览：外框宽度占满右栏，比例随照片变化并夹在 3:1～3:4。
 * 图像始终完整等比显示；视野框贴在实际图像盒上，留白不参与百分比坐标。
 */

import { createMemo, createSignal, onCleanup, onMount, Show, type JSX } from "solid-js";

import { fitPreviewSize, previewFrameAspect } from "../../lib/preview-frame.ts";

export interface PreviewFrameProps {
  src: string | null;
  /** 按方向摆正后的原图尺寸；未知时从加载后的图片测量。 */
  natural?: { width: number; height: number } | null;
  emptyText: string;
  /** 原图像素坐标；不传则不画视野框。 */
  visibleRect?: { x: number; y: number; width: number; height: number } | null;
  class?: string;
}

export function PreviewFrame(props: PreviewFrameProps): JSX.Element {
  const [measured, setMeasured] = createSignal<{ src: string; width: number; height: number } | null>(null);
  const [box, setBox] = createSignal<{ width: number; height: number } | null>(null);
  let imageArea: HTMLDivElement | undefined;

  const natural = (): { width: number; height: number } | null => {
    const given = props.natural;
    if (given != null && given.width > 0 && given.height > 0) return given;
    const fallback = measured();
    if (fallback === null || fallback.src !== props.src) return null;
    return { width: fallback.width, height: fallback.height };
  };

  const fitted = createMemo(() => {
    const size = natural();
    const available = box();
    if (size === null || available === null) return null;
    const fittedSize = fitPreviewSize(size.width, size.height, available.width, available.height);
    return fittedSize === null ? null : { ...fittedSize, natural: size };
  });

  onMount(() => {
    const area = imageArea;
    if (area === undefined) return;
    const update = (width: number, height: number): void => {
      setBox((current) => current?.width === width && current.height === height
        ? current : { width, height });
    };
    update(area.clientWidth, area.clientHeight);
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver((entries) => {
      const rect = entries[0]?.contentRect;
      if (rect !== undefined) update(rect.width, rect.height);
    });
    observer.observe(area);
    onCleanup(() => observer.disconnect());
  });

  const onLoad = (event: Event): void => {
    const img = event.currentTarget as HTMLImageElement;
    const src = props.src;
    if (src !== null && img.naturalWidth > 0 && img.naturalHeight > 0) {
      setMeasured({ src, width: img.naturalWidth, height: img.naturalHeight });
    }
  };

  const asPercent = (value: number, total: number): string =>
    `${total <= 0 ? 0 : (value / total) * 100}%`;

  return (
    <div
      class={["flex w-full items-stretch justify-center rounded-ui bg-surface-bar p-2", props.class ?? ""]
        .filter(Boolean).join(" ")}
      style={{ "aspect-ratio": String(previewFrameAspect(natural()?.width, natural()?.height)) }}
      data-preview-frame
    >
      <div ref={imageArea} class="relative flex min-h-0 min-w-0 flex-1 items-center justify-center overflow-hidden">
        <Show when={props.src} fallback={<span class="text-fs-2 text-fg-3">{props.emptyText}</span>}>
          {(url) => (
            <Show when={fitted()} fallback={
              <img class="block h-full w-full object-contain" src={url()} alt="" draggable={false} onLoad={onLoad} />
            }>
              {(image) => (
                <div
                  class="relative shrink-0"
                  style={{ width: `${image().width}px`, height: `${image().height}px` }}
                >
                  <img class="block h-full w-full object-contain" src={url()} alt="" draggable={false} onLoad={onLoad} />
                  <Show when={props.visibleRect}>
                    {(area) => (
                      <div
                        class="pointer-events-none absolute border border-brand"
                        style={{
                          left: asPercent(area().x, image().natural.width),
                          top: asPercent(area().y, image().natural.height),
                          width: asPercent(area().width, image().natural.width),
                          height: asPercent(area().height, image().natural.height),
                        }}
                      />
                    )}
                  </Show>
                </div>
              )}
            </Show>
          )}
        </Show>
      </div>
    </div>
  );
}
