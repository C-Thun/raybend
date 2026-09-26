import { createSignal } from "solid-js";
import { clampTileStepIndex, tilePositionForSize, tileSizeAt } from "./tile-flow.ts";
export const EXPORT_TOP_SIZE_BOUNDS = { min: 240, max: 320 } as const;
export const EXPORT_PREFS_KEY = "raybend.export-display.v3";
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
      const v2 = storage?.getItem("raybend.export-display.v2") ?? null;
      const old = v2 ?? storage?.getItem("raybend.export-display.v1") ?? null;
      if (old !== null) {
        const previous = JSON.parse(old) as Partial<ExportDisplay>;
        const migrated = readExportDisplay(old);
        migrated.topStep = tilePositionForSize(tileSizeAt(typeof previous.topStep === "number" ? previous.topStep : 8), EXPORT_TOP_SIZE_BOUNDS);
        migrated.scope = v2 === null ? "all" : readExportDisplay(old).scope;
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
