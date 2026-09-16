/**
 * 左列：**两段可拖 + 底部自适应**（`design/main.md` §3.1；2026-09-16 按人类验收反馈重排）。
 *
 * ```text
 * ┌ 最近 ────────┐   ← SplitHandle（拖拽改高度）；minSize = "120px"
 * ├ ── ... ──────┤
 * │ 来源 (树)     │   ← 驱动器 + 目录（只列目录、逐级展开）；minSize = "160px"
 * ├──────────────┤   ← **没有把手**：显隐与高度由勾选数量决定
 * │ 已选目录      │   ← 勾 0 个 → 整块不存在；勾 n 个 → 高约 n 条；超上限 → 内部滚动
 * └──────────────┘
 * ```
 *
 * 两条纪律（都是验收反馈里点名要求的，别再改回去）：
 *
 *   1. **`minSize` 必须写带单位的字符串**（`"120px"`）。数字在 Ark/Zag 里是**百分比** ——
 *      上一版写成 `80 / 100` 意思是「最近至少 80%、来源至少 100%」，布局直接畸变：
 *      最近降不下来、来源拖不高、第三段被挤没。详见 `SplitStack` 的注释。
 *   2. **「已选目录」是「挤」不是「盖」**：它不 `absolute`、不浮在别人上面，而是 flex 里的
 *      一个兄弟段 —— 它长高，上面的段就**真的变矮**（内部照常滚动）。
 *      反过来也成立：它不能遮住来源树，更不能出现「面板实际高度比可视区大、滚到底也看不全」。
 *
 * 这一段是**组装层**（`ARCHITECTURE.md` §2）：只把三个 feature 与工作区的共享状态接起来，
 * 自己不做数据加载、不做业务判断。
 */

import { createSignal, Show } from "solid-js";
import { IconRefresh } from "@tabler/icons-solidjs";
import { IconButton } from "../../components/ui/Button.tsx";
import { Panel } from "../../components/ui/Panel.tsx";
import { SplitStack } from "../../components/ui/SplitStack.tsx";
import { RecentList } from "../../features/recent/index.ts";
import { SelectedDirs } from "../../features/selected-dirs/index.ts";
import { DirTree } from "../../features/dir-tree/index.ts";
import { t } from "../../i18n/index.ts";
import type { ImportStore } from "./store.ts";

export interface LeftColumnProps {
  store: ImportStore;
  /** 「最近」段的高度比例（0–1）；只在首次渲染生效（拖过之后以 Ark 的为准） */
  recentRatio?: number;
  /** 拖拽结束时的比例 —— 交给布局偏好店落盘（`lib/layout-prefs.ts`） */
  onRecentRatioChange?: (ratio: number) => void;
  class?: string;
}

export function LeftColumn(props: LeftColumnProps) {
  const store = props.store;
  const recentRatio = (): number => props.recentRatio ?? 0.32;

  /**
   * 「运行期刷新」：目录会在程序外面被创建/改名/删除，而我们**不做文件系统监听**
   * （明确取舍，见 `FUTURE.md` 的 `notify`）—— 所以要给一个手动刷新的入口。
   * 令牌一变，`DirTree` 就把展开着的目录全部重读（保留展开状态）。
   */
  const [refreshKey, setRefreshKey] = createSignal(0);
  const refreshAll = (): void => {
    setRefreshKey((key) => key + 1);
    // 卷本身也可能变（插上 U 盘、挂载网络盘），一起刷新
    void store.reloadVolumes();
  };

  return (
    <div class={["flex min-h-0 flex-1 flex-col", props.class ?? ""].join(" ")}>
      <SplitStack
        class="min-h-0 flex-1"
        keyboardResizeBy={16}
        onResizeEnd={(sizes) => {
          const first = sizes[0];
          if (typeof first === "number") props.onRecentRatioChange?.(first / 100);
        }}
        segments={[
          {
            id: "recent",
            // 初始比例来自布局偏好（拖过就记住），最矮 120px：够用且**降得下来** —— 它自己会滚
            defaultSize: recentRatio() * 100,
            minSize: "120px",
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
            defaultSize: (1 - recentRatio()) * 100,
            minSize: "160px",
            content: (
              <Panel
                title={t("source.tree")}
                scroll
                pad={false}
                actions={
                  <IconButton
                    label={t("common.refresh")}
                    onClick={refreshAll}
                  >
                    <IconRefresh size={14} />
                  </IconButton>
                }
              >
                <DirTree
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
                  refreshKey={refreshKey()}
                />
              </Panel>
            ),
          },
        ]}
      />

      {/*
        已选目录：**自动高度**。
        勾选 0 个 → 整块不存在（连标题都不留）；勾 n 个 → 自然长到约 n 条；
        到上限（40% 列高）后不再长，改为内部滚动 —— 上面的段被**挤压**而不是被盖住。
        上限是 `--selected-max-h`（四成列高，见 tokens.css）：勾 20 个来源也不能把上面的树挤没。
      */}
      <Show when={store.checkedDirs().length > 0}>
        <div class="flex max-h-(--selected-max-h) min-h-0 shrink-0 flex-col overflow-hidden">
          <Panel title={t("source.selected")} scroll pad={false}>
            <SelectedDirs
              entries={store.checkedDirs()}
              onSelect={store.selectDir}
              onRemove={store.removeChecked}
              onIncludeSubdirsChange={store.setIncludeSubdirs}
            />
          </Panel>
        </div>
      </Show>
    </div>
  );
}
