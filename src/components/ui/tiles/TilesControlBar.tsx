/**
 * `TilesControlBar` —— **所有 tiles 视图共用的那条底部状态/控制条**。
 *
 * 人类 2026-09-19 的两条要求合起来就是它的存在理由：
 *
 * 1. 「import tiles 和 browse tiles 各自的缩放比在软件中记录可以不一样，**但是组件必须是同一个**……
 *    我们沟通语境下的 tiles，就是包括下面状态条的，因为在业务层面就是一个不可分割的整体」；
 * 2. 逐项给出**统一口径**（下面每条都对应一处实现）：
 *    * 计数**含选中数**（导入侧本来有，浏览侧也补上）；
 *    * 中间 = 「容器（目录 / 库名）」+ **当前那张的文件名**；
 *      那个不知所云的九点图标**去掉**（`IconGridDots` 已不再出现在这里）；
 *    * 高度用**浏览侧那档矮的**（`h-8`，够操作就行），但「按时间」前的**图标保留**
 *      （它是重要功能，不是装饰）；
 *    * 缩放条用**导入侧那档**（两端带加减号图标、`w-40`）；
 *    * 排序**保留为可配置项**：浏览侧传 `sort`，导入侧现在不传
 *      （以后想开就传同一份 props，不用改这个文件）；
 *    * `信息`由工作区注入：browse 是三态，import 是「关 / 文件名」两态，持久化互不串线。
 *
 * 组件只吃**数据 + 回调**，不认识任何 store、不 import 任何 feature ——
 * 所以它住在 `components/ui/`，两个工作区各自把配置灌进来。
 */

import { Show, untrack, type JSX } from "solid-js";
import {
  IconArrowDown,
  IconArrowUp,
  IconClock,
  IconArrowsHorizontal,
  IconInfoCircle,
  IconLock,
  IconZoomIn,
  IconZoomOut,
} from "@tabler/icons-solidjs";
import { PathText } from "../PathText.tsx";
import { PhotoMarks } from "../PhotoMarks.tsx";
import { Slider } from "../Slider.tsx";
import { ToggleBlock } from "../ToggleBlock.tsx";
import { Menu } from "../Menu.tsx";
import { locale as uiLocale, t } from "../../../i18n/index.ts";
import { formatCount, type GroupingLocale } from "../../../lib/format.ts";
import { TILE_SIZE_STEPS } from "../../../lib/tile-flow.ts";
import type { TileInfoMode } from "../../../lib/display-prefs.ts";

/** 排序控件要的全部东西（浏览侧现在是排序键 + 升降两件；以后还有别的键也无所谓） */
export interface TilesSortConfig {
  /** 可选的排序键（顺序即菜单顺序） */
  keys: readonly string[];
  /** 当前键 */
  value: string;
  /** 键 → 显示文案（**文案活在语言包里**，由调用方给翻译后的串） */
  labelOf: (key: string) => string;
  /** 是否降序 */
  desc: boolean;
  onKeyChange: (key: string) => void;
  onDirectionToggle: () => void;
}

/**
 * 看图态的**内容**（人类 2026-09-20 定：看图时中列底部与 tiles 是**同一条**状态栏，
 * 只换内容 —— 文件名 + 锁 + 这一张的标记；不再另开一条跨列的信息条）。
 *
 * 数据来自看图件当前那张照片（`ViewerPhoto`）—— 由工作区映射成这个小对象，
 * 状态栏组件本身不认识 store、也不发请求。
 */
export interface TilesViewingInfo {
  /** 当前那张的文件名 */
  fileName: string | null;
  /** 锁级别（0/1/2）—— **紧挨着文件名右边**显示（`BROWSE.md` §5.8） */
  lockLevel: number;
  rating: number;
  colorLabel: string | null;
  flag: "pick" | "reject" | null;
  like: "like" | "dislike" | null;
}

export interface TilesControlBarProps {
  /** 当前列表里的张数 */
  count: number;
  /** 业务计数/队列标识仍画在同一个状态栏。 */
  countLabel?: string;
  centerContent?: JSX.Element;
  compactControls?: boolean;
  /** 选中的张数（`>0` 才显示「已选 n」——与导入侧原来的口径一致） */
  selectedCount?: number;
  /**
   * 中间那段的左侧：**目录**（用 `PathText` 缩写显示，悬停看全路径）。
   * 与 `label` 二选一：给了 `label` 就用 `label`。
   */
  dir?: string | null;
  /** 中间那段的左侧（**纯文本**）：浏览侧给「库名 / 最后一级目录」 */
  label?: string | null;
  /** 中间那段的右侧：**当前那张**的文件名（没有就不显示） */
  fileName?: string | null;
  /** 是否处于「按时间」模式 */
  byTime: boolean;
  onByTimeChange: (value: boolean) => void;
  /** 当前工作区自己的信息档位与切换动作。 */
  infoMode: TileInfoMode;
  onInfoToggle: () => void;
  /** 连续档位位置（锚点来自 `lib/tile-flow.ts` 的 17 档，允许落在两档之间） */
  tileStep: number;
  onTileStepChange: (step: number) => void;
  /** 拖拽结束（用于落盘，别在拖动过程中写设置） */
  onTileStepCommit?: (step: number) => void;
  /** 把当前横向一排铺满窗口（由 TilesShell 交给网格计算）。 */
  onFitRow?: () => void;
  /**
   * 语言（数字分组用它）。**不给就按当前界面语言自动取** ——
   * 免得每个调用方再抄一遍「locale → GroupingLocale」的映射
   * （之前导入侧与网格里各有一份一模一样的实现）。
   */
  locale?: GroupingLocale;
  /** 正在补读拍摄时间（按时间模式下给个提示） */
  loadingTimes?: boolean;
  /** 排序（可选：浏览侧传；导入侧以后想开就传同一份） */
  sort?: TilesSortConfig;
  class?: string;
}

export interface TilesControlBarComponentProps {
  /**
   * 配置必须以 accessor 传入：父级每次缩放都会产生一份新配置对象，但组件实例不能因此
   * 重建，否则 Ark Slider 会在拖动第一格后丢掉 pointer capture。
   */
  config: () => TilesControlBarProps;
  onFitRow: () => void;
  /**
   * 「横向适合窗口」现在**能不能**做（网格算完回填，见 `fit.ts`）。
   *
   * `false` = 按钮**禁用**（人类 2026-09-23：算出来的格宽超过最大档时按钮无效）——
   * 那时只有「少放几列」才铺得满，而列数不能为了铺满而凭空改（网格会跳一次列），
   * 所以老实禁用比「按了没反应」诚实。不给就当作能做。
   */
  fitAvailable?: boolean;
}

export function TilesControlBar(input: TilesControlBarComponentProps) {
  /*
   * 保留下面清晰的 props.xxx 写法，同时让每一次读取都落到最新配置。
   * 这个代理只服务二十来个状态栏字段，不进列表热路径。
   */
  const props = new Proxy({} as TilesControlBarProps, {
    get: (_target, key) => {
      if (key === "onFitRow") return input.onFitRow;
      return input.config()[key as keyof TilesControlBarProps];
    },
  });
  /** 分组语言：调用方给了就用它的，否则跟界面语言走 */
  const groupLocale = (): GroupingLocale =>
    props.locale ?? (uiLocale() === "en-US" ? "en-US" : "zh-CN");

  /** 中间那段左边显示什么：文本优先，其次目录缩写 */
  const leadText = (): string => props.label ?? "";
  const showDir = (): string | null =>
    props.label === undefined || props.label === null || props.label === ""
      ? (props.dir ?? null)
      : null;
  const hasCenter = (): boolean =>
    props.centerContent !== undefined || showDir() !== null || leadText() !== "" || (props.fileName ?? "") !== "";

  return (
    <BarFrame mode="tiles" class={props.class}>
      {/* 计数：**含选中数**（人类 2026-09-19：导入侧本来就有，浏览侧也要有） */}
      <span class="shrink-0 text-fs-1 text-fg-2 tnum">
        {props.countLabel ?? t("grid.count", { n: formatCount(props.count, groupLocale()) })}
      </span>
      <Show when={(props.selectedCount ?? 0) > 0}>
        <span class="shrink-0 text-fs-1 text-fg-2 tnum">
          {t("browse.selected").replace("{n}", String(props.selectedCount ?? 0))}
        </span>
      </Show>

      <span class="min-w-0 flex-1" />

      {/* 中间：容器 + 当前那张的文件名（真正居中） */}
      {props.centerContent}
      <Show when={props.centerContent === undefined && hasCenter()}>
        <span class="flex min-w-0 max-w-96 shrink-0 items-center gap-1.5 text-fs-1 text-fg-3">
          <Show when={showDir()}>
            {(dir) => <PathText path={dir()} maxLength={40} class="max-w-64 shrink-0" />}
          </Show>
          <Show when={leadText() !== ""}>
            <span class="truncate">{leadText()}</span>
          </Show>
          <Show when={(props.fileName ?? "") !== ""}>
            <span class="truncate text-fg-2">· {props.fileName}</span>
          </Show>
        </span>
      </Show>

      <span class="min-w-0 flex-1" />

      {/* 正在补读时间：给一句轻提示，别让用户以为界面卡住 */}
      <Show when={props.loadingTimes}>
        <span class="shrink-0 text-fs-0 text-fg-3">{t("common.loading")}</span>
      </Show>

      {/*
        排序（可选）：键 + 方向。浏览侧现在开着；导入侧不传就整块不出现 ——
        这就是人类说的「组件每个功能支持可配置，同时每个功能提供给外部接口灌数据调 handle」。
      */}
      <Show when={props.sort}>
        {(sort) => (
          <div class="flex shrink-0 items-center gap-1" data-sort>
            <span class="text-fs-1 text-fg-3">{t("browse.sort")}</span>
            <Menu
              label={t("browse.sort")}
              placement="top"
              items={sort().keys.map((key) => ({
                value: key,
                label: sort().labelOf(key),
                selected: sort().value === key,
              }))}
              onSelect={(value) => sort().onKeyChange(value)}
            >
              {(triggerProps) => (
                <button
                  {...triggerProps()}
                  class="rounded-ui px-1.5 py-0.5 text-fs-1 text-fg-2 hover:bg-state-hover hover:text-fg-1"
                >
                  {sort().labelOf(sort().value)}
                </button>
              )}
            </Menu>
            <button
              type="button"
              aria-label={sort().desc ? t("browse.sortDesc") : t("browse.sortAsc")}
              class="flex h-5 w-5 items-center justify-center rounded-ui text-fg-3 hover:bg-state-hover hover:text-fg-1"
              onClick={() => sort().onDirectionToggle()}
            >
              {sort().desc ? <IconArrowDown size={13} /> : <IconArrowUp size={13} />}
            </button>
          </div>
        )}
      </Show>

      <Show when={!props.compactControls}>
      {/* `信息`档位由调用方控制；import 只会给 off / marks-name，browse 保留三态。 */}
      <button
        type="button"
        data-tile-info={props.infoMode}
        aria-pressed={props.infoMode !== "off"}
        aria-label={t("grid.info")}
        title={t("grid.info")}
        onClick={props.onInfoToggle}
        class={[
          "flex h-6 shrink-0 cursor-pointer items-center gap-1 rounded-ui px-1.5 text-fs-1 transition-colors",
          infoButtonClass(props.infoMode),
        ].join(" ")}
      >
        <IconInfoCircle size={16} aria-hidden="true" />
        <span class="whitespace-nowrap">{t("grid.info")}</span>
      </button>

      {/* 按时间（可按下式）：**图标要保留**（人类：它是重要功能），高度用矮的那档 */}
      <ToggleBlock
        pressed={props.byTime}
        onPressedChange={props.onByTimeChange}
        label={t("grid.by_time")}
        icon={<IconClock size={16} />}
      >
        <span class="whitespace-nowrap">{t("grid.by_time")}</span>
      </ToggleBlock>

      </Show>
      {/* 缩放：17 个预设锚点之间允许连续落点，两端带加减号 */}
      <Slider
        value={props.tileStep}
        min={0}
        max={TILE_SIZE_STEPS.length - 1}
        step={0.01}
        label={t("grid.zoom")}
        onValueChange={props.onTileStepChange}
        {...(props.onTileStepCommit === undefined
          ? {}
          : { onValueCommit: props.onTileStepCommit })}
        startIcon={<IconZoomOut size={14} />}
        endIcon={<IconZoomIn size={14} />}
        class="w-40 shrink-0"
      />
      {/* 横向适合窗口（稳定测试钩子：与 `data-tiles-control-bar` / `data-tile-info` / `data-sort` 同一条规矩） */}
      <button
        type="button"
        data-tiles-fit-row
        data-fit-available={input.fitAvailable === false ? "no" : "yes"}
        disabled={input.fitAvailable === false}
        aria-label={t("grid.fit_row")}
        title={t("grid.fit_row")}
        onClick={input.onFitRow}
        class={[
          "flex size-6 shrink-0 items-center justify-center rounded-ui transition-colors",
          input.fitAvailable === false
            ? "cursor-default text-fg-3 opacity-60"
            : "bg-state-hover text-fg-2 hover:text-fg-1",
        ].join(" ")}
      >
        <IconArrowsHorizontal size={16} aria-hidden="true" />
      </button>
    </BarFrame>
  );
}

/**
 * `信息`按钮的底色：三态各一档（关 / 只标记 / 标记+文件名）。
 * 写成函数而不是嵌套三元 —— 三个档位一眼能对上是哪个。
 */
function infoButtonClass(mode: TileInfoMode): string {
  if (mode === "marks-name") return "bg-state-selected text-fg-1";
  if (mode === "marks") return "bg-state-hover text-fg-1";
  return "text-fg-2 hover:bg-state-hover hover:text-fg-1";
}

/**
 * 那条栏的**外框** —— tiles 与看图**共用**（人类 2026-09-20 定的结构）。
 *
 * 他说得很直：中列底部只有一条状态栏，“就是 tiles 那一条的容器”；
 * 看图只是**换内容**（文件名 + 锁 + 标记），不是另开一条跨列的条。
 * 所以高度 / 面色 / 上边线 / 标记属性都只写在这里一份。
 */
function BarFrame(props: {
  mode: "tiles" | "view";
  class?: string;
  children: JSX.Element;
}): JSX.Element {
  /*
   * children **只能取一次**（untrack），然后当静态节点插进去。
   *
   * ## 为什么（2026-09-23 真机排查，别再删这一行）
   *
   * `{props.children}` 会被 Solid 编译成 `insert(el, () => props.children)` ——
   * 那是一个 **render effect**，不是一次性插入；而 children 的 getter 每求值一次，
   * 里面的 `createComponent(...)` 就重跑一遍。
   *
   * 于是：**「创建 children 期间被读到的任何信号」都成了「重建整棵子树」的开关**。
   * 这条栏上正好有一个：Ark 的 `SliderRoot` 用 `createSplitProps()` **同步**读走
   * `props.value`（= `tileStep`）—— 那次读被记在了上面那个 render effect 头上。
   * 后果就是人类报的「缩放杆拖一格就断」：值一变 → 整条栏重建 →
   * 正在拖的 Ark Slider 实例被换掉 → 拖动与焦点一起丢。
   *
   * ⚠️ **untrack 要写在插入点这一行**（`{untrack(() => props.children)}`），
   * **别**提到组件 body 里去提前求值：Solid 的 `useContext` 按「创建时的 owner 链」查找，
   * 提前造出来的子节点就落在这个组件自己返回的 Provider **之外**了
   *（`TilesShell` 里真踩过：拖动修好了，「横向适合窗口」按钮却按不动了）。
   *
   * 同理可推：**任何包装组件把 children 透传出去时都要就地 untrack 一次**。
   * 诊断办法：`insert`/`insertBefore` 钩子 + `element === 上次那个元素` 的身份比对
   *（见 `scripts/check-browse-boot.mjs` 里那两条拖动 / 自动宽度回归）。
   */
  return (
    <div
      class={[
        "flex h-8 shrink-0 items-center gap-2 border-t border-t-surface-track px-3",
        props.class ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
      data-tiles-control-bar
      data-tiles-bar-mode={props.mode}
    >
      {untrack(() => props.children)}
    </div>
  );
}

/**
 * **看图态的**内容（与 `TilesControlBar` 同一条栏、同一个 `BarFrame`）：
 * 左 = 文件名 + **紧挨着**的锁徽标；右 = 这一张的标记（`BROWSE.md` §5.8）。
 *
 * 人类 2026-09-20 把旧的那条全宽 `ViewerStatusBar` 删了换成它 ——
 * 理由见 `AGENTS.md` §11.1 的结构红线：workspace 只有纵向分列、没有跨列行。
 */
export function PhotoStatusBar(props: { info: TilesViewingInfo; class?: string; compare?: { reference: string; result: string; choices: { value: string; label: string; selected: boolean; disabled?: boolean }[]; onReferenceChange: (value: string) => void } }): JSX.Element {
  const info = (): TilesViewingInfo => props.info;
  return (
    <BarFrame mode="view" class={props.class}>
      <Show when={props.compare}>
        {(compare) => <>
          <Menu label={t("editor.base.label")} placement="top"
            items={compare().choices} onSelect={compare().onReferenceChange}>
            {(triggerProps) => <button {...triggerProps()}
              class="rounded-ui bg-brand px-2 text-fs-1 text-fg-on-brand hover:bg-brand-2"
              data-editor-compare-reference>{compare().reference} ▾</button>}
          </Menu>
          <span class="min-w-0 flex-1" />
          <span class="text-fs-1 text-fg-1" data-editor-compare-result>{compare().result}</span>
        </>}
      </Show>
      <Show when={!props.compare}>
      {/* 左：文件名 + 紧挨着的锁徽标 */}
      <span class="min-w-0 shrink truncate text-fs-1 text-fg-1" title={info().fileName ?? ""}>
        {info().fileName ?? ""}
      </span>
      <Show when={info().lockLevel > 0}>
        <span
          class="flex shrink-0 items-center gap-1 rounded-(--radius) bg-surface-layer px-1.5 text-fs-0 text-fg-2"
          title={info().lockLevel >= 2 ? t("browse.lockNoEdit") : t("browse.lockNoDelete")}
        >
          <IconLock size={11} aria-hidden="true" />
          {info().lockLevel >= 2 ? 2 : 1}
        </span>
      </Show>

      <span class="min-w-0 flex-1" />

      {/* 右：这一张的标记（只显示「有值」的那些 —— 状态栏不是设置控件） */}
      <PhotoMarks
        class="shrink-0"
        rating={info().rating}
        colorLabel={info().colorLabel}
        flag={info().flag}
        like={info().like}
      />
      </Show>
    </BarFrame>
  );
}
