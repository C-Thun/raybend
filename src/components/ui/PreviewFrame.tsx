/**
 * **预览框**：固定 4:3 的外框 + 内层「按原图比例」的照片盒 + 可选的视野框。
 *
 * 2026-09-24 从 `features/browse/ViewerReadout.tsx` 抽出来：编辑右栏「总览」页
 * 要的是**同一个东西**（同一个 4:3 框、同一套铺边规则），而 `features/*` 之间
 * 不许互相 import（`scripts/check-architecture.mjs` 规则 2）—— `HistogramPanel`
 * 就是这么抽的先例。抽完之后两边都只用这一份：同一个东西两种表达本身就是 bug。
 *
 * 三条纪律：
 *
 * 1. **几何在 `lib/preview-frame.ts`**（`PREVIEW_FRAME_ASPECT` / `fitAxisFor`，有单测），
 *    这里只负责画；框是 4:3 **固定**的（人类 2026-09-20 定），不随照片比例撑大；
 * 2. **视野框按百分比摆**：内层盒子就是照片的盒子（`aspect-ratio` = 原图比例），
 *    所以 `visibleRect` 只要按原图像素折算成百分比，不需要去读 DOM 尺寸；
 * 3. **图源与尺寸由调用方注入**（`components/ui/` 不许 import `api`）——
 *    与 `HistogramPanel` 的取数注入是同一套做法。
 *
 * 尺寸未知时（元数据还没读到）先按 `object-contain` 画着，并在图片 `load` 后
 * **自己量一次** —— 这样调用方不需要为了摆框先等一次元数据往返。
 */

import { createSignal, Show, type JSX } from "solid-js";

import { PREVIEW_FRAME_ASPECT, fitAxisFor } from "../../lib/preview-frame.ts";

export interface PreviewFrameProps {
  /** 照片地址；`null` = 还没取到（画 `emptyText`） */
  src: string | null;
  /**
   * 原图尺寸（**已按方向换算**的宽高）；未知传 `null` / 不传 —— 组件会等图片加载后自己量。
   * 有值时先用它摆好盒子（避免图加载完才跳一下）。
   */
  natural?: { width: number; height: number } | null;
  /** 空态文案（已是译好的文案） */
  emptyText: string;
  /** 视野框（**原图像素坐标**，与 `visibleRect()` 同一口径）；不传 = 不画 */
  visibleRect?: { x: number; y: number; width: number; height: number } | null;
  class?: string;
}

export function PreviewFrame(props: PreviewFrameProps): JSX.Element {
  /** 图片加载后量到的尺寸（按 `src` 记住 —— 换图时旧的那份不能接着用） */
  const [measured, setMeasured] = createSignal<
    { src: string; width: number; height: number } | null
  >(null);

  /** 元数据优先（先摆好，不跳）；没有就用加载后量到的 */
  const natural = (): { width: number; height: number } | null => {
    const given = props.natural;
    if (given != null && given.width > 0 && given.height > 0) return given;
    const fallback = measured();
    if (fallback === null || fallback.src !== props.src) return null;
    return { width: fallback.width, height: fallback.height };
  };

  const onLoad = (event: Event): void => {
    const img = event.currentTarget as HTMLImageElement;
    if (img.naturalWidth > 0 && img.naturalHeight > 0) {
      setMeasured({ src: img.currentSrc || img.src, width: img.naturalWidth, height: img.naturalHeight });
    }
  };

  /** 框按**百分比**摆：这样预览缩小多少都不会错位 */
  const asPercent = (value: number, total: number): string =>
    `${total <= 0 ? 0 : (value / total) * 100}%`;

  return (
    <div
      class={[
        "flex w-full items-center justify-center rounded-ui bg-surface-bar p-2",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
      style={{ "aspect-ratio": String(PREVIEW_FRAME_ASPECT) }}
      data-preview-frame
    >
      <Show when={props.src} fallback={<span class="text-fs-2 text-fg-3">{props.emptyText}</span>}>
        {(url) => (
          <Show
            when={natural()}
            fallback={
              /* 尺寸还不知道：先按 object-contain 画着，`onLoad` 量到之后换成精确盒子。
                 尺寸要用 `h-full w-full`（不是 `max-*`）—— `IMAGING.md` §3.1-4：
                 比显示区小的图**要放大**，`max-*` 那套会把小图按原始像素摆着。 */
              <img
                class="block h-full w-full object-contain"
                src={url()}
                alt=""
                draggable={false}
                onLoad={onLoad}
              />
            }
          >
            {(size) => (
              <div
                class="relative"
                style={{
                  "aspect-ratio": `${size().width} / ${size().height}`,
                  ...(fitAxisFor(size().width, size().height, PREVIEW_FRAME_ASPECT) === "width"
                    ? { width: "100%", height: "auto" }
                    : { height: "100%", width: "auto" }),
                }}
              >
                <img
                  class="block h-full w-full object-contain"
                  src={url()}
                  alt=""
                  draggable={false}
                  onLoad={onLoad}
                />
                <Show when={props.visibleRect}>
                  {(area) => (
                    <div
                      class="pointer-events-none absolute border border-brand"
                      style={{
                        left: asPercent(area().x, size().width),
                        top: asPercent(area().y, size().height),
                        width: asPercent(area().width, size().width),
                        height: asPercent(area().height, size().height),
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
  );
}
