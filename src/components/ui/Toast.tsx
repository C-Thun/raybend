/**
 * Toast 的**视图**（状态机在 `toast.ts`）。
 *
 * ```text
 * ┌──────────────────────────────┐   ← `titlebar` 正下方、右上角
 * │ ⚠ 2 张有锁，没有改动    [撤销] │      带边框：成功 `$brand` / 失败 `$danger`
 * └──────────────────────────────┘      底 `$surface-layer`，约 5 秒后自己走
 * ```
 *
 * * **不阻塞操作**：容器 `pointer-events-none`、只有卡片本身收事件（设计稿里的「不遮挡操作」）；
 * * **可撤销**：带动作的提示把「撤销」放在自己身上 —— 人对误操作的第一反应就是找撤销；
 * * **最多 3 条**（在状态机里夹取）：再多就顶掉最老的，提示不是日志。
 */

import { For, Show, type JSX } from "solid-js";
import { IconAlertTriangle, IconCheck, IconInfoCircle, IconX } from "@tabler/icons-solidjs";

import { t } from "../../i18n/index.ts";
import type { ToastSpec, ToastStore } from "./toast.ts";

export interface ToastHostProps {
  store: ToastStore;
}

/** 语气 → 边框色与图标 */
function toneLook(tone: ToastSpec["tone"]): { border: string; icon: JSX.Element } {
  switch (tone) {
    case "success":
      return { border: "border-brand", icon: <IconCheck size={14} class="text-brand" /> };
    case "danger":
      return { border: "border-danger", icon: <IconAlertTriangle size={14} class="text-danger" /> };
    default:
      return { border: "border-line-2", icon: <IconInfoCircle size={14} class="text-fg-2" /> };
  }
}

/**
 * 提示容器：挂在根层（`App.tsx`），用 `--z-toast`（永远最上面）。
 *
 * 容器本身 `pointer-events-none`（不挡点击），卡片自己恢复 `pointer-events-auto`。
 */
export function ToastHost(props: ToastHostProps): JSX.Element {
  return (
    <div
      data-toast-host="open"
      /*
     * 位置（人类 2026-09-19）：压在 **titlebar 之下** —— 挡住 flowinfo 那一带没问题，
     * 但不能盖住右上角的窗口三键（最小化/最大化/关闭），那里被压住「怪怪的」。
     */
    class="pointer-events-none fixed right-3 top-[calc(var(--bar-title-h)+8px)] flex w-[320px] flex-col gap-2"
      style={{ "z-index": "var(--z-toast)" }}
      aria-live="polite"
    >
      <For each={props.store.items()}>
        {(item) => {
          const look = () => toneLook(item.tone);
          return (
            <div
              data-toast={item.tone}
              class={[
                "pointer-events-auto flex items-start gap-2 rounded-ui border bg-surface-layer px-2.5 py-2 text-fs-2 text-fg-1 shadow-sm",
                look().border,
              ].join(" ")}
            >
              <span class="mt-0.5 shrink-0">{look().icon}</span>
              <span class="min-w-0 flex-1 break-words">{item.message}</span>
              <Show when={item.action}>
                {(action) => (
                  <button
                    type="button"
                    class="shrink-0 rounded-ui px-1.5 py-0.5 text-fs-2 text-brand hover:bg-state-hover"
                    onClick={() => {
                      action().onAction();
                      props.store.dismiss(item.id);
                    }}
                  >
                    {action().label}
                  </button>
                )}
              </Show>
              <button
                type="button"
                class="shrink-0 rounded-ui p-0.5 text-fg-3 hover:bg-state-hover hover:text-fg-1"
                onClick={() => props.store.dismiss(item.id)}
                aria-label={t("toast.close")}
              >
                <IconX size={12} aria-hidden="true" />
              </button>
            </div>
          );
        }}
      </For>
    </div>
  );
}

export type { ToastItem, ToastSpec, ToastStore } from "./toast.ts";
export { createToastStore, toastDisposer, TOAST_DURATION_MS, TOAST_MAX } from "./toast.ts";
