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
 * │ [编辑/issue]  RAW │  ← 角落覆盖层：锚在**外框**四角，不跟照片走
 * │ ▒ 文件名      ORF ▒ │  ← 底部信息条：**覆盖在照片上**，默认隐藏
 * └────────────────────┘     指向/聚焦/选中时出现；选中时常亮
 * ```
 *
 * ## 角落覆盖层（2026-09-22 收口）
 *
 * 角标（`RAW` / `+RAW`，以后还有编辑数 / issue 数）一律挂在 **外框**的角上，
 * **不跟照片走**（`[data-tile-corners]` 那一层）。原因很具体：照片在正方外框里是
 * 保比例居中的，极端比例（1:3 的长条）下它只占外框中间的一条 —— 角标如果挂在照片上，
 * 同一行里比例不同的两张就会一个贴外边、一个缩在中间（人类 2026-09-22 报的那条）。
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
 * 带 `*` 的都在**角落覆盖层**（外框四角）里，见上面的「角落覆盖层」。
 *
 * | 位置 | 将来放什么 |
 * | --- | --- |
 * | 外框右上角 * | 动作槽（排除等，指向/聚焦时出现）—— `actions` |
 * | 外框右下角 * | `RAW` / `+RAW`（未指向、未选中时浮出）—— `raw` |
 * | 外框左下角 * | **编辑数 / issue 数**（M3 编辑里程碑）—— 已留好空位（`data-tile-corner="issue"`），直接往里放 |
 * | 顶部条（**库内**） | 星标 / 颜色 / 旗标 / 赞踩 —— `rating` / `colorLabel` / `flag` / `like` |
 * | 底部条右端 | 加锁标记 `locked`；再往后还有别的属性也往这放 |
 */

import {
  IconBan,
  IconLock,
} from "@tabler/icons-solidjs";
import { Show, splitProps, type JSX } from "solid-js";
import { t } from "../../i18n/index.ts";
import { PhotoMarks } from "./PhotoMarks.tsx";
import {
  DEFAULT_DISPLAY_ASPECT,
  MAX_DISPLAY_ASPECT,
} from "../../lib/tile-flow.ts";
import type { TileInfoMode } from "../../lib/display-prefs.ts";

/** 颜色标记（库内才有）—— 取值与类名映射的唯一来源是 `lib/color-labels.ts` */
export type TileColorLabel = ColorLabel;

/**
 * 「信息」档位：见 `TileProps.info`。
 *
 * 值表与类型住在 `lib/display-prefs.ts`（**与持久化同一份** —— 这里再定义一遍就是两份）
 * 并从这里继续对外导出，免得所有调用方都改 import。
 */
export type { TileInfoMode } from "../../lib/display-prefs.ts";

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
  /**
   * 「信息」档位（人类 2026-09-19 引入、**2026-09-20 改口径**；由 tiles 状态栏的 `i`
   * 开关 / `i` 键控制）：
   *
   *   - `off`（默认）：信息条只在指向/选中时出现，带半透明底纹（既有行为）；
   *   - `marks`：**未指向、未选中**时强制显示顶部标记（星标/色标/旗标）—— **无底纹**、
   *     文字加反色勾边；一旦指向/选中就回到标准方案（半透底 + `fg-1`、**去掉勾边**）；
   *   - `marks-name`：同上，但连**下面的文件名**也一起强制显示。
   *
   * 这条「没有指向/选中时才用加强显示、指向/选中后回到原方案」与照片右下角
   * `RAW` / `+RAW` 角标的显示逻辑是**同一个概念**（见 `Tile.raw`）。
   */
  info?: TileInfoMode;
  /**
   * 右下角 RAW 角标的**显示模式**（`undefined` = 不显示）：
   *
   *   * `"raw"` —— 这张就是 RAW；
   *   * `"plus"` —— **位图 + RAW** 的复合：展示的是 SOOC 位图，但同一张照片还有可编辑的 RAW
   *     （人类 2026-09-19：用 `+RAW` 与纯 RAW 区分）。
   *
   * 三种 tile 形态（位图 / RAW / 位图+RAW）靠**同一个角标、同一套显示规则**表达，
   * 只是文字不同 —— 不要在文件名那条上另造一个标记。
   *
   * 显示规则（人类 2026-09-17 定的，别改）：未指向、未选中时浮出；鼠标指向时淡出、
   * 选中时直接不渲染（那两种状态下本来就有信息条与选中底色，再挂角标是噪声）。
   */
  raw?: "raw" | "plus";
  /** 星标 0..5（0 = 什么都不显示） */
  rating?: number;
  flag?: "pick" | "reject" | null;
  /**
   * 赞 / 踩（人类 2026-09-20：**这两个标记也要在图片顶部显示**）。
   *
   * `null` = 没标；与工具条那条一样是**三态互斥**（喜欢 / 不喜欢 / 无）。
   */
  like?: "like" | "dislike" | null;
  locked?: boolean;
  colorLabel?: TileColorLabel | null;
  /** 窄格子（小尺寸档）：星标退化成「一颗星 + 数字」，避免挤成一团 */
  compact?: boolean;
  onClick?: JSX.EventHandlerUnion<HTMLDivElement, MouseEvent>;
  onActivate?: () => void;
}

/*
 * 色标的两张类名表（低浓底纹 / 实色圆点）**不在这里**：
 * 它们是 `lib/color-labels.ts` 的唯一一份，工具条与看图状态栏用的是同一张。
 * 以前这里各写一份，加一个色要改三处（人类 2026-09-19 统一时收敛掉）。
 */
import { COLOR_TINT_CLASS as LABEL_TINT, type ColorLabel } from "../../lib/color-labels.ts";

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
    "info",
    "raw",
    "rating",
    "flag",
    "like",
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

  /** 用户在状态栏选的档位 */
  const infoMode = (): TileInfoMode => local.info ?? "off";
  /**
   * **强制显示层**要不要上（人类 2026-09-20 定的口径）：
   * 开了信息档位、且**未选中** —— 此时在没有底纹的前提下常显（文字加反色勾边）。
   *
   * 「指向/聚焦」不必在这里判：那交给 CSS（`group-hover` / `group-focus-within` 淡出本层、
   * 淡入标准层），与 `RAW` 角标同一套做法。
   * 「选中」是持续状态，那时标准层本来就常亮，所以强制层**根本不渲染**（不是 opacity-0）。
   */
  const forceTopBar = (): boolean => infoMode() !== "off" && !local.selected;
  /** 底部条只在 `marks-name` 这一档才强制显示（`marks` 档只管标记） */
  const forceNameBar = (): boolean => infoMode() === "marks-name" && !local.selected;

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
              它居中在**照片**上（照片在外框里居中，两者中心重合）。
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
          </div>
        </Show>

        {/* 空态：内容为空（不是没加载出来），给一个「—」 */}
        <Show when={local.empty && !local.loading}>
          <span class="absolute text-fs-1 text-fg-3">—</span>
        </Show>
      </div>

      {/*
        ── 角落覆盖层：锚在**外框**四角，不跟照片走 ─────────────────

        为什么必须锚外框（人类 2026-09-22）：照片是保比例居中的，极端比例（比如 1:3
        的长条）只占外框中间一条 —— 角标挂在照片上就会随照片跑，同一行里比例不同的两张
        一个贴外边、一个缩在中间；而以后左下角还要放「编辑 / issue 数」，
        几个角各自贴不同的面就彻底收不住了。

        坐标：这一层 = 外框内缩一个 `--tile-pad`（照片贴满时就是它的边缘），
        所以角标到外框的距离只由 `--tile-pad` 与本层的 `-1` 决定，与照片比例无关。

        分层：在照片之后、信息条之前 —— 信息条（半透底纹那条）永远盖在角标上；
        角标本来就只在「未指向、未选中」时显示，那时信息条也不在。

        已占用的角：右下 = `RAW` / `+RAW`；右上 = 动作槽（暂时没人用）；
        左下 = **留给 M3 的编辑 / issue 数**（直接进这一层，不要再另算距离）。
      */}
      <div data-tile-corners class="pointer-events-none absolute inset-(--tile-pad)">
        <Show when={local.raw}>
          {/*
            RAW 角标：**未指向、未选中**时才在外框右下角浮出。

            实现要点：
            * 圆角用 `--tile-radius` —— 与照片圆角、`--tile-pad` 同一套令牌，
              贴在同一块面上才不突兀（用外面那种大圆角会看着像浮在另一个层上）；
            * `pointer-events-none`：它不是按钮，不睿鼠标事件（否则点到它就算点到照片了）。

            显示规则（人类 2026-09-17 定、2026-09-22 补全）：**与信息条二选一** ——
            只要信息条会出现（选中 / 指向 / 键盘聚焦 / `marks-name` 档强制显示文件名条），
            它就退场。前两个是持续状态，直接**不渲染**；指向 / 聚焦用**淡出**
            （与信息条的淡入同一拍，硬切会有一下呼哧感）。
          */}
          <span
            data-tile-badge="raw"
            class={[
              "pointer-events-none absolute end-1 bottom-1 rounded-(--tile-radius) bg-brand px-1",
              "font-600 text-fs-0 text-fg-on-brand transition-opacity",
              local.selected || forceNameBar()
                ? "hidden"
                : "group-hover/tile:opacity-0 group-focus-within/tile:opacity-0",
            ].join(" ")}
          >
            {local.raw === "plus" ? "+RAW" : "RAW"}
          </span>
        </Show>

        {/*
          左下角：**编辑数 / issue 数的预留位**（人类 2026-09-22 让先留好）。

          坐标与右下角的 `RAW` 角标**镜像对应**（`start-1 bottom-1` vs `end-1 bottom-1`），
          两者在同一条水平线上。M3 把正式图标塞进来即可 —— 不要挪到照片那一层，
          也不要另算距离；这里空着的时候是 0 尺寸，不占地方也不遮照片。
        */}
        <div data-tile-corner="issue" class="absolute start-1 bottom-1 flex items-center gap-1" />

        {/* 动作槽：外框右上角，指向 / 键盘聚焦时出现 */}
        <Show when={local.actions}>
          <div class="pointer-events-auto absolute end-1 top-1 hidden group-hover/tile:flex group-focus-within/tile:flex">
            {local.actions}
          </div>
        </Show>
      </div>

      {/* ── 顶部信息条（库内）：星标 / 颜色 / 旗标 ─────────────── */}
      <Show when={inLibrary()}>
        {/*
          ① 强制显示层（开信息档位、未选中）：无底纹 + 反色勾边。
             指向/聚焦时本层淡出（`group-hover` / `group-focus-within`），把位置让给下面那层。
        */}
        <Show when={forceTopBar()}>
          <div
            data-tile-bar="marks-forced"
            class="pointer-events-none absolute inset-x-0 top-0 flex items-center gap-1 px-(--tile-pad) tile-info-text transition-opacity group-hover/tile:opacity-0 group-focus-within/tile:opacity-0"
            style={{ height: "var(--tile-bar-h)" }}
            aria-hidden="true"
          >
            <PhotoMarks
              rating={rating()}
              compact={local.compact === true}
              colorLabel={local.colorLabel ?? null}
              flag={local.flag ?? null}
              like={local.like ?? null}
              outlined
              spread
            />
          </div>
        </Show>

        {/*
          ② 标准层：指向 / 聚焦 / 选中时出现 —— 半透底纹 + `fg-1`，**不要勾边**
             （有底纹就不需要描边了，人类 2026-09-20）。它就是「原来那套方案」。
        */}
        <div
          data-tile-bar="marks"
          class={[
            "pointer-events-none absolute inset-x-0 top-0 flex items-center gap-1",
            "bg-(--tile-bar-scrim) px-(--tile-pad) text-fg-1 transition-opacity",
            "opacity-0 group-hover/tile:opacity-100 group-focus-within/tile:opacity-100",
            local.selected ? "opacity-100" : "",
          ]
            .filter(Boolean)
            .join(" ")}
          style={{ height: "var(--tile-bar-h)" }}
        >
          <PhotoMarks
            rating={rating()}
            compact={local.compact === true}
            colorLabel={local.colorLabel ?? null}
            flag={local.flag ?? null}
            like={local.like ?? null}
            spread
          />
        </div>
      </Show>

      {/* ── 底部信息条：文件名 + 类型（+ 加锁）───────────────── */}
      {/* ① 强制显示层：只有 `marks-name` 档、且未选中时才上（无底纹 + 勾边） */}
      <Show when={forceNameBar()}>
        <div
          data-tile-bar="name-forced"
          class="pointer-events-none absolute inset-x-0 bottom-0 flex items-center gap-1 px-(--tile-pad) tile-info-text transition-opacity group-hover/tile:opacity-0 group-focus-within/tile:opacity-0"
          style={{ height: "var(--tile-bar-h)" }}
          aria-hidden="true"
        >
          <TileName label={local.label} name={displayName()} tag={local.tag} locked={local.locked} />
        </div>
      </Show>

      {/* ② 标准层：指向 / 聚焦 / 选中 */}
      <div
        data-tile-bar="name"
        class={[
          "pointer-events-none absolute inset-x-0 bottom-0 flex items-center gap-1",
          "bg-(--tile-bar-scrim) px-(--tile-pad) text-fg-1 transition-opacity",
          "opacity-0 group-hover/tile:opacity-100 group-focus-within/tile:opacity-100",
          local.selected ? "opacity-100" : "",
        ]
          .filter(Boolean)
          .join(" ")}
        style={{ height: "var(--tile-bar-h)" }}
      >
        <TileName label={local.label} name={displayName()} tag={local.tag} locked={local.locked} />
      </div>
    </div>
  );
}

/** 底部条的**内容**：文件名 + 类型（+ 加锁）—— 同样一份实现、两处渲染 */
function TileName(props: {
  label: string;
  name: string;
  tag?: string;
  locked?: boolean;
}): JSX.Element {
  return (
    <>
      <span class="min-w-0 flex-1 truncate text-fs-1" title={props.label}>
        {props.name}
      </span>
      <Show when={props.tag}>
        <span class="shrink-0 text-fs-0">{props.tag}</span>
      </Show>
      <Show when={props.locked}>
        <IconLock size={11} class="shrink-0" aria-label={t("grid.locked")} />
      </Show>
    </>
  );
}
