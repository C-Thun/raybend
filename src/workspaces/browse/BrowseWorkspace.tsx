/**
 * 浏览工作区：三列（`BROWSE.md` §1、`design/browse.md` §2）。
 *
 * ```text
 * ┌────────────┬──────────────────────────────┬──────────────┐
 * │ 左列 300   │ 中列（弹性，surface-bar）     │ 右列 300     │
 * │ 库目录选择器 │ 照片网格 + 底部控制条         │ 信息栏        │
 * └────────────┴──────────────────────────────┴──────────────┘
 * ```
 *
 * 这是**组装层**：把 feature 与工作区共享状态接起来，自己不做业务判断
 * （`ARCHITECTURE.md` §2）。库列表在这里拉一次 —— 属于工作区自己的生命周期。
 *
 * ⚠️ W1 的取舍（明确记下来，别当成遗漏）：
 * * 左右列宽度**固定**（拖拽调宽与导入工作区一样，属 W2 的活儿）；
 * * 控制条只有「尺寸档位」与「按时间」（筛选/排序的 UI 在 W2）；
 * * 看图 / 对比 / 胶片带在 W2 —— 这里是「看片、挑片」的第一屏。
 */

import { createMemo, createSignal, onMount, Show } from "solid-js";

import { listRepositories } from "../../api/db.ts";
import type { RepositoryView } from "../../api/types.ts";
import {
  AssetInfo,
  BrowseGrid,
  BrowseLeftColumn,
  type BrowseStore,
} from "../../features/browse/index.ts";
import { clampTileStepIndex, DEFAULT_TILE_STEP_INDEX, TILE_SIZE_STEPS, tileSizeAt } from "../../lib/tile-flow.ts";
import { t } from "../../i18n/index.ts";
import { Slider } from "../../components/ui/Slider.tsx";

export interface BrowseWorkspaceProps {
  store: BrowseStore;
  class?: string;
}

export function BrowseWorkspace(props: BrowseWorkspaceProps) {
  const store = props.store;
  const [repositories, setRepositories] = createSignal<RepositoryView[]>([]);
  /** 还在读库列表；用来把「还没读到」与「真的没有库」分开显示。 */
  const [reposLoading, setReposLoading] = createSignal(true);
  /** 读库列表失败时的原因（不再是静默空态）。 */
  const [reposError, setReposError] = createSignal<string | null>(null);
  const [tileStep, setTileStep] = createSignal(DEFAULT_TILE_STEP_INDEX);
  const [grouped, setGrouped] = createSignal(false);

  onMount(() => {
    void (async () => {
      try {
        const list = await listRepositories();
        setRepositories(list);
        // 第一次进来：默认选最近打开的库（没有就选第一个在线的）
        if (store.repositoryId() === null && list.length > 0) {
          const preferred =
            list.find((r) => r.online && r.lastOpenedAt !== null) ??
            list.find((r) => r.online) ??
            list[0];
          store.setRepository(preferred.id);
        }
      } catch (error) {
        // 拿不到库列表不该让工作区崩掉；但**不能装作「你没有库」** —— 把原因显示在左列。
        // 同时打一条控制台：只上界面、日志里查不到，排障时只能靠人转述一句文案。
        // 这是**控制台诊断**，不走语言包（`DESIGN.md` §11.1 的豁免项：终端/控制台输出）。
        console.error("[browse] 读库列表失败", error); // i18n-exempt: 控制台诊断，不是界面文案
        setRepositories([]);
        setReposError(error instanceof Error ? error.message : String(error));
      } finally {
        setReposLoading(false);
      }
    })();
  });

  const root = createMemo(
    () => repositories().find((r) => r.id === store.repositoryId())?.root ?? null,
  );

  /** 右栏显示谁：多选时是锚点那张（`BROWSE.md` §5.10）。 */
  const anchor = createMemo(() => {
    const id = store.anchorId();
    if (id === null) return null;
    for (const item of store.selectedItems()) {
      if (item.id === id) return item;
    }
    return store.selectedItems()[0] ?? null;
  });

  /** 状态栏中间那段：`库名 / 最后一级目录 · 文件名`（`BROWSE.md` §5.10）。 */
  const currentLabel = createMemo(() => {
    const repoName =
      repositories().find((r) => r.id === store.repositoryId())?.name ?? "";
    const scope = store.scopePath();
    const last = scope === null ? "" : (scope.split("/").pop() ?? "");
    const file = anchor()?.fileName ?? "";
    const left = [repoName, last].filter((s) => s !== "").join(" / ");
    return file === "" ? left : `${left}${left === "" ? "" : " · "}${file}`;
  });

  return (
    <div class={["flex min-h-0 flex-1", props.class ?? ""].filter(Boolean).join(" ")}>
      {/* 左列 */}
      <aside class="flex w-[300px] shrink-0 flex-col border-r border-line-1 bg-surface-main">
        <BrowseLeftColumn
          store={store}
          repositories={repositories()}
          reposLoading={reposLoading()}
          reposError={reposError()}
        />
      </aside>

      {/* 中列 */}
      <main class="flex min-h-0 min-w-0 flex-1 flex-col bg-surface-bar">
        <BrowseGrid
          store={store}
          root={root()}
          resetKey={`${store.repositoryId() ?? ""}:${store.scopePath() ?? ""}`}
          tileStep={tileStep()}
          grouped={grouped()}
        />

        {/* 控制条（`design/browse.md` §2.3）：计数 — 当前目录/文件名 — 按时间 + 缩放 */}
        <div class="flex h-8 shrink-0 items-center gap-3 px-2 text-fs-2 text-fg-3">
          <span>{t("browse.count").replace("{n}", String(store.total()))}</span>
          <Show when={store.selectedCount() > 0}>
            <span class="text-fg-2">
              {t("browse.selected").replace("{n}", String(store.selectedCount()))}
            </span>
          </Show>

          <span class="min-w-0 flex-1 truncate text-center text-fg-3">
            {currentLabel()}
          </span>

          <button
            type="button"
            onClick={() => setGrouped(!grouped())}
            class={[
              "h-6 shrink-0 rounded-(--radius) px-2",
              grouped()
                ? "bg-state-selected text-fg-1"
                : "text-fg-3 hover:bg-state-hover",
            ].join(" ")}
          >
            {t("grid.by_time")}
          </button>

          <span class="shrink-0 text-fg-3">{tileSizeAt(tileStep())}px</span>
          <div class="w-24 shrink-0">
            <Slider
              min={0}
              max={TILE_SIZE_STEPS.length - 1}
              step={1}
              value={tileStep()}
              label={t("grid.zoom")}
              onValueChange={(value) => setTileStep(clampTileStepIndex(value))}
            />
          </div>
        </div>
      </main>

      {/* 右列 */}
      <aside class="flex w-[300px] shrink-0 flex-col border-l border-line-1 bg-surface-main">
        <AssetInfo item={anchor()} />
      </aside>
    </div>
  );
}
