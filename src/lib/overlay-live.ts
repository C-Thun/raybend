/**
 * **覆盖层的实时变换**（拖 / 缩放照片时，画在照片上的标记要跟着动）。
 *
 * # 为什么需要它
 *
 * 照片由 wgpu 直绘，每帧都在动；而覆盖层（`ViewportOverlay` 的矩阵、
 * `AreaMarkers` 的圆片）读的是 Rust 报的变换 —— 平时靠 250ms 轮询，
 * 于是一次拖动看起来是「照片动、标记不动，松手才跳过去」。这不可接受
 * （崔总 2026-11-01：控制点必须完美实时跟随）。
 *
 * # 做法（不放宽任何红线）
 *
 * 仍是 **Rust 给矩阵、前端只贴上去**（`AGENTS.md` §6.1 红线 2）：这里只是把
 * 「多久读一次」从固定 250ms 改成「手势期间每帧读一次、静止时一次都不读」。
 *
 * * `overlayPulse()` —— 任何会让视口动一下的地方都点一下（拖动每帧、滚轮、
 *   缩放按钮、双击适配、上报变化）；它只把「还要盯多久」往后推 `HOLD_MS`。
 * * 宿主组件（`EditorViewport`）跑一个常驻的 rAF 循环，**只在还要盯的时候**
 *   去调 `editor_overlay_state`（那次 IPC 只锁一次、拷 6 个浮点）。
 * * 值没变就不写信号（拖动中每帧一次写入足以，静止时写零点几次）。
 */

import { createSignal } from "solid-js";

import type { OverlayMatrix } from "./overlay-math.ts";

/** 一次手势之后还要盯多久（毫秒）—— 覆盖尾样本与 IPC 往返。 */
const HOLD_MS = 400;

export interface OverlayLive {
  /** 图像像素 → 洞口内 CSS 像素 */
  transform: OverlayMatrix | null;
  /** = zoom / dpr（屏幕像素固定的标记要反向缩放） */
  scale: number;
  /** 图像旋转角（度） */
  rotation: number;
  /** 这份变换属于哪张**已经上屏**的照片（换图瞬间防张冠李戴） */
  photoPath: string | null;
}

const [live, setLive] = createSignal<OverlayLive | null>(null);

/** 最近一次的实时变换（`null` = 还没读到 / 没有视口）。 */
export function overlayLive(): OverlayLive | null {
  return live();
}

let holdUntil = 0;

/** 想让覆盖层跟着动的那些地方（宿主组件）—— 手心一热就叫醒它们。 */
const pumps = new Set<() => void>();

/**
 * 注册一个「该开始盯了」的回调（返回反注册）。
 *
 * 宿主不必常开 rAF 循环：有人 `overlayPulse()` 时它才醒，盯完 `HOLD_MS` 自动睡。
 */
export function onOverlayPulse(pump: () => void): () => void {
  pumps.add(pump);
  return () => {
    pumps.delete(pump);
  };
}

/** 「再盯一会儿」—— 手势每帧调一次即可（重复调用只是把截止时间往后推）。 */
export function overlayPulse(durationMs = HOLD_MS): void {
  const until = Date.now() + durationMs;
  if (until > holdUntil) holdUntil = until;
  for (const pump of pumps) pump();
}

/** 现在还需要轮询吗（宿主组件的 rAF 循环据此决定这一帧发不发 IPC）。 */
export function overlayLiveWanted(): boolean {
  return Date.now() < holdUntil;
}

/**
 * 写入一份实时变换；**与上一份逐项相同就丢弃**。
 *
 * 丢弃很重要：拖动中每帧都会读一次，而缩放 / 平移期间 `scale` 与 `rotation`
 * 常常没变 —— 不比较就会让覆盖层每帧重建一遍 DOM。
 */
export function publishOverlayLive(next: OverlayLive | null): void {
  const previous = live();
  if (next === null) {
    if (previous !== null) setLive(null);
    return;
  }
  if (
    previous !== null
    && previous.scale === next.scale
    && previous.rotation === next.rotation
    && previous.photoPath === next.photoPath
    && sameMatrix(previous.transform, next.transform)
  ) {
    return;
  }
  setLive(next);
}

function sameMatrix(
  a: OverlayMatrix | null,
  b: OverlayMatrix | null,
): boolean {
  if (a === null || b === null) return a === b;
  for (let index = 0; index < 6; index += 1) {
    if (Math.abs(a[index] - b[index]) > 1e-4) return false;
  }
  return true;
}
