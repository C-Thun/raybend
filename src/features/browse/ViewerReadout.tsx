/**
 * 看图态右栏的**预览 + 直方图**（`BROWSE.md` §5.9、`plans/M2-W2.md` 1.6）。
 *
 * ```text
 * ┌──────────────────┐
 * │ 预览              │   ← 整张照片缩到面板里
 * │ ┌──────────────┐ │
 * │ │      ┌────┐  │ │   ← 1px 主色框 = 当前**看得见的那块**（放大/拖动时动）
 * │ │      └────┘  │ │
 * │ └──────────────┘ │
 * │ 直方图            │   ← 24 根柱，三通道叠着画
 * │ ▁▂▅█▇▅▃▂▁▁▂▃▅▇▅▂ │
 * └──────────────────┘
 * ```
 *
 * 三条纪律：
 *
 * 1. **预览框的数学在 `visibleRect()`**（`components/ui/viewer/store.ts`），这里只按百分比摆 ——
 *    框和照片必须是同一个变换，否则放大后框会跟画面错位；
 * 2. **直方图是 Rust 算的**（`getHistogram` → `image_histogram`）：`AGENTS.md` §6.1 的红线，
 *    前端不碰像素；这里只把 24 个整数画成柱子；
 * 3. 两块的底都用 `$surface-bar`（面板内插入块的规定，`design/browse.md` §2.3.1）。
 *
 * 只在看图时出现：tiles 模式下这两块让位给 EXIF（`AssetInfo` 里切换）。
 */

import { createEffect, createSignal, For, onCleanup, Show, type JSX } from "solid-js";

import { getHistogram, type Histogram } from "../../api/db.ts";
import { t } from "../../i18n/index.ts";
import { visibleRect, type ViewerStore } from "../../components/ui/viewer/index.ts";
import { histogramBarHeights, histogramIsEmpty } from "./histogram.ts";

export interface ViewerReadoutProps {
  store: ViewerStore;
}

/**
 * 直方图按**路径**缓存（不是按照片 id）：同一张照片来回翻时不必重算。
 *
 * 上限 24 条、超出丢最早的 —— 像看图件那边的图片缓存一样，只为「来回翻」服务，
 * 不当长期缓存用（真正的缓存属于后端 `cache/` 的活）。
 */
const HISTOGRAM_CACHE_LIMIT = 24;
const histogramCache = new Map<string, Histogram | null>();

function cacheHistogram(path: string, value: Histogram | null): void {
  if (histogramCache.size >= HISTOGRAM_CACHE_LIMIT) {
    const oldest = histogramCache.keys().next();
    if (!oldest.done) histogramCache.delete(oldest.value);
  }
  histogramCache.set(path, value);
}

export function ViewerReadout(props: ViewerReadoutProps): JSX.Element {
  const [histogram, setHistogram] = createSignal<Histogram | null>(null);

  // 取直方图：跟着「当前这张」走，取完之前先是空态（不阻塞任何东西）
  createEffect(() => {
    const path = props.store.current()?.path;
    if (path === undefined) {
      setHistogram(null);
      return;
    }
    if (histogramCache.has(path)) {
      setHistogram(histogramCache.get(path) ?? null);
      return;
    }
    setHistogram(null);
    let alive = true;
    onCleanup(() => {
      alive = false;
    });
    void getHistogram(path)
      .then((value) => {
        if (!alive) return;
        cacheHistogram(path, value);
        setHistogram(value);
      })
      .catch(() => {
        if (alive) setHistogram(null);
      });
  });

  const bars = () => histogramBarHeights(histogram());
  const natural = () => props.store.state().natural;
  const rect = () => visibleRect(props.store.state());
  /** 框按**百分比**摆：这样预览缩小多少都不会错位 */
  const asPercent = (value: number, total: number): string =>
    `${total <= 0 ? 0 : (value / total) * 100}%`;

  return (
    <>
      {/* ── 预览（整张 + 视野框） ── */}
      <section class="mb-5" data-viewer-readout="open">
        <h3 class="mb-1.5 text-fs-3 font-semibold text-fg-2">{t("browse.preview")}</h3>
        <div class="flex h-[180px] items-center justify-center rounded-ui bg-surface-bar p-2">
          <Show
            when={natural().width > 0 && natural().height > 0}
            fallback={<span class="text-fs-2 text-fg-3">{t("browse.noSelection")}</span>}
          >
            {/*
              这层盒子**就是照片的盒子**（`aspect-ratio` = 原图比例、高度撑满、宽度跟着算），
              所以视野框用百分比定位就天然对齐 —— 不需要去读 DOM 尺寸。
            */}
            <div
              class="relative"
              style={{
                "aspect-ratio": `${natural().width} / ${natural().height}`,
                height: "100%",
                "max-width": "100%",
                "max-height": "100%",
              }}
            >
              <img
                class="block h-full w-full object-contain"
                src={props.store.imageUrl() ?? undefined}
                alt=""
                draggable={false}
              />
              <Show when={rect()}>
                {(area) => (
                  <div
                    class="pointer-events-none absolute border border-brand"
                    style={{
                      left: asPercent(area().x, natural().width),
                      top: asPercent(area().y, natural().height),
                      width: asPercent(area().width, natural().width),
                      height: asPercent(area().height, natural().height),
                    }}
                  />
                )}
              </Show>
            </div>
          </Show>
        </div>
      </section>

      {/* ── 直方图（24 柱，三通道叠画） ── */}
      <section class="mb-5">
        <h3 class="mb-1.5 text-fs-3 font-semibold text-fg-2">{t("browse.histogram")}</h3>
        <div
          class="rounded-ui bg-surface-bar px-2 py-1.5"
          data-histogram={histogramIsEmpty(bars()) ? "empty" : "bars"}
        >
          <Show
            when={!histogramIsEmpty(bars())}
            fallback={
              <p class="py-3 text-center text-fs-2 text-fg-3">{t("browse.histogramEmpty")}</p>
            }
          >
            <div class="flex h-[72px] items-end gap-px" title={t("browse.histogramHint")}>
              <For each={bars()?.r ?? []}>
                {(_, index) => (
                  <div class="relative h-full flex-1">
                    <div
                      class="absolute inset-x-0 bottom-0"
                      style={{
                        height: `${(bars()?.r[index()] ?? 0) * 100}%`,
                        background: "var(--hist-r)",
                        opacity: "var(--hist-alpha)",
                      }}
                    />
                    <div
                      class="absolute inset-x-0 bottom-0"
                      style={{
                        height: `${(bars()?.g[index()] ?? 0) * 100}%`,
                        background: "var(--hist-g)",
                        opacity: "var(--hist-alpha)",
                      }}
                    />
                    <div
                      class="absolute inset-x-0 bottom-0"
                      style={{
                        height: `${(bars()?.b[index()] ?? 0) * 100}%`,
                        background: "var(--hist-b)",
                        opacity: "var(--hist-alpha)",
                      }}
                    />
                  </div>
                )}
              </For>
            </div>
          </Show>
        </div>
      </section>
    </>
  );
}
