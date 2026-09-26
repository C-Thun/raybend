import { createActionSlot } from "../../lib/action-slot.ts";
import type { FullscreenTarget } from "../../lib/fullscreen-target.ts";
export interface ExportActions {
  viewing(): boolean;
  filmVisible(): boolean;
  fullscreenTarget(): FullscreenTarget | null;
  enqueue(): void;
  remove(): void;
  selectAll(): void;
  requestReset(): void;
}
const slot = createActionSlot<ExportActions>();
export const exportActions = slot.read;
export const registerExportActions = slot.register;
