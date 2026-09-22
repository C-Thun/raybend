/**
 * 编辑模式的 `toolsbar` 装配（`design/editor.md` §2.1）。
 *
 * 三段式（`AGENTS.md` §11.1）：**左段**放与 workspace 左列有关的面板开关（LUT），
 * **中段**放画布工具（裁切 / 旋转 / 对比，互斥），**右段**暂时是空的（未来加东西才出现在那一侧）。
 *
 * ```text
 * [LUT]              [裁切][旋转][对比]                    （右侧留白）
 *  └ toolsbar left     └ toolsbar / center
 * ```
 *
 * 三工具互斥由 `store.tool()` 保证（同一时刻只有一个控制块）；**再点一次同一个 = 退出**
 * （`.pd`：裁切可反复点击开关）。三个按钮与左段的 LUT 都已登记进命令注册表
 * （`features/commands/catalog.ts`），所以命令面板 / 快捷键 / 菜单三处都能触发同一个动作。
 */

import { For, type JSX } from "solid-js";
import {
  IconArrowsHorizontal,
  IconCrop,
  IconPalette,
  IconRotateClockwise,
} from "@tabler/icons-solidjs";

import { ToggleBlock } from "../../components/ui/ToggleBlock.tsx";
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

/** 中段：三个画布工具（互斥）。 */
export function EditorToolbar(props: EditorToolbarProps): JSX.Element {
  return (
    <For each={TOOL_SPEC}>
      {(tool) => (
        <ToggleBlock
          pressed={props.store.tool() === tool.id}
          disabled={!props.enabled}
          icon={<tool.icon size={14} />}
          label={t(tool.labelKey)}
          title={t("editor.tool.pending")}
          onClick={() => props.store.toggleTool(tool.id)}
        >
          {t(tool.labelKey)}
        </ToggleBlock>
      )}
    </For>
  );
}

/** 左段：workspace 左列的面板开关（现在只有 LUT；一组互斥开关的第一个）。 */
export function EditorPanelToggles(props: EditorToolbarProps): JSX.Element {
  return (
    <ToggleBlock
      pressed={props.store.lutVisible()}
      icon={<IconPalette size={14} />}
      label={t("editor.toolsbar.lut")}
      onClick={() => props.store.toggleLut()}
    >
      {t("editor.toolsbar.lut")}
    </ToggleBlock>
  );
}
