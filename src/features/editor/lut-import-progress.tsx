/**
 * `LutImportProgressView` —— LUT 导入进行中的状态块（编辑左列 LUT 面板的导入弹窗里用）。
 *
 * 画稿：`design/editor.md` §3.1.2 / `design/editor.pen` 的 `States / Editor / 导入 LUT 进度`。
 *
 * 两条口径：
 *   * **没有总数就不编百分比**（扫描阶段走不确定态）；总数 = 待处理文件数，来自 Rust 的事件；
 *   * **当前项要看得见** —— 「它没卡住」只能靠这一行证明（与照片导入弹窗同一条理由）。
 *
 * 抽成独立组件的理由：陈列室（`/dev/kitchen-sink`）要在没有后端的情况下演示这三种状态，
 * 而 `LutPanel` 需要完整的 `EditorStore`。渲染逻辑只留这一份。
 */

import { Show, type JSX } from "solid-js";

import { Progress } from "../../components/ui/Progress.tsx";
import { t } from "../../i18n/index.ts";
import { percentOf } from "../../lib/progress.ts";
import type { LutImportProgress } from "../../api/lut.ts";

export interface LutImportProgressViewProps {
  /** `null` = 还没收到第一条事件（等价于扫描阶段） */
  progress: LutImportProgress | null;
  /** 用户已点过「取消」，正在等当前文件处理完 */
  stopping: boolean;
  /** 目标分类名（`正在复制到分类「…」`） */
  categoryName: string;
}

export function LutImportProgressView(props: LutImportProgressViewProps): JSX.Element {
  const phase = () => props.progress?.phase ?? "scan";
  /** 进度条的值：扫描阶段（还没有总数）给 `null` */
  const percent = (): number | null => {
    const progress = props.progress;
    if (progress === null || progress.phase === "scan") return null;
    return percentOf(progress.done, progress.total);
  };

  return (
    <div data-lut-import-progress class="flex flex-col gap-3">
      <p role="status" class="text-fs-1 text-fg-2">
        <Show when={phase() !== "scan"} fallback={t("editor.lut.importScanning")}>
          {t("editor.lut.importCopying", { category: props.categoryName })}
        </Show>
      </p>

      {/* 有总数才给数字（扫描阶段不编百分比） */}
      <Show when={percent() !== null}>
        <div class="flex items-center justify-between text-fs-1 tabular-nums text-fg-2">
          <span>
            {props.progress?.done ?? 0} / {props.progress?.total ?? 0}
          </span>
          <span>{percent() ?? 0}%</span>
        </div>
      </Show>
      <Progress value={percent()} label={t("editor.lut.importProgressLabel")} />

      <Show when={percent() !== null}>
        <p class="text-fs-1 tabular-nums text-fg-2">
          {t("editor.lut.importCounts", {
            imported: props.progress?.imported ?? 0,
            duplicates: props.progress?.duplicates ?? 0,
            skipped: props.progress?.skipped ?? 0,
          })}
        </p>
      </Show>

      <Show when={props.progress?.current}>
        {(current) => (
          <div class="flex flex-col gap-0.5 rounded-ui bg-surface-bar px-2 py-1.5">
            <span class="overflow-hidden whitespace-nowrap fade-x-end text-fs-1 text-fg-1">
              {current()}
            </span>
            <span class="text-fs-0 text-fg-3">{t("editor.lut.importKeepPartial")}</span>
          </div>
        )}
      </Show>

      <Show when={props.stopping}>
        <p role="status" class="text-fs-0 text-fg-3">
          {t("editor.lut.importStopping")}
        </p>
      </Show>
    </div>
  );
}
