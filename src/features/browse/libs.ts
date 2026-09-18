/**
 * 库列表的**紧缩 / 展开**规则（`BROWSE.md` §4.2，人类 2026-09-18 逐条重述）。
 *
 * 为什么单独抽成纯函数：这一段全是「数几个、露几成、什么时候收」的判定，
 * 放在组件里既难测也容易被改回去 —— 而它恰恰是上一版实现做错的地方
 * （≤3 个库时也显示「查看所有库」、展开时把目录树整个藏掉）。
 *
 * 规则：
 *
 * | 库数 | 缩起态 | 能否展开 |
 * | --- | --- | --- |
 * | ≤ 3 | 全部显示，**高度只够内容**，不显示「查看所有库」 | **不能** |
 * | > 3 | 前 3 张 + 第 4 位一张**伪卡片**（`查看所有库`），容器高度 = 3.5 张卡片 | 能 |
 *
 * 展开态：显示全部、可滚动；下面的目录树**缩到最小值**（不是藏掉）。
 */

/** 缩起态最多显示几张真卡片（第 4 位永远是伪卡片）。 */
export const COMPACT_REPO_LIMIT = 3;

/** 展开态下目录树保留的最小高度（px）：够看到三四行，不至于「一点都不能点了」。 */
export const TREE_MIN_HEIGHT_PX = 88;

/** 展开态的自动收起时限（`BROWSE.md` §4.2 的三条件之一）。 */
export const EXPANDED_IDLE_MS = 15_000;

export interface CompactList<T> {
  /** 这一屏要渲染的真卡片。 */
  visible: readonly T[];
  /** 是否渲染第 4 位的伪卡片（`查看所有库`）。 */
  showAll: boolean;
  /**
   * 容器是否要**钉死成 3.5 张卡片的高度**（多出来的部分被 `overflow:hidden` 切掉）。
   * `false` = 高度随内容（≤3 个库时不该留一片空白等第 4 张）。
   */
  clipped: boolean;
}

/**
 * 算这一屏显示什么。
 *
 * `expanded` 为真时全部显示（列表自己滚）；否则按上面那张表来。
 * 空输入也给「不高、不裁」的结果 —— 空态自己有话说。
 */
export function compactList<T>(items: readonly T[], expanded: boolean): CompactList<T> {
  if (expanded) {
    return { visible: items, showAll: false, clipped: false };
  }
  if (items.length <= COMPACT_REPO_LIMIT) {
    return { visible: items, showAll: false, clipped: false };
  }
  return {
    visible: items.slice(0, COMPACT_REPO_LIMIT),
    showAll: true,
    clipped: true,
  };
}

/**
 * 容器高度（CSS 值）。
 *
 * 钉死高度用 `calc` 而不是写死像素：卡片高度是密度令牌（`--card-h`），
 * 紧凑 52 / 宽松 60 —— 写死就会在另一档上露出「半 + 半」的怪相。
 */
export function listHeightStyle(clipped: boolean): string | undefined {
  if (!clipped) return undefined;
  // 3.5 张卡片 + 3 段 8px 间距（卡片之间的 `gap`，与画布上的 `gap: 8` 对齐）
  return "calc(var(--card-h) * 3.5 + 24px)";
}
