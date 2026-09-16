/**
 * `GridControlBar` —— 照片网格底部的控制条（`design/main.md` §3.2）。
 *
 * ```text
 * [ 1 248 张 ]  ←弹性→  [D:\Photos]  ←弹性→  [加载提示] [按时间] [缩放滑块]
 * ```
 *
 * 四个细节都是设计明确要求的：
 *   - 中间那一段**真正居中** → 两侧各一个弹性 spacer；
 *   - **中间只放「当前目录」**，「按时间」归到右侧的控件组（2026-09-16 人类要求：
 *     它是视图选项，应当挨着缩放，而不是和目录名凑一起）；
 *   - `按时间` 是**可按下式**按钮（与吸附磁铁同款），不是圆点开关；
 *   - 计数用 `formatCount`（中文空格分组：`1 248 张`）。
 */

import { Show } from "solid-js";
import { IconClock, IconGridDots, IconZoomIn, IconZoomOut } from "@tabler/icons-solidjs";
import { PathText } from "../../components/ui/PathText.tsx";
import { Slider } from "../../components/ui/Slider.tsx";
import { ToggleBlock } from "../../components/ui/ToggleBlock.tsx";
import { t } from "../../i18n/index.ts";
import { formatCount, type GroupingLocale } from "../../lib/format.ts";
import { TILE_SIZE_STEPS } from "../../lib/tile-flow.ts";

export interface GridControlBarProps {
  /** 当前目录里的张数 */
  count: number;
  /** 当前浏览的目录（`null` = 还没选） */
  dir: string | null;
  /** 是否处于「按时间」模式 */
  byTime: boolean;
  onByTimeChange: (value: boolean) => void;
  /** 档位下标（`lib/tile-flow.ts` 的 9 档） */
  tileStep: number;
  onTileStepChange: (step: number) => void;
  /** 语言（数字分组与日期都用它） */
  locale: GroupingLocale;
  /** 正在补读拍摄时间（按时间模式下给个提示） */
  loadingTimes?: boolean;
  class?: string;
}

export function GridControlBar(props: GridControlBarProps) {
  return (
    <div
      class={[
        "flex h-bar-tool-h shrink-0 items-center gap-2 border-t border-t-surface-track px-3",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      {/* 计数 */}
      <span class="shrink-0 text-fs-1 text-fg-2 tnum">
        {t("grid.count", { n: formatCount(props.count, props.locale) })}
      </span>

      <span class="min-w-0 flex-1" />

      {/* 当前目录（缩写 + 悬停看全路径）—— 中间只留这一项，真正居中 */}
      <Show when={props.dir}>
        {(dir) => (
          <PathText
            path={dir()}
            maxLength={40}
            icon={<IconGridDots size={14} />}
            class="max-w-64 shrink-0"
          />
        )}
      </Show>

      <span class="min-w-0 flex-1" />

      {/* 正在补读时间：给一句轻提示，别让用户以为界面卡住 */}
      <Show when={props.loadingTimes}>
        <span class="shrink-0 text-fs-0 text-fg-3">{t("common.loading")}</span>
      </Show>

      {/* 按时间（可按下式，见 design/main.md §4.8.1）——
          放在**缩放组件的左边**：它是视图选项，跟缩放是一类东西 */}
      <ToggleBlock
        pressed={props.byTime}
        onPressedChange={props.onByTimeChange}
        label={t("grid.by_time")}
        icon={<IconClock size={16} />}
      >
        <span class="whitespace-nowrap">{t("grid.by_time")}</span>
      </ToggleBlock>

      {/* 缩放：9 档，离散 */}
      <Slider
        value={props.tileStep}
        min={0}
        max={TILE_SIZE_STEPS.length - 1}
        label={t("grid.zoom")}
        onValueChange={props.onTileStepChange}
        startIcon={<IconZoomOut size={14} />}
        endIcon={<IconZoomIn size={14} />}
        class="w-40 shrink-0"
      />
    </div>
  );
}
