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
  /**
   * 浏览工作区左列 / 右列的宽度（**像素**，不是比例）。
   *
   * 为什么这两条用像素而上面那条用比例：信息栏与库列表是**侧栏** ——
   * 桌面软件里侧栏宽度是人手动定死的（改窗口大小不该让信息栏跟着变宽窄），
   * 而导入工作区那条左列是「整页的一部分」，跟着窗口缩放更自然。
   */
  browseLeftWidth: number;
  browseRightWidth: number;
}

/**
 * 允许的范围：既给拖拽留余量，也挡住存储里的垃圾值把某一段挤没。
 * （左列另有 `minSize: "220px"` 的像素下限 —— 两个闸门都要有：比例管「拖」，
 * 像素管「窗口特别小时」；缩到很窄的窗口上，220px 会先兜住。）
 */
export const LAYOUT_BOUNDS = {
  leftRatio: { min: 0.12, max: 0.5 },
  recentRatio: { min: 0.15, max: 0.7 },
  /** 侧栏像素下限 220：再窄标签/目录名就只剩省略号；上限 520：别把网格挤没 */
  browseLeftWidth: { min: 220, max: 520 },
  browseRightWidth: { min: 220, max: 520 },
} as const;

export const DEFAULT_LAYOUT: LayoutPrefs = {
  leftRatio: 0.22,
  recentRatio: 0.32,
  // 与 M2-W1 时代的固定宽度一致（两侧各 300），所以默认视觉上什么都没变
  browseLeftWidth: 300,
  browseRightWidth: 300,
};

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
    return { ...fallback };
  }
  const record = raw as Record<string, unknown>;
  return {
    leftRatio: clamp(record.leftRatio, LAYOUT_BOUNDS.leftRatio, fallback.leftRatio),
    recentRatio: clamp(record.recentRatio, LAYOUT_BOUNDS.recentRatio, fallback.recentRatio),
    // 像素宽度取整：半像素宽度在 subpixel 布局下会渗出 1px 的缝
    browseLeftWidth: Math.round(
      clamp(record.browseLeftWidth, LAYOUT_BOUNDS.browseLeftWidth, fallback.browseLeftWidth),
    ),
    browseRightWidth: Math.round(
      clamp(record.browseRightWidth, LAYOUT_BOUNDS.browseRightWidth, fallback.browseRightWidth),
    ),
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
  /** 浏览工作区侧栏宽度（像素）；拖拽**松手**时调它落盘 */
  setBrowseLeftWidth: (width: number) => void;
  setBrowseRightWidth: (width: number) => void;
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

  /**
   * 落盘 + 更新信号。**值没变就什么都不做** —— 这不是省事，是**防回路**：
   * `onResizeEnd` 在窗口缩放/最大化时也会被 Ark 触发，若每次都写信号，
   * 上层就会重渲染 → splitter 收到新的 `defaultSize` → 再触发一次 resize……
   * （真机症状：启动时抖几秒、一最大化就卡死。见 `App.tsx` 的注释。）
   */
  const commit = (next: LayoutPrefs): void => {
    const current = prefs();
    // ⚠️ 四个字段**都要比**：早先只比了前两个，于是「只改侧栏宽度」会被当成没变化吞掉
    //（2026-09-19 加宽度时发现的：改了值、信号不动、界面纹丝不动）。
    if (
      current.leftRatio === next.leftRatio &&
      current.recentRatio === next.recentRatio &&
      current.browseLeftWidth === next.browseLeftWidth &&
      current.browseRightWidth === next.browseRightWidth
    ) {
      return;
    }
    setPrefs(next);
    writeLayout(next, storage);
  };

  return {
    prefs,
    setLeftRatio: (ratio) => commit(sanitizeLayout({ ...prefs(), leftRatio: ratio })),
    setRecentRatio: (ratio) => commit(sanitizeLayout({ ...prefs(), recentRatio: ratio })),
    setBrowseLeftWidth: (width) =>
      commit(sanitizeLayout({ ...prefs(), browseLeftWidth: width })),
    setBrowseRightWidth: (width) =>
      commit(sanitizeLayout({ ...prefs(), browseRightWidth: width })),
  };
}
