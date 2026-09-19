/**
 * 命令模块的对外出口（`ARCHITECTURE.md` §2：别的层只能从这里拿东西）。
 */

export { createCommandRegistry, type CommandDeps } from "./catalog.ts";
export { createCommandDispatcher, type CommandDispatcher } from "./dispatcher.ts";
export { CommandPalette, type CommandPaletteProps } from "./CommandPalette.tsx";
export { ShortcutSettingsDialog, type ShortcutSettingsDialogProps } from "./ShortcutSettingsDialog.tsx";
export { issueText, problemText } from "./messages.ts";
