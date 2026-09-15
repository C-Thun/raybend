/**
 * 左列：三段可拖拽（`design/main.md` §3.1）。
 *
 * ```text
 * ┌ 最近 ────────┐   ← 自动记录的最近 50 个导入目录
 * │ ...          │
 * ├ ── ... ──────┤   ← SplitHandle（拖拽改高度）
 * │ 来源 (树)     │   ← 驱动器 + 目录（只列目录、逐级展开）
 * ├ ── ... ──────┤
 * │ 已选目录      │   ← 勾选进来、准备导入的那些（横条）
 * └──────────────┘
 * ```
 *
 * 三段的高度是**初始值**（`defaultSize`，百分比），拖过之后以用户的为准 ——
 * 这是 Ark `Splitter` 的行为，也正是我们要的：M1 不做尺寸持久化
 * （`FUTURE.md` 里也没有这条，等有人真抱怨了再说）。
 *
 * 这一段是**组装层**（`ARCHITECTURE.md` §2）：它只把三个 feature 与工作区的
 * 共享状态接起来，自己不做数据加载、不做业务判断。
 */

import { Panel } from "../../components/ui/Panel.tsx";
import { SplitStack } from "../../components/ui/SplitStack.tsx";
import { RecentList } from "../../features/recent/index.ts";
import { SelectedDirs } from "../../features/selected-dirs/index.ts";
import { SourceTree } from "../../features/source-tree/index.ts";
import { t } from "../../i18n/index.ts";
import type { ImportStore } from "./store.ts";

export interface LeftColumnProps {
  store: ImportStore;
  class?: string;
}

export function LeftColumn(props: LeftColumnProps) {
  const store = props.store;

  return (
    <SplitStack
      class={["min-h-0 flex-1", props.class ?? ""].join(" ")}
      keyboardResizeBy={16}
      segments={[
        {
          id: "recent",
          defaultSize: "28%",
          minSize: 80,
          content: (
            <Panel title={t("source.recent")} scroll pad={false}>
              <RecentList
                entries={store.recentDirs()}
                status={store.recentStatus()}
                error={store.recentError()}
                isSelected={store.isSelected}
                isChecked={store.isChecked}
                onSelect={store.selectDir}
                onToggleCheck={(path, checked) => {
                  // 点圆圈：勾选就加入待导入（并把「包含子目录」按上次的选择带上）
                  if (checked) {
                    const known = store
                      .recentDirs()
                      .find((row) => row.path === path)?.includeSubdirs;
                    store.toggleChecked(path, known ?? false);
                  } else {
                    store.removeChecked(path);
                  }
                }}
                onRemove={(path) => void store.forgetRecent(path)}
                onRetry={() => void store.reloadRecent()}
              />
            </Panel>
          ),
        },
        {
          id: "source",
          defaultSize: "42%",
          minSize: 100,
          content: (
            <Panel title={t("source.tree")} scroll pad={false}>
              <SourceTree
                volumes={store.volumes()}
                status={store.volumesStatus()}
                error={store.volumesError()}
                isSelected={store.isSelected}
                isChecked={store.isChecked}
                onSelect={store.selectDir}
                onToggleCheck={(path, checked) => {
                  if (checked) store.toggleChecked(path, false);
                  else store.removeChecked(path);
                }}
                loadDirs={store.loadDirs}
                onRetry={() => void store.reloadVolumes()}
              />
            </Panel>
          ),
        },
        {
          id: "selected",
          defaultSize: "30%",
          minSize: 80,
          content: (
            <Panel title={t("source.selected")} scroll pad={false}>
              <SelectedDirs
                entries={store.checkedDirs()}
                onSelect={store.selectDir}
                onRemove={store.removeChecked}
                onIncludeSubdirsChange={store.setIncludeSubdirs}
              />
            </Panel>
          ),
        },
      ]}
    />
  );
}
