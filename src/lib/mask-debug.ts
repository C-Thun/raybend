/**
 * **控制点容差映射的调试参数**（`dev/MaskDebugDialog`，只在 DEV 构建里能改）。
 *
 * ```text
 *   容差 100  ───────────────────────────────  容差 1000
 *      │                                              │
 *      ├ 亮度轴：完全容差 core / 过渡终点 end            │
 *      └ 色相轴：完全容差 core / 过渡终点 end            │
 * ```
 *
 * 权重 = `band_luma(|ΔY′|) × band_chroma(‖Δ(Cb,Cr)‖)`（Rust `develop/areas.rs`）。
 * 这里存的八个数是**每个端点的值**，中间线性插值 —— 就是 Rust `PointWeightConfig` 的形状。
 *
 * 两条纪律：
 * 1. **只在开发期生效**：生产构建里没有入口（`import.meta.env.DEV`），载荷里的
 *    `maskDebug` 永远是 `undefined`，Rust 用固化常量；
 * 2. **白线长度是纯界面参数**，不进 Rust（它只决定那条线画多长）。
 */

import { createSignal } from "solid-js";

import { DEV_BUILD } from "./dev-build.ts";

/** 与 Rust `develop::PointWeightConfig` 逐字对应（camelCase 上送）。 */
export interface MaskWeightConfig {
  lumaCore: [number, number];
  lumaEnd: [number, number];
  chromaCore: [number, number];
  chromaEnd: [number, number];
}

/** 调试面板的完整状态 = 权重参数 + 两个纯界面参数。 */
export interface MaskDebugState extends MaskWeightConfig {
  /** 白线最短（容差 100 时，屏幕像素） */
  lineMin: number;
  /** 白线最长（容差 1000 时） */
  lineMax: number;
}

/** 白线长度的默认端点（`AGENTS.md`：屏幕像素，与缩放无关）。 */
export const MASK_LINE_DEFAULT = Object.freeze({ min: 32, max: 320 });

/** 存储键（设备级偏好；进的是 `localStorage`，不进 profile / XMD / 撤销）。 */
const STORAGE_KEY = "raybend.mask-debug.v1";

// `node --test` 没有 Vite 注入的 `import.meta.env`；判定集中在 `lib/dev-build.ts`
// （`pnpm dev` 与 `pnpm debug:win` 都要有面板，只有会发出去的构建没有）。
const IS_DEV = DEV_BUILD;

// 生产构建**连读都不读**：否则同一台机器上 DEV 留下的 localStorage 值
// 会被生产包拿去改渲染 / 改线长（2026-11-01 评审发现）。
const [state, setState] = createSignal<MaskDebugState | null>(
  IS_DEV ? readStored() : null,
);

/** 当前调试参数（`null` = 没有覆盖，Rust 用固化常量）。 */
export function maskDebug(): MaskDebugState | null {
  return state();
}

/** 写入 / 清除调试参数（`null` = 恢复固化常量）。会落盘，重启后还在。 */
export function setMaskDebug(next: MaskDebugState | null): void {
  if (!IS_DEV) return;
  setState(next);
  try {
    if (next === null) window.localStorage.removeItem(STORAGE_KEY);
    else window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
  } catch {
    // 写不进去不影响本次会话（调参是开发期的事）
  }
}

/** 载荷里要带的那一份（去掉两个纯界面参数）。 */
export function maskWeightPayload(): MaskWeightConfig | null {
  if (!IS_DEV) return null;
  const current = state();
  if (current === null) return null;
  const { lumaCore, lumaEnd, chromaCore, chromaEnd } = current;
  return { lumaCore, lumaEnd, chromaCore, chromaEnd };
}

/** 白线长度的当前端点（没有调试值时用默认）。 */
export function maskLineRange(): { min: number; max: number } {
  if (!IS_DEV) return { ...MASK_LINE_DEFAULT };
  const current = state();
  return {
    min: current?.lineMin ?? MASK_LINE_DEFAULT.min,
    max: current?.lineMax ?? MASK_LINE_DEFAULT.max,
  };
}

function readStored(): MaskDebugState | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (raw === null) return null;
    const parsed: unknown = JSON.parse(raw);
    return isState(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function isState(value: unknown): value is MaskDebugState {
  if (typeof value !== "object" || value === null) return false;
  const record = value as Record<string, unknown>;
  const pair = (key: string): boolean => {
    const entry = record[key];
    return Array.isArray(entry) && entry.length === 2
      && entry.every((item) => typeof item === "number" && Number.isFinite(item));
  };
  const scalar = (key: string): boolean =>
    typeof record[key] === "number" && Number.isFinite(record[key] as number);
  return pair("lumaCore") && pair("lumaEnd") && pair("chromaCore") && pair("chromaEnd")
    && scalar("lineMin") && scalar("lineMax");
}
