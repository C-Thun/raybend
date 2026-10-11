import { createSignal } from "solid-js";
import { clampTileStepIndex, tilePositionForSize, tileSizeAt } from "./tile-flow.ts";
/**
 * 导出**上部**网格的边长边界（崔总 2026-10-11：下限 240 → **192**，−20%）。
 *
 * 为什么缩：中列 920（设计值）时旧下限下 `4×240 + 3×3 = 969 > 920` —— 永远差一列；
 * 新下限下 `4×226 + 3×3 = 913 ≤ 920`，四列可达。档位取 192…320 共 10 档。
 */
export const EXPORT_TOP_SIZE_BOUNDS = { min: 192, max: 320 } as const;
/** 迁移用：v3 及更早的档位是在这组边界下存的 */
export const LEGACY_EXPORT_TOP_SIZE_BOUNDS = { min: 240, max: 320 } as const;
export const EXPORT_PREFS_KEY = "raybend.export-display.v4";
const LEGACY_KEYS = ["raybend.export-display.v3", "raybend.export-display.v2", "raybend.export-display.v1"] as const;
export interface ExportDisplay {
  topStep: number;
  queueStep: number;
  grouped: boolean;
  scope: "all" | "edited" | "issues";
  info: "off" | "marks" | "marks-name";
  ratio: number;
  selectedPreset: string | null;
  queueList: boolean;
}
export const DEFAULT_EXPORT_DISPLAY: ExportDisplay = {
  topStep: 0,
  queueStep: 1,
  grouped: false,
  scope: "all",
  info: "off",
  ratio: 0.58,
  selectedPreset: null,
  queueList: false,
};
export function readExportDisplay(raw: string | null): ExportDisplay {
  let p: Partial<ExportDisplay> = {};
  try {
    const parsed: unknown = JSON.parse(raw ?? "null");
    if (typeof parsed === "object" && parsed !== null && !Array.isArray(parsed))
      p = parsed;
  } catch {
    /* 损坏偏好落默认 */
  }
  const step = (v: unknown, fallback: number) =>
    typeof v === "number" && Number.isFinite(v)
      ? Math.min(16, Math.max(0, v))
      : fallback;
  return {
    topStep: clampTileStepIndex(typeof p.topStep === "number" ? p.topStep : 0, EXPORT_TOP_SIZE_BOUNDS),
    queueStep: step(p.queueStep, 1),
    grouped: p.grouped === true,
    scope: ["all", "edited", "issues"].includes(p.scope ?? "") ? p.scope! : "all",
    info: ["off", "marks", "marks-name"].includes(p.info ?? "")
      ? p.info!
      : "off",
    ratio:
      typeof p.ratio === "number" && Number.isFinite(p.ratio)
        ? Math.max(0.2, Math.min(0.8, p.ratio))
        : 0.58,
    queueList: p.queueList === true,
    selectedPreset:
      typeof p.selectedPreset === "string" ? p.selectedPreset : null,
  };
}
export function createExportPreferences(
  storage?: Pick<Storage, "getItem" | "setItem">,
) {
  if (storage === undefined) {
    try {
      storage = globalThis.localStorage;
    } catch {
      /* 禁用 localStorage */
    }
  }
  let raw: string | null = null;
  try {
    raw = storage?.getItem(EXPORT_PREFS_KEY) ?? null;
    if (raw === null) {
      /*
       * 旧 key 的一次性迁移（`AGENTS.md` §2.16：版本化 key + 显式迁移）。
       *
       * ⚠ 档位是**下标**，而 v4 的 bounds 下限变了（240 → 192）：同一个下标含义就变了。
       * 所以一律**按尺寸**回填（旧下标 → 旧 bounds 下的尺寸 → 在新 bounds 里的位置）——
       * 老用户的上部尺寸不会被静默顶小，想拿四列自己往左拖一档（240 → 226）。
       */
      const legacyKey = LEGACY_KEYS.find((key) => storage?.getItem(key) != null);
      const old = legacyKey === undefined ? null : storage?.getItem(legacyKey) ?? null;
      if (old !== null && legacyKey !== undefined) {
        const previous = JSON.parse(old) as Partial<ExportDisplay>;
        const migrated = readExportDisplay(old);
        const size = tileSizeAt(
          typeof previous.topStep === "number" ? previous.topStep : 0,
          legacyKey === "raybend.export-display.v3" ? LEGACY_EXPORT_TOP_SIZE_BOUNDS : undefined,
        );
        migrated.topStep = tilePositionForSize(size, EXPORT_TOP_SIZE_BOUNDS);
        // v1 的 scope 是旧的 `sooc` 档（v2 起才是 all/edited/issues）
        if (legacyKey === "raybend.export-display.v1") migrated.scope = "all";
        raw = JSON.stringify(migrated);
        storage?.setItem(EXPORT_PREFS_KEY, raw);
      }
    }
  } catch {
    /* 无读权限 */
  }
  const [value, setValue] = createSignal(readExportDisplay(raw));
  return {
    value,
    update(patch: Partial<ExportDisplay>, commit = true): void {
      setValue((previous) =>
        readExportDisplay(JSON.stringify({ ...previous, ...patch })),
      );
      if (commit) this.commit();
    },
    commit(): void {
      try {
        storage?.setItem(EXPORT_PREFS_KEY, JSON.stringify(value()));
      } catch {
        /* 设备偏好不影响工作 */
      }
    },
  };
}
