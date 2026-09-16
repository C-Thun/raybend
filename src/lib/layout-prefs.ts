/**
 * 布局偏好（**设备级**）——「三点把手」拖出来的比例都记在这里，下次启动还原。
 *
 * 为什么是设备级、为什么放 `localStorage`（与 `appearance.ts` 同一条理由）：
 * 窗口布局取决于**屏幕尺寸**，换台机器本来就该重来一次；而且要在首次渲染前就拿到，
 * 否则会先按默认比例画一帧再跳变（视觉上闪一下）。
 *
 * ## 「左边给缩放、右边不给」是**原则**，不是这一次的偶然
 *
 * 左列（导航 / 来源 / 目录）是可调的：不同人、不同屏幕对它的宽度需求差别很大；
 * 右侧（库 / 检查器）的宽度由内容决定，做成可拖只会带来「越拖越乱」的意外。
 * 所以把手一律加在**左侧**边界上 —— 以后新增工作区沿用这条（`DESIGN.md` §8.6）。
 *
 * 记的是**相对比例**而不是像素：窗口大小变了之后比例仍然对（像素就不对了）。
 */

import { createSignal } from "solid-js";

/** 存储的最小接口（便于测试注入；也兼容 localStorage 被禁用时的静默失败） */
export interface LayoutStorage {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
}

export const LAYOUT_STORAGE_KEY = "raybend.layout.v1";

export interface LayoutPrefs {
  /** 左列宽度占整个工作区的比例（0–1） */
  leftRatio: number;
  /** 左列内部「最近」段的高度比例（0–1；「来源」= 1 − 它） */
  recentRatio: number;
}

/**
 * 允许的范围：既给拖拽留余量，也挡住存储里的垃圾值把某一段挤没。
 * （左列另有 `minSize: "220px"` 的像素下限 —— 两个闸门都要有：比例管「拖」，
 * 像素管「窗口特别小时」；缩到很窄的窗口上，220px 会先兜住。）
 */
export const LAYOUT_BOUNDS = {
  leftRatio: { min: 0.12, max: 0.5 },
  recentRatio: { min: 0.15, max: 0.7 },
} as const;

export const DEFAULT_LAYOUT: LayoutPrefs = { leftRatio: 0.22, recentRatio: 0.32 };

function defaultStorage(): LayoutStorage | undefined {
  try {
    return globalThis.localStorage;
  } catch {
    // 隐私模式 / 禁用存储时访问 localStorage 会抛错，那时就当作没有存储
    return undefined;
  }
}

function clamp(
  value: unknown,
  bounds: { min: number; max: number },
  fallback: number,
): number {
  const numeric = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(numeric)) return fallback;
  return Math.min(bounds.max, Math.max(bounds.min, numeric));
}

/** 解析存下来的值：任何非法输入都落到默认或夹到范围内，**绝不抛错**。 */
export function sanitizeLayout(
  raw: unknown,
  fallback: LayoutPrefs = DEFAULT_LAYOUT,
): LayoutPrefs {
  if (typeof raw !== "object" || raw === null) {
    return { leftRatio: fallback.leftRatio, recentRatio: fallback.recentRatio };
  }
  const record = raw as Record<string, unknown>;
  return {
    leftRatio: clamp(record.leftRatio, LAYOUT_BOUNDS.leftRatio, fallback.leftRatio),
    recentRatio: clamp(record.recentRatio, LAYOUT_BOUNDS.recentRatio, fallback.recentRatio),
  };
}

export function readLayout(
  storage: LayoutStorage | undefined = defaultStorage(),
): LayoutPrefs {
  if (!storage) return { ...DEFAULT_LAYOUT };
  try {
    const raw = storage.getItem(LAYOUT_STORAGE_KEY);
    if (raw === null || raw === "") return { ...DEFAULT_LAYOUT };
    return sanitizeLayout(JSON.parse(raw));
  } catch {
    return { ...DEFAULT_LAYOUT };
  }
}

export function writeLayout(
  prefs: LayoutPrefs,
  storage: LayoutStorage | undefined = defaultStorage(),
): void {
  if (!storage) return;
  try {
    storage.setItem(LAYOUT_STORAGE_KEY, JSON.stringify(prefs));
  } catch {
    // 存储不可用：静默（布局偏好丢了不影响用）
  }
}

export interface LayoutStore {
  prefs: () => LayoutPrefs;
  /** 拖拽**结束**时调用（只记最终比例，中间过程由 Ark 自己管） */
  setLeftRatio: (ratio: number) => void;
  setRecentRatio: (ratio: number) => void;
}

/**
 * 建一个布局偏好店：`prefs()` 给出当前比例，`setXxxRatio()` 写入并落盘。
 *
 * 用法（`ImportWorkspace`）：把 `prefs().leftRatio` 换算成百分比交给 splitter 的
 * `defaultSize`，在 `onResizeEnd` 里调 `setLeftRatio(sizes[0] / 100)`。
 */
export function createLayoutStore(
  deps: { storage?: LayoutStorage | undefined } = {},
): LayoutStore {
  const storage = "storage" in deps ? deps.storage : defaultStorage();
  const [prefs, setPrefs] = createSignal<LayoutPrefs>(readLayout(storage));

  const commit = (next: LayoutPrefs): void => {
    setPrefs(next);
    writeLayout(next, storage);
  };

  return {
    prefs,
    setLeftRatio: (ratio) => commit(sanitizeLayout({ ...prefs(), leftRatio: ratio })),
    setRecentRatio: (ratio) => commit(sanitizeLayout({ ...prefs(), recentRatio: ratio })),
  };
}
