import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import { aiModelStatus, aiInstall, aiUninstall, type AiModelStatus } from "../../api/photo-ai.ts";
import { pickDirectory } from "../../api/dialog.ts";
import { Button } from "../../components/ui/Button.tsx";
import { aiBuildEnabled } from "../../lib/ai-capability.ts";
import { t } from "../../i18n/index.ts";
/** 共用设置外壳，TinyCLIP 随应用提供，明确安装才复制到设备模型目录。 */
export function AiModelSettings(props: { active: boolean }) {
  const [status, setStatus] = createSignal<AiModelStatus | null>(null);
  const [busy, setBusy] = createSignal(false);
  let epoch = 0;
  const [error, setError] = createSignal<string | null>(null);
  createEffect(() => {
    if (!props.active) return;
    const request = ++epoch; setStatus(null); setError(null);
    void aiModelStatus().then((value) => { if (request === epoch) setStatus(value); }).catch((reason: unknown) => { if (request === epoch) setError(String(reason)); });
    // Settings remains mounted; ignore a late read after switching pages.
    onCleanup(() => { if (epoch === request) ++epoch; });
  });
  const change = async (operation: "bundled" | "offline" | "remove") => {
    if (busy() || !aiBuildEnabled() || status()?.compiled !== true) return;
    const request = ++epoch;
    setBusy(true); setError(null);
    try {
      if (operation === "remove") await aiUninstall();
      else if (operation === "bundled") await aiInstall();
      else { const directory = await pickDirectory({ title: t("ai.offlineInstall") }); if (directory === null) return; await aiInstall(directory); }
      const value = await aiModelStatus();
      if (request === epoch || props.active) setStatus(value);
    } catch (reason) { if (request === epoch) setError(String(reason)); } finally { setBusy(false); }
  };
  return <div class="space-y-5">
    <header><h2 class="text-fs-3 font-semibold text-fg-1">{t("ai.title")}</h2><p class="mt-1 text-fs-1 text-fg-3">{t("ai.modelDescription")}</p></header>
    <Show when={aiBuildEnabled() && status()?.compiled !== false} fallback={<><section class="space-y-3 rounded-ui bg-surface-layer p-5"><h3 class="text-fs-2 font-semibold text-fg-1">{t("ai.buildDisabled")}</h3><p class="text-fs-1 text-fg-2">{t("ai.basicAvailable")}</p><p class="text-fs-1 text-fg-2">{t("ai.keepExisting")}</p></section><p class="text-fs-1 text-fg-2">{t("ai.useAiBuild")}</p></>}>
    <section class="space-y-3 rounded-ui bg-surface-layer p-5">
      <h3 class="text-fs-2 font-semibold text-fg-1">{t("ai.modelName")}</h3><p class="text-fs-1 text-fg-2">{t("ai.classes")}</p><p class="text-fs-1 text-fg-3">{t("ai.modelSize")}</p>
      <Show when={status()} fallback={<p class="text-fs-1 text-fg-3" role="status">{t(error() ? "ai.missing" : "ai.loadingTags")}</p>}><p class="text-fs-1 text-fg-2">{t(status()?.state === "ready" ? "ai.ready" : status()?.state === "missing" ? "ai.missing" : status()?.state === "damaged" ? "ai.damaged" : "ai.unpublished")}</p></Show>
      <Show when={status()?.manifestSha256}><p class="break-all text-fs-0 text-fg-3">{t("ai.modelVersion")} · {Math.ceil((status()?.bytes ?? 0) / 1048576)} MiB</p></Show>
      <div class="flex flex-wrap gap-2"><Button variant="primary" size="sm" disabled={busy() || !status()?.bundled || status()?.state === "unpublished"} onClick={() => void change("bundled")}>{busy() ? t("ai.installing") : t("ai.install")}</Button><Button variant="secondary" size="sm" disabled={busy() || !status() || status()?.state === "unpublished"} onClick={() => void change("offline")}>{t("ai.offlineInstall")}</Button><Show when={status()?.manifestSha256 || status()?.state === "damaged"}><Button variant="secondary" size="sm" disabled={busy()} onClick={() => void change("remove")}>{t("ai.uninstall")}</Button></Show></div>
    </section>
    <Show when={error()}>{(message) => <p class="text-fs-1 text-danger" role="alert">{message()}</p>}</Show>
    <p class="text-fs-1 text-fg-2">{t("ai.tradeoff")}</p><p class="text-fs-1 text-fg-2">{t("ai.survival")}</p><p class="text-fs-1 text-fg-2">{t("ai.explicit")}</p>
    </Show>
  </div>;
}
