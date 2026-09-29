/**
 * `MigrationGate` —— 数据库升级时的**阻塞遮罩**（人类 2026-09-19 定）。
 *
 * 要挡住的东西有两样：**鼠标**（全屏遮罩）与**键盘**（外壳级快捷键：
 * 网格的方向键/数字打星、浏览的 Tab 四态、Delete 删除……）。少挡键盘那一半，
 * 用户按住方向键就能在「结构正在被改写」的库上乱点，遮罩等于没做。
 *
 * 视觉沿用 `Dialog` 的两层：`bg-scrim` 压暗 + `surface-layer` 浮层（不做阴影/毛玻璃）。
 * 与对话框的区别是**不给任何出口**：没有叉、没有取消、Esc 也不关 ——
 * 升级按现有逐条事务执行，操作会话要等待整个执行返回。
 */

import { createEffect, createMemo, onCleanup, Show } from "solid-js";
import { IconLoader2 } from "@tabler/icons-solidjs";
import { Portal } from "solid-js/web";
import { t } from "../../i18n/index.ts";
import { activeNotice, type MigrationMap } from "./notice.ts";

export interface MigrationGateProps {
  /** 正在升级的库（执行编号 → 通知）；空表 = 不显示 */
  notices: MigrationMap;
}

export function MigrationGate(props: MigrationGateProps) {
  const notice = () => activeNotice(props.notices);
  const open = createMemo(() => notice() !== null);
  let dialog: HTMLDivElement | undefined;

  // window 捕获先于工作区和已打开弹窗；阻断按键、滚轮与真实指针输入。
  createEffect(() => {
    if (!open()) return;
    const previous = document.activeElement;
    if (previous instanceof HTMLElement) previous.blur();
    dialog?.focus({ preventScroll: true });
    const block = (event: Event): void => {
      // 程序驱动的 click 不属输入；演示页用它发 Done，真实鼠标/触屏一律拦截。
      if (["pointerdown", "mousedown", "click", "dblclick", "contextmenu"].includes(event.type) && !event.isTrusted) return;
      event.preventDefault();
      event.stopImmediatePropagation();
    };
    const events = ["keydown", "keyup", "wheel", "pointerdown", "mousedown", "click", "dblclick", "contextmenu"];
    for (const target of [document, window]) {
      for (const name of events) target.addEventListener(name, block, { capture: true, passive: false });
    }
    onCleanup(() => {
      for (const target of [document, window]) {
        for (const name of events) target.removeEventListener(name, block, true);
      }
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus({ preventScroll: true });
    });
  });

  return (
    <Show when={notice()}>
      {(current) => (
        <Portal>
          <div class="fixed inset-0 z-(--z-migration)">
            <div class="absolute inset-0 bg-scrim" aria-hidden="true" />
            <div class="pointer-events-none relative flex h-full items-center justify-center">
              <div
                ref={dialog}
                tabIndex={-1}
                class="pointer-events-auto flex w-80 flex-col gap-2 rounded-ui bg-surface-layer p-(--dialog-pad)"
                data-migration-gate="open"
                role="alertdialog"
                aria-modal="true"
                aria-labelledby="migration-gate-title"
                aria-describedby="migration-gate-description"
                aria-busy="true"
              >
                <div class="flex items-center gap-2">
                  <IconLoader2 size={16} class="shrink-0 animate-spin text-fg-2" aria-hidden="true" />
                  <h2 id="migration-gate-title" class="text-[15px] leading-6 font-semibold text-fg-1">
                    {t(current().kind === "catalog" ? "migration.title_catalog" : "migration.title")}
                  </h2>
                </div>
                <p id="migration-gate-description" class="text-[13px] leading-normal text-fg-2">
                  {t("migration.body")}
                </p>
                <p class="text-fs-0 leading-normal text-fg-3">{t("migration.hint")}</p>
              </div>
            </div>
          </div>
        </Portal>
      )}
    </Show>
  );
}
