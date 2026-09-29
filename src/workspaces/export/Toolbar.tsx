import {
  IconEraser,
  IconPlayerStop,
  IconPlaylistAdd,
  IconPlaylistX,
  IconSquareOff,
  IconVersions,
} from "@tabler/icons-solidjs";
import { Button } from "../../components/ui/Button.tsx";
import { ToolsSeparator } from "../../components/ui/ToolsSeparator.tsx";
import { t } from "../../i18n/index.ts";
import type { ExportStore } from "./store.ts";
import { exportActions } from "./actions.ts";
/*
 * toolsbar 的文字按钮**一律带图标**（崔总 2026-09-28；规范在 `memory/DESIGN.md` §12.12）：
 * 图标 14px、间距走 `Button` 内置的 gap（md=1.5），不要自己另写间距。
 */
export function ExportScopeTool(props: { store: ExportStore }) {
  return (
    <Button icon={<IconVersions size={14} />} onClick={() => props.store.cycleScope()}>
      {t(
        props.store.preferences.value().scope === "all"
          ? "export.scope.all"
          : props.store.preferences.value().scope === "edited"
            ? "export.scope.edited"
            : "export.scope.issues",
      )}
    </Button>
  );
}
export function ExportToolbar(props: { store: ExportStore }) {
  /*
   * 分组（崔总 2026-09-28 的 toolsbar 分组规矩，唯一事实源 `memory/DESIGN.md` §12.12）：
   * [送入队列][移出队列]（队列动作，组内紧）｜[取消选中]（选择动作）｜[重置所有队列]
   * （全局且带确认 —— 自己一组，和前面的隔开）。
   */
  return (
    <div class="flex items-center">
      <div class="flex items-center gap-0.5">
        <Button
          icon={<IconPlaylistAdd size={14} />}
          disabled={!props.store.canEnqueue()}
          onClick={() => exportActions()?.enqueue()}
        >
          {t("export.enqueue")}
        </Button>
        <Button icon={<IconPlaylistX size={14} />} disabled={!props.store.canRemove()} onClick={props.store.removeSelected}>{t("export.remove")}</Button>
      </div>
      <ToolsSeparator />
      <Button
        icon={<IconSquareOff size={14} />}
        disabled={props.store.activeSelection().ids.size === 0}
        onClick={() => props.store.clear()}
      >
        {t("export.clear")}
      </Button>
      <ToolsSeparator />
      <Button
        icon={<IconEraser size={14} />}
        disabled={![...props.store.queues().values()].some((q) => q.length > 0)}
        onClick={() => exportActions()?.requestReset()}
      >
        {t("export.reset")}
      </Button>
    </div>
  );
}
export function ExportStopTool(props: { store: ExportStore }) {
  return (
    <Button
      icon={<IconPlayerStop size={14} />}
      disabled={props.store.enabled().size === 0}
      onClick={() => props.store.stopAll()}
    >
      {t("export.stopAll")}
    </Button>
  );
}
