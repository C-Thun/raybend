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
 * │ 直方图            │   ← 三条完整填充曲线，可选择通道
 * │ ▁▂▅█▇▅▃▂▁▁▂▃▅▇▅▂ │
 * └──────────────────┘
 * ```
 *
 * 三条纪律：
 *
 * 1. **预览框的数学在 `visibleRect()`**（`components/ui/viewer/store.ts`），这里只按百分比摆 ——
 *    框和照片必须是同一个变换，否则放大后框会跟画面错位；
 * 2. **直方图是 Rust 算的**（`getHistogram` → `image_histogram`）：`AGENTS.md` §6.1 的红线，
 *    前端不碰像素；这里只把 86 个平均采样连成曲线；
 * 3. 两块的底都用 `$surface-bar`（面板内插入块的规定，`design/browse.md` §2.3.1）。
 *
 * 只在看图时出现：tiles 模式下这两块让位给 EXIF（`AssetInfo` 里切换）。
 */

import { createEffect, createSignal, onCleanup, Show, type JSX } from "solid-js";

/*
 * ⚠️ DTO 类型与组件同名（都叫 Histogram）：这里把**后端数据**那个改名 `HistogramData`，
 * 组件保留自己的短名字（它是界面上那个东西，读代码的地方更多）。
 */
import { getHistogram, type Histogram as HistogramData } from "../../api/db.ts";
import { t } from "../../i18n/index.ts";
import { visibleRect, type ViewerStore } from "../../components/ui/viewer/index.ts";
import { HISTOGRAM_SAMPLES, histogramBarHeights, histogramIsEmpty } from "../../lib/histogram.ts";
import { PREVIEW_FRAME_ASPECT, fitAxisFor } from "../../lib/preview-frame.ts";
import { Histogram } from "../../components/ui/Histogram.tsx";

export interface ViewerReadoutProps {
  store: ViewerStore;
  /**
   * 要不要画「视野框」（默认画）。
   *
   * 对比态传 `false`：视野框描述的是**单张看图那一个窗口**里看得见哪一块，
   * 而对比是好几个各自独立的窗口 —— 一个框表达不了（2026-09-20）。
   */
  showVisibleBox?: boolean;
}

/**
 * 直方图按**路径**缓存（不是按照片 id）：同一张照片来回翻时不必重算。
 *
 * 上限 24 条、超出丢最早的 —— 像看图件那边的图片缓存一样，只为「来回翻」服务，
 * 不当长期缓存用（真正的缓存属于后端 `cache/` 的活）。
 */
const HISTOGRAM_CACHE_LIMIT = 24;
const histogramCache = new Map<string, HistogramData | null>();

function cacheHistogram(path: string, value: HistogramData | null): void {
  if (histogramCache.size >= HISTOGRAM_CACHE_LIMIT) {
    const oldest = histogramCache.keys().next();
    if (!oldest.done) histogramCache.delete(oldest.value);
  }
  histogramCache.set(path, value);
}

export function ViewerReadout(props: ViewerReadoutProps): JSX.Element {
  const [histogram, setHistogram] = createSignal<HistogramData | null>(null);

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
    void getHistogram(path, HISTOGRAM_SAMPLES)
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
      {/*
        框是**固定 4:3**（人类 2026-09-20：「比例改成 4:3，不要 3:2，这样对纵图支持更好」）——
        不是按窗口高度写死的像素，也不是跟着照片走：固定下来横图铺宽、纵图铺高，
        同一块面板宽度下纵图能占到更多高度。几何规则在 `lib/preview-frame.ts`（有单测）。
      */}
      <section class="mb-5" data-viewer-readout="open">
        <h3 class="mb-1.5 text-fs-3 font-semibold text-fg-2">{t("browse.preview")}</h3>
        <div
          class="flex w-full items-center justify-center rounded-ui bg-surface-bar p-2"
          style={{ "aspect-ratio": String(PREVIEW_FRAME_ASPECT) }}
        >
          <Show
            when={natural().width > 0 && natural().height > 0}
            fallback={<span class="text-fs-2 text-fg-3">{t("browse.noSelection")}</span>}
          >
            {/*
              这层盒子**就是照片的盒子**（`aspect-ratio` = 原图比例，铺满框的一条边），
              所以视野框用百分比定位就天然对齐 —— 不需要去读 DOM 尺寸。
            */}
            <div
              class="relative"
              style={{
                "aspect-ratio": `${natural().width} / ${natural().height}`,
                ...(fitAxisFor(natural().width, natural().height, PREVIEW_FRAME_ASPECT) === "width"
                  ? { width: "100%", height: "auto" }
                  : { height: "100%", width: "auto" }),
              }}
            >
              <img
                class="block h-full w-full object-contain"
                src={props.store.imageUrl() ?? undefined}
                alt=""
                draggable={false}
              />
              <Show when={props.showVisibleBox !== false ? rect() : null}>
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

      {/* ── 直方图（三条完整通道曲线；组件在 `components/ui/Histogram.tsx`） ── */}
      <section class="mb-5">
        <h3 class="mb-1.5 text-fs-3 font-semibold text-fg-2">{t("browse.histogram")}</h3>
        <div class="rounded-ui bg-surface-bar px-2 py-1.5">
          <Show
            when={!histogramIsEmpty(bars())}
            fallback={
              <p class="py-3 text-center text-fs-2 text-fg-3">{t("browse.histogramEmpty")}</p>
            }
          >
            {/*
              组件吃的是**归一化后的采样**（不是原始计数）：这样将来编辑模块每帧重算
              （拖曝光/对比时）只要把新的 `bars` 传进来就换一帧 —— 不需要重新请求后端、
              也不需要重新挂载。绘制数学与颜色分层都在 `lib` / 组件内部，不在这层。
            */}
            <Histogram bars={bars()} />
          </Show>
        </div>
      </section>
    </>
  );
}
