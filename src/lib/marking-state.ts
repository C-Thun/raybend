/**
 * 标记的三态语义（`BROWSE.md` §3.2 的「三态控件」）。
 *
 * 这是 `toolsbar` 上每个标记控件要回答的问题：**当前选中的这些照片，这个标记是什么状态？**
 *
 * | 情形 | 控件显示 | 用户改一下会怎样 |
 * | --- | --- | --- |
 * | 没有选中任何照片 | **无值**（全暗） | 不做事（按钮应当禁用） |
 * | 选中的照片取值**全部相同** | 值（例如 3 星点亮 3 颗） | 全部改成新值 |
 * | 取值**不一致** | **混合态**（与无值视觉可区分，见画布） | 全部改成新值（改完就统一了） |
 *
 * ⚠️ 三个状态**必须互相可分**。最容易写错的是把「混合」当成「无值」——
 * 那样用户点一下会把一批已经打了标的照片全部清掉。
 *
 * 纯函数、不碰 DOM、不依赖框架（`ARCHITECTURE.md` §1 的纯逻辑层要求）。
 */

/** 一个标记字段的取值：可能是数字（星级 / 锁）或字符串（色标 / 喜欢），也可能是「无」。 */
export type MarkValue = number | string | null;

/** 三态之一。 */
export type TriState<T extends MarkValue> =
  | { kind: "none" }
  | { kind: "value"; value: T }
  | { kind: "mixed" };

/**
 * 把一批取值收敛成三态。
 *
 * `null` 与 `undefined` 都当「无值」处理（`undefined` 出现在「这一项还没读到」时）。
 */
export function triState<T extends MarkValue>(
  values: readonly (T | undefined)[],
): TriState<T> {
  if (values.length === 0) return { kind: "none" };

  const first = values[0] ?? null;
  for (const value of values) {
    if ((value ?? null) !== first) return { kind: "mixed" };
  }
  if (first === null) return { kind: "none" };
  return { kind: "value", value: first as T };
}

/**
 * 一批照片的取值是否**全部**等于 `value`（用于「再点一次取消」这类判断）。
 *
 * 空集合返回 `false` —— 什么都没选时不该说「已经是这个值了」。
 */
export function allEqual<T extends MarkValue>(
  values: readonly (T | undefined)[],
  value: T,
): boolean {
  if (values.length === 0) return false;
  return values.every((v) => (v ?? null) === value);
}

/**
 * 点一下星级按钮该设成几星（`BROWSE.md` §3.2：「在已选中且为 1 星时再点一次 → 扣成 0 星」）。
 *
 * 规则复述：点第 n 颗星 →
 * * 当前**全部**都是 n 星 → 归零（再点一次就是取消）；
 * * 否则全部设成 n 星。
 */
export function nextRating(current: readonly (number | undefined)[], star: number): number {
  if (allEqual(current, star)) return 0;
  return star;
}

/** 星级显示形态：五颗星，还是「一颗 + 数字」（窄格子）。 */
export interface RatingDisplay {
  /** 显示几颗实心星（0–5）。 */
  filled: number;
  /** 是否退化成「一颗星 + 数字」。 */
  compact: boolean;
}

/**
 * 星级在给定宽度下的显示形态（`design/browse.md` §2.3 的两种模式）。
 *
 * `compact` 为真时退化成「一颗星 + 数字」；0 星**什么都不显示**
 * （画布与实现口径一致：没打星就别占地方）。
 */
export function ratingDisplay(rating: number, compact: boolean): RatingDisplay | null {
  if (rating <= 0) return null;
  const clamped = Math.min(5, Math.max(0, Math.round(rating)));
  return compact ? { filled: 1, compact: true } : { filled: clamped, compact: false };
}

/*
 * 色标的**名字**不在这个文件里：它是界面文案，必须走语言包
 * （W2 接线时用 `t("grid.color.red")` 这类 key，两边各自翻译）。
 * 这里原本有一张中文名表（`COLOR_LABELS`）， 2026-09-17 的文案普查删掉了 ——
 * 它当时无人引用，且是「中文硬编码进界面」的隐患。
 */

/** 色标的可选值（顺序 = 面板上的顺序；`null` = 无色，是那个空心圈）。 */
export const COLOR_VALUES: readonly (string | null)[] = [
  "red",
  "yellow",
  "green",
  "blue",
  "purple",
  null,
];

/** 锁的两级（`BROWSE.md` §3.4）。 */
export const LOCK_LEVELS = {
  none: 0,
  /** 一级：不能删。 */
  noDelete: 1,
  /** 二级：不能编辑（更严）。 */
  noEdit: 2,
} as const;
