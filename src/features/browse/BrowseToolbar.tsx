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

import { For, Show } from "solid-js";
import {
  IconFlag,
  IconLock,
  IconArrowBackUp,
  IconFilter,
  IconFlagFilled,
  IconFlagOff,
  IconArrowForwardUp,
  IconTag,
  IconStar,
  IconStarFilled,
  IconThumbDownFilled,
  IconThumbUpFilled,
} from "@tabler/icons-solidjs";

import { ToggleBlock } from "../../components/ui/ToggleBlock.tsx";
import { ConfirmDialog } from "../../components/ui/Dialog.tsx";
import { createEasyDestroy } from "../../lib/easy-destroy.ts";
import { markNotice, type MarkNotice } from "./mark-feedback.ts";
import type { MarkResult } from "../../api/types.ts";
import type { ToastStore } from "../../components/ui/Toast.tsx";
import { filterFromSelection } from "./filter.ts";
import { colorText } from "./labels.ts";
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
import { COLOR_DOT_CLASS, isColorLabel } from "../../lib/color-labels.ts";

export interface BrowseToolbarProps {
  store: BrowseStore;
  /** 打开标签弹窗（W2 接线；现在只把按钮摆在那里并禁用）。 */
  onOpenTags?: () => void;
  /**
   * 提示通道（`components/ui/Toast.tsx`）。
   *
   * 为什么走 prop 而不是让工具条自己建一个：提示要**挂在根层**（`--z-toast`、不被条带的
   * 层叠上下文困住），所以 store 由组装层建、这里只往里推。可选 = 陈列室/单测里不必准备。
   */
  toast?: ToastStore;
}

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
  /** 筛选态住在 store 里（结果区的 chips 也要读它，见 `store.filterMode`） */
  const filterMode = () => store.filterMode();
  /**
   * 「清空旗标」的确认（`easy destroy` 范式：默认弹确认，按住 `Shift` 跳过）。
   *
   * 为什么它用 `easy destroy` 而**删照片不用**（人类 2026-09-19 的批注）：
   * `easy destroy` 是给「不为了批量而把界面变拥挤、又要能连续快速处理单张」准备的 ——
   * 清旗标是个全局开关动作，值一次确认；删照片本身支持多选批量，不需要它。
   */
  const clearFlags = createEasyDestroy();
  /**
   * 标记之后要说的话（被锁挡住 / 什么都没改）—— `plans/M2-W2-tail.md` 3.1。
   *
   * 2026-09-19：从「工具条里的一行小字」改成 **toast**（4.2 落地）——
   * 同一类信息只该有一个去处，而且 toast 能顺手挂「撤销」。
   * `markNotice()` 那套判定一个字没变，只是渲染换了地方。
   */
  const notice = (result: MarkResult | null): MarkNotice | null => markNotice(result);

  const selected = () => store.selectedItems();
  const hasSelection = () => selected().length > 0;

  /** 选中照片的某个字段收敛成三态。 */
  const pick = <T extends number | string>(
    read: (item: (typeof selected extends () => (infer U)[] ? U : never)) => T | null,
  ): TriState<T> => triState(selected().map((item) => read(item) ?? undefined));

  /*
   * 三态取值**只在标记态用**（筛选态一律读 `store.filter()`，见文件头的表）。
   * 保留 `pick` 是因为标记态仍要看「选中的照片是什么状态」。
   */
  const ratingState = () => pick<number>((item) => item.rating);
  const colorState = () => pick<string>((item) => item.colorLabel);
  const likeState = () => pick<string>((item) => item.likeState);
  const lockState = () => pick<number>((item) => item.lockLevel);

  /** 筛选态的星标阈值（`null` = 没筛星） */
  const filterRating = (): number | null => store.filter().minRating ?? null;
  /** 筛选态的色标条件 */
  const filterColors = (): readonly string[] => store.filter().colors ?? [];
  /** 筛选态的喜欢条件（`"like"` / `"dislike"`，空 = 没筛） */
  const filterLike = (): string | null => (store.filter().likes ?? [])[0] ?? null;
  /** 筛选条件里有没有这个喜欢值（`.includes` —— 与色标 / 锁同一口径） */
  const likeFiltered = (value: string): boolean => (store.filter().likes ?? []).includes(value);
  /** 筛选态的锁条件 */
  const filterLocks = (): readonly number[] => store.filter().locks ?? [];
  /** 筛选态的旗标条件（`pick` / `reject` / `none`） */
  const filterFlag = (): string | null => store.filter().flag?.mode ?? null;
  /** 选中的这些照片是不是**都有旗标**（旗标在内存里，只有 store 知道） */
  const selectedAllPicked = (): boolean => {
    const items = selected();
    return items.length > 0 && items.every((item) => store.picks().has(item.id));
  };

  /** 三态「恰好等于某个值」——写成具名函数，TS 才收窄得了（重复调用表达式不行）。 */
  const isExactly = (state: TriState<string>, value: string): boolean =>
    state.kind === "value" && state.value === value;

  /**
   * 赞 / 踩按钮的按下态：标记态看选中照片，筛选态看**筛选条件**。
   *
   * 2026-09-20 人类报的 bug：筛选态下点「赞 / 踩」确实筛了，但按钮不亮 ——
   * 以前这里只读 `likeState()`（选中照片的三态），而筛选条件一变 `reload()`
   * 就把选中清空了，于是永远显示「没值」。色标 / 锁早就是「筛选态读条件」的口径。
   */
  const likePressed = (value: "like" | "dislike"): boolean =>
    filterMode() ? likeFiltered(value) : isExactly(likeState(), value);

  /**
   * 星标点亮到第几颗：标记态是「选中照片有几星」，筛选态是**阈值**（`minRating`）。
   *
   * 阈值语义下点亮 1..N 颗 —— 与 chips 里的 `≥N 星` 是同一件事（同 `likePressed` 的修复）。
   */
  const starFilled = (star: number): boolean => {
    if (filterMode()) {
      const threshold = filterRating();
      return threshold !== null && star <= threshold;
    }
    const state = ratingState();
    return state.kind === "value" && state.value >= star;
  };

  /**
   * 撤销 / 重做：做完给一条提示（说了「撤了什么」），失败也说话。
   *
   * 背面也要给反馈的理由：撤销之后照片上的标记会变，但**变回什么样**用户不一定记得 ——
   * 「撤销：标 3 星」这句话正是补上这一点。
   */
  async function runHistory(kind: "undo" | "redo"): Promise<void> {
    const before = store.undoState();
    const label = kind === "undo" ? before.undoLabel : before.redoLabel;
    const result = kind === "undo" ? await store.undo() : await store.redo();
    if (result === null) return;
    const key = kind === "undo" ? "browse.undoWith" : "browse.redoWith";
    props.toast?.show({
      tone: "success",
      message: label === null ? t(kind === "undo" ? "browse.undo" : "browse.redo") : t(key).replace("{label}", label),
      action:
        kind === "undo" && result.canRedo
          ? { label: t("browse.redo"), onAction: () => void store.redo() }
          : undefined,
    });
  }

  /** 撤销/重做按钮的悬停文案：有具体动作名就带上（「撤销：标 3 星」） */
  const undoTitle = (): string => {
    const state = store.undoState();
    return state.canUndo && state.undoLabel !== null
      ? t("browse.undoWith").replace("{label}", state.undoLabel)
      : t("browse.undo");
  };
  const redoTitle = (): string => {
    const state = store.undoState();
    return state.canRedo && state.redoLabel !== null
      ? t("browse.redoWith").replace("{label}", state.redoLabel)
      : t("browse.redo");
  };

  /** 选中的 id（打标记用）。 */
  const ids = () => store.selectedIds();

  /**
   * 标记态的旗标开关：选中的**都已有旗标** ⇒ 取消；否则全部打上。
   *
   * 「都取消」而不是「逐个取反」：旗标是个整体状态，逐个取反会让一批照片里的旗子
   * 一半亮一半灭，用户根本看不出点了以后发生了什么。
   */
  async function toggleFlag(): Promise<void> {
    const all = selectedAllPicked();
    await store.setFlag(ids(), all ? null : "pick");
  }

  /**
   * 走一次标记动作，并把「该说的话」收下来。
   *
   * 所有入口（星/色/喜欢/锁）都经过它 —— 否则总有一条路径忘了提示，
   * 而「被锁挡住」恰恰是最需要说话的那种（照片上一个像素都不会变）。
   */
  async function runMark(action: Parameters<typeof store.mark>[0]): Promise<void> {
    const result = await store.mark(action);
    const warning = notice(result);
    if (warning !== null) {
      // 边界情况（被锁挡住 / 一个都没改）：必须说话，否则用户以为点错了
      props.toast?.show({
        tone: warning.key === "browse.markSkippedLocked" ? "danger" : "info",
        message: t(warning.key).replace("{n}", String(warning.count)),
      });
      return;
    }
    if (result === null) return;
    // 成功：给一条带「撤销」的提示 —— 人对误操作的第一反应就是找撤销（画布上就这么画的）
    if (result.changed > 0) {
      props.toast?.show({
        tone: "success",
        message: t("browse.markedCount").replace("{n}", String(result.changed)),
        action: result.canUndo
          ? { label: t("browse.undo"), onAction: () => void store.undo() }
          : undefined,
      });
    }
  }

  /**
   * 控件禁用与否。
   *
   * 标记态：**没有选中照片就不能打标**（点了也不知道往哪张打）。
   * 筛选态：**永远可用** —— 筛选不需要先选照片（人类 2026-09-19：
   * 「一旦开了筛选……顶部的那些图标就变成了设置筛选条件」）。
   */
  const markDisabled = () => (filterMode() ? false : !hasSelection());

  async function markRating(star: number): Promise<void> {
    if (filterMode()) {
      /*
       * 筛选模式：星标是**阈值**（人类 2026-09-19：「选 3 星，那么 4 星、5 星的也能出现」）。
       * 再点同一个值 = 取消这个条件（与赞/踩、旗标同一套手感）。
       */
      store.patchFilter({ minRating: filterRating() === star ? null : star });
      return;
    }
    const values = selected().map((item) => item.rating);
    await runMark({ kind: "rating", value: nextRating(values, star) });
  }

  async function markColor(color: string | null): Promise<void> {
    if (filterMode()) {
      // 组内是「或」：绿 + 蓝都能加；再点同一个 = 取消那一个
      const key = color ?? "none";
      const current = filterColors();
      const next = current.includes(key)
        ? current.filter((c) => c !== key)
        : [...current, key];
      store.patchFilter({ colors: next });
      return;
    }
    await runMark({ kind: "color", value: color });
  }

  async function markLike(value: "like" | "dislike"): Promise<void> {
    if (filterMode()) {
      /*
       * 赞 / 踩**互斥**（人类 2026-09-19）：点另一个会把前一个换掉；
       * 再点同一个 = 取消这个条件（界面上的按钮随之弹起）。
       */
      store.patchFilter({ likes: filterLike() === value ? [] : [value] });
      return;
    }
    /*
     * 赞 / 踩互斥，重复点同一个 = **取消**（人类 2026-09-20 报「取消不了、提示没有需要改动的照片」）。
     *
     * 与锁那条同一条口径：已经全是这个值就发 `null`（后端把它当「清掉」）。
     * 以前这里无条件是 `value`，于是「再点一次」送回同一个值 → 后端判定无改动 →
     * 弹「没有需要改动的照片」，用户就永远取消不掉。
     */
    const target = isExactly(likeState(), value) ? null : value;
    await runMark({ kind: "like", value: target });
  }

  async function markLock(level: number): Promise<void> {
    if (filterMode()) {
      const current = filterLocks();
      const next = current.includes(level) ? current.filter((l) => l !== level) : [level];
      store.patchFilter({ locks: next });
      return;
    }
    // 再点一次已锁的级别 = 解锁（否则锁上就撤不掉）
    const current = lockState();
    const target =
      current.kind === "value" && current.value === level ? LOCK_LEVELS.none : level;
    await runMark({ kind: "lock", value: target });
  }

  return (
    <div class="flex items-center gap-1">
      {/*
        撤销 / 重做（`plans/M2-W2-tail.md` 4.1）：按钮的可用性与文案都来自
        后端每次动作回的 `undoLabel` / `redoLabel`（「标 3 星」这种可读动作名）——
        前端不猜栈里有什么，也不自己拼动作名。
      */}
      <Button
        variant="ghost"
        icon={<IconArrowBackUp size={14} />}
        disabled={!store.undoState().canUndo}
        title={undoTitle()}
        aria-label={undoTitle()}
        onClick={() => void runHistory("undo")}
      >
        {t("browse.undo")}
      </Button>
      <Button
        variant="ghost"
        icon={<IconArrowForwardUp size={14} />}
        disabled={!store.undoState().canRedo}
        title={redoTitle()}
        aria-label={redoTitle()}
        onClick={() => void runHistory("redo")}
      >
        {t("browse.redo")}
      </Button>

      <span class="w-3" />

      {/* 筛选开关：打开后右侧控件全部变成筛选语义 */}
      <ToggleBlock
        pressed={filterMode()}
        onPressedChange={(pressed) => {
          /*
           * 打开筛选的那一刻**从选中照片取同类**（`BROWSE.md` §3.1 的「爽用法」）：
           * 选一张 3 星红标图 → 打开 → 所有 3 星或红标图留下。
           * 没有选中就不预置条件（空条件 + 一句提示，见 `FilterBar`）。
           */
          if (pressed) {
            const items = selected();
            if (items.length > 0) {
              store.patchFilter(filterFromSelection(items));
            }
          }
          // 关掉筛选的清理在 store.setFilterMode 里做（四组条件一起清）
          store.setFilterMode(pressed);
        }}
        /* 漏斗 = 筛选的通用符号；`IconBan` 在本项目里是「移除/排除」的意思（AGENTS.md §11.3），不能混用 */
        icon={<IconFilter size={16} />}
        label={t("browse.filterHint")}
      >
        {t("browse.filter")}
      </ToggleBlock>

      <span class="w-5" />

      {/*
        旗标（人类 2026-09-19 重定）：
        * **不是「留下 / 丢弃」，就是开关一个旗标**（旗标只活在内存、可跨目录，`BROWSE.md` §3.2）；
        * 标记态：**一个按钮开/关**（图标是**实心旗**）+ 右边一个「清空旗标」；
        * 筛选态：**两个按钮**「有旗标」（实心旗）/「无旗标」（空心旗），再点一次取消条件。

        为什么标记态只留一个按钮：旗标是「这张我要留着处理」的标记，
        「不想要」由删除表达（有回收站兜底），再来一个「弃」按钮只会让工具条更挤、
        而且两张旗子长得很像、点错也看不出来。
      */}
      <Show
        when={filterMode()}
        fallback={
          <>
            <ToggleBlock
              pressed={selectedAllPicked()}
              disabled={markDisabled()}
              onPressedChange={() => void toggleFlag()}
              icon={<IconFlagFilled size={14} />}
              label={t("browse.flagToggle")}
            >
              {t("browse.flagToggle")}
            </ToggleBlock>
            <Button
              variant="ghost"
              disabled={store.picks().size + store.rejects().size === 0}
              icon={<IconFlagOff size={14} />}
              onClick={(event) => {
                clearFlags.request(
                  t("browse.flagClearConfirm"),
                  () => {
                    void store.clearFlags();
                    // 清旗标是全局动作，做完了说一句（它是「移除类」，用户需要确认真的发生了）
                    props.toast?.show({ tone: "success", message: t("browse.flagCleared") });
                  },
                  event,
                );
              }}
            >
              {t("browse.flagClear")}
            </Button>
          </>
        }
      >
        <ToggleBlock
          pressed={filterFlag() === "pick"}
          onPressedChange={() =>
            store.patchFilter({ flag: filterFlag() === "pick" ? null : { mode: "pick" } })
          }
          icon={<IconFlagFilled size={14} />}
          label={t("browse.flagWith")}
        >
          {t("browse.flagWith")}
        </ToggleBlock>
        <ToggleBlock
          pressed={filterFlag() === "none"}
          onPressedChange={() =>
            store.patchFilter({ flag: filterFlag() === "none" ? null : { mode: "none" } })
          }
          icon={<IconFlag size={14} />}
          label={t("browse.flagWithout")}
        >
          {t("browse.flagWithout")}
        </ToggleBlock>
      </Show>

      <span class="w-5" />

      {/* 星级：五颗；混合态用「短横 + 半亮」区分 */}
      <div class="flex items-center gap-0.5">
        <Show when={ratingState().kind === "mixed"}>
          <MixedMark />
        </Show>
        <For each={[1, 2, 3, 4, 5]}>
          {(star) => {
            const state = () => ratingState();
            const iconClass = () => {
              const s = state();
              if (s.kind === "mixed") return "text-fg-2 opacity-70";
              return starFilled(star) ? "text-brand" : "text-fg-3";
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
                  {starFilled(star) ? <IconStarFilled size={14} /> : <IconStar size={14} />}
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
                aria-label={colorText(color)}
                onClick={() => void markColor(color)}
                class={[
                  "h-3.5 w-3.5 rounded-full",
                  /*
                   * 有颜色的圆点**只要颜色**，不加描边（人类 2026-09-19：降低框线感）；
                   * 但「取消标色」那个空圆必须留一圈 —— 否则它在面色上根本看不见。
                   */
                  color === null
                    ? "ring-1 ring-line-2 bg-transparent"
                    : (isColorLabel(color) ? COLOR_DOT_CLASS[color] : ""),
                  /*
                   * 选中态 = **两层环**（人类 2026-09-19）：
                   *   * 外圈：固定主色（`ring-2 ring-brand`，与全局「点击后=主色底」一脉）；
                   *   * 内圈：**所在面的颜色**（`inset-ring-1`）——
                   *     让它把主色与色标本色隔开一圈，形成「主色环扣着一颗色标」的效果。
                   * 代价是加内圈后色标看起来小了一点，这是**预期的**（内圈占了本色）。
                   */
                  active() || filtered()
                    ? "ring-2 ring-brand inset-ring-1 inset-ring-(--label-ring-inset)"
                    : "",
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
          pressed={likePressed("like")}
          disabled={markDisabled()}
          onPressedChange={() => void markLike("like")}
          icon={<IconThumbUpFilled size={16} />}
          label={t("browse.like")}
        />
        <ToggleBlock
          pressed={likePressed("dislike")}
          disabled={markDisabled()}
          onPressedChange={() => void markLike("dislike")}
          icon={<IconThumbDownFilled size={16} />}
          label={t("browse.dislike")}
        />
      </div>

      <span class="w-5" />

      {/* 标签：开弹窗（`TagDialog`，单张可增删、批量只加） */}
      <Button
        variant="ghost"
        disabled={markDisabled()}
        icon={<IconTag size={14} />}
        onClick={props.onOpenTags}
      >
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
                onPressedChange={() => void markLock(level)}
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

      {/* 「清空旗标」的确认弹窗（Shift 可跳过，提示语在弹窗里） */}
      <ConfirmDialog
        open={clearFlags.pending() !== null}
        title={clearFlags.pending()?.title}
        message={clearFlags.pending()?.message ?? ""}
        onConfirm={clearFlags.confirm}
        onCancel={clearFlags.cancel}
      />
    </div>
  );
}
