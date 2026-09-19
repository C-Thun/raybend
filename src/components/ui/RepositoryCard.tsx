/**
 * 库卡片 —— **全项目唯一一份**（人类 2026-09-19：「这种东西怎么可能有出现 2 个组件？
 * 拿 import 里的替换掉」）。
 *
 * 导入侧左列与浏览侧左列用的是同一个组件，差异全部由 props 决定：
 *
 * ```text
 * ┌──────────────────────────────────────────────┐
 * │ [▣] 库名                              [⚙]    │   ← 图标块 + 名字 + 行尾那一格
 * │     C:\…\photos      1,234 张                │   ← 路径（缩写）+ 相片总数
 * └──────────────────────────────────────────────┘
 * ```
 *
 * 三条纪律：
 * 1. **行尾那一格**：在线 = 齿轮（开库设置）、离线 = 离线图标（点它重新查找）；
 *    「离线」二字只进无障碍名与悬停提示（人类 2026-09-16 明确要求）。
 * 2. 点那一格要 `stopPropagation` —— 整张卡是「选中这个库」，不能顺带把设置也开了。
 * 3. **相片总数（不含 `_RAW`）两个工作区都显示**（人类 2026-09-19）：
 *    `null` = 还没同步过，显示 `—`，不要编一个 0 出来。
 */

import { Show, type JSX } from "solid-js";
import { IconCloudOff, IconFolder, IconLoader2, IconSettings } from "@tabler/icons-solidjs";

import { t } from "../../i18n/index.ts";
import { formatCount, type GroupingLocale } from "../../lib/format.ts";
import { shortPath } from "../../lib/shortpath.ts";

/** 重新挂载失败的事实；状态层只存事实，卡片按当前语言成句。 */
export type RepositoryRemountError =
  | { kind: "not_found"; tried: number }
  | { kind: "message"; text: string };

export interface RepositoryCardProps {
  name: string;
  /** 展示用路径（离线时也显示登记过的那条）；`null` = 没有可显示的路径 */
  displayPath: string | null;
  /** 相片总数（**不含 `_RAW`**）；`null` = 还没同步过 */
  photosCount: number | null;
  online: boolean;
  /** 这一张是不是「当前选中的库」 */
  selected?: boolean;
  /** 正在重新查找（离线卡片上转圈） */
  remounting?: boolean;
  /** 重新查找失败的原因（显示在卡片下方）；`null` = 没有错误 */
  remountError?: RepositoryRemountError | null;
  locale: GroupingLocale;
  /** 点卡片 = 选中这个库 */
  onSelect: () => void;
  /** 齿轮：开库设置（仅在线时显示） */
  onOpenSettings?: () => void;
  /** 离线图标：重新查找（仅离线时显示） */
  onRemount?: () => void;
}

/** 重新查找失败：按「事实」成句（判断窄化交给这个函数，别塞进 JSX 的三元里） */
function remountText(error: RepositoryRemountError): string {
  return error.kind === "not_found"
    ? t("repo.remount_failed", { tried: String(error.tried) })
    : error.text;
}

export function RepositoryCard(props: RepositoryCardProps): JSX.Element {
  const countLabel = (): string =>
    props.photosCount === null
      ? t("repo.count_unknown")
      : t("grid.count", { n: formatCount(props.photosCount, props.locale) });

  return (
    <div
      role="option"
      aria-selected={props.selected === true}
      tabindex={0}
      class={[
        /*
         * 上下内边距**显式给**（`py-(--pad-y)`）：只靠 `justify-center` 撑的话，
         * 字行盒与图标块的高度差会让上下的留白看起来不一样（人类 2026-09-16 报的
         * 「顶部几乎没有 padding，和底部有明显差异」）。
         */
        "flex cursor-pointer flex-col justify-center gap-(--gap) rounded-ui px-(--pad-x) py-(--pad-y)",
        "transition-colors",
        props.selected === true ? "bg-state-selected" : "hover:bg-state-hover",
      ].join(" ")}
      style={{ "min-height": "var(--card-h)" }}
      onClick={() => props.onSelect()}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          props.onSelect();
        }
      }}
    >
      {/* ── 第一行：图标块 + 库名 + 行尾那一格 ── */}
      <div class="flex min-w-0 items-center gap-2">
        <span
          class={[
            "flex size-6 shrink-0 items-center justify-center rounded-ui",
            props.online ? "bg-brand text-fg-on-brand" : "bg-surface-track text-fg-3",
          ].join(" ")}
          aria-hidden="true"
        >
          <IconFolder size={14} />
        </span>
        <span class="min-w-0 flex-1 truncate text-fs-2 text-fg-1">{props.name}</span>

        <Show
          when={props.online}
          fallback={
            <button
              type="button"
              aria-label={t("repo.remount")}
              title={t("repo.remount")}
              class="flex size-6 shrink-0 items-center justify-center rounded-ui text-danger hover:bg-state-hover"
              onClick={(event) => {
                // 整张卡是「选中这个库」，点这一格不能顺带把库也选了
                event.stopPropagation();
                props.onRemount?.();
              }}
            >
              <Show
                when={props.remounting === true}
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
            class="flex size-6 shrink-0 items-center justify-center rounded-ui text-fg-2 hover:bg-state-hover hover:text-fg-1"
            onClick={(event) => {
              event.stopPropagation();
              props.onOpenSettings?.();
            }}
          >
            <IconSettings size={14} aria-hidden="true" />
          </button>
        </Show>
      </div>

      {/* ── 第二行：路径缩写 + 相片总数（两个工作区都显示） ── */}
      <div class="flex min-w-0 items-center gap-1.5 ps-8 text-fs-0 text-fg-3">
        <Show when={(props.displayPath ?? "") !== ""}>
          <span class="min-w-0 flex-1 truncate" title={props.displayPath ?? ""}>
            {shortPath(props.displayPath ?? "", { maxLength: 40 })}
          </span>
        </Show>
        <span class="shrink-0 tnum">{countLabel()}</span>
      </div>

      <Show when={props.remountError}>
        {(error) => (
          <p class="ps-8 text-fs-0 text-danger">
            {/* 两种事实各自成句：`not_found` 带上试过几处；`message` 是后端原话 */}
            {remountText(error())}
          </p>
        )}
      </Show>
    </div>
  );
}
