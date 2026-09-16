/**
 * `DirTree` —— 目录树（**通用组件**：导入工作区与将来的浏览侧共用一份）。
 *
 * `design/main.md` §3.1.2 定义了它的形态：**第一层是驱动器/挂载点**（前方必须有来源类型图标，
 * 不能只标盘符 —— 将来要扩展到 NAS、云盘），往下逐级展开**只列目录、不列文件**。
 *
 * ## 两种用法（差别只在「能不能勾选」）
 *
 * | 用法 | 传参 | 勾选圈 |
 * | --- | --- | --- |
 * | 导入工作区 | 传 `isChecked` + `onToggleCheck` | 有（勾选 = 加入待导入集合） |
 * | 浏览侧（`BROWSE.md`） | **不传** `onToggleCheck` | **没有**（浏览时勾选毫无意义） |
 *
 * 这一层刻意用**「回调传没传」**来决定，而不是再加一个 `checkable` 布尔：
 * 少一个可能与回调矛盾的开关（`TreeNode` 也是同样的口径）。
 *
 * ## 三条容易做错的规则（都靠结构而不是自觉保证）
 *
 * 1. **点行 = 选中；点箭头 = 展开**。箭头 `stopPropagation`，所以「点箭头顺带选中」不会发生；
 *    反过来，「选中某行就把它展开」在 `dir-tree/store.ts` 里**根本做不到** ——
 *    那个 store 里没有「选中」这个输入（`DESIGN.md` §12.4.1）；
 * 2. **双击行名 = 展开/折叠**（与点箭头同效，2026-09-16 人类要求）。只对**可展开**的行生效 ——
 *    空目录双击不做无意义的加载；
 * 3. **子目录懒加载**：展开才读（读过的会记住，折叠再展开是秒开）。
 */

import { createMemo, For, onCleanup, onMount, Show } from "solid-js";
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
import { buildTreeRows, type TreeRow } from "./rows.ts";
import { createDirTreeStore } from "./store.ts";

export interface DirTreeProps {
  /** 驱动器 / 挂载点（第一层） */
  volumes: readonly Volume[];
  status: LoadStatus;
  error: string | null;
  isSelected: (path: string) => boolean;
  /** 勾选态查询（不传 `onToggleCheck` 时用不到） */
  isChecked?: (path: string) => boolean;
  onSelect: (path: string) => void;
  /**
   * 勾选变化。**不传它就没有勾选圈** —— 浏览侧正是这么用的（`BROWSE.md`）。
   * 传了它，导入侧才有「勾选 = 加入待导入集合」这套交互。
   */
  onToggleCheck?: (path: string, checked: boolean) => void;
  /** 读子目录（默认走 `src/api/db.ts`；测试可注入） */
  loadDirs: (path: string) => Promise<DirEntry[]>;
  onRetry?: () => void;
  class?: string;
}

export function DirTree(props: DirTreeProps) {
  // 树自己的状态（展开 / 已读子目录 / 加载与错误）—— 不进共享 store
  const tree = createDirTreeStore({ loadDirs: (path) => props.loadDirs(path) });

  /*
   * 窗口重新获得焦点 → 把展开着的目录重读一遍。
   *
   * 这**不是**「刷新按钮」的替代品：展开本来就会重读那一级（见 store 的 `expand`）。
   * 这里管的是另一种情形 —— 树开着不动，用户在程序外面改了东西又切回来，
   * 文件管理器都会在重新激活时对一遍现实，我们也对一遍。成本 = 展开着的那几级。
   */
  onMount(() => {
    const onFocus = (): void => void tree.refreshAll();
    window.addEventListener("focus", onFocus);
    onCleanup(() => window.removeEventListener("focus", onFocus));
  });

  /**
   * 行对象缓存：`<For>` 是按**对象身份**做 key 的，每次摊平都造新对象就会把**所有行**的
   * DOM 拆掉重建 —— 展开一个目录时整棵树闪一下，行多了还会明显变慢。
   * 这里按路径缓存，只有「路径 + 展开状态 + 可展开性 + 深度」真变了才换对象。
   */
  const rowCache = new Map<string, { key: string; row: TreeRow }>();

  const rows = createMemo(() => {
    const built = buildTreeRows({
      volumes: props.volumes,
      childrenOf: tree.childrenOf,
      isExpanded: tree.isExpanded,
    });
    const next: TreeRow[] = [];
    for (const row of built) {
      const key = `${row.depth}|${row.expanded}|${row.expandable}|${row.volumeKind ?? ""}`;
      const cached = rowCache.get(row.path);
      if (cached && cached.key === key) {
        next.push(cached.row);
      } else {
        rowCache.set(row.path, { key, row });
        next.push(row);
      }
    }
    // 已经折叠掉的分支要清出去，别让缓存随着浏览历史无限长
    if (rowCache.size > built.length * 2 + 16) {
      const alive = new Set(built.map((row) => row.path));
      for (const path of [...rowCache.keys()]) {
        if (!alive.has(path)) rowCache.delete(path);
      }
    }
    return next;
  });

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
                    checked={props.isChecked?.(row.path) ?? false}
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
                    // 双击行名 = 展开/折叠（只对可展开的行；空目录不白跑一次加载）。
                    // 双击落在勾选圈上时不算 —— 那是「勾选」的地盘，不是展开。
                    onDoubleClick={(event) => {
                      const target = event.target as HTMLElement | null;
                      if (target?.closest('[role="checkbox"]')) return;
                      if (row.expandable) void tree.toggle(row.path);
                    }}
                    // 不传回调 = 不渲染勾选圈（浏览侧）
                    onCheckedChange={
                      props.onToggleCheck
                        ? (checked) => props.onToggleCheck?.(row.path, checked)
                        : undefined
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
