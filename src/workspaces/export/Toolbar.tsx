/**
 * 导出工作流的 `toolsbar` mid 装配（`design/export.md` §2.1）。
 *
 * 两大类（崔总 2026-10-11）：
 *
 * ```text
 * [送入][移出][取消选中][重置队列] │ [旗标][标签][★★★★★][●×6][赞][踩]
 * └────── 队列动作（纯图标）──────┘   └── 筛选组（恒筛选、仅筛不标）──┘
 * ```
 *
 * * **只有一根分隔线**：分隔线只画在功能大类之间（`design/export.md` §2.1，
 *   旧口径「一个独立控件也算一组」作废）；
 * * 队列动作是**纯图标**：悬停提示＝`aria-label`，由 `Button` 内置的 `Tooltip` 给出；
 * * 筛选组**没有漏斗开关** —— 导出只有筛选模式、没有设置模式，点这些控件就是加/去筛选条件；
 *   条件与 browse **共用同一份**（唯一实现 `features/browse/mark-actions.ts::applyMarkIntent`，
 *   这里传 `forceFilter` 直接走筛选分支；`AGENTS.md` §2.12）。
 *   锁筛选不在导出出现（崔总 2026-10-11 口述）。
 */

import {
  IconEraser,
  IconPlayerStop,
  IconPlaylistAdd,
  IconPlaylistX,
  IconSquareOff,
  IconVersions,
} from "@tabler/icons-solidjs";
import { Button } from "../../components/ui/Button.tsx";
import { ToolsSeparator } from "../../components/ui/ToolsSeparator.tsx";
import { ColorRow, FlagFilterPair, LikeRow, RatingRow } from "../../features/browse/MarkControls.tsx";
import { TagFilterMenu } from "../../features/browse/TagFilterMenu.tsx";
import { applyMarkIntent, type MarkIntent } from "../../features/browse/mark-actions.ts";
import type { BrowseStore } from "../../features/browse/store.ts";
import { t } from "../../i18n/index.ts";
import type { ExportStore } from "./store.ts";
import { exportActions } from "./actions.ts";

export function ExportScopeTool(props: { store: ExportStore }) {
  return (
    <Button
      icon={<IconVersions size={14} />}
      onClick={() => props.store.cycleScope()}
    >
      {t(
        props.store.preferences.value().scope === "all"
          ? "export.scope.all"
          : props.store.preferences.value().scope === "edited"
            ? "export.scope.edited"
            : "export.scope.issues",
      )}
    </Button>
  );
}

export function ExportToolbar(props: { store: ExportStore; browse: BrowseStore }) {
  const store = props.store;
  const browse = props.browse;

  const filter = () => browse.filter();
  /** 导出侧一律按**筛选语义**应用（没有设置模式） */
  const apply = (intent: MarkIntent): void => {
    void applyMarkIntent(browse, intent, undefined, { forceFilter: true });
  };

  return (
    <div class="flex items-center gap-1">
      {/*
        队列动作（一大类，纯图标）：[送入队列][移出队列][取消选中][重置所有队列]。
        重置是全局且带确认的动作，但在大类上仍属「队列动作」，不再单独画线。
      */}
      <Button
        data-export-enqueue
        icon={<IconPlaylistAdd size={14} />}
        aria-label={t("export.enqueue")}
        disabled={!store.canEnqueue()}
        onClick={() => exportActions()?.enqueue()}
      />
      <Button
        data-export-remove
        icon={<IconPlaylistX size={14} />}
        aria-label={t("export.remove")}
        disabled={!store.canRemove()}
        onClick={store.removeSelected}
      />
      <Button
        data-export-clear
        icon={<IconSquareOff size={14} />}
        aria-label={t("export.clear")}
        disabled={store.activeSelection().ids.size === 0 && store.organizationHandoff().length === 0}
        onClick={() => store.clear()}
      />
      <Button
        data-export-reset
        icon={<IconEraser size={14} />}
        aria-label={t("export.reset")}
        disabled={![...store.queues().values()].some((queue) => queue.length > 0)}
        onClick={() => exportActions()?.requestReset()}
      />

      {/* 筛选组：与队列动作是两件不同的事 —— 这是本流程**唯一**的一根线 */}
      <ToolsSeparator />

      <FlagFilterPair
        mode={filter().flag?.mode ?? null}
        onPick={(mode) =>
          browse.patchFilter({ flag: mode === null ? null : { mode } })
        }
      />
      <TagFilterMenu store={browse} />
      <RatingRow
        active={(star) => {
          const threshold = filter().minRating;
          return threshold !== null && threshold !== undefined && star <= threshold;
        }}
        mixed={false}
        tip={(star) => t("browse.filterRatingAtLeast").replace("{n}", String(star))}
        onPick={(star) => apply({ kind: "rating", value: star })}
      />
      <ColorRow
        selected={(color) => (filter().colors ?? []).includes(color ?? "none")}
        mixed={false}
        onPick={(color) => apply({ kind: "color", value: color })}
      />
      <LikeRow
        pressed={(value) => (filter().likes ?? []).includes(value)}
        mixed={false}
        onPick={(value) => apply({ kind: "like", value })}
      />
    </div>
  );
}

export function ExportStopTool(props: { store: ExportStore }) {
  return (
    <Button
      data-export-stop
      icon={<IconPlayerStop size={14} />}
      aria-label={t("export.stopAll")}
      disabled={props.store.enabled().size === 0}
      onClick={() => props.store.stopAll()}
    />
  );
}
