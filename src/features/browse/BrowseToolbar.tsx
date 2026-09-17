/**
 * 浏览模式的 `toolsbar` 装配（`BROWSE.md` §3、`design/browse.md` §2.1）。
 *
 * 四组能力，从左到右：
 *
 * ```text
 * [筛选]  [旗标][弃掉][移除旗标]  [★★★★★]  [●色标×6]  [喜欢]  |  [标签]  [锁1][锁2]
 * ```
 *
 * # 三态是这一行最容易写错的地方
 *
 * 每个标记控件显示的是**当前选中照片的状态**：
 *
 * | 选中情况 | 控件 |
 * | --- | --- |
 * | 没选 | 无值（暗） |
 * | 选中且取值一致 | 显示那个值 |
 * | 取值不一致 | **混合态**（与无值视觉可分 —— 无值=暗、混合=半亮 + 短横） |
 *
 * 「混合」绝不能被当成「无值」：那样点一下会把一批已经打了标的照片全部清掉
 * （语义在 `lib/marking-state.ts`，那里有测试）。
 *
 * # 筛选模式（`BROWSE.md` §3.1）
 *
 * 顶部那个开关一打开，**后面的标记控件全部从「设置」变成「筛选」**：
 * 点 3 星不再是「给选中的照片打 3 星」，而是「只看 3 星的照片」。
 * 这是浏览模式最独特的一条机制，也是用户点名的「很爽」用法。
 *
 * 旗标**不进筛选**：它只活在内存里（库里没有这一列），按旗标过滤要动网格的可见集合，
 * 属 W2（`plans/M2.md` 的阶段 6 只出控件本身）。
 */

import { For, Show, createSignal } from "solid-js";
import {
  IconBan,
  IconFlag,
  IconLock,
  IconStar,
  IconStarFilled,
  IconThumbDown,
  IconThumbUp,
} from "@tabler/icons-solidjs";

import { ToggleBlock } from "../../components/ui/ToggleBlock.tsx";
import { Button } from "../../components/ui/Button.tsx";
import { t } from "../../i18n/index.ts";
import {
  COLOR_VALUES,
  LOCK_LEVELS,
  nextRating,
  triState,
  type TriState,
} from "../../lib/marking-state.ts";
import type { BrowseStore } from "./store.ts";

export interface BrowseToolbarProps {
  store: BrowseStore;
  /** 打开标签弹窗（W2 接线；现在只把按钮摆在那里并禁用）。 */
  onOpenTags?: () => void;
}

/** 色标 → 令牌类名（必须是字面量，Tailwind 才扫得到）。 */
const COLOR_DOT: Record<string, string> = {
  red: "bg-(--label-red)",
  yellow: "bg-(--label-yellow)",
  green: "bg-(--label-green)",
  blue: "bg-(--label-blue)",
  purple: "bg-(--label-purple)",
};

/** 混合态的视觉：半亮 + 一个短横。 */
function MixedMark() {
  return (
    <span class="text-fg-2 opacity-70" aria-hidden="true">
      —
    </span>
  );
}

export function BrowseToolbar(props: BrowseToolbarProps) {
  const store = props.store;
  const [filterMode, setFilterMode] = createSignal(false);

  const selected = () => store.selectedItems();
  const hasSelection = () => selected().length > 0;

  /** 选中照片的某个字段收敛成三态。 */
  const pick = <T extends number | string>(
    read: (item: (typeof selected extends () => (infer U)[] ? U : never)) => T | null,
  ): TriState<T> => triState(selected().map((item) => read(item) ?? undefined));

  const ratingState = () => pick<number>((item) => item.rating);
  const colorState = () => pick<string>((item) => item.colorLabel);
  const likeState = () => pick<string>((item) => item.likeState);
  const lockState = () => pick<number>((item) => item.lockLevel);

  /** 三态「恰好等于某个值」——写成具名函数，TS 才收窄得了（重复调用表达式不行）。 */
  const isExactly = (state: TriState<string>, value: string): boolean =>
    state.kind === "value" && state.value === value;

  /** 选中的 id（打标记用）。 */
  const ids = () => store.selectedIds();

  /** 有选中照片时才让标记控件可用。 */
  const markDisabled = () => !hasSelection();

  async function markRating(star: number): Promise<void> {
    if (filterMode()) {
      // 筛选模式：点几星就是「只看几星」（再点一次同一个 → 取消筛选）
      const current = store.filter().ratings ?? [];
      const next = current.length === 1 && current[0] === star ? [] : [star];
      store.patchFilter({ ratings: next });
      return;
    }
    const values = selected().map((item) => item.rating);
    await store.mark({ kind: "rating", value: nextRating(values, star) });
  }

  async function markColor(color: string | null): Promise<void> {
    if (filterMode()) {
      const current = store.filter().colors ?? [];
      const key = color ?? "none";
      const next = current.includes(key) ? current.filter((c) => c !== key) : [key];
      store.patchFilter({ colors: next });
      return;
    }
    await store.mark({ kind: "color", value: color });
  }

  async function markLike(value: "like" | "dislike" | null): Promise<void> {
    if (filterMode()) {
      const current = store.filter().likes ?? [];
      const key = value ?? "none";
      const next = current.includes(key) ? current.filter((l) => l !== key) : [key];
      store.patchFilter({ likes: next });
      return;
    }
    await store.mark({ kind: "like", value });
  }

  async function markLock(level: number): Promise<void> {
    if (filterMode()) {
      const current = store.filter().locks ?? [];
      const next = current.includes(level) ? current.filter((l) => l !== level) : [level];
      store.patchFilter({ locks: next });
      return;
    }
    // 再点一次已锁的级别 = 解锁（否则锁上就撤不掉）
    const current = lockState();
    const target =
      current.kind === "value" && current.value === level ? LOCK_LEVELS.none : level;
    await store.mark({ kind: "lock", value: target });
  }

  return (
    <div class="flex items-center gap-1">
      {/* 筛选开关：打开后右侧控件全部变成筛选语义 */}
      <ToggleBlock
        pressed={filterMode()}
        onPressedChange={(pressed) => {
          setFilterMode(pressed);
          // 关掉筛选时把筛选条件清干净 —— 否则界面看起来「没筛」却还少着照片
          if (!pressed) store.patchFilter({ ratings: [], colors: [], likes: [], locks: [] });
        }}
        icon={<IconBan size={14} />}
        label={t("browse.filterHint")}
      >
        {t("browse.filter")}
      </ToggleBlock>

      <span class="w-5" />

      {/* 旗标（只有两态；不进筛选，见文件头说明） */}
      <ToggleBlock
        pressed={false}
        disabled={markDisabled()}
        onClick={() => void store.setFlag(ids(), "pick")}
        icon={<IconFlag size={14} />}
        label={t("browse.pick")}
      >
        {t("browse.pick")}
      </ToggleBlock>
      <ToggleBlock
        pressed={false}
        disabled={markDisabled()}
        onClick={() => void store.setFlag(ids(), "reject")}
        icon={<IconBan size={14} />}
        label={t("browse.reject")}
      >
        {t("browse.reject")}
      </ToggleBlock>
      <Button
        variant="ghost"
        disabled={store.picks().size + store.rejects().size === 0}
        icon={<IconBan size={14} />}
        onClick={() => void store.clearFlags()}
      >
        {t("browse.flagClear")}
      </Button>

      <span class="w-5" />

      {/* 星级：五颗；混合态用「短横 + 半亮」区分 */}
      <div class="flex items-center gap-0.5">
        <Show when={ratingState().kind === "mixed"}>
          <MixedMark />
        </Show>
        <For each={[1, 2, 3, 4, 5]}>
          {(star) => {
            const state = () => ratingState();
            const filled = () => {
              const s = state();
              return s.kind === "value" && s.value >= star;
            };
            const iconClass = () => {
              const s = state();
              if (s.kind === "mixed") return "text-fg-2 opacity-70";
              return filled() ? "text-brand" : "text-fg-3";
            };
            return (
              <button
                type="button"
                disabled={markDisabled()}
                aria-label={t("grid.rating").replace("{n}", String(star))}
                onClick={() => void markRating(star)}
                class={[
                  "rounded-(--radius) p-0.5 hover:bg-state-hover",
                  markDisabled() ? "opacity-40" : "",
                ].join(" ")}
              >
                <span class={iconClass()}>
                  {filled() ? <IconStarFilled size={14} /> : <IconStar size={14} />}
                </span>
              </button>
            );
          }}
        </For>
      </div>

      <span class="w-3" />

      {/* 色标：5 个实心点 + 1 个空心圈（无色） */}
      <div class="flex items-center gap-1">
        <Show when={colorState().kind === "mixed"}>
          <MixedMark />
        </Show>
        <For each={COLOR_VALUES}>
          {(color) => {
            const active = () => {
              const s = colorState();
              return s.kind === "value" && s.value === color;
            };
            const filtered = () =>
              filterMode() &&
              (store.filter().colors ?? []).includes(color ?? "none");
            return (
              <button
                type="button"
                disabled={markDisabled()}
                aria-label={color === null ? t("browse.colorNone") : color}
                onClick={() => void markColor(color)}
                class={[
                  "h-3.5 w-3.5 rounded-full ring-1 ring-line-2",
                  color === null ? "bg-transparent" : (COLOR_DOT[color] ?? ""),
                  active() || filtered() ? "ring-2 ring-brand" : "",
                  markDisabled() ? "opacity-40" : "",
                ].join(" ")}
              />
            );
          }}
        </For>
      </div>

      <span class="w-3" />

      {/* 喜欢：三态（喜欢 / 不喜欢 / 取消） */}
      <div class="flex items-center gap-0.5">
        <Show when={likeState().kind === "mixed"}>
          <MixedMark />
        </Show>
        <ToggleBlock
          pressed={isExactly(likeState(), "like")}
          disabled={markDisabled()}
          onClick={() => void markLike("like")}
          icon={<IconThumbUp size={14} />}
          label={t("browse.like")}
        />
        <ToggleBlock
          pressed={isExactly(likeState(), "dislike")}
          disabled={markDisabled()}
          onClick={() => void markLike("dislike")}
          icon={<IconThumbDown size={14} />}
          label={t("browse.reject")}
        />
      </div>

      <span class="w-5" />

      {/* 标签（弹窗在 W2） */}
      <Button variant="ghost" disabled onClick={props.onOpenTags}>
        {t("browse.tag")}
      </Button>

      {/* 锁：两级（再点一次解锁） */}
      <div class="flex items-center gap-0.5">
        <Show when={lockState().kind === "mixed"}>
          <MixedMark />
        </Show>
        <For each={[LOCK_LEVELS.noDelete, LOCK_LEVELS.noEdit]}>
          {(level) => {
            const isSet = () => {
              const s = lockState();
              return s.kind === "value" && s.value >= level;
            };
            const filtered = () =>
              filterMode() && (store.filter().locks ?? []).includes(level);
            return (
              <ToggleBlock
                pressed={isSet() || filtered()}
                disabled={markDisabled()}
                onClick={() => void markLock(level)}
                icon={<IconLock size={14} />}
                label={
                  level === LOCK_LEVELS.noDelete
                    ? t("browse.lockNoDelete")
                    : t("browse.lockNoEdit")
                }
              >
                {level}
              </ToggleBlock>
            );
          }}
        </For>
      </div>
    </div>
  );
}
