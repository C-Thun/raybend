/**
 * 对比视图（`BROWSE.md` §5.7、`plans/M2-W2.md` 2.2–2.3）。
 *
 * ```text
 * ┌──────────────┬──────────────┬──────────────┐
 * │ ▣ 第一幅      │              │              │   第一幅带 1px 主色内描边
 * │ （基准画幅）   │   同一倍率    │   同一倍率    │   所有画幅等宽、扣成同一比例
 * └──────────────┴──────────────┴──────────────┘
 * ```
 *
 * ## 为什么「扣等比例区域」是纯 CSS 的事
 *
 * 画幅比例不一致时以**第一幅**为准，别的图在自己像素里**居中扣**出同比例的一块
 * （数学在 `compare.ts` 的 `cropToAspect()`，有单测）。**不需要动像素**：
 * 把图片按 `原图 / 扣取区` 放大，再按 `扣取起点` 平移，扣出来的那块就正好铺满画框 ——
 * 全是几何变换（`AGENTS.md` §6.1：前端只做几何，不碰像素）。
 *
 * ## 同步
 *
 * * **缩放同步**：所有画框一样大、基准比例一样 ⇒ 同一个倍率对所有画幅含义相同；
 * * **位移按百分比**：平移量以**画框尺寸的百分比**表达（`translate` 的百分比正是
 *   相对自身尺寸），所以「第一幅挪了 5%」在第二幅上也是 5% —— 而不是各挪 200px。
 */

import { createSignal, For, Show, type JSX } from "solid-js";

import { t } from "../../../i18n/index.ts";
import type { ViewerPhoto, ViewerStore } from "./index.ts";
import { ViewerControls } from "./ViewerControls.tsx";
import { baselineAspect, COMPARE_MAX, compareFrames, compareLayout, panPercent } from "../../../lib/viewer-compare.ts";

export interface CompareViewProps {
  /** 参与对比的照片（已按显示顺序、已截到上限，顺序里第一个是基准） */
  photos: readonly ViewerPhoto[];
  /**
   * **选中总数** —— 大于 4 时界面上要说一句「只对比最近选中的 4 张」。
   *
   * 为什么不静默截断：用户 Ctrl 选了 7 张、画面只出 4 张，不说一声会让人以为漏了图。
   * 只在**真的挤掉了图**（总数 > 上限）时才出现，不占地盘。
   */
  selectedCount?: number;
  /**
   * 点了某一幅画幅（`BROWSE.md` §5.7：点哪张图就是**当前**那张，
   * 右栏与底部状态栏跟着它走）。**不改选择集合** —— 那会当场散掉对比。
   */
  onFocus?: (photo: ViewerPhoto) => void;
  /** 看图件的 store：倍率与位移的唯一来源（对比例看与单张看图共用一套） */
  store: ViewerStore;
  /** 返回 tiles（左上角那颗按钮用它） */
  onClose?: () => void;
  class?: string;
}

export function CompareView(props: CompareViewProps): JSX.Element {
  let host: HTMLDivElement | undefined;
  const [dragging, setDragging] = createSignal<{ x: number; y: number } | null>(null);

  const frames = () => compareFrames(props.photos);
  const aspect = () => baselineAspect(props.photos) ?? 1;
  /** 几张怎么摆：2 张一排 / 3 张一排三个 / 4 张 2×2（人类 2026-09-19 定） */
  const layout = () => compareLayout(props.photos.length);

  /**
   * 位移折算成**画框的百分比**。
   *
   * 看图件里 `pan` 是**像素**（单张看图时是相对原图尺寸的），对比时换算成百分比：
   * 以**第一帧的扣取区**为参考 —— 因为 store 的 `pan` 就是在「当前这张」的坐标系里
   * 累加出来的，而对比态下「当前这张」就是第一帧（锚点）。
   */
  const panPct = () => {
    const anchor = frames()[0];
    if (anchor === undefined) return { x: 0, y: 0 };
    return panPercent(props.store.state().pan, {
      width: anchor.crop.width,
      height: anchor.crop.height,
    });
  };

  const onWheel = (event: WheelEvent): void => {
    event.preventDefault();
    const factor = event.deltaY < 0 ? 1.1 : 1 / 1.1;
    props.store.zoomBy(factor);
  };

  const onPointerDown = (event: PointerEvent): void => {
    if (event.button !== 0) return;
    setDragging({ x: event.clientX, y: event.clientY });
    host?.setPointerCapture(event.pointerId);
  };

  const onPointerMove = (event: PointerEvent): void => {
    const from = dragging();
    if (from === null) return;
    props.store.panBy(event.clientX - from.x, event.clientY - from.y);
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
        dragging() === null ? "cursor-zoom-in" : "cursor-grabbing",
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
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
    >
      {/* 超过上限时把话说清楚（不静默截断） */}
      <Show when={(props.selectedCount ?? 0) > COMPARE_MAX}>
        <span
          data-compare-note="open"
          class="absolute bottom-1 left-1/2 -translate-x-1/2 rounded-ui bg-surface-layer px-2 py-1 text-fs-0 text-fg-3"
        >
          {t("browse.compareLimit").replace(
            "{n}",
            String(props.selectedCount ?? 0),
          )}
        </span>
      </Show>

      <For each={frames()}>
        {(frame, at) => (
          <div
            data-compare-frame={at()}
            data-baseline={at() === 0 ? "true" : undefined}
            data-current={props.store.current()?.id === frame.photo.id ? "true" : undefined}
            aria-label={frame.photo.fileName}
            /* 外框：等宽（`flex-1`）、铺满高，内容居中；点它就把它当「当前那张」 */
            class={[
              "flex h-full min-w-0 flex-1 items-center justify-center overflow-hidden",
              "cursor-pointer",
            ].join(" ")}
            onClick={() => props.onFocus?.(frame.photo)}
          >
            {/*
              内框：**按基准比例定形**（`aspect-ratio` + 高撑满 + 宽度跟着算，
              太宽时被 max-width 拉回来）。这一层是必需的 —— 否则画框的比例是「剩下的宽
              比整个高」，扣出来的 4:3 会被拉成那个比例，对比时两边都变形。
              第一幅是基准 ⇒ 它永远不扣，加 1px 主色内描边点明「以它为准」。
            */}
            <div
              class={[
                "relative overflow-hidden rounded-ui bg-surface-main",
                at() === 0 ? "border border-brand" : "border border-transparent",
              ].join(" ")}
              style={{
                "aspect-ratio": `${aspect()}`,
                height: "100%",
                "max-width": "100%",
                "max-height": "100%",
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
                {/*
                  图片本体：按「原图 / 扣取区」放大铺满画框，再挪到扣取位置，
                  最后叠上共享的倍率与百分比位移。

                  三处百分比都是相对**画框**（父级）算的，所以：
                    width  = 原图宽 / 扣取宽 × 100%
                    left   = −扣取起点x / 扣取宽 × 100%
                  这两条合起来表示「扣取区正好铺满画框」。
                */}
                <img
                  class="absolute max-w-none"
                  src={props.store.imageUrl() ?? undefined}
                  alt=""
                  draggable={false}
                  style={{
                    width: `${(frame.photo.natural!.width / Math.max(1, frame.crop.width)) * 100}%`,
                    height: `${(frame.photo.natural!.height / Math.max(1, frame.crop.height)) * 100}%`,
                    left: `${-(frame.crop.x / Math.max(1, frame.crop.width)) * 100}%`,
                    top: `${-(frame.crop.y / Math.max(1, frame.crop.height)) * 100}%`,
                    transform: `translate(${panPct().x * 100}%, ${panPct().y * 100}%) scale(${
                      props.store.state().zoom
                    })`,
                    "transform-origin": "center",
                  }}
                />
              </Show>
            </div>
          </div>
        )}
      </For>
      {/* 整个对比区**只有一组**控件（人类 2026-09-19）：左上返回 + 右下缩放，常驻可见 */}
      <ViewerControls store={props.store} onClose={props.onClose} visible={true} />
    </div>
  );
}
