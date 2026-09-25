/**
 * 编辑模式的 `toolsbar` 装配（`design/editor.md` §2.1）。
 * 左段只放 LUT 面板开关；中段同一组按 SOOC/RAW、撤销/重做、
 * 裁切 / 旋转 / 对比排序并整体居中；右段放统一重置。
 */

import { For, type JSX } from "solid-js";
import {
  IconArrowsHorizontal,
  IconCrop,
  IconPalette,
  IconRotateClockwise,
  IconArrowBackUp,
  IconArrowForwardUp,
  IconWand,
} from "@tabler/icons-solidjs";

import { ToggleBlock } from "../../components/ui/ToggleBlock.tsx";
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
      <SegmentedControl
        value={props.store.editBase()}
        onValueChange={(value) => props.onBaseChange(value as DevelopEditBase)}
        label={t("editor.base.label")}
        options={[
          { value: "sooc", label: t("editor.base.sooc"), disabled: !props.enabled || !props.store.editBaseAvailable().bitmap },
          { value: "raw", label: t("editor.base.raw"), disabled: !props.enabled || !props.store.editBaseAvailable().raw },
        ]}
      />
      <Button variant="ghost" disabled={!props.history.state().canUndo} title={undoTitle()}
        aria-label={undoTitle()} icon={<IconArrowBackUp size={14} />}
        onClick={props.history.undo}>{t("browse.undo")}</Button>
      <Button variant="ghost" disabled={!props.history.state().canRedo} title={redoTitle()}
        aria-label={redoTitle()} icon={<IconArrowForwardUp size={14} />}
        onClick={props.history.redo}>{t("browse.redo")}</Button>
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
  );
}

/** 左段：与左列对应的 LUT 面板开关。 */
export function EditorPanelToggles(props: EditorToolbarProps): JSX.Element {
  return <ToggleBlock
    pressed={props.store.lutVisible()}
    icon={<IconPalette size={14} />}
    label={t("editor.toolsbar.lut")}
    onClick={() => props.store.toggleLut()}
  >
    {t("editor.toolsbar.lut")}
  </ToggleBlock>;
}

/** 右段：自动调整和统一重置；命令面板复用相同动作。 */
export function EditorResetTool(props: EditorToolbarProps & {
  onRequestReset: () => void; onAutoAdjust: () => void;
}): JSX.Element {
  return <div class="flex items-center gap-1">
    <Button variant="ghost" disabled={!props.enabled || props.store.autoAdjusting()}
      title={t("editor.autoAdjustHint")} icon={<IconWand size={14} />}
      onClick={props.onAutoAdjust}>{t(props.store.autoAdjusting() ? "editor.autoAdjusting" : "editor.autoAdjust")}</Button>
    <Button variant="ghost" disabled={!props.enabled} onClick={props.onRequestReset}>
      {t("editor.panel.resetAll")}
    </Button>
  </div>;
}
