/**
 * `MigrationGate` —— 数据库升级时的**阻塞遮罩**（人类 2026-09-19 定）。
 *
 * 要挡住的东西有两样：**鼠标**（全屏遮罩）与**键盘**（外壳级快捷键：
 * 网格的方向键/数字打星、浏览的 Tab 四态、Delete 删除……）。少挡键盘那一半，
 * 用户按住方向键就能在「结构正在被改写」的库上乱点，遮罩等于没做。
 *
 * 视觉沿用 `Dialog` 的两层：`bg-scrim` 压暗 + `surface-layer` 浮层（不做阴影/毛玻璃）。
 * 与对话框的区别是**不给任何出口**：没有叉、没有取消、Esc 也不关 ——
 * 升级是原子操作，用户此刻唯一能做的就是等。
 */

import { createEffect, onCleanup, Show } from "solid-js";
import { IconLoader2 } from "@tabler/icons-solidjs";
import { t } from "../../i18n/index.ts";
import { activeNotice, type MigrationMap } from "./notice.ts";

export interface MigrationGateProps {
  /** 正在升级的库（`kind` → 通知）；空表 = 不显示 */
  notices: MigrationMap;
}

export function MigrationGate(props: MigrationGateProps) {
  const notice = () => activeNotice(props.notices);
  const open = () => notice() !== null;

  /*
   * 键盘拦截：**捕获阶段**挂在 `document` 与 `window` 上，拦下所有按键。
   *
   * 为什么必须捕获：各处快捷键监听在 `window` 上（气泡阶段），
   * 而真实按键的传播路径是 window → document → …（捕获）→ target → 再冒泡回 window，
   * 所以在 document 的捕获阶段 `stopPropagation`，那些监听一次都收不到。
   * `preventDefault` 是另一半：免得浏览器自己响应（空格滚动、Tab 焦点跳走）。
   *
   * window 那一份是**兜底**：事件若被直接派发到 window 上（测试、脚本），
   * 它走的是 target 阶段 —— 那时同名节点上的监听按**注册顺序**触发（不看捕获位），
   * 我们注册得比外壳晚，兜不住；真实按键不会这样，所以主战场仍是 document。
   */
  createEffect(() => {
    if (!open()) return;
    /*
     * 形参用 `Event` 而不是 `KeyboardEvent`：`Document | Window` 的联合上
     * `addEventListener` 只会落到通用重载（监听器收 `Event`）。
     * 我们只用 `preventDefault` / `stopPropagation` 这两个 `Event` 就有的方法，
     * 所以不需要键盘字段，也不用窄化 —— 联合类型下的类型安全反而更干净。
     */
    const block = (event: Event): void => {
      event.preventDefault();
      event.stopPropagation();
    };
    for (const target of [document, window]) {
      target.addEventListener("keydown", block, true);
      target.addEventListener("keyup", block, true);
    }
    onCleanup(() => {
      for (const target of [document, window]) {
        target.removeEventListener("keydown", block, true);
        target.removeEventListener("keyup", block, true);
      }
    });
  });

  return (
    <Show when={notice()}>
      {(current) => (
        <>
          <div class="fixed inset-0 z-(--z-scrim) bg-scrim" aria-hidden="true" />
          <div class="pointer-events-none fixed inset-0 z-(--z-modal) flex items-center justify-center">
            <div
              class="pointer-events-auto flex w-80 flex-col gap-2 rounded-ui bg-surface-layer p-(--dialog-pad)"
              data-migration-gate="open"
              role="alertdialog"
              aria-modal="true"
              aria-labelledby="migration-gate-title"
            >
              <div class="flex items-center gap-2">
                <IconLoader2 size={16} class="shrink-0 animate-spin text-fg-2" aria-hidden="true" />
                <h2 id="migration-gate-title" class="text-[15px] leading-6 font-semibold text-fg-1">
                  {t("migration.title")}
                </h2>
              </div>
              <p class="text-[13px] leading-normal text-fg-2">
                {t("migration.body", {
                  label: current().label,
                  from: current().from,
                  to: current().to,
                })}
              </p>
              <p class="text-fs-0 leading-normal text-fg-3">{t("migration.hint")}</p>
            </div>
          </div>
        </>
      )}
    </Show>
  );
}
