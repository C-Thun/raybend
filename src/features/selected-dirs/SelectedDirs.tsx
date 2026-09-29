/**
 * `SelectedDirs` —— 左列第三段「已选目录」（`design/main.md` §3.1.3）。
 *
 * 形态是**横向长条**（不是 tile 流）：用户实测后反馈 tile 放不下目录长度，
 * 横条能把路径横向铺开，且与右列库卡片风格统一。
 *
 * 每条上两个操作，**分居上下两角**（这是刻意的 —— 它们职责不同，
 * 分开就不会误点）：
 *   - **右上角：移除**（禁行图标 + `easy destroy` 确认；只是从待导入集合里拿掉，不动磁盘）
 *   - **右下角：「包含子目录」开关**（默认**关**）
 */

import { For, Show } from "solid-js";
import { IconFolder } from "@tabler/icons-solidjs";
import { EasyDestroyButton } from "../../components/ui/EasyDestroy.tsx";
import { Switch } from "../../components/ui/Form.tsx";
import { PathText } from "../../components/ui/PathText.tsx";
import { t } from "../../i18n/index.ts";
import type { CheckedDir } from "../../lib/checked-dir.ts";

export interface SelectedDirsProps {
  entries: readonly CheckedDir[];
  /** 点路径 → 选中并浏览这个目录（不改变勾选） */
  onSelect?: (path: string) => void;
  onRemove?: (path: string) => void;
  onIncludeSubdirsChange?: (path: string, value: boolean) => void;
  /**
   * 只读（**导入确认弹窗**用，2026-09-28）：只展示「选了哪些目录、各自含不含子目录」，
   * 不给移除与开关 —— 那里是拍板的地方，不是改清单的地方。
   * 只读时三个回调都不需要（也就不用传）。
   */
  readOnly?: boolean;
  class?: string;
}

export function SelectedDirs(props: SelectedDirsProps) {
  return (
    <div class={["flex min-h-0 flex-col gap-1 p-1", props.class ?? ""].join(" ")}>
      <Show
        when={props.entries.length > 0}
        fallback={
          <p class="p-1 text-fs-1 text-fg-3">{t("source.selected.empty")}</p>
        }
      >
        <For each={props.entries}>
          {(entry) => (
            <div
              class={[
                "flex min-h-selected-bar-h flex-col justify-between gap-1 rounded-ui",
                "bg-surface-track px-2 py-1",
              ].join(" ")}
            >
              {/* 第一行：图标 + 路径（占满剩余宽度，点它 = 选中并浏览）+ 移除 */}
              <div class="flex min-w-0 items-center gap-1.5">
                <span
                  class="flex size-5 shrink-0 items-center justify-center text-fg-2"
                  aria-hidden="true"
                >
                  <IconFolder size={20} />
                </span>
                <button
                  type="button"
                  class="min-w-0 flex-1 cursor-pointer text-start"
                  onClick={() => props.onSelect?.(entry.path)}
                  aria-label={entry.path}
                >
                  <PathText path={entry.path} full class="min-w-0 w-full" />
                </button>
                <Show when={props.readOnly !== true}>
                  <EasyDestroyButton
                    label={t("source.remove_dir", { path: entry.path })}
                    onRemove={() => props.onRemove?.(entry.path)}
                  />
                </Show>
              </div>

              {/*
                第二行：包含子目录（默认关）。
                **文字必须真的渲染出来** —— 设计稿里它是 `SubdirLabel`（`$fg-3` / 13px），
                只给 `aria-label` 的话屏幕上什么都没有（2026-09-16 人类报的「文字看不见」）。
                `aria-hidden`：读屏交给 Switch 自己的无障碍名，避免念两遍。

                只读时不给开关，但**开了子目录的目录要把这句写出来**（它改变导入范围，
                不能在确认时隐身）；没开的就不占位。
              */}
              <Show when={props.readOnly !== true || entry.includeSubdirs}>
                <div class="flex items-center justify-end gap-1.5">
                  <span class="text-fs-1 text-fg-3" aria-hidden="true">
                    {t("source.include_subdirs")}
                  </span>
                  <Show when={props.readOnly !== true}>
                    <Switch
                      checked={entry.includeSubdirs}
                      onCheckedChange={(value) =>
                        props.onIncludeSubdirsChange?.(entry.path, value)
                      }
                      label={t("source.include_subdirs")}
                    />
                  </Show>
                </div>
              </Show>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
}
