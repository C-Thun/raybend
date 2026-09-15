/**
 * `SourceTree` —— 左列第二段「来源」目录树（`design/main.md` §3.1.2）。
 *
 * 结构：**第一层是驱动器/挂载点**（前方必须有来源类型图标，不能只标盘符 ——
 * 将来要扩展到 NAS、云盘），往下逐级展开**只列目录、不列文件**。
 *
 * 两条容易做错的规则（都靠结构而不是自觉保证）：
 *
 * 1. **点行 = 选中；点箭头 = 展开发**。箭头 `stopPropagation`，
 *    所以「点箭头顺带选中」这种事不会发生；反过来，
 *    「选中某行就把它展开」在 `source-tree/store.ts` 里**根本做不到** ——
 *    那个 store 里没有「选中」这个输入（`DESIGN.md` §12.4.1）；
 * 2. **子目录懒加载**：展开才读（读过的会记住，折叠再展开是秒开）。
 */

import { createMemo, For, Show } from "solid-js";
import {
  IconAlertTriangle,
  IconCloud,
  IconDeviceSdCard,
  IconDeviceUsb,
  IconDisc,
  IconFolder,
  IconFolderOpen,
  IconServer,
} from "@tabler/icons-solidjs";
import type { DirEntry, Volume, VolumeKind } from "../../api/types.ts";
import { ScrollBox } from "../../components/ui/ScrollBar.tsx";
import { TreeNode } from "../../components/ui/TreeNode.tsx";
import { t } from "../../i18n/index.ts";
import type { LoadStatus } from "../../lib/load-status.ts";
import { buildTreeRows } from "./rows.ts";
import { createSourceTreeStore } from "./store.ts";

export interface SourceTreeProps {
  /** 驱动器 / 挂载点（第一层） */
  volumes: readonly Volume[];
  status: LoadStatus;
  error: string | null;
  isSelected: (path: string) => boolean;
  isChecked: (path: string) => boolean;
  onSelect: (path: string) => void;
  onToggleCheck: (path: string, checked: boolean) => void;
  /** 读子目录（默认走 `src/api/db.ts`；测试可注入） */
  loadDirs: (path: string) => Promise<DirEntry[]>;
  onRetry?: () => void;
  class?: string;
}

export function SourceTree(props: SourceTreeProps) {
  // 树自己的状态（展开 / 已读子目录 / 加载与错误）—— 不进共享 store
  const tree = createSourceTreeStore({ loadDirs: (path) => props.loadDirs(path) });

  const rows = createMemo(() =>
    buildTreeRows({
      volumes: props.volumes,
      childrenOf: tree.childrenOf,
      isExpanded: tree.isExpanded,
    }),
  );

  return (
    <div class={["flex min-h-0 flex-col", props.class ?? ""].join(" ")}>
      <Show
        when={props.status !== "error"}
        fallback={
          <div class="flex items-start gap-1.5 p-2 text-fs-1 text-fg-2">
            <IconAlertTriangle
              size={14}
              class="mt-0.5 shrink-0"
              aria-hidden="true"
            />
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
        }
      >
        <Show
          when={rows().length > 0}
          fallback={
            <p class="p-2 text-fs-1 text-fg-3">
              {props.status === "loading"
                ? t("common.loading")
                : t("source.tree.empty")}
            </p>
          }
        >
          <ScrollBox class="min-h-0 flex-1">
            <For each={rows()}>
              {(row) => (
                <>
                  <TreeNode
                    label={row.name}
                    depth={row.depth}
                    hasChildren={row.expandable}
                    expanded={row.expanded}
                    checked={props.isChecked(row.path)}
                    selected={props.isSelected(row.path)}
                    icon={
                      row.volumeKind === null ? (
                        // 展开的目录用「打开的文件夹」——一眼看出哪些是展开的
                        row.expanded ? (
                          <IconFolderOpen size={14} />
                        ) : (
                          <IconFolder size={14} />
                        )
                      ) : (
                        volumeIcon(row.volumeKind)
                      )
                    }
                    checkLabel={`${t("source.check")} ${row.path}`}
                    onClick={() => props.onSelect(row.path)}
                    onToggleExpand={() => void tree.toggle(row.path)}
                    onCheckedChange={(checked) =>
                      props.onToggleCheck(row.path, checked)
                    }
                  />
                  {/* 读不了这一层时，在它下面挂一句可读的错误（而不是静默不出东西） */}
                  <Show when={tree.errorOf(row.path)}>
                    {(message) => (
                      <p
                        class="flex items-start gap-1 py-1 pe-2 text-fs-0 text-fg-3"
                        style={{ "padding-left": `${(row.depth + 1) * 1.25}em` }}
                      >
                        <IconAlertTriangle
                          size={12}
                          class="mt-0.5 shrink-0"
                          aria-hidden="true"
                        />
                        <span class="min-w-0 flex-1 break-words">
                          {t("source.load_error", { message: message() })}
                        </span>
                        <button
                          type="button"
                          class="shrink-0 cursor-pointer underline"
                          onClick={() => void tree.refresh(row.path)}
                        >
                          {t("common.retry")}
                        </button>
                      </p>
                    )}
                  </Show>
                </>
              )}
            </For>
          </ScrollBox>
        </Show>
      </Show>
    </div>
  );
}

/** 来源类型 → 图标（`design/main.md` §3.1.2 要求一眼可辨）。 */
function volumeIcon(kind: VolumeKind) {
  switch (kind) {
    case "removable":
      return <IconDeviceUsb size={14} />;
    case "optical":
      return <IconDisc size={14} />;
    case "network":
      return <IconServer size={14} />;
    case "cloud":
      return <IconCloud size={14} />;
    default:
      return <IconDeviceSdCard size={14} />;
  }
}

/** 卷类型的中文名（Tooltip / 提示用）。 */
export function volumeKindLabel(kind: VolumeKind): string {
  return t(`source.volume.${kind}`);
}
