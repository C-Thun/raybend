/**
 * `FlowBar` —— 工作流行（`design/main.md` §2.2）。
 *
 * 左：工作流切换器（**有序流水线**，顺序固定不重排）—— 用**专属**的 `FlowSwitcher`，
 *     不是通用分段控件（理由见那个文件顶部：它是外壳的招牌部件）
 * 中：弹性空白
 * 右：`ExifStrip`（四组 `easy copy`）+ **全屏看图**按钮
 *
 * 两处「刻意如此」：
 *   1. **这一行比别处更圆润、也更大**（胶囊形轨道与色块）—— 设计稿明确要求，
 *      因为它是「阶段切换」，与旁边的方形工具按钮在语义上不同
 *   2. **全屏按钮跟随 picture info 一起出现**（人类 2026-09-23 定）：信息只在选中照片时
 *      才有内容，而只有选中了照片才谈得上全屏看它 —— 两者是同一件事的两个面。
 *      没有当前照片时（`exif == null` 或工作区没给出清单）按钮**整个不出现**，
 *      而不是灰着占位。
 */

import { Show } from "solid-js";
import type { Component } from "solid-js";
import { Dynamic } from "solid-js/web";
import {
  IconAdjustmentsHorizontal,
  IconFolderDown,
  IconMaximize,
  IconPhoto,
  IconUpload,
} from "@tabler/icons-solidjs";
import { IconButton } from "../components/ui/Button.tsx";
import { ExifStrip, type ExifData } from "../features/exif-strip/index.ts";
import { t } from "../i18n";
import { WORKFLOWS, WORKFLOW_LABEL_KEY, type WorkflowId } from "./flow.ts";
import { FlowSwitcher, type FlowSwitcherOption } from "./FlowSwitcher.tsx";
import type { ShellStore } from "./store.ts";

export interface FlowBarProps {
  store: ShellStore;
  exportProcessing?: boolean;
  /** 当前照片的 EXIF；没有选中照片时为 `null` → EXIF 区显示空态，全屏按钮也不出现 */
  exif?: ExifData | null;
  /**
   * 点全屏看图。**只在真的能开时传**（工作区给了清单）——
   * 传了才渲染按钮，所以不会出现「按下去没反应」的按钮。
   */
  onFullscreen?: () => void;
}

export function FlowBar(props: FlowBarProps) {
  /*
   * 选项身份必须稳定：FlowSwitcher 的 For 按对象身份复用节点，Ark 则持续观察
   * 当前 item 的尺寸。把 map 放在响应式 options getter 中，会在队列刷新（即便
   * processing 仍是 false）或切语言时换掉全部 item；旧节点的 ResizeObserver
   * 随后报 0×0，背景块就被隐藏。只让文案/处理态响应更新，不重建选项及按钮。
   */
  const options: readonly FlowSwitcherOption<WorkflowId>[] = WORKFLOWS.map((id) => ({
    value: id,
    get processing() { return id === "export" && props.exportProcessing === true; },
    get label() { return t(WORKFLOW_LABEL_KEY[id]); },
    icon: () => <Dynamic component={WORKFLOW_ICONS[id]} size={16} />,
  }));

  return (
    <div class="flex h-bar-flow-h shrink-0 items-center gap-2 bg-surface-main px-pad-x">
      <FlowSwitcher<WorkflowId>
        label={t("flow.label")}
        value={props.store.workflow()}
        onValueChange={props.store.setWorkflow}
        options={options}
      />

      {/* 弹性空白：把 EXIF 与开关推到右边 */}
      <div class="h-px min-w-2 flex-1" />

      <ExifStrip data={props.exif ?? null} class="justify-end" />

      {/*
        全屏看图：**跟随 picture info**（见文件头第 2 条）。
        判据用 `props.exif` 而不是「有没有选中照片」：两者在这里本来就是同一件事，
        而 exif 是这一行**已经有**的事实（`flowinfo` 只在选中照片时非空）。
      */}
      <Show when={props.exif !== null && props.exif !== undefined && props.onFullscreen !== undefined}>
        <IconButton
          label={t("flow.tool.fullscreen")}
          onClick={() => props.onFullscreen?.()}
        >
          <IconMaximize size={16} />
        </IconButton>
      </Show>
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
