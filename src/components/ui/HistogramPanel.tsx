/**
 * `HistogramPanel` —— 直方图那一块（标题 + 恒定高度的图 + 取数与缓存）。
 *
 * 2026-09-23 从 `features/browse/ViewerReadout.tsx` 抽出来：编辑右栏的「总览」页签
 * 要的是**同一个东西**（同一条 Rust 取数、同一份缓存口径、同一个画图组件），
 * 而 `features/*` 之间不许互相 import（`scripts/check-architecture.mjs` 规则 2）。
 * 抽到 `components/ui/` 之后两边用法完全一样，也不会有第二份缓存。
 *
 * 分层的处理方式：**取数由调用方注入**（`components/ui/` 不许 import `api`）——
 * 与 `createViewerStore` 的 `loadScreen` 是同一套做法。类型用 `lib/histogram.ts` 的
 * `HistogramCounts`（结构类型），`api/db.ts` 的 `Histogram` 直接传就行。
 *
 * ⚠️ 缓存是**模块级**的（按路径，上限 24 条）：同一张照片来回翻不重算。
 * 它只为「来回翻」服务，不当长期缓存用 —— 真正的缓存是后端 `cache/` 的活。
 */

import { createEffect, createSignal, onCleanup, type JSX } from "solid-js";

import {
  HISTOGRAM_SAMPLES,
  histogramBarHeights,
  type HistogramBars,
  type HistogramCounts,
} from "../../lib/histogram.ts";
import { Histogram } from "./Histogram.tsx";

/** 缓存条数上限（超出丢最早的）。 */
const HISTOGRAM_CACHE_LIMIT = 24;
const histogramCache = new Map<string, HistogramCounts | null>();

function cacheHistogram(path: string, value: HistogramCounts | null): void {
  if (histogramCache.size >= HISTOGRAM_CACHE_LIMIT) {
    const oldest = histogramCache.keys().next();
    if (!oldest.done) histogramCache.delete(oldest.value);
  }
  histogramCache.set(path, value);
}

export interface HistogramPanelProps {
  /** 取一张图的直方图（Rust 算的）；`null` = 拿不到（浏览器 / 认不出的文件） */
  load: (path: string, bins: number) => Promise<HistogramCounts | null>;
  /** 当前照片的路径；`null` = 没有选中（画空态） */
  path: string | null;
  /** 小标题（已是译好的文案） */
  title: string;
  /** 空态那句话（已是译好的文案） */
  emptyText: string;
  /** 采样点数（默认 86，与后端口径一致） */
  samples?: number;
  /**
   * 一帧的覆盖值（**编辑模块用**）：给了就画它、不再去读后端 ——
   * 拖曝光/对比时每帧重算直方图只需把这里换掉，不需要重新请求。
   */
  override?: HistogramBars | null;
  /** 编辑器显影帧的计数；定义后直接画它，跳过按文件路径的缓存。 */
  countsOverride?: HistogramCounts | null;
  class?: string;
}

export function HistogramPanel(props: HistogramPanelProps): JSX.Element {
  const [counts, setCounts] = createSignal<HistogramCounts | null>(null);

  // 取直方图：跟着「当前这张」走，取完之前先是空态（不阻塞任何东西）
  createEffect(() => {
    if (props.countsOverride !== undefined) return;
    const path = props.path;
    if (path === null) {
      setCounts(null);
      return;
    }
    if (histogramCache.has(path)) {
      setCounts(histogramCache.get(path) ?? null);
      return;
    }
    setCounts(null);
    let alive = true;
    onCleanup(() => {
      alive = false;
    });
    const bins = props.samples ?? HISTOGRAM_SAMPLES;
    void props
      .load(path, bins)
      .then((value) => {
        if (!alive) return;
        cacheHistogram(path, value);
        setCounts(value);
      })
      .catch(() => {
        if (alive) setCounts(null);
      });
  });

  const bars = (): HistogramBars | null =>
    props.override !== undefined && props.override !== null
      ? props.override
      : histogramBarHeights(props.countsOverride !== undefined ? props.countsOverride : counts());

  return (
    <section class={["mb-3", props.class ?? ""].filter(Boolean).join(" ")}>
      <h3 class="mb-1.5 text-fs-3 font-semibold text-fg-2">{props.title}</h3>
      <div class="rounded-ui bg-surface-bar px-2 py-1.5">
        {/* 空数据也保留同一组件和高度；有数据时只补上峰状图，不重建面板。 */}
        <Histogram bars={bars()} emptyText={props.emptyText} />
      </div>
    </section>
  );
}
