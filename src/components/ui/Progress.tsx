/**
 * `Progress` —— 进度条（全项目唯一一份）。
 *
 * 规格走 `memory/DESIGN.md` 与 `design/main.md` §3.5 / `design/editor.md` §3.1.2：
 * 高 6、轨道 `surface-track`、填充 `brand`、两端圆角。
 *
 * 两种状态：
 *   * **确定态**（`value` 是数字）：按百分比画填充；
 *   * **不确定态**（`value` 为 `null` / 省略）：**没有总数**时不编百分比，
 *     画一道来回跑的亮条（`memory/FUNCTION-REPOSITORY.md` §4.4 的「扫描阶段」口径）。
 *
 * 为什么单独抽出来：导入（`ImportProgressDialog`）与 LUT 导入（编辑器左列）是同一件事的两处入口，
 * 各写一条进度条必然久而久之长得不一样（`AGENTS.md` §2.12）。
 */

import { Show, splitProps, type JSX } from "solid-js";

export interface ProgressProps extends JSX.HTMLAttributes<HTMLDivElement> {
  /** 0–100。`null` / `undefined` = 不确定态（没有总数） */
  value?: number | null;
  /** 无障碍名（进度条自己不产出文字，读屏靠它）。调用方传 i18n 文案 */
  label?: string;
}

export function Progress(props: ProgressProps) {
  const [local, rest] = splitProps(props, ["value", "label", "class"]);

  /** 夹到 0–100；`null` 表示不确定态 */
  const percent = (): number | null => {
    const raw = local.value;
    if (raw == null || !Number.isFinite(raw)) return null;
    return Math.min(100, Math.max(0, raw));
  };

  return (
    <div
      {...rest}
      role="progressbar"
      aria-label={local.label}
      aria-valuemin={percent() === null ? undefined : 0}
      aria-valuemax={percent() === null ? undefined : 100}
      aria-valuenow={percent() ?? undefined}
      class={[
        "h-1.5 w-full overflow-hidden rounded-ui bg-surface-track",
        local.class ?? "",
      ].join(" ")}
    >
      <Show
        when={percent() !== null}
        fallback={
          // 不确定态：走 CSS 动画（`motion.css` 的 `rb-progress-pulse`），尊重减少动态效果
          <div class="rb-progress-pulse h-full w-1/3 rounded-ui bg-brand" />
        }
      >
        <div
          class="h-full rounded-ui bg-brand transition-[width] duration-200"
          style={{ width: `${percent() ?? 0}%` }}
        />
      </Show>
    </div>
  );
}
