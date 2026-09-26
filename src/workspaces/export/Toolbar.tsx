import { Button } from "../../components/ui/Button.tsx";
import { t } from "../../i18n/index.ts";
import type { ExportStore } from "./store.ts";
import { exportActions } from "./actions.ts";
export function ExportScopeTool(props: { store: ExportStore }) {
  return (
    <Button onClick={() => props.store.cycleScope()}>
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
  return (
    <>
      <Button
        disabled={!props.store.canEnqueue()}
        onClick={() => exportActions()?.enqueue()}
      >
        {t("export.enqueue")}
      </Button>
      <Button disabled={!props.store.canRemove()} onClick={props.store.removeSelected}>{t("export.remove")}</Button>
      <Button
        disabled={props.store.activeSelection().ids.size === 0}
        onClick={() => props.store.clear()}
      >
        {t("export.clear")}
      </Button>
      <Button
        disabled={![...props.store.queues().values()].some((q) => q.length > 0)}
        onClick={() => exportActions()?.requestReset()}
      >
        {t("export.reset")}
      </Button>
    </>
  );
}
export function ExportStopTool(props: { store: ExportStore }) {
  return (
    <Button
      disabled={props.store.enabled().size === 0}
      onClick={() => props.store.stopAll()}
    >
      {t("export.stopAll")}
    </Button>
  );
}
