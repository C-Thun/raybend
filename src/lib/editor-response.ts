/**
 * 编辑调节的**响应率**偏好（设备级，`localStorage`）。
 *
 * 背景（崔总 2026-10-09）：拉杆拖动时的画面更新率 = 「前端发送频率」与「后端重算耗时」
 * 里更慢的那一个。前端本来**没有**防抖/限流（帧级合并 + 后端「最新者优先」），
 * 在重算很重的大图上就表现为「等了半天才动一下」。这个开关给两档：
 *
 *   * **高**（默认）：两次发送之间至少 `100ms`；
 *   * **低**：至少 `500ms`（弱机 / 超大图，牺牲跟手换稳定）。
 *
 * 不论哪档，**松手那一次必然发出**（`createLatestCoalescer` 的 `flush()` 无视限流）——
 * 「松开鼠标必然触发一次计算」是硬要求，限流只压拖动期间。
 *
 * 为什么存 `localStorage` 而不是 `app.db`：它是「这台机器扛不扛得住实时重算」的设备事实，
 * 与外观偏好同一族；而且它在拖动热路径上被读取，同步读比 IPC 合适。
 * key 是首版、没有历史包袱，所以不带版本号（将来改语义时按 §2.16 做带版本的新 key + 一次性迁移）。
 */

import { createSignal } from "solid-js";

export type EditorResponseRate = "high" | "low";

export const DEFAULT_EDITOR_RESPONSE_RATE: EditorResponseRate = "high";

/** 两档的限流间隔（ms）—— 数字来自崔总 2026-10-09 的口径，改这里就是改产品行为 */
export const EDITOR_RESPONSE_INTERVAL_MS: Readonly<Record<EditorResponseRate, number>> = {
  high: 100,
  low: 500,
};

export const EDITOR_RESPONSE_STORAGE_KEY = "raybend.editor.response_rate";

/** 存储的最小接口（便于测试注入；也兼容存储被禁用时的静默失败） */
export interface EditorResponseStorage {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
}

function defaultStorage(): EditorResponseStorage | undefined {
  try {
    return globalThis.localStorage;
  } catch {
    // 隐私模式 / 禁用存储时访问 localStorage 会抛错，那时就当作没有存储
    return undefined;
  }
}

/** 非法 / 缺失值一律回落到默认值 —— 存储里的垃圾不能让编辑器起不来 */
export function normalizeEditorResponseRate(value: unknown): EditorResponseRate {
  return value === "high" || value === "low"
    ? value
    : DEFAULT_EDITOR_RESPONSE_RATE;
}

export function readEditorResponseRate(
  storage: EditorResponseStorage | undefined = defaultStorage(),
): EditorResponseRate {
  let raw: string | null = null;
  try {
    raw = storage?.getItem(EDITOR_RESPONSE_STORAGE_KEY) ?? null;
  } catch {
    raw = null;
  }
  return normalizeEditorResponseRate(raw);
}

export function writeEditorResponseRate(
  rate: EditorResponseRate,
  storage: EditorResponseStorage | undefined = defaultStorage(),
): void {
  try {
    storage?.setItem(EDITOR_RESPONSE_STORAGE_KEY, rate);
  } catch {
    // 存不下就算了：本次会话里设置仍然有效，只是下次启动回到默认档
  }
}

export interface EditorResponseStore {
  rate: () => EditorResponseRate;
  /** 当前的限流间隔（ms）—— `EditorWorkspace` 的参数发送器读它 */
  intervalMs: () => number;
  setRate: (rate: EditorResponseRate) => void;
}

export function createEditorResponseStore(
  storage: EditorResponseStorage | undefined = defaultStorage(),
): EditorResponseStore {
  const [rate, setRateSignal] = createSignal<EditorResponseRate>(
    readEditorResponseRate(storage),
  );
  return {
    rate,
    intervalMs: () => EDITOR_RESPONSE_INTERVAL_MS[rate()],
    setRate: (next) => {
      setRateSignal(next);
      writeEditorResponseRate(next, storage);
    },
  };
}
