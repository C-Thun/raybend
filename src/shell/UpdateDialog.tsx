/**
 * 更新对话框：设置（更新源 + 启动检查开关）与状态（进度 / 已就绪 / 上次失败）。
 *
 * 崔总 2026-10-15 定：面向普通用户只留**稳定 / 预览**两个源，预览先灰着留口；
 * 界面里不出现密钥与自定义地址。**用户被问到的只有两个决策点**（下载 / 跳过、安装 / 取消），
 * 它们住在 `update-center.tsx` 的启动流程里；这里是「自己来看一眼」的地方，永远如实显示状态。
 *
 * 规格 `specs/update-flow-v2.md`。
 */
import { createSignal, Show } from "solid-js";
import { ConfirmDialog, Dialog } from "../components/ui/Dialog.tsx";
import { Button } from "../components/ui/Button.tsx";
import { Switch } from "../components/ui/Form.tsx";
import { CompactChoice } from "../components/ui/CompactChoice.tsx";
import { t } from "../i18n/index.ts";
import type { AvailableUpdate } from "../api/updates.ts";
import { BETA_CHANNEL_ENABLED, UPDATE_ENDPOINTS } from "../lib/update-source.ts";
import {
  beginUpdate,
  checkForUpdates,
  installUpdate,
  readPrefs,
  refreshUpdateStatus,
  savePrefs,
  updateBusy as busy,
  updateSourceForBuild,
  updateStatusView as statusView,
} from "./update-center.tsx";

function megabytes(bytes: number | null | undefined): string {
  return `${((bytes ?? 0) / (1024 * 1024)).toFixed(1)}`;
}

export function UpdateDialog(props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 有没有正在跑的导入 / 导出 / 迁移（有就不许装） */
  canInstall: () => boolean;
}) {
  const storeManaged = () => __RAYBEND_DISTRIBUTION__ === "store";
  const configured = () => updateSourceForBuild() !== null;
  const [prefs, setPrefs] = createSignal(readPrefs());
  const [checking, setChecking] = createSignal(false);
  /** 手动查过的结果：`null` = 没查过，`{version:null}` = 已是最新 */
  const [checked, setChecked] = createSignal<AvailableUpdate | "none" | null>(null);
  const [error, setError] = createSignal("");
  const [confirm, setConfirm] = createSignal(false);
  const status = statusView;

  function writePrefs(next: ReturnType<typeof readPrefs>): void {
    setPrefs(next);
    try {
      savePrefs(next);
    } catch {
      setError(t("updates.storage_error"));
    }
  }

  async function check(): Promise<void> {
    if (checking() || busy()) return;
    setError("");
    setChecking(true);
    try {
      setChecked((await checkForUpdates(true)) ?? "none");
    } catch (caught) {
      setError(t("updates.failed", { message: String(caught) }));
    } finally {
      setChecking(false);
      await refreshUpdateStatus();
    }
  }

  const phase = () => status()?.phase ?? "idle";
  const percent = () => {
    const current = status();
    if (!current?.totalBytes) return null;
    return Math.min(100, Math.round((current.doneBytes / current.totalBytes) * 100));
  };
  const found = () => {
    const result = checked();
    return result && result !== "none" ? result : null;
  };

  return (
    <>
      <Dialog
        open={props.open}
        onOpenChange={(open) => {
          if (!checking() && !busy()) props.onOpenChange(open);
        }}
        title={t("updates.title")}
        footer={
          <>
            <Button
              variant="secondary"
              disabled={checking() || busy()}
              onClick={() => props.onOpenChange(false)}
            >
              {t("common.close")}
            </Button>
            <Show when={phase() === "ready"}>
              <Button disabled={!props.canInstall()} onClick={() => setConfirm(true)}>
                {t("updates.installNow")}
              </Button>
            </Show>
            <Show when={phase() === "paused"}>
              <Button onClick={() => void beginUpdate(false)}>{t("updates.resume")}</Button>
            </Show>
            <Show when={phase() === "idle" && found()}>
              {(update) => (
                <Button onClick={() => void beginUpdate(false)}>
                  {t("updates.download")} {update().version}
                </Button>
              )}
            </Show>
            <Button
              variant="secondary"
              loading={checking()}
              disabled={checking() || busy() || storeManaged() || !configured()}
              onClick={() => void check()}
            >
              {t("updates.check")}
            </Button>
          </>
        }
      >
        <div class="flex flex-col gap-3">
          <p class="text-fs-1 text-fg-2">
            {t("updates.current", { version: status()?.currentVersion ?? "" })}
          </p>

          {/* ── 状态区：跟着 Rust 的 phase 走，永远说真话 ── */}
          <Show when={phase() === "downloading"}>
            <div class="flex flex-col gap-1.5">
              <p class="text-fs-1 text-fg-2">
                {t("updates.progress", {
                  version: status()?.version ?? "",
                  done: megabytes(status()?.doneBytes),
                  total: status()?.totalBytes ? megabytes(status()?.totalBytes) : t("updates.unknown"),
                })}
              </p>
              <div class="h-1 w-full overflow-hidden rounded-full bg-surface-track">
                <div
                  class="h-full bg-primary transition-[width]"
                  style={{ width: `${percent() ?? 0}%` }}
                />
              </div>
            </div>
          </Show>
          <Show when={phase() === "ready"}>
            <p class="text-fs-1 text-fg-2">
              {t("updates.state.ready", {
                version: status()?.version ?? "",
              })}
            </p>
          </Show>
          <Show when={phase() === "paused"}>
            <p class="text-fs-1 text-fg-2">
              {t("updates.state.paused", {
                error: status()?.error ?? "",
              })}
            </p>
          </Show>
          <Show when={phase() === "idle"}>
            <p class="text-fs-1 text-fg-2">
              {checked() === "none"
                ? t("updates.state.current")
                : found()
                  ? t("updates.state.available", { version: found()?.version ?? "" })
                  : t("updates.state.never")}
            </p>
          </Show>

          {/* ── 设置 ── */}
          <div class="flex flex-col gap-2 rounded-ui bg-surface-track px-2 py-2">
            <div class="flex shrink-0 items-center justify-between gap-2 text-fs-1 text-fg-2">
              <span>{t("updates.auto.label")}</span>
              <Switch
                checked={prefs().autoCheck}
                disabled={checking() || busy() || storeManaged()}
                onCheckedChange={(value) => writePrefs({ ...prefs(), autoCheck: value })}
                label={t("updates.auto.label")}
              />
            </div>
            <CompactChoice
              label={t("updates.source.label")}
              value={prefs().channel}
              options={[
                { value: "stable", label: t("updates.mode.stable") },
                {
                  value: "beta",
                  label: t("updates.mode.beta"),
                  // 预览通道先灰着留口：等真的有预览发布流程了再开（`BETA_CHANNEL_ENABLED`）
                  disabled: !BETA_CHANNEL_ENABLED,
                },
              ]}
              onValueChange={(value) => writePrefs({ ...prefs(), channel: value })}
            />
          </div>

          <p class="text-fs-1 text-fg-3">
            {t(storeManaged() ? "updates.store" : "updates.privacy")}
          </p>
          <Show when={!configured() && !storeManaged()}>
            <p class="text-fs-1 text-fg-3">{t("updates.not_configured")}</p>
          </Show>
          <Show when={!props.canInstall()}>
            <p class="text-fs-1 text-fg-3">{t("updates.tasks_active")}</p>
          </Show>
          <Show when={error()}>
            <p role="alert" class="text-fs-1 text-danger">
              {error()}
            </p>
          </Show>
        </div>
      </Dialog>
      <ConfirmDialog
        open={confirm()}
        scrim={false}
        title={t("updates.installNow")}
        message={t("updates.confirm")}
        onCancel={() => setConfirm(false)}
        onConfirm={() => {
          setConfirm(false);
          void installUpdate();
        }}
      />
    </>
  );
}

/** 稳定源的地址（对话框不显示，留给诊断与将来打开预览时引用）。 */
export const STABLE_ENDPOINT = UPDATE_ENDPOINTS.stable;
