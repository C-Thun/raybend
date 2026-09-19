/**
 * **网格显示偏好**（设备级、import 与 browse 共用一份、下次启动还原）。
 *
 * 三件事：
 *
 * | 偏好 | 是什么 | 谁在读 |
 * | --- | --- | --- |
 * | `byTime` | tiles 是否**按时间分组** | 两个工作区的网格（与状态条上的「按时间」开关） |
 * | `infoMode` | 图片**信息档位**（无 / 标记 / 标记+文件名） | `components/ui/tile-info.ts` 与 `Tile` |
 * | `tileStep` | 格子尺寸档位（滑块） | 两个工作区的网格 + 状态条 |
 *
 * ## 为什么要有这个模块（而不是各存各的）
 *
 * 三者都是「**怎么看**」而不是「看什么」，而且人在 import 与 browse 里看的是**同一批 tiles** ——
 * 切工作流不该变样（人类 2026-09-20：「按时间需要在 import 和 browse 之间都要支持持久化，
 * 信息显示级别也要持久化」）。此前 `by_time` / `tile_step` 是各工作区自己读 `app.db`
 * 的设置，于是**浏览侧每次进都重置**（它的那两份状态压根没落盘）；
 * 信息档位则只是一个内存单例，关掉软件就忘了。
 *
 * ## 为什么用 `localStorage` 而不是 `app.db`
 *
 * 与 `appearance.ts` / `layout-prefs.ts` 同一条理由：设备级偏好，且**要在首次渲染前拿到** ——
 * 异步读库会先按默认值画一帧再跳变（分组一动就是整屏重排，跳变很明显）。
 * 存储任何非法输入都落回默认（`sanitizeDisplayPrefs`），读写都**不抛错**。
 *
 * ## 一份状态、两处读
 *
 * 这里是**模块级单例信号**（与 `tile-info.ts` / `appearance` 一样）：
 * 两个工作区、状态条、`Tile` 拿到的都是同一个值，谁改都立刻反映到另一边 ——
 * 不需要任何跨工作区的同步代码（那正是以前两边会漂移的原因）。
 */

import { createSignal, type Accessor } from "solid-js";

import { clampTileStepIndex, DEFAULT_TILE_STEP_INDEX } from "./tile-flow.ts";

/** 「信息」档位（`Tile` 与状态条上的 `i` 开关共用这一份类型） */
export type TileInfoMode = "off" | "marks" | "marks-name";

export interface DisplayPrefs {
  /** tiles 按时间分组 */
  byTime: boolean;
  /** 图片信息档位 */
  infoMode: TileInfoMode;
  /** 格子尺寸档位索引（0..8，见 `lib/tile-flow.ts`） */
  tileStep: number;
}

export const DISPLAY_STORAGE_KEY = "raybend.display.v1";

export const DEFAULT_DISPLAY_PREFS: DisplayPrefs = {
  byTime: false,
  infoMode: "off",
  tileStep: DEFAULT_TILE_STEP_INDEX,
};

/** 存储的最小接口（便于测试注入；也兼容 localStorage 被禁用时的静默失败） */
export interface DisplayStorage {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
}

function defaultStorage(): DisplayStorage | undefined {
  try {
    return globalThis.localStorage;
  } catch {
    // 隐私模式 / 禁用存储时访问 localStorage 会抛错，那时就当作没有存储
    return undefined;
  }
}

const INFO_MODES: readonly TileInfoMode[] = ["off", "marks", "marks-name"];

/** 解析存下来的值：任何非法输入都落到默认，**绝不抛错**。 */
export function sanitizeDisplayPrefs(
  raw: unknown,
  fallback: DisplayPrefs = DEFAULT_DISPLAY_PREFS,
): DisplayPrefs {
  if (typeof raw !== "object" || raw === null) return { ...fallback };
  const record = raw as Record<string, unknown>;
  const mode = record.infoMode;
  return {
    byTime: typeof record.byTime === "boolean" ? record.byTime : fallback.byTime,
    infoMode:
      typeof mode === "string" && (INFO_MODES as readonly string[]).includes(mode)
        ? (mode as TileInfoMode)
        : fallback.infoMode,
    // 档位越界/非数字都夹回有效档（与滑块那条闸门同一实现）
    tileStep: clampTileStepIndex(
      typeof record.tileStep === "number" && Number.isFinite(record.tileStep)
        ? record.tileStep
        : fallback.tileStep,
    ),
  };
}

export function readDisplayPrefs(
  storage: DisplayStorage | undefined = defaultStorage(),
): DisplayPrefs {
  if (!storage) return { ...DEFAULT_DISPLAY_PREFS };
  try {
    const raw = storage.getItem(DISPLAY_STORAGE_KEY);
    if (raw === null || raw === "") return { ...DEFAULT_DISPLAY_PREFS };
    return sanitizeDisplayPrefs(JSON.parse(raw));
  } catch {
    return { ...DEFAULT_DISPLAY_PREFS };
  }
}

export function writeDisplayPrefs(
  prefs: DisplayPrefs,
  storage: DisplayStorage | undefined = defaultStorage(),
): void {
  if (!storage) return;
  try {
    storage.setItem(DISPLAY_STORAGE_KEY, JSON.stringify(prefs));
  } catch {
    // 存储不可用：静默（偏好丢了不影响用）
  }
}

/**
 * 单例状态。**改值就落盘** —— 这里没有「拖拽中不写」的问题：
 * 格子尺寸那条有（拖一次几十次），所以它走 `setTileStep`（只改内存）
 * + `commitTileStep`（松手时调一次，见 `features/photo-grid/store.ts`）。
 */
const [prefs, setPrefs] = createSignal<DisplayPrefs>(readDisplayPrefs());

function patch(next: Partial<DisplayPrefs>, persist: boolean): void {
  const current = prefs();
  const merged = sanitizeDisplayPrefs({ ...current, ...next }, current);
  if (
    merged.byTime === current.byTime &&
    merged.infoMode === current.infoMode &&
    merged.tileStep === current.tileStep
  ) {
    return;
  }
  setPrefs(merged);
  if (persist) writeDisplayPrefs(merged);
}

export const displayPrefs: Accessor<DisplayPrefs> = prefs;
export const displayByTime: Accessor<boolean> = () => prefs().byTime;
export const displayInfoMode: Accessor<TileInfoMode> = () => prefs().infoMode;
export const displayTileStep: Accessor<number> = () => prefs().tileStep;

/** 「按时间」开关：立刻落盘（一次点击就是一个终值） */
export function setDisplayByTime(value: boolean): void {
  patch({ byTime: value }, true);
}

/** 信息档位：立刻落盘 */
export function setDisplayInfoMode(value: TileInfoMode): void {
  patch({ infoMode: value }, true);
}

/** 格子尺寸档位：**只改内存**（拖拽中每动一格都调它） */
export function setDisplayTileStep(value: number): void {
  patch({ tileStep: value }, false);
}

/** 把当前格子尺寸落盘（滑块松手时调一次） */
export function commitDisplayTileStep(): void {
  writeDisplayPrefs(prefs());
}

/**
 * **仅测试用**：把单例状态复位。
 *
 * 这是模块级单例（两个工作区共用一份），Node 里同一个测试文件共享同一个模块实例 ——
 * 改了档位/开关的用例必须能把它放回去，否则后面的用例会看到别人留下的状态。
 */
export function resetDisplayPrefsForTests(
  next: DisplayPrefs = DEFAULT_DISPLAY_PREFS,
): void {
  setPrefs({ ...next });
}
