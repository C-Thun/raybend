/**
 * 编辑模式的 `toolsbar` 装配（`design/editor.md` §2.1）。
 *
 * 2026-11-05 崔总重排后的分工：
 * * **left**：左列四个面板开关（编辑区 / 定稿 / LUT / 预设），四选一互斥；
 * * **mid**：编辑源 | 重置 · 撤销 · 重做 · 眼睛 | 分隔线 | 拟合 · 画布工具，整组居中；
 * * **right**：编辑流为空（眼睛与重置搬进了 mid —— 它们作用于当前照片，
 *   跟撤销/重做是同一类动作，放一起才不用跨半个屏幕去找）。
 */

import { For, type JSX } from "solid-js";
import {
  IconAdjustmentsHorizontal,
  IconArrowsHorizontal,
  IconBookmark,
  IconCardsFilled,
  IconCrop,
  IconPalette,
  IconRefresh,
  IconRotateClockwise,
  IconArrowBackUp,
  IconArrowForwardUp,
  IconWand,
  IconEye,
  IconEyeOff,
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
  /** 统一重置（两层确认）；搬进 mid 后与撤销/重做同组（崔总 2026-11-05） */
  onRequestReset: () => void;
  canReset: () => boolean;
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

      {/*
        可逆的历史动作是一组（纯图标；提示里带具体动作名）。
        2026-11-05 崔总：**重置在最左、眼睛在最右** —— 顺序为 重置 · 撤销 · 重做 · 眼睛。
      */}
      <div class="flex items-center gap-0.5">
        <Button variant="ghost" icon={<IconRefresh size={14} />} aria-label={t("editor.panel.resetAll")}
          disabled={!props.enabled || !props.canReset()} onClick={props.onRequestReset} />
        <Button variant="ghost" disabled={!props.history.state().canUndo}
          aria-label={undoTitle()} icon={<IconArrowBackUp size={14} />}
          onClick={props.history.undo} />
        <Button variant="ghost" disabled={!props.history.state().canRedo}
          aria-label={redoTitle()} icon={<IconArrowForwardUp size={14} />}
          onClick={props.history.redo} />
        {/*
          `显示控制点`（眼睛）：**只管标记**（圆点 / 引线 / 名字 / handle）显示与否 ——
          纯视图偏好，`localStorage`（不进 profile / 哈希 / 撤销 / XMP）。
          隐藏时抓取也一起关掉（画布上点不到看不见的圆片）。
        */}
        <Button
          variant="ghost"
          icon={props.store.showPoints() ? <IconEye size={14} /> : <IconEyeOff size={14} />}
          aria-label={t("editor.area.showPoints")}
          aria-pressed={props.store.showPoints()}
          disabled={!props.enabled}
          onClick={() => props.store.setShowPoints(!props.store.showPoints())}
        />
      </div>

      {/* 本流程**唯一**一根分隔线（崔总 2026-10-11：只在功能大类之间用） */}
      <ToolsSeparator />

      {/* 拟合基础曲线：**带文字**（崔总 2026-11-05）—— 一颗没有名字的魔术棒看不懂 */}
      <Button variant="ghost" disabled={!props.canAutoAdjust?.()} loading={props.store.autoAdjusting()}
        tip={t("editor.autoAdjustHint")} icon={<IconWand size={14} />}
        onClick={() => props.onAutoAdjust?.()}>{t("editor.autoAdjust")}</Button>

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

/** 左段：与左列对应的四个面板开关（编辑区 / 定稿 / LUT / 预设；同一时刻只开一个）。 */
export function EditorPanelToggles(props: EditorToolbarProps): JSX.Element {
  return <>
    <ToggleBlock
      pressed={props.store.leftPanel() === "areas" && props.store.leftVisible()}
      icon={<IconAdjustmentsHorizontal size={14} />}
      label={t("editor.toolsbar.areas")}
      onClick={() => props.store.toggleLeft("areas")}
    />
    {/* 图标与 browse tile 左下角的「定稿数量」角标同款（`IconCardsFilled`）—— 同一件事同一张脸 */}
    <ToggleBlock
      pressed={props.store.issuesVisible()}
      icon={<IconCardsFilled size={14} />}
      label={t("editor.toolsbar.issues")}
      onClick={() => props.store.toggleLeft("issues")}
    />
    <ToggleBlock
      pressed={props.store.lutVisible()}
      icon={<IconPalette size={14} />}
      label={t("editor.toolsbar.lut")}
      onClick={() => props.store.toggleLeft("lut")}
    />
    {/* 预设（崔总 2026-11-05：从右栏页签改回左列独立面板，入口放 LUT 右边） */}
    <ToggleBlock
      pressed={props.store.presetVisible()}
      icon={<IconBookmark size={14} />}
      label={t("editor.toolsbar.preset")}
      onClick={() => props.store.toggleLeft("preset")}
    />
  </>;
}

