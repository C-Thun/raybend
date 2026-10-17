/**
 * 更新中心：启动流程 + 两个「只问一次」的对话框 + 标题栏指示器的状态源。
 *
 * 判据（崔总 2026-10-15）：**更新永远不阻塞使用**；用户只有两个决策点、各只问一次
 * （下载 / 跳过、安装 / 取消）；下载完成后的**下一次启动**才问安装。规格 `specs/update-flow-v2.md`。
 *
 * 状态只有一个事实源：Rust 的 `updates_status`（落盘在 `<app_local_data>/updates/state.json`），
 * 变化通过 `updates://state` 广播；这里只订阅、不推导。
 */
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { Dialog } from "../components/ui/Dialog.tsx";
import { Button } from "../components/ui/Button.tsx";
import type { ToastStore } from "../components/ui/toast.ts";
import { t } from "../i18n/index.ts";
import * as api from "../api/updates.ts";
import { currentBuildInfo } from "../lib/build-info.ts";
import {
  AUTO_UPDATE_DELAY_MS,
  AUTO_UPDATE_KEY_V1,
  UPDATE_KEY_V1,
  UPDATE_PREF_KEY,
  autoCheckAllowed,
  readUpdatePrefs,
  updateSource,
  updatesAvailable,
  writeUpdatePrefs,
  type UpdatePrefs,
  type UpdateSource,
} from "../lib/update-source.ts";

type Prompt = { kind: "download"; version: string } | { kind: "install"; version: string } | null;

/** 启动流程只跑一次（重复挂载、HMR 都不重来） */
let started = false;
/** 提示队列在 App 里建；这里只借它弹「已在后台下载」这类轻提示 */
let toastStore: ToastStore | null = null;

const [status, setStatus] = createSignal<api.UpdateStatus | null>(null);
const [prompt, setPrompt] = createSignal<Prompt>(null);
const [busy, setBusy] = createSignal(false);

export const updateStatusView = status;
export const updateBusy = busy;

/** 标题栏指示器读它：只有「后台下载中 / 已下载待安装」时有内容，其余时候整块不出现。 */
export function updateIndicator(): {
  phase: "downloading" | "ready";
  version: string | null;
  percent: number | null;
} | null {
  const current = status();
  if (!current || (current.phase !== "downloading" && current.phase !== "ready")) return null;
  const percent =
    current.totalBytes && current.totalBytes > 0
      ? Math.min(100, Math.round((current.doneBytes / current.totalBytes) * 100))
      : null;
  return { phase: current.phase, version: current.version, percent };
}

function readStored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

/** 用户偏好（更新对话框与启动流程读同一份）。 */
export function readPrefs(): UpdatePrefs {
  return readUpdatePrefs(
    readStored(UPDATE_PREF_KEY),
    readStored(UPDATE_KEY_V1),
    readStored(AUTO_UPDATE_KEY_V1),
  );
}

/** 写偏好；存储不可用时抛错，由调用方提示（偏好写不进去不该静默）。 */
export function savePrefs(prefs: UpdatePrefs): void {
  writeUpdatePrefs(prefs);
}

/** 本次构建能不能检查更新：带签名公钥才行（test/dev 构建是空串）。 */
export function updateSourceForBuild(): UpdateSource | null {
  const source = updateSource(readPrefs(), __RAYBEND_UPDATER_PUBLIC_KEY__);
  return updatesAvailable(source.publicKey) ? source : null;
}

export async function refreshUpdateStatus(): Promise<api.UpdateStatus | null> {
  try {
    const next = await api.updateStatus();
    setStatus(next);
    return next;
  } catch (error) {
    console.debug("[update] 状态读取失败", error); // i18n-exempt: 控制台诊断
    return null;
  }
}

/** 查更新。`manual = false`（启动静默查）由 Rust 压住「已跳过 / 已在办」的版本。 */
export async function checkForUpdates(manual: boolean): Promise<api.AvailableUpdate | null> {
  const source = updateSourceForBuild();
  if (!source) return null;
  return api.updateCheck(source, manual);
}

/** 开始或继续下载：后台跑，界面照常用。 */
export async function beginUpdate(automatic = false): Promise<void> {
  const source = updateSourceForBuild();
  if (!source || busy()) return;
  setBusy(true);
  try {
    const update = await api.updateBegin(source, automatic);
    toastStore?.show({
      tone: "info",
      message: t("updates.started", { version: update.version }),
    });
  } catch (error) {
    console.debug("[update] 下载未能开始", error); // i18n-exempt: 控制台诊断
  } finally {
    setBusy(false);
    await refreshUpdateStatus();
  }
}

/** 安装已下载好的成品（进程会被安装器接管）。 */
export async function installUpdate(): Promise<void> {
  const source = updateSourceForBuild();
  if (!source || busy()) return;
  setBusy(true);
  try {
    await api.updateInstall(source);
  } catch (error) {
    toastStore?.show({ tone: "danger", message: t("updates.failed", { message: String(error) }) });
  } finally {
    setBusy(false);
    await refreshUpdateStatus();
  }
}

/** 跳过这个版本：只压自动提示，手动检查永远还能看到它。 */
export async function skipVersion(version: string): Promise<void> {
  try {
    await api.updateSkip(version);
  } finally {
    await refreshUpdateStatus();
  }
}

/**
 * 启动流程（延迟一次，先把首屏与建库让过去）：
 *   1. 上次下载被进程退出打断 → 接着下（静默）
 *   2. 上次失败且这个版本还没重试过 → 静默重试一次
 *   3. 已下载待安装且还没问过 → 弹一次「安装 / 取消」
 *   4. 否则按偏好做那次静默检查（有新版本才弹「下载 / 跳过」）
 */
async function startupFlow(): Promise<void> {
  const source = updateSourceForBuild();
  if (!source || __RAYBEND_DISTRIBUTION__ === "store") return;
  const current = await refreshUpdateStatus();
  if (!current) return;
  if (current.phase === "downloading") {
    await beginUpdate(false);
    return;
  }
  if (current.autoRetry) {
    await beginUpdate(true);
    return;
  }
  if (
    current.phase === "ready" &&
    current.version &&
    current.installPromptedVersion !== current.version
  ) {
    // 先记账再弹：中途崩了也不会第二次问
    try {
      await api.updateMarkInstallPrompted();
    } catch {
      /* 记账失败只影响「会不会再问一次」，不挡这次提示 */
    }
    setPrompt({ kind: "install", version: current.version });
    return;
  }
  const prefs = readPrefs();
  if (!autoCheckAllowed(prefs, currentBuildInfo().channel, __RAYBEND_UPDATER_PUBLIC_KEY__)) return;
  try {
    const found = await checkForUpdates(false);
    if (found) setPrompt({ kind: "download", version: found.version });
  } catch (error) {
    console.debug("[update] 启动检查失败", error); // i18n-exempt: 控制台诊断
  }
}

/**
 * 挂一次：订阅状态广播、跑一遍启动流程，并渲染那两个只问一次的对话框。
 * 放在 App 的浮层区（和别的对话框同一层）。
 */
export function UpdateCenter(props: { toast: ToastStore }) {
  toastStore = props.toast;
  onMount(() => {
    let dispose: (() => void) | undefined;
    void api
      .onUpdateState((next) => setStatus(next))
      .then((unlisten) => {
        dispose = unlisten;
      })
      .catch(() => {});
    void refreshUpdateStatus();
    const timer = window.setTimeout(() => {
      if (started) return;
      started = true;
      void startupFlow();
    }, AUTO_UPDATE_DELAY_MS);
    onCleanup(() => {
      window.clearTimeout(timer);
      dispose?.();
    });
  });

  return (
    <Show when={prompt()}>
      {(current) => (
        <Show
          when={current().kind === "download"}
          fallback={<InstallPrompt version={current().version} />}
        >
          <DownloadPrompt version={current().version} />
        </Show>
      )}
    </Show>
  );
}

/** 只问一次的「下载 / 跳过」：关掉（含 Esc / 点遮罩）都算跳过。 */
function DownloadPrompt(props: { version: string }) {
  const skip = () => void skipVersion(props.version).then(() => setPrompt(null));
  return (
    <Dialog
      open
      title={t("updates.prompt.title", { version: props.version })}
      onOpenChange={(open) => {
        // 关掉 = 跳过（「只问一次」的账必须记上，否则下次启动又冒出来）
        if (!open) skip();
      }}
      footer={
        <>
          <Button variant="secondary" onClick={skip}>
            {t("updates.prompt.skip")}
          </Button>
          <Button
            variant="primary"
            onClick={() => {
              setPrompt(null);
              void beginUpdate(false);
            }}
          >
            {t("updates.download")}
          </Button>
        </>
      }
    >
      <p class="text-fs-1 text-fg-2">{t("updates.prompt.body")}</p>
    </Dialog>
  );
}

/** 只问一次的「安装 / 取消」：取消保留已下载的包，不再追着问。 */
function InstallPrompt(props: { version: string }) {
  const close = () => setPrompt(null);
  return (
    <Dialog
      open
      title={t("updates.ready.title", { version: props.version })}
      onOpenChange={(open) => {
        if (!open) close();
      }}
      footer={
        <>
          <Button variant="secondary" onClick={close}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            onClick={() => {
              close();
              void installUpdate();
            }}
          >
            {t("updates.installNow")}
          </Button>
        </>
      }
    >
      <p class="text-fs-1 text-fg-2">{t("updates.ready.body")}</p>
    </Dialog>
  );
}
