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
 * 1. 行尾齿轮始终可开设置；左侧图标表达连接状态，离线可点它重查。
 *    状态文字只进无障碍状态与悬停提示，不增加卡片行。
 * 2. 操作按钮 `stopPropagation` —— 不顺带选中这个库。
 * 3. **相片总数（不含 `_RAW`）两个工作区都显示**（人类 2026-09-19）：
 *    `null` = 还没同步过，显示 `—`，不要编一个 0 出来。
 */

import { Show, type JSX } from "solid-js";
import { IconAlertTriangle, IconCloudOff, IconFolder, IconLoader2, IconSettings } from "@tabler/icons-solidjs";

import { repositoryErrorKey } from "../../i18n/repository-feedback.ts";

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
  /** 连接呈现所需的两项；UI 不依赖 IPC DTO，其余连接事实由状态层持有。 */
  connection?: { state: string; reason: string | null } | undefined;
  /** 这一张是不是「当前选中的库」 */
  selected?: boolean;
  /** 正在重新查找（离线卡片上转圈） */
  remounting?: boolean;
  /** 重新查找失败的原因（通过状态图标提示）；`null` = 没有错误 */
  remountError?: RepositoryRemountError | null;
  locale: GroupingLocale;
  /**
   * 可交互（默认 `true`）：整张卡是「选中这个库」的选项（`role=option` + 键盘）。
   * `false` = **只展示**（导入确认弹窗里那张卡）：没有点击/键盘/悬停，也不出行尾那一格
   * —— 那里是拍板的地方，不是换库、改设置的地方。
   */
  interactive?: boolean;
  /** 点卡片 = 选中这个库（`interactive: false` 时不显示） */
  onSelect?: () => void;
  /** 齿轮：所有状态下都能开库设置 */
  onOpenSettings?: () => void;
  /** 离线图标：重新查找（仅离线时显示） */
  onRemount?: () => void;
}

/** 重新查找失败：按「事实」成句（判断窄化交给这个函数，别塞进 JSX 的三元里） */
function remountText(error: RepositoryRemountError): string {
  return error.kind === "not_found"
    ? t("repo.remount_failed", { tried: String(error.tried) })
    : t("repo.location_error.io_failure");
}

export function RepositoryCard(props: RepositoryCardProps): JSX.Element {
  const interactive = (): boolean => props.interactive !== false;
  const checking = () => props.remounting === true || props.connection?.state === "checking" || props.connection?.state === "releasing";
  const fault = () => props.connection?.state === "unavailable" || props.remountError?.kind === "message";
  const feedback = (): string => {
    if (props.connection?.state === "released") return t("repo.connection.released");
    if (props.connection?.state === "releasing") return t("repo.connection.releasing");
    if (checking()) return t("repo.connection.checking");
    if (props.connection?.reason) return t(repositoryErrorKey(props.connection.reason));
    if (props.remountError) return remountText(props.remountError);
    if (props.connection?.state === "unknown") return t("repo.connection.unknown");
    return t(props.online ? "repo.connection.online" : "repo.location_error.not_found");
  };
  const countLabel = (): string =>
    props.photosCount === null
      ? t("repo.count_unknown")
      : t("grid.count", { n: formatCount(props.photosCount, props.locale) });

  return (
    <div
      role={interactive() ? "option" : undefined}
      aria-selected={interactive() ? props.selected === true : undefined}
      tabindex={interactive() ? 0 : undefined}
      class={[
        /*
         * 上下内边距**显式给**（`py-(--pad-y)`）：只靠 `justify-center` 撑的话，
         * 字行盒与图标块的高度差会让上下的留白看起来不一样（人类 2026-09-16 报的
         * 「顶部几乎没有 padding，和底部有明显差异」）。
         */
        "flex flex-col justify-center gap-(--gap) rounded-ui px-(--pad-x) py-(--pad-y)",
        "transition-colors",
        interactive() ? "cursor-pointer" : "",
        props.selected === true
          ? "bg-state-selected"
          : interactive()
            ? "hover:bg-state-hover"
            : "bg-surface-track",
      ].join(" ")}
      style={{ "min-height": "var(--card-h)" }}
      onClick={interactive() ? () => props.onSelect?.() : undefined}
      onKeyDown={
        interactive()
          ? (event) => {
              if (event.target !== event.currentTarget) return;
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                props.onSelect?.();
              }
            }
          : undefined
      }
    >
      {/* ── 第一行：图标块 + 库名 + 行尾那一格 ── */}
      <div class="flex min-w-0 items-center gap-2">
        <button type="button" disabled={!interactive() || props.online || checking()}
          aria-label={interactive() && !props.online ? t("repo.remount") : feedback()} aria-description={feedback()} title={feedback()}
          class={[
            "flex size-6 shrink-0 items-center justify-center rounded-ui",
            props.online ? "bg-brand text-fg-on-brand" : fault() ? "bg-surface-bar text-danger" : "bg-surface-bar text-fg-3",
            interactive() && !props.online ? "hover:bg-state-hover" : "",
          ].join(" ")}
          onClick={event => { event.stopPropagation(); props.onRemount?.(); }}>
          <Show when={checking()} fallback={
            <Show when={props.online} fallback={
              <Show when={fault()} fallback={<IconCloudOff size={14} aria-hidden="true" />}>
                <IconAlertTriangle size={14} aria-hidden="true" />
              </Show>
            }><IconFolder size={14} aria-hidden="true" /></Show>
          }><IconLoader2 size={14} class="animate-spin" aria-hidden="true" /></Show>
        </button>
        <span class="min-w-0 flex-1 truncate text-fs-2 text-fg-1">{props.name}</span>

        {/* 永久设置入口；离线时仍能登记新盘符。 */}
        <Show when={interactive()}>
          <button type="button" aria-label={t("repo.settings_title")} title={t("repo.settings_title")}
            class="flex size-6 shrink-0 items-center justify-center rounded-ui text-fg-2 hover:bg-state-hover hover:text-fg-1"
            onClick={event => { event.stopPropagation(); props.onOpenSettings?.(); }}>
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

      <span class="sr-only" role="status" aria-live="polite">{feedback()}</span>
    </div>
  );
}
