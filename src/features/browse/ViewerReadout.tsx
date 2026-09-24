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

import { type JSX } from "solid-js";

import { getHistogram } from "../../api/db.ts";
import { t } from "../../i18n/index.ts";
import { visibleRect, type ViewerStore } from "../../components/ui/viewer/index.ts";
import { HistogramPanel } from "../../components/ui/HistogramPanel.tsx";
import { PreviewFrame } from "../../components/ui/PreviewFrame.tsx";

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

  return (
    <>
      {/* ── 预览（整张 + 视野框） ── */}
      {/*
        框是**固定 4:3**（人类 2026-09-20：「比例改成 4:3，不要 3:2，这样对纵图支持更好」）——
        几何与画法都在 `components/ui/PreviewFrame.tsx`（编辑右栏「总览」用的是同一份）。
      */}
      <section class="mb-5" data-viewer-readout="open">
        <h3 class="mb-1.5 text-fs-3 font-semibold text-fg-2">{t("browse.preview")}</h3>
        <PreviewFrame
          src={props.store.imageUrl()}
          natural={natural()}
          visibleRect={props.showVisibleBox !== false ? rect() : null}
          emptyText={t("browse.noSelection")}
        />
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
