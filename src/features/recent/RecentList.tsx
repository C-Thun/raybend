/**
 * `RecentList` —— 左列第一段「最近」（`design/main.md` §3.1.1）。
 *
 * 三条设计约束（都在界面上看不出来，但决定了实现）：
 *
 * 1. **不需要用户收藏**：这份列表由「勾选目录」这个动作自动维护（写入在
 *    `workspaces/import/store.ts` 里），界面只负责显示与移除；
 * 2. **显示缩写路径（`shortpath`），完整路径走悬停**（`DESIGN.md` §12.3）——
 *    所以行内容用 `PathText` 而不是纯文字（`TreeNode` 的 `labelNode` 槽）；
 * 3. **勾选与选中是两件事**：左侧圆圈管勾选（可多选），整行底色管选中
 *    （同一时间只有一个）；点行只选中，点圆圈只勾选。
 *
 * 移除用 `easy destroy`（禁行图标，默认确认、`Shift` 跳过）——
 * 它只是从这份便利列表里拿掉一条，**不动磁盘**（`AGENTS.md` §11.3）。
 */

import { For, Show } from "solid-js";
import { IconAlertTriangle, IconFolder } from "@tabler/icons-solidjs";
import type { RecentDir } from "../../api/types.ts";
import { EasyDestroyButton } from "../../components/ui/EasyDestroy.tsx";
import { PathText } from "../../components/ui/PathText.tsx";
import { TreeNode } from "../../components/ui/TreeNode.tsx";
import { t } from "../../i18n/index.ts";
import type { LoadStatus } from "../../lib/load-status.ts";

export interface RecentListProps {
  entries: readonly RecentDir[];
  status: LoadStatus;
  /** 读失败时给用户看的原因（`status === "error"` 时才有意义） */
  error: string | null;
  /** 这一条是不是**当前被浏览**的目录（跨面板同步由调用方决定，见 `DESIGN.md` §12.4.1） */
  isSelected: (path: string) => boolean;
  /** 这一条有没有被勾选（要加入导入） */
  isChecked: (path: string) => boolean;
  /** 点行 → 选中并浏览（不勾选） */
  onSelect: (path: string) => void;
  /** 点圆圈 → 勾选 / 取消勾选（不选中） */
  onToggleCheck: (path: string, checked: boolean) => void;
  onRemove: (path: string) => void;
  onRetry?: () => void;
  class?: string;
}

export function RecentList(props: RecentListProps) {
  return (
    <div class={["flex min-h-0 flex-col", props.class ?? ""].join(" ")}>
      <Show when={props.status === "error"} fallback={<Rows {...props} />}>
        <div class="flex items-start gap-1.5 p-2 text-fs-1 text-fg-2">
          <IconAlertTriangle size={14} class="mt-0.5 shrink-0" aria-hidden="true" />
          <span class="min-w-0 flex-1 break-words">
            {t("source.load_error", { message: props.error ?? "" })}
          </span>
          <Show when={props.onRetry}>
            <button
              type="button"
              class="shrink-0 cursor-pointer underline"
              onClick={() => props.onRetry?.()}
            >
              {t("common.retry")}
            </button>
          </Show>
        </div>
      </Show>
    </div>
  );
}

function Rows(props: RecentListProps) {
  return (
    <Show
      when={props.entries.length > 0}
      fallback={
        <p class="p-2 text-fs-1 text-fg-3">
          {props.status === "loading"
            ? t("common.loading")
            : t("source.recent.empty")}
        </p>
      }
    >
      <For each={props.entries}>
        {(entry) => (
          <TreeNode
            label={entry.path}
            labelNode={
              <PathText
                path={entry.path}
                maxLength={36}
                icon={<IconFolder size={14} />}
                selected={props.isSelected(entry.path)}
              />
            }
            depth={0}
            checked={props.isChecked(entry.path)}
            selected={props.isSelected(entry.path)}
            checkLabel={`${t("source.check")} ${entry.path}`}
            onClick={() => props.onSelect(entry.path)}
            onCheckedChange={(checked) =>
              props.onToggleCheck(entry.path, checked)
            }
            trailing={
              /* 悬停才出现（`design/main.md` §3.1.1）；键盘焦点进入行内也显示 */
              <span class="opacity-0 transition-opacity group-hover/row:opacity-100 group-focus-within/row:opacity-100">
                <EasyDestroyButton
                  label={t("source.remove_from_recent")}
                  onRemove={() => props.onRemove(entry.path)}
                />
              </span>
            }
          />
        )}
      </For>
    </Show>
  );
}
