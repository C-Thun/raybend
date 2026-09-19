/**
 * `PhotoMarks` —— 一张照片的**标记簇**（星标 / 色标 / 赞踩 / 旗标），只读。
 *
 * 为什么要有它：同一套「显示哪些标记、按什么规则显示」现在有三处要用 ——
 *   * `Tile` 顶部信息条（强制显示层 + 标准层，两处）；
 *   * 中列底部那条**状态栏**的看图态（人类 2026-09-20 定：看图时中列底部与 tiles 是
 *     **同一条**状态栏，文件名 + 锁之后就是这一簇）。
 * 三处各写一遍必然漂移（改了一处忘了另一处），所以抽成这一份、谁要谁用。
 *
 * 显示规则（唯一实现，别再抄）：
 *   * 星标：0 星什么都不显示；窄格子（`compact`）退化成「一颗星 + 数字」；
 *   * 色标：**只有认识的值才画点**（`isColorLabel` 守卫，类名映射来自 `lib/color-labels.ts`）；
 *   * 赞 / 踩：`like` / `dislike` 各一颗，`null` 不显示；
 *   * 旗标：只有 `pick`（「弃」那一态界面上已取消，见 `BrowseToolbar` 的说明）。
 *
 * 两个布局开关：
 *   * `spread`：星标与「色 / 赞 / 旗」之间要不要一段弹性空隙 ——
 *     `Tile` 顶部条要（星在左、其余贴右），状态栏不要（整簇靠右）；
 *   * `outlined`：图案要不要反色描边（tile 的强制显示层要，有底纹的层不要）。
 */

import { Show } from "solid-js";
import {
  IconFlagFilled,
  IconStar,
  IconStarFilled,
  IconThumbDownFilled,
  IconThumbUpFilled,
} from "@tabler/icons-solidjs";

import { t } from "../../i18n/index.ts";
import { COLOR_DOT_CLASS, isColorLabel } from "../../lib/color-labels.ts";

export interface PhotoMarksProps {
  /** 星级（0 = 不显示星标） */
  rating: number;
  /** 窄格子：星标退化成「一颗星 + 数字」 */
  compact?: boolean;
  colorLabel: string | null;
  flag?: "pick" | "reject" | null;
  like?: "like" | "dislike" | null;
  /** 图案要不要反色描边（`.tile-info-icon` / `.tile-info-dot`） */
  outlined?: boolean;
  /** 星标与其余标记之间要不要弹性空隙 */
  spread?: boolean;
  class?: string;
}

export function PhotoMarks(props: PhotoMarksProps) {
  /** 图案描边类（强制显示层才有） */
  const iconClass = (): string => (props.outlined === true ? "tile-info-icon" : "");
  /** 色标类名（不认识的值一律不画 —— 与工具条 / Tile 同一份映射） */
  const dotClass = (): string | null =>
    isColorLabel(props.colorLabel) ? COLOR_DOT_CLASS[props.colorLabel] : null;

  return (
    <span
      class={[
        "flex min-w-0 items-center gap-1",
        /*
         * `spread` 时本簇要**吃满宽度**（内部那段弹性空隙才能把「色 / 赞 / 旗」推到右边）；
         * 不给 spread 时按内容宽（调用方自己摆位置 —— 状态栏靠右那条弹性空隙在它外面）。
         */
        props.spread === true ? "flex-1" : "shrink-0",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
      data-photo-marks
    >
      {/* 星标：0 星什么都不显示；窄格子退化成「一颗星 + 数字」 */}
      <Show when={props.rating > 0}>
        <span
          class="flex shrink-0 items-center gap-0.5"
          aria-label={t("grid.rating", { n: props.rating })}
        >
          <Show
            when={props.compact !== true}
            fallback={
              <>
                <IconStarFilled size={12} class={iconClass()} aria-hidden="true" />
                <span class="text-fs-0 tnum">{props.rating}</span>
              </>
            }
          >
            {[1, 2, 3, 4, 5].map((index) =>
              index <= props.rating ? (
                <IconStarFilled size={11} class={iconClass()} aria-hidden="true" />
              ) : (
                <IconStar size={11} class={["opacity-50", iconClass()].join(" ")} aria-hidden="true" />
              ),
            )}
          </Show>
        </span>
      </Show>

      <Show when={props.spread === true}>
        <span class="min-w-0 flex-1" />
      </Show>

      {/* 颜色标记：只有认识的值才画点 */}
      <Show when={dotClass()}>
        {(dot) => (
          <span
            class={[
              "size-2 shrink-0 rounded-full",
              props.outlined === true ? "tile-info-dot" : "",
              dot(),
            ]
              .filter(Boolean)
              .join(" ")}
            aria-label={t("grid.color_label")}
          />
        )}
      </Show>

      {/*
        赞 / 踩（人类 2026-09-20：**这两个也要在图片顶部显示**）。
        与工具条同一套三态语义：`like` = 大拇指朝上、`dislike` = 朝下、`null` = 不显示。
      */}
      <Show when={props.like === "like"}>
        <IconThumbUpFilled size={11} class={iconClass()} aria-hidden="true" />
      </Show>
      <Show when={props.like === "dislike"}>
        <IconThumbDownFilled size={11} class={iconClass()} aria-hidden="true" />
      </Show>

      {/*
        旗标用**实心小旗**（人类 2026-09-19：以前这里是星星，与星标撞在一起分不清）。
        「弃」的那一态在界面上已经取消了（见 BrowseToolbar 的说明），所以这里只有一种旗。
      */}
      <Show when={props.flag === "pick"}>
        <IconFlagFilled size={11} class={iconClass()} aria-hidden="true" />
      </Show>
    </span>
  );
}
