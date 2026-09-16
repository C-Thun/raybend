/**
 * `RepositoryList` —— 右列的库列表（`design/main.md` §3.3）。
 *
 * 一张卡片上要表达的东西（信息密度不低，所以卡片比其他列表行高）：
 *
 * ```text
 * ┌──────────────────────────────────────────┐
 * │ ▨  Kowloon Studio               [在线]   │  ← 图标块 + 名称 + 状态徽标
 * │    D:\Photos\Library            1 248 张 │  ← 当前在线的那条路径 + 张数
 * └──────────────────────────────────────────┘
 * ```
 *
 * 三条规则（都在 `REPOSITORY.md` §2 / §5 里）：
 *   * **一个库可能有多条路径**：卡片显示**当前在线的那条**；离线时显示上次已知路径 + 徽标；
 *   * **在线时那一格是齿轮**（开库设置）、**离线时是离线图标**（点它 = 对所有登记路径
 *     重新查找一次，找不到不是错误）—— **一律图标化，不写「离线」二字**（人类 2026-09-16）；
 *   * **照片数读不到时显示「—」而不是 0** —— 0 会让用户以为库是空的。
 */

import { createSignal, For, Show } from "solid-js";
import {
  IconAlertTriangle,
  IconCloudOff,
  IconFolder,
  IconLoader2,
  IconSettings,
} from "@tabler/icons-solidjs";
import { LibrarySettingsDialog } from "./LibrarySettingsDialog.tsx";
import type { RepositoryView } from "../../api/types.ts";
import { ScrollBox } from "../../components/ui/ScrollBar.tsx";
import { t } from "../../i18n/index.ts";
import { formatCount, type GroupingLocale } from "../../lib/format.ts";
import type { LoadStatus } from "../../lib/load-status.ts";

export interface RepositoryListProps {
  repositories: readonly RepositoryView[];
  status: LoadStatus;
  error: string | null;
  selectedId: string | null;
  /** 正在重新查找的库 id（按钮转圈） */
  remountingId?: string | null;
  /** 重挂载失败时的提示（库 id → 文案） */
  remountErrors?: Record<string, string>;
  onSelect: (id: string) => void;
  onRemount: (id: string) => void;
  /** 模版保存成功后通知外面（列表拿到的是缓存的模版，要重新读一遍） */
  onTemplateSaved?: (id: string, template: string) => void;
  /** 库设置里发现这个库读不到了 → 交给中央状态降级（所有界面同步） */
  onRepositoryStale?: (id: string) => void;
  onCreate: () => void;
  onRetry?: () => void;
  locale?: GroupingLocale;
  class?: string;
}

export function RepositoryList(props: RepositoryListProps) {
  const locale = (): GroupingLocale => props.locale ?? "zh-CN";
  /** 正在看设置的库（`null` = 没开）；对话框由列表自己持有，调用方不必管 */
  const [settingsId, setSettingsId] = createSignal<string | null>(null);
  /** 正在看设置的那个库**当前的状态行**（中央状态里那一份，不是副本） */
  const repository = (): RepositoryView | undefined =>
    props.repositories.find((row) => row.id === settingsId());

  return (
    <div class={["flex min-h-0 flex-col gap-1", props.class ?? ""].join(" ")}>
      <Show
        when={props.status !== "error"}
        fallback={
          <div class="flex items-start gap-1.5 p-1 text-fs-1 text-fg-2">
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
        }
      >
        <ScrollBox class="min-h-0 flex-1">
          <Show
            when={props.repositories.length > 0}
            fallback={
              <p class="p-1 text-fs-1 text-fg-3">
                {props.status === "loading"
                  ? t("common.loading")
                  : t("repo.empty")}
              </p>
            }
          >
            <For each={props.repositories}>
              {(repository) => (
                <RepositoryCard
                  repository={repository}
                  selected={repository.id === props.selectedId}
                  remounting={props.remountingId === repository.id}
                  remountError={props.remountErrors?.[repository.id] ?? null}
                  locale={locale()}
                  onSelect={props.onSelect}
                  onRemount={props.onRemount}
                  onOpenSettings={(id) => setSettingsId(id)}
                />
              )}
            </For>
          </Show>

          {/* 建库入口在列表最下面（`design/main.md` §3.3：做成「+ 号库」的样子） */}
          <button
            type="button"
            class={[
              "mt-1 flex w-full cursor-pointer items-center gap-2 rounded-ui",
              "px-(--pad-x) text-fs-2 text-fg-2 transition-colors",
              "hover:bg-state-hover hover:text-fg-1",
            ].join(" ")}
            style={{ height: "var(--card-h)" }}
            onClick={() => props.onCreate()}
          >
            <span
              class="flex size-6 shrink-0 items-center justify-center rounded-ui border border-dashed border-fg-3 text-fg-3"
              aria-hidden="true"
            >
              +
            </span>
            <span class="min-w-0 flex-1 truncate text-start">
              {t("repo.create")}
            </span>
          </button>
        </ScrollBox>
      </Show>

      {/* 库设置：整个列表共用**一个**对话框实例（同一时刻只会看一个库） */}
      <LibrarySettingsDialog
        open={settingsId() !== null}
        repositoryId={settingsId()}
        repositoryName={repository()?.name}
        {...(repository() === undefined ? {} : { repository: repository() })}
        onOpenChange={(open) => {
          if (!open) setSettingsId(null);
        }}
        onSaved={(template) => {
          const id = settingsId();
          if (id !== null) props.onTemplateSaved?.(id, template);
        }}
        {...(props.onRepositoryStale === undefined
          ? {}
          : { onStale: (id: string) => props.onRepositoryStale?.(id) })}
      />
    </div>
  );
}

function RepositoryCard(props: {
  repository: RepositoryView;
  selected: boolean;
  remounting: boolean;
  remountError: string | null;
  locale: GroupingLocale;
  onSelect: (id: string) => void;
  onRemount: (id: string) => void;
  onOpenSettings: (id: string) => void;
}) {
  const countLabel = () =>
    props.repository.photoCount === null
      ? t("repo.count_unknown")
      : t("grid.count", {
          n: formatCount(props.repository.photoCount, props.locale),
        });

  return (
    <div
      role="option"
      aria-selected={props.selected}
      tabindex={0}
      class={[
        "flex cursor-pointer flex-col justify-center gap-(--gap) rounded-ui px-(--pad-x)",
        "transition-colors",
        props.selected
          ? "bg-state-selected"
          : "hover:bg-state-hover",
      ].join(" ")}
      style={{ "min-height": "var(--card-h)" }}
      onClick={() => props.onSelect(props.repository.id)}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          props.onSelect(props.repository.id);
        }
      }}
    >
      <div class="flex min-w-0 items-center gap-2">
        <span
          class={[
            "flex size-6 shrink-0 items-center justify-center rounded-ui",
            props.repository.online
              ? "bg-brand text-fg-on-brand"
              : "bg-surface-track text-fg-3",
          ].join(" ")}
          aria-hidden="true"
        >
          <IconFolder size={14} />
        </span>
        <span class="min-w-0 flex-1 truncate text-fs-2 text-fg-1">
          {props.repository.name}
        </span>

        {/*
          右侧那一格：**在线 = 齿轮**（开库设置）、**离线 = 离线图标**（点它重新查找）。
          设计稿（`RepoGear` / `RepoOffline`）给了两个形态，但**不带文字** ——
          「离线」二字只进无障碍名与悬停提示（人类 2026-09-16 明确要求）。
          点它自己 `stopPropagation`：卡片整块是「选中这个库」，不能顺带把设置也开了。
        */}
        <Show
          when={props.repository.online}
          fallback={
            <button
              type="button"
              aria-label={t("repo.remount")}
              title={t("repo.remount")}
              class={[
                "flex size-6 shrink-0 cursor-pointer items-center justify-center rounded-ui",
                "bg-surface-bar text-fg-2 hover:text-fg-1",
              ].join(" ")}
              onClick={(event) => {
                event.stopPropagation();
                props.onRemount(props.repository.id);
              }}
            >
              <Show
                when={props.remounting}
                fallback={<IconCloudOff size={14} aria-hidden="true" />}
              >
                <IconLoader2 size={14} class="animate-spin" aria-hidden="true" />
              </Show>
            </button>
          }
        >
          <button
            type="button"
            aria-label={t("repo.settings_title")}
            title={t("repo.settings_title")}
            class="flex size-6 shrink-0 cursor-pointer items-center justify-center rounded-ui text-fg-2 hover:bg-state-hover hover:text-fg-1"
            onClick={(event) => {
              event.stopPropagation();
              props.onOpenSettings(props.repository.id);
            }}
          >
            <IconSettings size={14} aria-hidden="true" />
          </button>
        </Show>

        {/* 多路径：多于一条时给个小提示（卡片只显示当前在线那条） */}
        <Show when={props.repository.paths.length > 1}>
          <span class="shrink-0 text-fs-0 text-fg-3 tnum">
            {t("repo.path_count", { n: props.repository.paths.length })}
          </span>
        </Show>
      </div>

      <div class="flex min-w-0 items-center gap-2 ps-8">
        <span
          dir="ltr"
          class="min-w-0 flex-1 truncate text-fs-1 text-fg-3"
          title={props.repository.displayPath}
        >
          {props.repository.displayPath}
        </span>
        <span class="shrink-0 text-fs-1 text-fg-2 tnum">{countLabel()}</span>
      </div>

      <Show when={props.remountError}>
        {(message) => (
          <p class="ps-8 text-fs-0 text-fg-3">{message()}</p>
        )}
      </Show>
    </div>
  );
}
