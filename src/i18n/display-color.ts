import type { MessageKey } from "./index.ts";

export interface DisplayColorCopy { title: MessageKey; detail: MessageKey; warning: boolean }
export const displayColorMessages = {
  pending: {title:"color.display.pending",detail:"color.display.pendingDetail",warning:false},
  inactive: {title:"color.display.inactive",detail:"color.display.inactiveDetail",warning:false},
  error: {title:"color.display.error",detail:"color.display.errorDetail",warning:true},
  icc: {title:"color.display.icc",detail:"color.display.iccDetail",warning:false},
  scRgb: {title:"color.display.scRgb",detail:"color.display.scRgbDetail",warning:false},
  systemSrgb: {title:"color.display.systemSrgb",detail:"color.display.systemSrgbDetail",warning:true},
  fallback: {title:"color.display.fallback",detail:"color.display.fallbackDetail",warning:true},
  preparationFailed: {title:"color.display.fallback",detail:"color.display.preparationFailedDetail",warning:true},
  unavailable: {title:"color.display.unavailable",detail:"color.display.unavailableDetail",warning:true},
} as const satisfies Record<string,DisplayColorCopy>;
export function displayColorCopy(mode: keyof typeof displayColorMessages): DisplayColorCopy { return displayColorMessages[mode]; }
export function displayDetectionKey(kind: string | undefined): MessageKey {
  switch (kind) {
    case "icc": return "settings.display.icc";
    case "systemManaged": return "settings.display.systemManaged";
    case "srgbFallback": return "settings.display.fallback";
    default: return "settings.display.unavailable";
  }
}
