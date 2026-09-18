/**
 * 渲染 spike 的 IPC 封装（`src/` 里只有本目录可以直接 `import invoke`，见 `ARCHITECTURE.md` §1.1）。
 *
 * ⚠️ **这里的类型不进 `dto-contract.json`**：契约那套是给**产品**的 IPC 用的
 * （防的是产品功能静默漂移）。spike 是开发期诊断工具，`?spike=1` 才会加载，
 * 而且它的生命周期到「A.2 结论落定」为止 —— 为它维护一份契约表不值当。
 * 真有一天它变成产品功能（比如查看器），那时再搬进契约。
 */

import { isTauriRuntime } from "./tauri-env.ts";

/** 缓存的 `@tauri-apps/api/core` 模块（浏览器里根本不会加载它）。 */
let coreModule: Promise<typeof import("@tauri-apps/api/core")> | undefined;

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  coreModule ??= import("@tauri-apps/api/core");
  return coreModule.then((core) => core.invoke<T>(cmd, args));
}

/** 前端发去的交互意图（只有意图，没有坐标数学 —— `AGENTS.md` §6.1 红线 #1）。 */
export type SpikeCommand =
  | { kind: "zoom"; x: number; y: number; factor: number }
  | { kind: "hitTest"; x: number; y: number }
  | { kind: "pan"; dx: number; dy: number }
  | { kind: "fit"; mode: "fit" | "fill" | "oneToOne" | "free" }
  | { kind: "rotate"; degrees: number }
  | { kind: "setHole"; on: boolean }
  | { kind: "holeRect"; x: number; y: number; width: number; height: number }
  | { kind: "webviewOrigin"; screenX: number; screenY: number; dpr: number }
  | { kind: "reset" }
  | { kind: "scripted"; name: string; seconds: number }
  | { kind: "scenario"; name: string | null }
  | { kind: "deviceLoss" }
  | { kind: "recover" }
  | { kind: "resize"; width: number; height: number; dpr: number };

export interface AdapterInfo {
  backend: string;
  name: string;
  deviceType: string;
  driver: string;
  driverInfo: string;
}

export interface ViewportView {
  zoom: number;
  panX: number;
  panY: number;
  rotation: number;
  fitMode: string;
  holeCss: [number, number, number, number] | null;
  /** 洞口的物理像素矩形（Rust 实际用来摆图/裁剪的那个）—— 与 CSS 值一比就知道单位对不对 */
  holePhysical: [number, number, number, number] | null;
  dpr: number;
  imageWidth: number;
  imageHeight: number;
}

export interface ScenarioView {
  name: string;
  how: string;
  frames: number;
  p50Ms: number | null;
  p95Ms: number | null;
  fps: number | null;
  cpuP50Ms: number | null;
  overBudget: number;
}

export interface HitView {
  imageX: number;
  imageY: number;
  inside: boolean;
  atCenter: boolean;
}

export interface SpikeSnapshot {
  open: boolean;
  adapter: AdapterInfo;
  format: string;
  alphaMode: string;
  surfaceSize: [number, number];
  cssSize: [number, number];
  monitorScale: number;
  monitorName: string;
  maximized: boolean;
  fullscreen: boolean;
  decorated: boolean;
  transparent: boolean;
  uploadMs: number;
  rendered: number;
  viewport: ViewportView;
  scenarios: ScenarioView[];
  deviceLost: string[];
  coordMaxError: number;
  lastHit: HitView | null;
  lastFrameMs: number;
  lastCpuMs: number;
  lastError: string | null;
  viewportProblems: string[];
  /** 客户区在屏幕上的原点（物理像素） */
  clientOrigin: [number, number];
  /** 窗口（含边框）在屏幕上的原点（物理像素） */
  windowOrigin: [number, number];
  /** webview 在屏幕上的原点（物理像素，= `screenX × dpr`） */
  webviewOrigin: [number, number] | null;
  /**
   * `webview 原点 − 客户区原点`（物理像素）。
   * **非零就是「图看着对、鼠标读出的坐标却差一截」的根因** —— 输入的 CSS 坐标
   * 与 wgpu 表面的坐标系差这一截。
   */
  inputOffset: [number, number] | null;
}

/** 人填的那几项（报告里唯一不是程序算出来的部分）。 */
export interface HumanNotes {
  transparencyOk: boolean | null;
  oneToOneSharp: boolean | null;
  acrossMonitorsOk: boolean | null;
  feel: string;
  notes: string;
}

const EMPTY: SpikeSnapshot = {
  open: false,
  adapter: { backend: "", name: "", deviceType: "", driver: "", driverInfo: "" },
  format: "",
  alphaMode: "",
  surfaceSize: [0, 0],
  cssSize: [0, 0],
  monitorScale: 1,
  monitorName: "",
  maximized: false,
  fullscreen: false,
  decorated: false,
  transparent: false,
  uploadMs: 0,
  rendered: 0,
  viewport: {
    zoom: 1,
    panX: 0,
    panY: 0,
    rotation: 0,
    fitMode: "",
    holeCss: null,
    holePhysical: null,
    dpr: 1,
    imageWidth: 0,
    imageHeight: 0,
  },
  scenarios: [],
  deviceLost: [],
  coordMaxError: 0,
  lastHit: null,
  lastFrameMs: 0,
  lastCpuMs: 0,
  lastError: null,
  clientOrigin: [0, 0],
  windowOrigin: [0, 0],
  webviewOrigin: null,
  inputOffset: null,
  viewportProblems: [],
};

/** 浏览器里（`pnpm dev` 直接打开本页）没有后端：返回空状态而不是抛错。 */
export async function spikeEmpty(): Promise<SpikeSnapshot> {
  return EMPTY;
}

export async function spikeOpen(): Promise<SpikeSnapshot> {
  if (!isTauriRuntime()) return EMPTY;
  return call<SpikeSnapshot>("spike_open");
}

export async function spikeClose(): Promise<void> {
  if (!isTauriRuntime()) return;
  await call<void>("spike_close");
}

export async function spikeCommand(command: SpikeCommand): Promise<SpikeSnapshot> {
  if (!isTauriRuntime()) return EMPTY;
  return call<SpikeSnapshot>("spike_command", { command });
}

export async function spikeSnapshot(): Promise<SpikeSnapshot> {
  if (!isTauriRuntime()) return EMPTY;
  return call<SpikeSnapshot>("spike_snapshot");
}

export async function spikeWriteReport(
  dir: string,
  notes: HumanNotes,
): Promise<string[]> {
  if (!isTauriRuntime()) return [];
  return call<string[]>("spike_write_report", { dir, notes });
}
