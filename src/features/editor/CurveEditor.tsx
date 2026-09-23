/**
 * **曲线编辑器**（`design/editor.md` §3.7：本仓唯一自研的控件，Ark UI 没有对应件）。
 *
 * # 人类 2026-09-24 定的口径（照这个做，别自作主张）
 *
 * * 曲线区**是正方形**，并占满右栏内容区的宽；
 * * 通道选择**不复用功能块的 tab / 分段控件**（那样分不出主次）——
 *   小一号、集成在曲线组件里，样式**照直方图那一套**（只是多一个 `RGB` 全色通道）；
 *   通道靠左、重置靠右，同一排；
 * * 背景是**当前通道的直方图**（同一时刻只显示单通道），50% 半透明浅浅印在底纹上；
 *   ❗ 等分格子仍然要有，并且**压在直方图峰值之上**（与直方图组件里「格子在下」相反）；
 * * 交互：点线上（附近）加点 / 拖动控制点 / **双击移除** /
 *   拖两个端点（画成**正方形**）左右平衡最亮最暗基点 / 重置当前通道；
 * * 通道按钮**有调整时给辅色底纹**（示意这个通道动过）。
 *
 * # 曲线数学在前端也有一份（画图要用），但与 Rust 有共同基准
 *
 * 求值在 `lib/curve.ts`（单调三次），与 `crates/raybend/src/develop/curve.rs`
 * 对着**同一份** `src/lib/curve-vectors.json` 断言 —— 两份实现不许各走各的。
 */

import { For, Show, createSignal, type JSX } from "solid-js";

import { histogramPath, type HistogramCounts } from "../../lib/histogram.ts";
import {
  addPoint,
  curvePath,
  curveFunction,
  isIdentityCurve,
  movePoint,
  nearestPoint,
  removePoint,
  type CurvePoint,
} from "../../lib/curve.ts";
import { t } from "../../i18n/index.ts";
import type { CurveChannel, EditorStore } from "./store.ts";

/** SVG 的坐标系（正方形，边长 100 —— 与容器 1:1 对应）。 */
const VIEW = 100;
/** 命中控制点的容差（归一化距离；方块边长按 1 算）。 */
const POINT_TOLERANCE = 0.055;
/** 认作「点在曲线上」的容差（纵向）。 */
const LINE_TOLERANCE = 0.06;

const CHANNELS: readonly CurveChannel[] = ["rgb", "r", "g", "b"];

/** 通道按钮的字母（RGB 是「全色」那一个）。 */
const CHANNEL_LABEL: Record<CurveChannel, string> = {
  rgb: "RGB",
  r: "R",
  g: "G",
  b: "B",
};

/**
 * 直方图那层的颜色（与 `Histogram.tsx` 的令牌同一套）。
 *
 * `rgb` 用 `--hist-triple`（三通道重叠处那个中间灰）：这里要的是「全色」的轮廓，
 * 不是三条彩色叠在一起 —— 三条叠起来在 50% 透明度下会糊成一片。
 */
const CHANNEL_FILL: Record<CurveChannel, string> = {
  rgb: "fill-(--hist-triple)",
  r: "fill-(--label-red)",
  g: "fill-(--label-green)",
  b: "fill-(--label-blue)",
};

export interface CurveEditorProps {
  store: EditorStore;
  /** 这张照片的直方图（背景那层）；`null` = 还没读到，只画格子 */
  histogram: HistogramCounts | null;
  /** 整块禁用（空态 / 二级锁） */
  disabled?: boolean;
  /** 松手 / 改动完成 → 工作区落库 */
  onCommit?: () => void;
}

/** 某个通道在背景里那条轮廓的采样值（RGB = 三通道包络）。 */
function channelValues(
  histogram: HistogramCounts | null,
  channel: CurveChannel,
): readonly number[] {
  if (histogram === null) return [];
  if (channel === "r") return histogram.r;
  if (channel === "g") return histogram.g;
  if (channel === "b") return histogram.b;
  // RGB：取三通道的**包络**（逐点最大），画成一条灰轮廓
  const length = Math.max(histogram.r.length, histogram.g.length, histogram.b.length);
  const out: number[] = [];
  for (let index = 0; index < length; index += 1) {
    out.push(
      Math.max(
        histogram.r[index] ?? 0,
        histogram.g[index] ?? 0,
        histogram.b[index] ?? 0,
      ),
    );
  }
  return out;
}

export function CurveEditor(props: CurveEditorProps): JSX.Element {
  const [dragging, setDragging] = createSignal<number | null>(null);

  const channel = (): CurveChannel => props.store.curveChannel();
  const points = (): readonly CurvePoint[] => props.store.curvePoints(channel());

  /** 把一次指针事件换算成方块里的归一化坐标（左下为原点，与曲线同向）。 */
  function positionOf(event: PointerEvent, svg: SVGSVGElement): { x: number; y: number } {
    const rect = svg.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return { x: 0, y: 0 };
    const x = (event.clientX - rect.left) / rect.width;
    const y = 1 - (event.clientY - rect.top) / rect.height;
    return {
      x: Math.min(1, Math.max(0, x)),
      y: Math.min(1, Math.max(0, y)),
    };
  }

  function onPointerDown(event: PointerEvent): void {
    if (props.disabled === true) return;
    const svg = event.currentTarget;
    if (!(svg instanceof SVGSVGElement)) return;
    const { x, y } = positionOf(event, svg);
    const current = points();

    // ① 命中已有的点 → 拖它
    const hit = nearestPoint(current, x, y, POINT_TOLERANCE);
    if (hit >= 0) {
      setDragging(hit);
      svg.setPointerCapture(event.pointerId);
      return;
    }

    // ② 点在**曲线上（附近）** → 在那条曲线上加一个控制点，并直接进入拖动
    const evaluate = curveFunction(current);
    const onLine = evaluate(x);
    if (Math.abs(onLine - y) <= LINE_TOLERANCE) {
      const next = addPoint(current, x, onLine);
      if (next.length !== current.length) {
        props.store.setCurvePoints(channel(), next);
        // 新点在排序后的位置（按 x 找回去）
        const index = next.findIndex((point) => Math.abs(point[0] - x) < 1e-6);
        setDragging(index >= 0 ? index : null);
        svg.setPointerCapture(event.pointerId);
      }
    }
  }

  function onPointerMove(event: PointerEvent): void {
    const index = dragging();
    if (index === null) return;
    const svg = event.currentTarget;
    if (!(svg instanceof SVGSVGElement)) return;
    const { x, y } = positionOf(event, svg);
    props.store.setCurvePoints(channel(), movePoint(points(), index, x, y));
  }

  function onPointerUp(event: PointerEvent): void {
    if (dragging() === null) return;
    setDragging(null);
    const svg = event.currentTarget;
    if (svg instanceof SVGSVGElement && svg.hasPointerCapture(event.pointerId)) {
      svg.releasePointerCapture(event.pointerId);
    }
    props.onCommit?.();
  }

  function onDoubleClick(event: MouseEvent): void {
    if (props.disabled === true) return;
    const svg = event.currentTarget;
    if (!(svg instanceof SVGSVGElement)) return;
    const rect = svg.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return;
    const x = (event.clientX - rect.left) / rect.width;
    const y = 1 - (event.clientY - rect.top) / rect.height;
    const hit = nearestPoint(points(), x, y, POINT_TOLERANCE);
    if (hit < 0) return;
    const next = removePoint(points(), hit);
    if (next.length !== points().length) {
      props.store.setCurvePoints(channel(), next);
      props.onCommit?.();
    }
  }

  return (
    <div class="flex flex-col gap-2" data-editor-curve>
      {/*
        正方形、占满内容区宽：`aspect-square` + `w-full`。
        用 `preserveAspectRatio="none"`（与直方图同一套）：容器已经是正方形，
        所以不会有拉伸变形，而 viewBox 的 0..100 可以直接当百分比用。
      */}
      <div class="relative w-full aspect-square overflow-hidden rounded-ui bg-surface-bar">
        <svg
          viewBox={`0 0 ${VIEW} ${VIEW}`}
          preserveAspectRatio="none"
          class="absolute inset-0 h-full w-full touch-none select-none"
          classList={{ "cursor-crosshair": props.disabled !== true }}
          role="img"
          aria-label={t("editor.curve.hint")}
          data-curve-plot={channel()}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
          onDblClick={onDoubleClick}
        >
          {/* ① 直方图（背景，50% 半透明） */}
          <Show when={channelValues(props.histogram, channel()).length > 0}>
            <path
              d={histogramPath(channelValues(props.histogram, channel()), VIEW, VIEW)}
              class={CHANNEL_FILL[channel()]}
              opacity="0.5"
              data-curve-histogram={channel()}
            />
          </Show>

          {/* ② 等分格子（**压在直方图之上** —— 人类明确要求，与直方图组件相反） */}
          <g data-curve-grid stroke="var(--hist-grid)" stroke-width="0.4">
            <For each={[25, 50, 75]}>
              {(percent) => (
                <>
                  <line x1={percent} y1={0} x2={percent} y2={VIEW} />
                  <line x1={0} y1={percent} x2={VIEW} y2={percent} />
                </>
              )}
            </For>
          </g>

          {/* ③ 曲线本体（`non-scaling-stroke`：线条粗细不随容器缩放而变） */}
          <path
            d={curvePath(points(), VIEW, VIEW)}
            fill="none"
            stroke="var(--brand)"
            stroke-width="1.4"
            vector-effect="non-scaling-stroke"
            data-curve-line
          />

          {/* ④ 控制点：中间是圆点，**首尾是正方形**（黑场 / 白场，可左右拖） */}
          <For each={points()}>
            {(point, index) => (
              <Show
                when={index() > 0 && index() < points().length - 1}
                fallback={
                  <rect
                    x={point[0] * VIEW - 2.6}
                    y={(1 - point[1]) * VIEW - 2.6}
                    width={5.2}
                    height={5.2}
                    rx={0.8}
                    /* 包边用**面板底色**（`--surface-bar`）：控制点压在曲线/直方图上时
                       自带一圈「挖空」，比 `--overlay-line` 那种给照片用的覆盖线更合适
                       （那两个令牌是画在**照片**上的，见 `DESIGN.md` §14.8） */
                    class="fill-brand stroke-surface-bar"
                    stroke-width="1"
                    vector-effect="non-scaling-stroke"
                    data-curve-endpoint={index() === 0 ? "black" : "white"}
                  />
                }
              >
                <circle
                  cx={point[0] * VIEW}
                  cy={(1 - point[1]) * VIEW}
                  r={2.4}
                  class="fill-brand stroke-surface-bar"
                  stroke-width="1"
                  vector-effect="non-scaling-stroke"
                  data-curve-point={index()}
                />
              </Show>
            )}
          </For>
        </svg>
      </div>

      {/* ⑤ 通道（靠左）+ 重置（靠右）—— 样式照直方图那一套，但小一号 */}
      <div class="flex items-center justify-between gap-2">
        <div class="flex items-center gap-1" role="group" aria-label={t("editor.curve.channels")}>
          <For each={CHANNELS}>
            {(item) => (
              <button
                type="button"
                aria-pressed={channel() === item}
                aria-label={CHANNEL_LABEL[item]}
                title={CHANNEL_LABEL[item]}
                disabled={props.disabled === true}
                onClick={() => props.store.setCurveChannel(item)}
                data-curve-channel={item}
                data-curve-dirty={isIdentityCurve(props.store.curvePoints(item)) ? "no" : "yes"}
                class={[
                  "flex h-5 min-w-6 items-center justify-center rounded-ui px-1 text-fs-0 font-semibold transition-colors",
                  channel() === item
                    ? "bg-state-selected text-fg-1"
                    : "text-fg-3 hover:bg-state-hover hover:text-fg-2",
                  // 有调整的通道给**辅色底纹**（人类 2026-09-24：示意这个通道动过）
                  !isIdentityCurve(props.store.curvePoints(item))
                    ? "bg-state-hover text-fg-1"
                    : "",
                ]
                  .filter(Boolean)
                  .join(" ")}
              >
                {CHANNEL_LABEL[item]}
              </button>
            )}
          </For>
        </div>
        <button
          type="button"
          disabled={props.disabled === true}
          onClick={() => {
            props.store.resetCurve(channel());
            props.onCommit?.();
          }}
          class="rounded-ui px-1.5 py-0.5 text-fs-0 text-fg-3 transition-colors hover:bg-state-hover hover:text-fg-1 disabled:opacity-50"
          data-curve-reset
        >
          {t("editor.curve.reset")}
        </button>
      </div>
    </div>
  );
}
