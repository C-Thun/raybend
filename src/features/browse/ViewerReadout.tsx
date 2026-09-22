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

import { Show, type JSX } from "solid-js";

import { getHistogram } from "../../api/db.ts";
import { t } from "../../i18n/index.ts";
import { visibleRect, type ViewerStore } from "../../components/ui/viewer/index.ts";
import { HistogramPanel } from "../../components/ui/HistogramPanel.tsx";
import { PREVIEW_FRAME_ASPECT, fitAxisFor } from "../../lib/preview-frame.ts";

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

export function ViewerReadout(props: ViewerReadoutProps): JSX.Element {
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

      {/* ── 直方图（取数 + 缓存 + 画图都在 `components/ui/HistogramPanel.tsx`，与编辑右栏共用一份） ── */}
      <HistogramPanel
        load={getHistogram}
        path={props.store.current()?.path ?? null}
        title={t("browse.histogram")}
        emptyText={t("browse.histogramEmpty")}
      />
    </>
  );
}
