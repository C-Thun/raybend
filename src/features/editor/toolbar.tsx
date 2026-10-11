/**
 * 编辑模式的 `toolsbar` 装配（`design/editor.md` §2.1）。
 * 左段只放 LUT 面板开关；中段同一组按 SOOC/RAW、撤销/重做、
 * 裁切 / 旋转 / 对比排序并整体居中；右段放统一重置。
 */

import { For, type JSX } from "solid-js";
import {
  IconArrowsHorizontal,
  IconBookmark,
  IconCrop,
  IconPalette,
  IconRefresh,
  IconRotateClockwise,
  IconArrowBackUp,
  IconArrowForwardUp,
  IconWand,
} from "@tabler/icons-solidjs";

import { ToggleBlock } from "../../components/ui/ToggleBlock.tsx";
import { ToolsSeparator } from "../../components/ui/ToolsSeparator.tsx";
import { SegmentedControl } from "../../components/ui/SegmentedControl.tsx";
import { Button } from "../../components/ui/Button.tsx";
import type { DevelopEditBase } from "../../api/types.ts";
import { t } from "../../i18n/index.ts";
import type { MessageKey } from "../../i18n/index.ts";
import type { EditorStore, EditorTool } from "./store.ts";

/** 工具按钮的图标与文案（顺序即 toolsbar 里的顺序）。 */
const TOOL_SPEC: readonly {
  id: EditorTool;
  icon: typeof IconCrop;
  labelKey: MessageKey;
}[] = [
  { id: "crop", icon: IconCrop, labelKey: "editor.tool.crop" },
  { id: "rotate", icon: IconRotateClockwise, labelKey: "editor.tool.rotate" },
  { id: "compare", icon: IconArrowsHorizontal, labelKey: "editor.tool.compare" },
];

export interface EditorToolbarProps {
  store: EditorStore;
  /** 没有可编辑的照片时禁用（空态下工具无从作用） */
  enabled: boolean;
  canAutoAdjust?: () => boolean;
  onAutoAdjust?: () => void;
}

export interface EditorSourceHistoryProps extends EditorToolbarProps {
  onBaseChange: (base: DevelopEditBase) => void;
  history: {
    state: () => { canUndo: boolean; canRedo: boolean; undoLabel: string | null; redoLabel: string | null };
    undo: () => void;
    redo: () => void;
  };
}

/** mid：编辑源与历史动作在这一组按钮的最左边，整组按工具栏全宽居中。 */
export function EditorToolbar(props: EditorSourceHistoryProps): JSX.Element {
  const undoTitle = (): string => props.history.state().undoLabel === null
    ? t("browse.undo")
    : t("browse.undoWith").replace("{label}", props.history.state().undoLabel ?? "");
  const redoTitle = (): string => props.history.state().redoLabel === null
    ? t("browse.redo")
    : t("browse.redoWith").replace("{label}", props.history.state().redoLabel ?? "");
  return (
    <div class="flex h-full shrink-0 items-center gap-1">
      {/*
        编辑源（SOOC/RAW）：形态自带**边框感**，右侧只留一点留白，不再叠一根竖线
        （崔总 2026-10-11：边框感 + 竖线太重复）。
      */}
      <SegmentedControl
        class="me-2"
        value={props.store.editBase()}
        onValueChange={(value) => props.onBaseChange(value as DevelopEditBase)}
        label={t("editor.base.label")}
        options={[
          { value: "sooc", label: t("editor.base.sooc"), disabled: !props.enabled || !props.store.editBaseAvailable().bitmap },
          { value: "raw", label: t("editor.base.raw"), disabled: !props.enabled || !props.store.editBaseAvailable().raw },
        ]}
      />

      {/* 可逆的历史动作是一组（纯图标；提示里带具体动作名） */}
      <div class="flex items-center gap-0.5">
        <Button variant="ghost" disabled={!props.history.state().canUndo}
          aria-label={undoTitle()} icon={<IconArrowBackUp size={14} />}
          onClick={props.history.undo} />
        <Button variant="ghost" disabled={!props.history.state().canRedo}
          aria-label={redoTitle()} icon={<IconArrowForwardUp size={14} />}
          onClick={props.history.redo} />
      </div>

      {/* 本流程**唯一**一根分隔线（崔总 2026-10-11：只在功能大类之间用） */}
      <ToolsSeparator />

      <Button variant="ghost" disabled={!props.canAutoAdjust?.()} loading={props.store.autoAdjusting()}
        tip={t("editor.autoAdjustHint")} aria-label={t("editor.autoAdjust")} icon={<IconWand size={14} />}
        onClick={() => props.onAutoAdjust?.()} />

      {/* 画布工具（裁切/旋转/对比）：互斥的一组开关 */}
      <div class="flex items-center gap-0.5">
        <For each={TOOL_SPEC}>
          {(tool) => (
            <ToggleBlock
              pressed={props.store.tool() === tool.id}
              disabled={!props.enabled}
              icon={<tool.icon size={14} />}
              label={t(tool.labelKey)}
              onClick={() => props.store.toggleTool(tool.id)}
            >
              {t(tool.labelKey)}
            </ToggleBlock>
          )}
        </For>
      </div>
    </div>
  );
}

/** 左段：与左列对应的 LUT 面板开关。 */
export function EditorPanelToggles(props: EditorToolbarProps): JSX.Element {
  return <ToggleBlock
    pressed={props.store.lutVisible()}
    icon={<IconPalette size={14} />}
    label={t("editor.toolsbar.lut")}
    onClick={() => props.store.toggleLut()}
  />;
}

/** 右段：重置之后是定稿；两者都走工作区动作槽。 */
export function EditorResetTool(props: EditorToolbarProps & {
  onRequestReset: () => void;
  canReset?: () => boolean;
  canFinalize?: () => boolean;
  onFinalize?: () => void;
}): JSX.Element {
  return <div class="flex items-center gap-1">
    <Button variant="ghost" icon={<IconRefresh size={14} />} aria-label={t("editor.panel.resetAll")}
      disabled={!props.enabled || !(props.canReset?.() ?? props.store.resetStage() !== "none")} onClick={props.onRequestReset} />
    <Button variant="primary" icon={<IconBookmark size={14} />} disabled={!props.canFinalize?.()} onClick={() => props.onFinalize?.()}>
      {t("editor.issue.toolbar")}
    </Button>
  </div>;
}
