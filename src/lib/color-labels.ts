/**
 * 色标（color label）的**唯一取值表与唯一类名映射**（`DESIGN.md` §1.4）。
 *
 * 为什么要收成一处：这份映射原先在**三处**各写了一遍
 * （`components/ui/Tile.tsx` 的 `LABEL_DOT`、`features/browse/BrowseToolbar.tsx` 的 `COLOR_DOT`、
 * 以及 `features/browse/ViewerStatusBar.tsx` 又一份 `COLOR_DOT`），
 * 值表还散在 `lib/marking-state.ts`（顺序）、`BrowseGrid.tsx`（合法性集合）、
 * `features/browse/labels.ts`（文案 switch）里。加一个「青」要在六个地方改 ——
 * 这正是 `AGENTS.md` §2 #12「同一个能力只允许有一套实现」要拦的那种事。
 *
 * 类名必须**字面写出**（Tailwind 扫源码找类名，拼出来的字符串它看不见）——
 * 所以这里是显式表，不是 `bg-(--label-${v})` 那种模板写法。
 */

/** 六个色标（顺序 = 面板上的顺序；无色是那个空心圈，不在这个表里）。 */
export const COLOR_LABELS = [
  "red",
  "yellow",
  "green",
  "cyan",
  "blue",
  "purple",
] as const;

/** 一个色标。 */
export type ColorLabel = (typeof COLOR_LABELS)[number];

/** 面板顺序：六色 + 「无色」（`null` = 那个空心圈）。 */
export const COLOR_VALUES: readonly (ColorLabel | null)[] = [
  ...COLOR_LABELS,
  null,
];

/** 判断某个来自数据库的字符串是不是已知色标。 */
export function isColorLabel(value: string | null | undefined): value is ColorLabel {
  return typeof value === "string" && (COLOR_LABELS as readonly string[]).includes(value);
}

/** 色标 → 实色圆点（工具条、tile 底栏、看图状态栏共用）。 */
export const COLOR_DOT_CLASS: Record<ColorLabel, string> = {
  red: "bg-(--label-red)",
  yellow: "bg-(--label-yellow)",
  green: "bg-(--label-green)",
  cyan: "bg-(--label-cyan)",
  blue: "bg-(--label-blue)",
  purple: "bg-(--label-purple)",
};

/**
 * 色标 → 低浓底纹（tile 的外框底色）。
 *
 * 用的是 `tokens.css` 里**现算**的那几个令牌（`color-mix(in oklab, …)`，
 * 浓度按主题分别给）——不是写死的一组「变淡色」，所以改色标色时底纹自动跟着变。
 */
export const COLOR_TINT_CLASS: Record<ColorLabel, string> = {
  red: "bg-(--label-red-tint)",
  yellow: "bg-(--label-yellow-tint)",
  green: "bg-(--label-green-tint)",
  cyan: "bg-(--label-cyan-tint)",
  blue: "bg-(--label-blue-tint)",
  purple: "bg-(--label-purple-tint)",
};
