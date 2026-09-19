/**
 * 看图态的底部状态栏（`BROWSE.md` §5.8、`design/browse.md` §2.5）。
 *
 * ```text
 * ┌──────────────────────────────────────────────────────────────┐
 * │ P0005.RW2 [🔒]                       ★★★☆☆  ●  ⚑  👍        │
 * └──────────────────────────────────────────────────────────────┘
 * ```
 *
 * * **左边**：文件名，**紧挨着文件名右边**就是锁徽标 —— 人类点名的位置关系（不要挪到别处）；
 * * **右边**：这张照片的标记（星 / 色标 / 旗标 / 喜欢）；
 * * 与 tiles 模式的底部控制条**不是同一个东西**：那边是「计数 / 当前目录 / 视图控制」。
 *
 * 数据来自看图件当前那张照片自带的 `marks` / `flag`（见 `viewer/store.ts` 的 `ViewerPhoto`）——
 * 所以这里**不发请求**，也不会因为「为了显示四个数」而卡一下。
 */

import { For, Show } from "solid-js";
import {
  IconBan,
  IconFlag,
  IconLock,
  IconStar,
  IconStarFilled,
  IconThumbDown,
  IconThumbUp,
} from "@tabler/icons-solidjs";

import { t } from "../../i18n/index.ts";
import type { ViewerPhoto } from "../../components/ui/viewer/index.ts";

/** 色标的类名映射与工具条**共用同一份**（`lib/color-labels.ts`）。 */
import { COLOR_DOT_CLASS, isColorLabel } from "../../lib/color-labels.ts";

export interface ViewerStatusBarProps {
  /** 当前正在看的那张（看图件自己的 `current()`）。 */
  photo: ViewerPhoto | null;
  class?: string;
}

export function ViewerStatusBar(props: ViewerStatusBarProps) {
  const marks = () => props.photo?.marks;

  /**
   * 色标 → 类名（不是色标就是 `null`）。
   *
   * 先算成字符串再交给 `Show`：`isColorLabel` 是**类型守卫**，
   * 直接写 `when={isColorLabel(...)}` 的话子函数拿到的是 `true` 而不是那个值。
   */
  const colorDotClass = (): string | null => {
    const value = marks()?.colorLabel;
    return isColorLabel(value) ? COLOR_DOT_CLASS[value] : null;
  };

  return (
    <div
      class={[
        // **无边线设计**（DESIGN.md §6）：状态条与照片区靠面色区分，不上分隔线
        "flex h-8 shrink-0 items-center gap-3 bg-surface-bar px-2",
        "text-fs-2 text-fg-3",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
      data-viewer-status="open"
    >
      {/* 左：文件名 + 紧挨着的锁 */}
      <span class="min-w-0 shrink truncate text-fg-1" title={props.photo?.fileName ?? ""}>
        {props.photo?.fileName ?? ""}
      </span>
      <Show when={(marks()?.lockLevel ?? 0) > 0}>
        <span
          class="flex shrink-0 items-center gap-1 rounded-(--radius) bg-surface-layer px-1.5 text-fs-0 text-fg-2"
          title={
            (marks()?.lockLevel ?? 0) >= 2 ? t("browse.lockNoEdit") : t("browse.lockNoDelete")
          }
        >
          <IconLock size={11} aria-hidden="true" />
          {(marks()?.lockLevel ?? 0) >= 2 ? 2 : 1}
        </span>
      </Show>

      <span class="min-w-0 flex-1" />

      {/* 右：这张照片的标记（只显示「有值」的那些 —— 状态栏不是设置控件） */}
      <div class="flex shrink-0 items-center gap-2">
        <Show when={(marks()?.rating ?? 0) > 0}>
          <span class="flex items-center" aria-label={t("grid.rating").replace("{n}", String(marks()?.rating ?? 0))}>
            <For each={[1, 2, 3, 4, 5]}>
              {(star) => (
                <span class={star <= (marks()?.rating ?? 0) ? "text-brand" : "text-fg-3"}>
                  {star <= (marks()?.rating ?? 0) ? (
                    <IconStarFilled size={12} aria-hidden="true" />
                  ) : (
                    <IconStar size={12} aria-hidden="true" />
                  )}
                </span>
              )}
            </For>
          </span>
        </Show>

        <Show when={colorDotClass()}>
          {(dot) => (
            <span
              class={["size-3 shrink-0 rounded-full ring-1 ring-line-2", dot()].join(" ")}
              aria-label={String(marks()?.colorLabel ?? "")}
            />
          )}
        </Show>

        <Show when={props.photo?.flag === "pick"}>
          <IconFlag size={13} class="text-brand" aria-label={t("browse.pick")} />
        </Show>
        <Show when={props.photo?.flag === "reject"}>
          <IconBan size={13} class="text-fg-2" aria-label={t("browse.reject")} />
        </Show>

        <Show when={marks()?.likeState === "like"}>
          <IconThumbUp size={13} class="text-brand" aria-label={t("browse.like")} />
        </Show>
        <Show when={marks()?.likeState === "dislike"}>
          <IconThumbDown size={13} class="text-fg-2" aria-label={t("browse.reject")} />
        </Show>
      </div>
    </div>
  );
}
