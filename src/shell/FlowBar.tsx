/**
 * `FlowBar` —— 工作流行（`design/main.md` §2.2）。
 *
 * 左：工作流切换器（**有序流水线**，顺序固定不重排）—— 用**专属**的 `FlowSwitcher`，
 *     不是通用分段控件（理由见那个文件顶部：它是外壳的招牌部件）
 * 中：弹性空白
 * 右：`ExifStrip`（三组 `easy copy`）+ 吸附开关
 *
 * 两处「刻意如此」：
 *   1. **这一行比别处更圆润、也更大**（胶囊形轨道与色块）—— 设计稿明确要求，
 *      因为它是「阶段切换」，与旁边的方形工具按钮在语义上不同
 *   2. **吸附开关目前只是视觉**：设计稿把它标为「以后会加」的占位。
 *      M1 不做吸附行为（已登记 `FUTURE.md`），但**状态的存续**是真的 ——
 *      按下去就是按下去，不会自己弹回来
 */

import { createSignal } from "solid-js";
import type { Component } from "solid-js";
import { Dynamic } from "solid-js/web";
import {
  IconAdjustmentsHorizontal,
  IconFolderDown,
  IconMagnet,
  IconPhoto,
  IconUpload,
} from "@tabler/icons-solidjs";
import { ToggleBlock } from "../components/ui/ToggleBlock.tsx";
import { ExifStrip, type ExifData } from "../features/exif-strip/index.ts";
import { t } from "../i18n";
import { WORKFLOWS, WORKFLOW_LABEL_KEY, type WorkflowId } from "./flow.ts";
import { FlowSwitcher } from "./FlowSwitcher.tsx";
import type { ShellStore } from "./store.ts";

export interface FlowBarProps {
  store: ShellStore;
  /** 当前照片的 EXIF；没有选中照片时为 `null` → EXIF 区显示空态 */
  exif?: ExifData | null;
}

export function FlowBar(props: FlowBarProps) {
  const [snap, setSnap] = createSignal(false);

  return (
    <div class="flex h-bar-flow-h shrink-0 items-center gap-2 bg-surface-main px-pad-x">
      <FlowSwitcher<WorkflowId>
        label={t("flow.label")}
        value={props.store.workflow()}
        onValueChange={props.store.setWorkflow}
        options={WORKFLOWS.map((id) => ({
          value: id,
          label: t(WORKFLOW_LABEL_KEY[id]),
          icon: <Dynamic component={WORKFLOW_ICONS[id]} size={16} />,
        }))}
      />

      {/* 弹性空白：把 EXIF 与开关推到右边 */}
      <div class="h-px min-w-2 flex-1" />

      <ExifStrip data={props.exif ?? null} class="justify-end" />

      <ToggleBlock
        pressed={snap()}
        onPressedChange={setSnap}
        label={t("flow.tool.snap")}
        icon={<IconMagnet size={16} />}
      />
    </div>
  );
}

/**
 * 工作流的图标（图标属于视图层，`flow.ts` 保持纯数据）。
 *
 * ⚠️ 存**组件引用**而不是 JSX 元素：模块级写 `<IconFolder/>` 会在 `render` 之外创建组件，
 * Solid 会警告「computations created outside a createRoot ... will never be disposed」
 * （实测：陈列室与外壳页各刷 5 条）。写成引用后由渲染期实例化，警告消失。
 */
const WORKFLOW_ICONS: Record<WorkflowId, Component<{ size?: number }>> = {
  // 流水线语义：照片**进来** → 看 → 调 → **出去**
  import: IconFolderDown,
  browse: IconPhoto,
  edit: IconAdjustmentsHorizontal,
  export: IconUpload,
};
