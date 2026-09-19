/**
 * 看图与对比**共用**的一组浮动控件：左上「返回」+ 右下「缩放」。
 *
 * 人类 2026-09-19：import 进对比之后「左上角的返回和右下角的缩放工具都没了」——
 * 那两个控件原来长在 `Viewer` 里，而对比态渲染的是 `CompareView`，自然就缺了。
 * 于是抽成独立组件，**两处共用同一份**：
 *
 *   - 单张看图：沿用「鼠标靠近右下角才浮出」的规则（`visible` 由调用方按光标位置给）；
 *   - 对比：**常驻可见**（对比时更需要它，而且整个对比区**只有一组**控件，
 *     不是每幅画幅各来一组）。
 *
 * 定位：两个控件都是 `absolute`，所以父级必须是定位容器
 * （`Viewer` / `CompareView` 的根节点都满足）。
 */

import { IconArrowLeft, IconMinus, IconPlus } from "@tabler/icons-solidjs";
import type { JSX } from "solid-js";

import { t } from "../../../i18n/index.ts";
import type { ViewerStore } from "./store.ts";

export interface ViewerControlsProps {
  store: ViewerStore;
  /** 关闭（返回 tiles）。给了才渲染左上角那颗 */
  onClose?: () => void;
  /** 是否可见（看图态按光标位置给；对比态恒为 true） */
  visible: boolean;
  class?: string;
}

export function ViewerControls(props: ViewerControlsProps): JSX.Element {
  /** 显示哪个数字：适配状态只说「适配」，用户自己缩放过才报百分比（见 Viewer 的说明） */
  const zoomLabel = (): string => {
    const state = props.store.state();
    if (!state.fit) return `${Math.round(state.zoom * 100)}%`;
    const fitText = t("viewer.fit");
    return props.store.sharp() ? `${fitText} · ${Math.round(state.zoom * 100)}%` : fitText;
  };

  return (
    <>
      {/* 左上角：返回 */}
      {props.onClose ? (
        <button
          type="button"
          class={[
            "absolute start-3 top-3 z-20 flex cursor-pointer items-center gap-1 rounded-ui bg-surface-layer px-2 py-1 text-fs-1 text-fg-1",
            "transition-opacity",
            "opacity-0 focus-visible:opacity-100 focus-within:opacity-100",
            props.visible ? "opacity-100" : "",
            props.class ?? "",
          ]
            .filter(Boolean)
            .join(" ")}
          onClick={props.onClose}
          aria-label={t("viewer.back")}
        >
          <IconArrowLeft size={14} aria-hidden="true" />
          {t("viewer.back")}
        </button>
      ) : null}

      {/* 右下角：缩放控件 + 百分比（给鼠标用户一个不靠滚轮的入口） */}
      <div
        class={[
          "absolute end-3 bottom-3 z-20 flex items-center gap-1 rounded-ui bg-surface-layer px-1.5 py-1",
          "transition-opacity",
          "opacity-0 focus-within:opacity-100",
          props.visible ? "opacity-100" : "",
        ]
          .filter(Boolean)
          .join(" ")}
        data-viewer-controls="zoom"
      >
        <button
          type="button"
          class="flex size-6 cursor-pointer items-center justify-center rounded-ui text-fg-2 hover:bg-state-hover hover:text-fg-1"
          aria-label={t("viewer.zoom_out")}
          onClick={() => props.store.zoomBy(1 / 1.25)}
        >
          <IconMinus size={14} aria-hidden="true" />
        </button>
        <button
          type="button"
          class="min-w-16 cursor-pointer rounded-ui px-1 text-center text-fs-1 text-fg-2 hover:bg-state-hover hover:text-fg-1 tnum"
          aria-label={t("viewer.fit")}
          onClick={() => props.store.toggleFit()}
        >
          {zoomLabel()}
        </button>
        <button
          type="button"
          class="flex size-6 cursor-pointer items-center justify-center rounded-ui text-fg-2 hover:bg-state-hover hover:text-fg-1"
          aria-label={t("viewer.zoom_in")}
          onClick={() => props.store.zoomBy(1.25)}
        >
          <IconPlus size={14} aria-hidden="true" />
        </button>
        <span class="max-w-64 truncate ps-1 text-fs-1 text-fg-3">
          {props.store.current()?.fileName ?? ""}
        </span>
      </div>
    </>
  );
}
