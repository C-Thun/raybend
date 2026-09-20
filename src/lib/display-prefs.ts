/**
 * import / browse 各自的 tiles statusbar 显示偏好（设备级、同步落在 localStorage）。
 *
 * 两个 flow 仍然使用同一个 tiles 组件，但偏好不是同一份：摄影师在导入时常要看
 * 文件名、在库里则可能只看标记；两边的分组与缩放密度也未必一样。
 *
 * 存储升级只从这里走：v1 是一份共享值，v2 把它复制成 import / browse 两份；旧的
 * import 三态信息在迁移后收敛为 off / marks-name（开 = 显示全部，也就是文件名）。
 */

import { createSignal, type Accessor } from "solid-js";

import {
  clampTileStepIndex,
  DEFAULT_TILE_STEP_INDEX,
  migrateTileStepIndex,
  TILE_SIZE_STEPS,
} from "./tile-flow.ts";

export type TileInfoMode = "off" | "marks" | "marks-name";
export type DisplayScope = "import" | "browse";

export interface ScopedDisplayPrefs {
  byTime: boolean;
  infoMode: TileInfoMode;
  /** 0..16 的连续位置；小数表示落在相邻两个预设档之间。 */
  tileStep: number;
}

export interface DisplayPrefs {
  import: ScopedDisplayPrefs;
  browse: ScopedDisplayPrefs;
}

/** 新结构；v1 只用于一次性迁移，迁移后不会再写。 */
export const DISPLAY_STORAGE_KEY = "raybend.display.v2";
export const LEGACY_DISPLAY_STORAGE_KEY = "raybend.display.v1";
export const TILE_STEP_SCALE = TILE_SIZE_STEPS.length;

export const DEFAULT_SCOPED_DISPLAY_PREFS: ScopedDisplayPrefs = {
  byTime: false,
  infoMode: "off",
  tileStep: DEFAULT_TILE_STEP_INDEX,
};

export const DEFAULT_DISPLAY_PREFS: DisplayPrefs = {
  import: { ...DEFAULT_SCOPED_DISPLAY_PREFS },
  browse: { ...DEFAULT_SCOPED_DISPLAY_PREFS },
};

export interface DisplayStorage {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
}

function defaultStorage(): DisplayStorage | undefined {
  try {
    return globalThis.localStorage;
  } catch {
    return undefined;
  }
}

const INFO_MODES: readonly TileInfoMode[] = ["off", "marks", "marks-name"];

function cloneDefaults(): DisplayPrefs {
  return {
    import: { ...DEFAULT_SCOPED_DISPLAY_PREFS },
    browse: { ...DEFAULT_SCOPED_DISPLAY_PREFS },
  };
}

export function sanitizeScopedDisplayPrefs(
  raw: unknown,
  fallback: ScopedDisplayPrefs = DEFAULT_SCOPED_DISPLAY_PREFS,
): ScopedDisplayPrefs {
  if (typeof raw !== "object" || raw === null) return { ...fallback };
  const record = raw as Record<string, unknown>;
  const mode = record.infoMode;
  return {
    byTime: typeof record.byTime === "boolean" ? record.byTime : fallback.byTime,
    infoMode:
      typeof mode === "string" && (INFO_MODES as readonly string[]).includes(mode)
        ? (mode as TileInfoMode)
        : fallback.infoMode,
    tileStep: clampTileStepIndex(
      typeof record.tileStep === "number" && Number.isFinite(record.tileStep)
        ? record.tileStep
        : fallback.tileStep,
    ),
  };
}

export function sanitizeDisplayPrefs(
  raw: unknown,
  fallback: DisplayPrefs = DEFAULT_DISPLAY_PREFS,
): DisplayPrefs {
  if (typeof raw !== "object" || raw === null) {
    return {
      import: { ...fallback.import },
      browse: { ...fallback.browse },
    };
  }
  const record = raw as Record<string, unknown>;
  const imported = sanitizeScopedDisplayPrefs(record.import, fallback.import);
  imported.infoMode = imported.infoMode === "off" ? "off" : "marks-name";
  return {
    import: imported,
    browse: sanitizeScopedDisplayPrefs(record.browse, fallback.browse),
  };
}

/** v1 的共享偏好 → v2 两份偏好（只在读存储时执行一次）。 */
export function migrateLegacyDisplayPrefs(raw: unknown): DisplayPrefs {
  const record = (typeof raw === "object" && raw !== null ? raw : {}) as Record<string, unknown>;
  const freshStep =
    record.tileStepScale === TILE_STEP_SCALE
      ? record.tileStep
      : migrateTileStepIndex(
          typeof record.tileStep === "number" && Number.isFinite(record.tileStep)
            ? record.tileStep
            : DEFAULT_TILE_STEP_INDEX,
        );
  const shared = sanitizeScopedDisplayPrefs({ ...record, tileStep: freshStep });
  return {
    import: {
      ...shared,
      infoMode: shared.infoMode === "off" ? "off" : "marks-name",
    },
    browse: { ...shared },
  };
}

export function readDisplayPrefs(
  storage: DisplayStorage | undefined = defaultStorage(),
): DisplayPrefs {
  if (!storage) return cloneDefaults();
  try {
    const current = storage.getItem(DISPLAY_STORAGE_KEY);
    if (current !== null && current !== "") {
      return sanitizeDisplayPrefs(JSON.parse(current));
    }
    const legacy = storage.getItem(LEGACY_DISPLAY_STORAGE_KEY);
    if (legacy === null || legacy === "") return cloneDefaults();
    const migrated = migrateLegacyDisplayPrefs(JSON.parse(legacy));
    writeDisplayPrefs(migrated, storage);
    return migrated;
  } catch {
    return cloneDefaults();
  }
}

export function writeDisplayPrefs(
  prefs: DisplayPrefs,
  storage: DisplayStorage | undefined = defaultStorage(),
): void {
  if (!storage) return;
  try {
    storage.setItem(
      DISPLAY_STORAGE_KEY,
      JSON.stringify({ version: 2, tileStepScale: TILE_STEP_SCALE, ...prefs }),
    );
  } catch {
    // 偏好不可写不影响主流程。
  }
}

const [prefs, setPrefs] = createSignal<DisplayPrefs>(readDisplayPrefs());

function patchScope(scope: DisplayScope, next: Partial<ScopedDisplayPrefs>, persist: boolean): void {
  const current = prefs();
  const before = current[scope];
  const after = sanitizeScopedDisplayPrefs({ ...before, ...next }, before);
  if (scope === "import") after.infoMode = after.infoMode === "off" ? "off" : "marks-name";
  if (
    before.byTime === after.byTime &&
    before.infoMode === after.infoMode &&
    before.tileStep === after.tileStep
  ) return;
  const merged = { ...current, [scope]: after };
  setPrefs(merged);
  if (persist) writeDisplayPrefs(merged);
}

export const displayPrefs: Accessor<DisplayPrefs> = prefs;
export const importDisplayPrefs: Accessor<ScopedDisplayPrefs> = () => prefs().import;
export const browseDisplayPrefs: Accessor<ScopedDisplayPrefs> = () => prefs().browse;

export const importDisplayByTime: Accessor<boolean> = () => prefs().import.byTime;
export const importDisplayInfoMode: Accessor<TileInfoMode> = () => prefs().import.infoMode;
export const importDisplayTileStep: Accessor<number> = () => prefs().import.tileStep;
export const browseDisplayByTime: Accessor<boolean> = () => prefs().browse.byTime;
export const browseDisplayInfoMode: Accessor<TileInfoMode> = () => prefs().browse.infoMode;
export const browseDisplayTileStep: Accessor<number> = () => prefs().browse.tileStep;

export const setImportDisplayByTime = (value: boolean): void =>
  patchScope("import", { byTime: value }, true);
export const setImportDisplayInfoMode = (value: TileInfoMode): void =>
  patchScope("import", { infoMode: value }, true);
export const setImportDisplayTileStep = (value: number): void =>
  patchScope("import", { tileStep: value }, false);
export const commitImportDisplayTileStep = (): void => writeDisplayPrefs(prefs());

export const setBrowseDisplayByTime = (value: boolean): void =>
  patchScope("browse", { byTime: value }, true);
export const setBrowseDisplayInfoMode = (value: TileInfoMode): void =>
  patchScope("browse", { infoMode: value }, true);
export const setBrowseDisplayTileStep = (value: number): void =>
  patchScope("browse", { tileStep: value }, false);
export const commitBrowseDisplayTileStep = (): void => writeDisplayPrefs(prefs());

export function resetDisplayPrefsForTests(next: DisplayPrefs = DEFAULT_DISPLAY_PREFS): void {
  setPrefs({
    import: { ...next.import },
    browse: { ...next.browse },
  });
}
