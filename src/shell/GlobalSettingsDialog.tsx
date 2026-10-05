/**
 * 全局设置的唯一外壳。照片级色彩编辑留在 editor right，这里展示设备级状态、
 * 默认规则和配置资产入口。尚未接线的功能用明确的状态说明，避免虚假的可操作控件。
 */

import { IconDeviceDesktop, IconEyeOff, IconFileDescription, IconKeyboard, IconPalette, IconPlus, IconRefresh, IconScan } from "@tabler/icons-solidjs";
import { createEffect, createSignal, For, onCleanup, Show, type Component } from "solid-js";
import { getColorDefaults, setColorDefaults, getColorProfileLibrary, getDisplaySnapshot, hideColorProfile, importColorProfileFiles, onDisplayEnvironmentChange, openSystemDisplaySettings, type ColorDefaults, type ColorProfileEntry, type ColorProfileImport, type ColorProfileLibrary, type DisplaySnapshot } from "../api/color.ts";
import { ProfileSelect } from "../components/ui/ProfileSelect.tsx";
import { DisplayColorStatus } from "../components/ui/DisplayColorStatus.tsx";
import { getEditorRenderState } from "../api/editor.ts";
import type { EditorRenderState } from "../api/types.ts";
import { displayPresentationMatches, type DisplayPhase } from "../lib/display-color.ts";
import { displayDetectionKey } from "../i18n/display-color.ts";
import { pickOpenFiles } from "../api/dialog.ts";
import { isTauriRuntime } from "../api/tauri-env.ts";
import { Button, IconButton } from "../components/ui/Button.tsx";
import { Dialog } from "../components/ui/Dialog.tsx";
import type { CommandSpec } from "../lib/commands.ts";
import { t, type MessageKey } from "../i18n/index.ts";
import { AiModelSettings } from "../features/browse/AiModelSettings.tsx";
import { ShortcutSettingsPanel } from "../features/commands/index.ts";

export type SettingsPage = "color" | "profiles" | "shortcuts" | "ai";

export interface GlobalSettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  page: SettingsPage;
  onPageChange: (page: SettingsPage) => void;
  commands: readonly CommandSpec[];
  onShortcutsSaved?: () => void;
}

const PAGES: readonly { id: SettingsPage; label: MessageKey; icon: Component<{ size?: number }> }[] = [
  { id: "color", label: "settings.color.title", icon: IconPalette },
  { id: "profiles", label: "settings.profiles.title", icon: IconFileDescription },
  { id: "ai", label: "ai.title", icon: IconScan },
  { id: "shortcuts", label: "settings.shortcuts.title", icon: IconKeyboard },
];

function InfoRow(props: { title: MessageKey; value: MessageKey; detail: MessageKey }) {
  return (
    <div class="flex gap-8 border-b border-surface-bar py-5 last:border-0">
      <div class="w-36 shrink-0 text-fs-2 font-semibold text-fg-1">{t(props.title)}</div>
      <div class="min-w-0 flex-1">
        <div class="text-fs-2 font-medium text-fg-1">{t(props.value)}</div>
        <p class="mt-1 text-fs-1 leading-relaxed text-fg-3">{t(props.detail)}</p>
      </div>
    </div>
  );
}

export function GlobalSettingsDialog(props: GlobalSettingsDialogProps) {
  const [display, setDisplay] = createSignal<DisplaySnapshot | null>(null);
  const [displayLoading, setDisplayLoading] = createSignal(false);
  const [displayError, setDisplayError] = createSignal<string | null>(null);
  const [renderState, setRenderState] = createSignal<EditorRenderState | null>(null);
  const [renderLoading, setRenderLoading] = createSignal(false);
  const [renderError, setRenderError] = createSignal<string | null>(null);
  const [refreshTick, setRefreshTick] = createSignal(0);
  const [profiles, setProfiles] = createSignal<ColorProfileLibrary | null>(null);
  const [profilesLoading, setProfilesLoading] = createSignal(false);
  const [profilesError, setProfilesError] = createSignal<string | null>(null);
  const [importing, setImporting] = createSignal(false);
  const [hiding, setHiding] = createSignal<string | null>(null);
  const [importReport, setImportReport] = createSignal<ColorProfileImport | null>(null);
  const [defaults, setDefaults] = createSignal<ColorDefaults | null>(null);
  const [savedDefaults, setSavedDefaults] = createSignal<string | null>(null);
  const [defaultsError, setDefaultsError] = createSignal<string | null>(null);
  const [savingDefaults, setSavingDefaults] = createSignal(false);
  const defaultsDirty = () => defaults() !== null && JSON.stringify(defaults()) !== savedDefaults();
  createEffect(() => {
    if (!props.open || props.page !== "color") return;
    let active = true;
    setDefaults(null);
    setDefaultsError(null);
    void getColorDefaults().then(value => {
      if (active) { setDefaults(value); setSavedDefaults(JSON.stringify(value)); }
    }).catch((error: unknown) => { if (active) setDefaultsError(String(error)); });
    onCleanup(() => { active = false; });
  });
  const saveDefaults = async () => {
    const value = defaults();
    if (value === null || savingDefaults()) return;
    setSavingDefaults(true);
    setDefaultsError(null);
    try { await setColorDefaults(value); setSavedDefaults(JSON.stringify(value)); }
    catch (error) { setDefaultsError(String(error)); }
    finally { setSavingDefaults(false); }
  };

  createEffect(() => {
    if (!props.open || props.page !== "color") return;
    refreshTick();
    let active = true;
    setDisplayLoading(true);
    setDisplayError(null);
    void getDisplaySnapshot()
      .then((result) => {
        if (active) setDisplay(result?.snapshot ?? null);
      })
      .catch((error: unknown) => {
        if (active) {
          setDisplay(null);
          setDisplayError(String(error));
        }
      })
      .finally(() => {
        if (active) setDisplayLoading(false);
      });
    onCleanup(() => { active = false; });
  });

  createEffect(() => {
    if (!props.open || props.page !== "color") return;
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    setRenderState(null);
    setRenderLoading(true);
    setRenderError(null);
    const poll = async () => {
      try {
        const state = await getEditorRenderState();
        if (active) { setRenderState(state); setRenderError(null); }
      } catch (error) {
        if (active) { setRenderState(null); setRenderError(String(error)); }
      } finally {
        if (active) { setRenderLoading(false); timer = setTimeout(() => void poll(), 500); }
      }
    };
    void poll();
    onCleanup(() => { active = false; clearTimeout(timer); });
  });

  createEffect(() => {
    if (!props.open || (props.page !== "profiles" && props.page !== "color")) return;
    let active = true;
    setProfilesLoading(true);
    setProfilesError(null);
    void getColorProfileLibrary()
      .then((result) => { if (active) setProfiles(result); })
      .catch((error: unknown) => { if (active) setProfilesError(String(error)); })
      .finally(() => { if (active) setProfilesLoading(false); });
    onCleanup(() => { active = false; });
  });

  const importProfiles = async () => {
    const paths = await pickOpenFiles({
      title: t("settings.profiles.import"),
      filters: [{ name: t("settings.profiles.fileType"), extensions: ["icc", "icm"] }],
    });
    if (paths.length === 0) return;
    setImporting(true);
    setProfilesError(null);
    try {
      const result = await importColorProfileFiles(paths);
      setProfiles(result.library);
      setImportReport(result);
    } catch (error) {
      setProfilesError(String(error));
    } finally {
      setImporting(false);
    }
  };

  const hideProfile = async (entry: ColorProfileEntry) => {
    setHiding(entry.profileId);
    setProfilesError(null);
    try {
      setProfiles(await hideColorProfile(entry.profileId));
    } catch (error) {
      setProfilesError(String(error));
    } finally {
      setHiding(null);
    }
  };

  const profileRole = (entry: ColorProfileEntry): MessageKey => {
    if (entry.profileClass === "input") return "settings.profiles.inputOnly";
    if (entry.profileClass === "output") return "settings.profiles.outputOnly";
    return "settings.profiles.inputOutput";
  };

  createEffect(() => {
    if (!props.open) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    void onDisplayEnvironmentChange(() => {
      if (active) setRefreshTick((tick) => tick + 1);
    }).then((stop) => {
      if (active) unlisten = stop;
      else stop();
    }).catch((error: unknown) => {
      if (active) setDisplayError(String(error));
    });
    onCleanup(() => { active = false; unlisten?.(); });
  });

  const actualDisplay = () => {
    const renderer = renderState();
    const status = renderer?.bound && renderer.ready ? renderer.displayColor : null;
    const detected = display();
    if (!status || displayLoading() || (detected && !displayPresentationMatches(status, detected))) return null;
    return status;
  };
  const displayPhase = (): DisplayPhase => {
    if (renderError()) return "error";
    if (renderLoading() || displayLoading() || renderState()?.bound) return "pending";
    return "inactive";
  };

  return (
    <Dialog
      open={props.open}
      onOpenChange={props.onOpenChange}
      title={t("settings.title")}
      description={t("settings.description")}
      size="settings"
      class="overflow-hidden"
    >
      <div class="flex min-h-0 w-full flex-1 gap-0 overflow-hidden rounded-ui bg-surface-main">
        <nav class="flex w-44 shrink-0 flex-col gap-1 bg-surface-track p-3" aria-label={t("settings.category")}>
          <div class="px-2 pb-2 text-fs-0 font-semibold tracking-wide text-fg-3">{t("settings.category")}</div>
          <For each={PAGES}>
            {(item) => (
              <button
                type="button"
                class={[
                  "flex min-h-9 items-center gap-2 rounded-ui px-2 text-left text-fs-2 transition-colors",
                  props.page === item.id
                    ? "bg-state-selected font-semibold text-fg-1"
                    : "text-fg-2 hover:bg-state-hover hover:text-fg-1",
                ].join(" ")}
                aria-current={props.page === item.id ? "page" : undefined}
                onClick={() => props.onPageChange(item.id)}
              >
                <item.icon size={16} aria-hidden="true" />
                {t(item.label)}
              </button>
            )}
          </For>
        </nav>

        <main class="min-h-0 min-w-0 flex-1 overflow-y-auto px-7 py-6">
          <Show when={props.page === "color"}>
            <div class="space-y-5">
              <header>
                <h2 class="text-fs-3 font-semibold text-fg-1">{t("settings.color.title")}</h2>
                <p class="mt-1 text-fs-1 text-fg-3">{t("settings.color.description")}</p>
              </header>
              <section class="flex overflow-hidden rounded-ui bg-surface-bar">
                <div class="w-1 shrink-0 bg-brand" />
                <div class="min-w-0 flex-1 px-4 py-4">
                  <div class="flex items-center gap-2 text-fs-0 font-semibold tracking-wide text-brand">
                    <IconDeviceDesktop size={15} aria-hidden="true" />
                    {t("color.display.canvas")}
                  </div>
                  <div class="mt-2"><DisplayColorStatus status={actualDisplay()} phase={displayPhase()} prominent error={renderError()} /></div>
                  <p class="mt-3 border-t border-surface-main pt-3 text-fs-0 text-fg-3" title={display()?.state.kind === "icc" ? (display()!.state as {profile_path:string}).profile_path : undefined}>
                    {displayLoading() ? t("settings.display.loading") : t("settings.display.detected", { state: t(displayDetectionKey(display()?.state.kind)) })}
                  </p>
                  <Show when={displayError()}>{reason => <p class="mt-1 break-all text-fs-0 text-danger" role="alert">{reason()}</p>}</Show>
                  <div class="mt-4 flex flex-wrap items-center gap-3">
                    <button type="button" class="inline-flex items-center gap-1 rounded-ui px-2 py-1 text-fs-1 text-fg-2 hover:bg-state-hover hover:text-fg-1" onClick={() => setRefreshTick((tick) => tick + 1)}>
                      <IconRefresh size={15} aria-hidden="true" />{t("settings.display.refresh")}
                    </button>
                    <Show when={isTauriRuntime()}>
                      <button type="button" class="rounded-ui px-2 py-1 text-fs-1 text-brand hover:bg-state-hover" onClick={() => void openSystemDisplaySettings().catch((error: unknown) => setDisplayError(String(error)))}>
                        {t("settings.display.systemSettings")}
                      </button>
                    </Show>
                  </div>
                </div>
              </section>
              <div>
                <div class="flex items-center gap-5 border-b border-surface-bar py-5">
                  <div class="min-w-0 flex-1"><h3 class="text-fs-2 font-semibold text-fg-1">{t("settings.input.title")}</h3><p class="mt-1 text-fs-1 leading-relaxed text-fg-3">{t("settings.input.detail")}</p></div>
                  <ProfileSelect label={t("settings.input.title")} role="input" allowAssignment entries={profiles()?.entries ?? []}
                    value={defaults()?.untagged_input.kind === "rgb_icc" ? (defaults()!.untagged_input as {profile_id:string}).profile_id : defaults()?.untagged_input.kind ?? "srgb"}
                    disabled={!defaults() || profilesLoading() || savingDefaults()} onChange={value => setDefaults(old => old ? {...old, untagged_input: value === "srgb" || value === "require_assignment" ? {kind:value} : {kind:"rgb_icc",profile_id:value}} : old)} />
                </div>
                <div class="flex items-center gap-5 border-b border-surface-bar py-5">
                  <div class="min-w-0 flex-1"><h3 class="text-fs-2 font-semibold text-fg-1">{t("settings.output.title")}</h3><p class="mt-1 text-fs-1 leading-relaxed text-fg-3">{t("settings.output.detail")}</p></div>
                  <ProfileSelect label={t("settings.output.title")} role="output" entries={profiles()?.entries ?? []}
                    value={defaults()?.output.kind === "custom_rgb_icc" ? (defaults()!.output as {profile_id:string}).profile_id : defaults()?.output.kind ?? "srgb"}
                    disabled={!defaults() || profilesLoading() || savingDefaults()} onChange={value => setDefaults(old => old ? {...old, output: value === "srgb" || value === "display_p3" || value === "adobe_rgb" ? {kind:value} : {kind:"custom_rgb_icc",profile_id:value}} : old)} />
                </div>
                <InfoRow title="settings.working.title" value="settings.working.value" detail="settings.coming" />
              </div>
              <Show when={defaultsError() ?? profilesError()}>{error => <p class="text-fs-1 text-danger" role="alert">{error()}</p>}</Show>
              <div class="flex items-center justify-end gap-3">
                <span class="text-fs-1 text-fg-3" aria-live="polite">{t(!defaults() ? "settings.defaultsUnavailable" : savingDefaults() ? "settings.saving" : defaultsDirty() ? "settings.unsaved" : "settings.saved")}</span>
                <Button variant="accent" disabled={!defaultsDirty() || savingDefaults()} onClick={() => void saveDefaults()}>{t("settings.save")}</Button>
              </div>
            </div>
          </Show>

          <Show when={props.page === "profiles"}>
            <div class="space-y-5">
              <header>
                <h2 class="text-fs-3 font-semibold text-fg-1">{t("settings.profiles.title")}</h2>
                <p class="mt-1 text-fs-1 text-fg-3">{t("settings.profiles.description")}</p>
              </header>
              <div class="flex flex-wrap items-center justify-between gap-3">
                <p class="text-fs-1 text-fg-2">
                  {t("settings.profiles.count")}{profiles()?.entries.filter((entry) => !entry.hidden).length ?? 0}
                </p>
                <Button variant="secondary" size="sm" class="bg-state-selected" icon={<IconPlus size={16} />} loading={importing()} disabled={!isTauriRuntime() || profilesLoading()} onClick={() => void importProfiles()}>
                  {t("settings.profiles.import")}
                </Button>
              </div>
              <div class="flex items-center gap-3 rounded-ui bg-surface-bar px-4 py-3 text-fs-1 leading-relaxed text-fg-2">
                <IconFileDescription size={17} class="shrink-0 text-brand" aria-hidden="true" />
                {t("settings.profiles.safety")}
              </div>
              <Show when={profilesError()}>
                {(message) => <p class="rounded-ui bg-surface-bar px-3 py-2 text-fs-1 text-danger" role="alert">{message()}</p>}
              </Show>
              <Show when={importReport()}>
                {(report) => (
                  <details class="rounded-ui bg-surface-bar px-3 py-2 text-fs-1 text-fg-2">
                    <summary class="cursor-pointer select-none">
                      {t("settings.profiles.imported")}{report().imported} · {t("settings.profiles.duplicates")}{report().duplicates} · {t("settings.profiles.restored")}{report().restored} · {t("settings.profiles.skipped")}{report().skipped.length}
                    </summary>
                    <Show when={report().skipped.length > 0}>
                      <ul class="mt-2 max-h-28 space-y-1 overflow-y-auto break-all text-fg-3">
                        <For each={report().skipped}>{(item) => <li>{item}</li>}</For>
                      </ul>
                    </Show>
                  </details>
                )}
              </Show>
              <Show when={profilesLoading()}>
                <p class="text-fs-1 text-fg-3" role="status">{t("settings.profiles.loading")}</p>
              </Show>
              <div class="overflow-hidden rounded-ui bg-surface-layer px-4 py-2">
                <div class="flex items-center gap-3 py-2 text-fs-0 font-semibold text-fg-3">
                  <span class="min-w-0 flex-1">{t("settings.profiles.name")}</span>
                  <span class="w-32 shrink-0">{t("settings.profiles.use")}</span>
                  <span class="w-20 shrink-0">{t("settings.profiles.origin")}</span>
                  <span class="w-16 shrink-0">{t("settings.profiles.status")}</span>
                  <span class="w-6 shrink-0" />
                </div>
                <For each={profiles()?.entries.filter((entry) => !entry.hidden) ?? []}>
                  {(entry) => (
                    <div class="flex min-h-11 items-center gap-3 border-t border-surface-bar py-2 text-fs-1">
                      <div class="flex min-w-0 flex-1 items-center gap-2 text-fg-1">
                        <span class={entry.builtIn ? "size-2 shrink-0 rounded-full bg-fg-3" : "size-2 shrink-0 rounded-full bg-brand"} aria-hidden="true" />
                        <span class="truncate" title={entry.name}>{entry.name}</span>
                      </div>
                      <span class="w-32 shrink-0 text-fg-2">{t(profileRole(entry))}</span>
                      <span class="w-20 shrink-0 text-fg-2">{t(entry.builtIn ? "settings.profiles.builtIn" : "settings.profiles.importedSource")}</span>
                      <span class={entry.available ? "w-16 shrink-0 text-brand" : "w-16 shrink-0 text-danger"}>
                        {t(entry.available ? "settings.profiles.available" : "settings.profiles.missing")}
                      </span>
                      <Show when={!entry.builtIn} fallback={<span class="w-6 shrink-0" />}>
                        <IconButton label={t("settings.profiles.hide")} disabled={hiding() === entry.profileId} onClick={() => void hideProfile(entry)}>
                          <IconEyeOff size={16} aria-hidden="true" />
                        </IconButton>
                      </Show>
                    </div>
                  )}
                </For>
                <Show when={!profilesLoading() && !profiles()}>
                  <p class="border-t border-surface-bar py-5 text-fs-1 text-fg-3">{t("settings.profiles.empty")}</p>
                </Show>
              </div>
              <p class="text-fs-0 text-fg-3">{t("settings.profiles.footer")}</p>
            </div>
          </Show>

          <Show when={props.page === "ai"}><AiModelSettings active={props.open && props.page === "ai"} /></Show>

          <Show when={props.page === "shortcuts"}>
            <ShortcutSettingsPanel
              open={props.open && props.page === "shortcuts"}
              onOpenChange={props.onOpenChange}
              commands={props.commands}
              onSaved={props.onShortcutsSaved}
            />
          </Show>
        </main>
      </div>
    </Dialog>
  );
}
