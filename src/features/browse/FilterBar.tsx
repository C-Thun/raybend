/**
 * 筛选结果区（`BROWSE.md` §3.1、`specs/M2-W2-tail.md` 3.3/3.4、画布 `States / 浏览状态` 的 ②）。
 *
 * ```text
 * │ (≥3 星 ×) (红色 ×) (有旗标 ×)   共 42 张            [全部 | 任一] │
 * ```
 *
 * 三条：
 *
 * 1. **条件要看得见**：开关是按下去的，用户看不见到底筛了什么 —— 所以摊成 chips，
 *    每条都能单独摘掉（摘一条只动它自己，见 `filter.ts` 的测试）。
 * 2. **「共 N 张」是筛选后的数**（`store.total()` 跟的是当前查询）——
 *    这是「筛完还剩多少」的即时反馈，也是判断筛得太狠的依据。
 * 3. **全部 / 任一**只在**两个以上条件**时才出现：只有一个条件时问「与还是或」没意义。
 *    默认**「全部」（组间与）**（人类 2026-09-19 定的口径：选哪个标记就只显示哪个标记的图）。
 *
 * 排序控件**不在这里**：它跟着底部控制条（与「按时间」「缩放」同排），
 * 因为排序在没开筛选时也要能用 —— 画布 ② 里那个 `SortBar` 的位置会在阶段 6 的画布复核里同步。
 */

import { For, Show, type JSX } from "solid-js";
import { IconX } from "@tabler/icons-solidjs";

import { SegmentedControl } from "../../components/ui/SegmentedControl.tsx";
import { t } from "../../i18n/index.ts";
import { chipKey, filterChips, removeChip, type FilterChip } from "./filter.ts";
import { colorText, lockText } from "./labels.ts";
import type { BrowseStore } from "./store.ts";

export interface FilterBarProps {
  store: BrowseStore;
  class?: string;
}

export function FilterBar(props: FilterBarProps): JSX.Element {
  const chips = () => filterChips(props.store.filter());

  /** chip 的文案 —— 与工具条上的控件用**同一套**（`labels.ts`） */
  const label = (chip: FilterChip): string => {
    switch (chip.kind) {
      case "rating":
        // 阈值语义：chip 上必须写出「≥」，否则会被读成「正好 3 星」
        return t("browse.filterRatingAtLeast").replace("{n}", String(chip.value));
      case "color":
        return colorText(chip.value === "none" ? null : chip.value);
      case "like":
        return chip.value === "like"
          ? t("browse.like")
          : chip.value === "dislike"
            ? t("browse.dislike")
            : t("browse.likeNone");
      case "lock":
        return lockText(chip.value);
      case "flag":
        return chip.value === "pick"
          ? t("browse.flagWith")
          : chip.value === "reject"
            ? t("browse.flagRejected")
            : t("browse.flagWithout");
    }
  };

  return (
    <Show when={props.store.filterMode()}>
      <div
        data-filter-bar="open"
        class={[
          // **无边线设计**（DESIGN.md §6）：面板之间靠面色区分，不上分隔线
        "flex min-h-8 shrink-0 flex-wrap items-center gap-1.5 px-2 py-1",
          props.class ?? "",
        ]
          .filter(Boolean)
          .join(" ")}
      >
        <Show
          when={chips().length > 0}
          fallback={
            <span class="text-fs-2 text-fg-3">{t("browse.filterEmpty")}</span>
          }
        >
          <For each={chips()}>
            {(chip) => (
              <span
                data-filter-chip={chipKey(chip)}
                class="inline-flex h-6 items-center gap-0.5 rounded-ui bg-surface-bar pl-2 pr-0.5 text-fs-2 text-fg-1"
              >
                {label(chip)}
                <button
                  type="button"
                  aria-label={t("browse.filterRemove").replace("{name}", label(chip))}
                  class="flex h-4 w-4 items-center justify-center rounded-ui text-fg-3 hover:bg-state-hover hover:text-fg-1"
                  onClick={() =>
                    props.store.patchFilter(removeChip(props.store.filter(), chip))
                  }
                >
                  <IconX size={11} aria-hidden="true" />
                </button>
              </span>
            )}
          </For>

          {/* 筛完还剩多少张：`store.total()` 跟的是当前查询 */}
          <span class="ml-1 text-fs-2 text-fg-2" data-filter-count>
            {t("browse.count").replace("{n}", String(props.store.total()))}
          </span>
        </Show>

        {/* 两个以上条件才问「任一 / 全部」（一个条件时这个问题没有意义） */}
        <Show when={chips().length >= 2}>
          <div class="ml-auto">
            <SegmentedControl
              value={props.store.filter().combinator ?? "and"}
              options={[
                { value: "and", label: t("browse.filterAll") },
                { value: "or", label: t("browse.filterAny") },
              ]}
              onValueChange={(value) =>
                props.store.patchFilter({ combinator: value === "and" ? "and" : "or" })
              }
              label={t("browse.filterCombinator")}
            />
          </div>
        </Show>
      </div>
    </Show>
  );
}
