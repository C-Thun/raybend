/**
 * Tile —— 照片网格里的一个格子（`design/main.md` §3.2.1 的排版口径）。
 *
 * ## 结构（2026-09-16 重做，**正方外框 + 保比例居中**）
 *
 * ```text
 * ┌────────────────────┐  ← 外框：正方形圆角矩形，边长 = 尺寸档（9 档）
 * │   ╭──────────────╮ │     默认无底纹；库内打了颜色标记 → 该色低浓底纹
 * │   │              │ │
 * │   │     照片     │ │  ← 照片按自己的宽高比居中，四角圆角，四周留 --tile-pad
 * │   │              │ │     超过 3:1 / 1:3 的由**后端**居中截取（原图不受影响）
 * │   ╰──────────────╯ │
 * │ ▒ 文件名      ORF ▒ │  ← 底部信息条：**覆盖在照片上**，默认隐藏
 * └────────────────────┘     指向/聚焦/选中时出现；选中时常亮
 * ```
 *
 * ## 为什么是正方外框（而不是让格子跟着照片比例走）
 *
 * 「tile 尺寸可调 + 左列宽度可拖」要求**行高恒定** —— 否则拖动时行内成员一变，
 * 界面会扭成迪斯科舞厅（人类 2026-09-16）。所以：**格子定形状，照片在里面保比例**。
 * 附带的好处：元信息（宽高）迟到时**不会重排网格**，照片只是在自己的框里长大/缩小。
 *
 * ## 三条纪律
 *
 * 1. **信息条是覆盖层**：贴在照片之上，照片尺寸**不因选中/指向而改变**；
 *    它锚在**外框**的上下边缘（不是照片边缘），所以同一行的信息条始终在一条线上；
 *    并且由外框的 `overflow: hidden` 裁出圆角。
 * 2. **信息条底纹 = 状态底纹（外框那层）+ 主题中性蒙层**（`--tile-bar-scrim`）：
 *    只用状态色改浓度的话，文字对比度会随状态漂（指向是辅色、选中是主色、
 *    颜色标记又是任意色），迟早出现读不清的组合。中性蒙层定义在 `tokens.css` 的**主题层**
 *    （深色 65% 压暗 / 浅色 72% 提亮，浓度按「最坏情况是纯白（黑）照片」算过对比度）。
 * 3. **信息条上的文字一律 `text-fg-1`**：底是**照片**，压了蒙层也只能保证一档对比度 ——
 *    再用 `fg-2` 分次级色彩，在亮照片上就只有 2.4:1（实测过），层级改由**字号**表达。
 * 3. **加载中不要内框**：外框已经在那儿了，里面只给一个低存在感的占位形状，
 *    不要再来一个圆角矩形套圆角矩形（人类 2026-09-16 批注）。
 *
 * ## 扩展位（现在不显示，位置先留好）
 *
 * | 位置 | 将来放什么 |
 * | --- | --- |
 * | 照片右上角 | 动作槽（排除等，指向/聚焦时出现）—— `actions` |
 * | 顶部条（**库内**） | 星标 / 颜色 / 旗标 —— `rating` / `colorLabel` / `flag` |
 * | 底部条右端 | 加锁标记 `locked`；再往后还有别的属性也往这放 |
 */

import {
  IconBan,
  IconLock,
  IconStar,
  IconStarFilled,
} from "@tabler/icons-solidjs";
import { Show, splitProps, type JSX } from "solid-js";
import { t } from "../../i18n/index.ts";
import {
  DEFAULT_DISPLAY_ASPECT,
  MAX_DISPLAY_ASPECT,
} from "../../lib/tile-flow.ts";

/** 颜色标记（库内才有）—— 名字对应 `tokens.css` 里的 `--label-*` */
export type TileColorLabel = "red" | "yellow" | "green" | "blue" | "purple";

export interface TileProps
  extends Omit<JSX.HTMLAttributes<HTMLDivElement>, "children"> {
  /** 完整文件名（显示时会去掉后缀；`title` 与无障碍名用完整的） */
  label: string;
  /** 文件名行右端的标签（现在放扩展名，如 `ORF`） */
  tag?: string;
  /**
   * 展示用宽高比（**已应用方向、已按 3:1 夹取**；由 `clampDisplayAspect` 算好传进来）。
   * 缺省 = 占位比例（元信息还没到）。
   */
  aspect?: number;
  src?: string | null;
  icon?: JSX.Element;
  selected?: boolean;
  disabled?: boolean;
  loading?: boolean;
  empty?: boolean;
  /**
   * 被**排除**（`AGENTS.md` §11.3：本次导入不带这张，但不动库、不动磁盘）。
   *
   * 表现 = 照片变透明 + 中央一个禁行图标。两条纪律：
   *   1. **不能只靠置灰**（旧实现只把整块调到 40% 不透明度）：一张本身就暗的照片调完看不出区别，
   *      而「这张不要」是个强语义 —— 人得一眼看得出来；
   *   2. **颜色要克制**：加上图标之后意思已经够明确了，再用红/黄这种高饱和色就成了叫卖。
   *      所以用中性的 `fg-2`，并且**压在照片上**（中心）而不是挂在角上。
   */
  excluded?: boolean;
  /** 动作槽（照片右上角，指向/聚焦时出现） */
  actions?: JSX.Element;
  /** 库内才有的信息（导入工作流里这些事都不存在，槽位直接不渲染） */
  context?: "library" | "source";
  /** 星标 0..5（0 = 什么都不显示） */
  rating?: number;
  flag?: "pick" | "reject" | null;
  locked?: boolean;
  colorLabel?: TileColorLabel | null;
  /** 窄格子（小尺寸档）：星标退化成「一颗星 + 数字」，避免挤成一团 */
  compact?: boolean;
  onClick?: JSX.EventHandlerUnion<HTMLDivElement, MouseEvent>;
  onActivate?: () => void;
}

/** 颜色标记 → 外框的低浓底纹（类名必须**字面写出**，Tailwind 才扫得到） */
const LABEL_TINT: Record<TileColorLabel, string> = {
  red: "bg-(--label-red)/15",
  yellow: "bg-(--label-yellow)/15",
  green: "bg-(--label-green)/15",
  blue: "bg-(--label-blue)/15",
  purple: "bg-(--label-purple)/15",
};

/** 颜色标记圆点的实色（悬停/选中时底纹被盖住，用它兜底显示标色） */
const LABEL_DOT: Record<TileColorLabel, string> = {
  red: "bg-(--label-red)",
  yellow: "bg-(--label-yellow)",
  green: "bg-(--label-green)",
  blue: "bg-(--label-blue)",
  purple: "bg-(--label-purple)",
};

export function Tile(props: TileProps) {
  const [local, rest] = splitProps(props, [
    "label",
    "tag",
    "aspect",
    "src",
    "icon",
    "selected",
    "disabled",
    "loading",
    "empty",
    "excluded",
    "actions",
    "context",
    "rating",
    "flag",
    "locked",
    "colorLabel",
    "compact",
    "onClick",
    "onActivate",
    "class",
  ]);

  const interactive = (): boolean => !local.disabled;
  const inLibrary = (): boolean => (local.context ?? "source") === "library";
  const hasImage = (): boolean => Boolean(local.src) && !local.loading;

  /** 展示用的比例：夹到 3:1 之内（真值来自后端；这里再兜一次） */
  const aspect = (): number => {
    const value = local.aspect ?? DEFAULT_DISPLAY_ASPECT;
    if (!Number.isFinite(value) || value <= 0) return DEFAULT_DISPLAY_ASPECT;
    return Math.min(MAX_DISPLAY_ASPECT, Math.max(1 / MAX_DISPLAY_ASPECT, value));
  };

  /** 横图贴满宽、竖图贴满高 —— 两个方向都「短边贴边」，剩下一维居中 */
  const isWide = (): boolean => aspect() >= 1;

  /**
   * 显示用的文件名：**去掉末尾扩展名**（后缀已经在信息条右端做成标签了）。
   * 判据保守：只有「点后 1~5 位字母数字」才当后缀 —— `IMG.2024.raw` 只去最后一段。
   */
  const displayName = (): string => {
    const name = local.label;
    const dot = name.lastIndexOf(".");
    if (dot <= 0 || dot === name.length - 1) return name;
    const tail = name.slice(dot + 1);
    return /^[A-Za-z0-9]{1,5}$/.test(tail) ? name.slice(0, dot) : name;
  };

  const rating = (): number => {
    const value = local.rating ?? 0;
    if (!Number.isFinite(value)) return 0;
    return Math.min(5, Math.max(0, Math.round(value)));
  };

  /** 信息是否常亮（选中时不再依赖悬停） */
  const infoAlwaysOn = (): boolean => Boolean(local.selected);

  return (
    <div
      {...rest}
      role="option"
      aria-selected={Boolean(local.selected)}
      aria-disabled={local.disabled || undefined}
      tabindex={interactive() ? 0 : -1}
      /*
       * **外框自己撑成正方形**（边长 = 尺寸档；画廊里没定义 `--tile-cell` 时用回退值）。
       * 放在组件内部而不是调用方：形状是 tile 自己的事，网格只需要下发边长，
       * 谁渲染它都不会「忘了给尺寸」而导致高矮不一（画廊里就踩过一次）。
       */
      style={{
        width: "var(--tile-cell, 208px)",
        height: "var(--tile-cell, 208px)",
      }}
      class={[
        "group/tile relative flex shrink-0 flex-col overflow-hidden",
        "rounded-(--tile-radius) p-(--tile-pad) transition-colors",
        "outline-none focus-visible:ring-1 focus-visible:ring-focus-ring",
        // 底色优先级：选中（主色）> 库内颜色标记 > 指向（辅色）> 无所谓（透明）
        local.selected
          ? "bg-state-selected"
          : local.colorLabel == null
            ? interactive()
              ? "hover:bg-state-hover"
              : ""
            : LABEL_TINT[local.colorLabel],
        interactive() ? "cursor-pointer" : "cursor-default text-fg-3",
        local.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
      onClick={local.onClick}
      onDblClick={() => local.onActivate?.()}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          local.onActivate?.();
          return;
        }
        if (event.key === " ") {
          // Space 默认会滚动容器，必须挡掉
          event.preventDefault();
          event.currentTarget.click();
        }
      }}
    >
      {/* ── 照片区：保比例、居中 ─────────────────────────────── */}
      <div class="flex min-h-0 min-w-0 flex-1 items-center justify-center">
        <Show
          when={hasImage()}
          fallback={
            /*
             * 加载中 / 空态：**不要内框**（外框已经在了）。
             * 低存在感的形状就够 —— 一张占位图不该抢眼，也不该看起来像「一张照片」。
             */
            <span
              class={[
                "flex items-center justify-center text-fg-3",
                local.loading ? "animate-pulse" : "",
              ].join(" ")}
              aria-hidden="true"
            >
              {local.loading ? (
                <span class="size-4 animate-spin rounded-full border border-current border-t-transparent" />
              ) : (
                (local.icon ?? <span class="size-4 rounded-(--tile-radius) bg-fg-3/20" />)
              )}
            </span>
          }
        >
          <div
            class="relative overflow-hidden rounded-(--tile-radius) bg-surface-main"
            style={{
              "aspect-ratio": String(aspect()),
              width: isWide() ? "100%" : "auto",
              height: isWide() ? "auto" : "100%",
              "max-width": "100%",
              "max-height": "100%",
            }}
          >
            {/*
              `draggable=false`：桌面应用里拖拽图片会把 webview 变成「拖文件」状态，
              与后续要做的 tile 拖选冲突。

              排除态：照片**变透明**（`opacity-35`）—— 不是把整个 tile 调淡，
              这样外框的选中/指向底色还在，两种状态能同时读出来。
            */}
            <img
              src={local.src ?? ""}
              alt={local.label}
              draggable={false}
              class={[
                "size-full object-cover",
                local.excluded ? "opacity-35" : "",
              ]
                .filter(Boolean)
                .join(" ")}
            />

            {/*
              排除的标识：**照片正中央**一个禁行图标（圈 + 斜线，与 `toolsbar` 的批量排除同一套）。
              用 `pointer-events-none`：它不是按钮 —— 排除/恢复都走「先选中、再按批量排除」，
              在这里再挂一个可点图标，会给「轻点一下」赋予两种含义。
            */}
            <Show when={local.excluded}>
              <span class="pointer-events-none absolute inset-0 flex items-center justify-center">
                <IconBan
                  size={28}
                  stroke-width={1.5}
                  class="text-fg-2"
                  aria-label={t("grid.excluded")}
                />
              </span>
            </Show>

            {/* 动作槽：照片右上角，指向 / 键盘聚焦时出现 */}
            <Show when={local.actions}>
              <div class="absolute top-1 right-1 hidden group-hover/tile:flex group-focus-within/tile:flex">
                {local.actions}
              </div>
            </Show>
          </div>
        </Show>

        {/* 空态：内容为空（不是没加载出来），给一个「—」 */}
        <Show when={local.empty && !local.loading}>
          <span class="absolute text-fs-1 text-fg-3">—</span>
        </Show>
      </div>

      {/* ── 顶部信息条（库内）：星标 / 颜色 / 旗标 ─────────────── */}
      <Show when={inLibrary()}>
        <div
          class={[
            "pointer-events-none absolute inset-x-0 top-0 flex items-center gap-1",
            "bg-(--tile-bar-scrim) px-(--tile-pad) text-fg-1 transition-opacity",
            "opacity-0 group-hover/tile:opacity-100 group-focus-within/tile:opacity-100",
            infoAlwaysOn() ? "opacity-100" : "",
          ].join(" ")}
          style={{ height: "var(--tile-bar-h)" }}
        >
          {/* 星标：0 星什么都不显示；窄格子退化成「一颗星 + 数字」 */}
          <Show when={rating() > 0}>
            <span
              class="flex shrink-0 items-center gap-0.5"
              aria-label={t("grid.rating", { n: rating() })}
            >
              <Show
                when={!local.compact}
                fallback={
                  <>
                    <IconStarFilled size={12} aria-hidden="true" />
                    <span class="text-fs-0 tnum">{rating()}</span>
                  </>
                }
              >
                {[1, 2, 3, 4, 5].map((index) =>
                  index <= rating() ? (
                    <IconStarFilled size={11} aria-hidden="true" />
                  ) : (
                    <IconStar size={11} class="opacity-50" aria-hidden="true" />
                  ),
                )}
              </Show>
            </span>
          </Show>

          <span class="min-w-0 flex-1" />

          {/* 颜色标记：悬停/选中时底纹被盖住，用它兜底让人看到标色 */}
          <Show when={local.colorLabel}>
            {(label) => (
              <span
                class={["size-2 shrink-0 rounded-full", LABEL_DOT[label()]].join(" ")}
                aria-label={t("grid.color_label")}
              />
            )}
          </Show>

          <Show when={local.flag === "pick"}>
            <IconStarFilled size={11} aria-hidden="true" />
          </Show>
        </div>
      </Show>

      {/* ── 底部信息条：文件名 + 类型（+ 加锁）───────────────── */}
      <div
        class={[
          "pointer-events-none absolute inset-x-0 bottom-0 flex items-center gap-1",
          "bg-(--tile-bar-scrim) px-(--tile-pad) text-fg-1 transition-opacity",
          "opacity-0 group-hover/tile:opacity-100 group-focus-within/tile:opacity-100",
          infoAlwaysOn() ? "opacity-100" : "",
        ].join(" ")}
        style={{ height: "var(--tile-bar-h)" }}
      >
        <span class="min-w-0 flex-1 truncate text-fs-1" title={local.label}>
          {displayName()}
        </span>
        <Show when={local.tag}>
          <span class="shrink-0 text-fs-0">{local.tag}</span>
        </Show>
        <Show when={local.locked}>
          <IconLock size={11} class="shrink-0" aria-label={t("grid.locked")} />
        </Show>
      </div>
    </div>
  );
}
