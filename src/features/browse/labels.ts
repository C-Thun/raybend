/**
 * 标记类取值 → 界面文案（`DESIGN.md` §11.1：界面文字只出现在语言包）。
 *
 * 为什么单独一个文件：同一句文案在**三处**要用 —— 工具条的 `aria-label`、
 * 筛选 chips、以及看图状态栏的提示。以前工具条里直接拿原始值当标签
 * （`aria-label={color}` → 读屏读的是英文 "red"），中文界面上是漏的；
 * 收成一处之后，加一个色标只改这里 + 语言包。
 *
 * （2026-09-19：本文件的键曾连续被类型检查器报「不存在」——那是**陈旧快照**误报，
 * 已用「临时文件把 8 个键赋给 `MessageKey[]`、tsc 退出码 0」证伪，见 编译器诊断纪律：以 `tsc` 为准。）
 */

import { t } from "../../i18n/index.ts";

/** 色标（`BROWSE.md` §3.2 的 7 态：红黄绿青蓝紫 + 无） */
export function colorText(color: string | null): string {
  switch (color) {
    case "red":
      return t("browse.colorRed");
    case "yellow":
      return t("browse.colorYellow");
    case "green":
      return t("browse.colorGreen");
    case "cyan":
      return t("browse.colorCyan");
    case "blue":
      return t("browse.colorBlue");
    case "purple":
      return t("browse.colorPurple");
    default:
      return t("browse.colorNone");
  }
}

/** 喜欢（三态：喜欢 / 不喜欢 / 没标） */
export function likeText(like: string | null): string {
  switch (like) {
    case "like":
      return t("browse.like");
    case "dislike":
      return t("browse.dislike");
    default:
      return t("browse.likeNone");
  }
}

/** 锁（0 = 没锁 / 1 = 不可删 / 2 = 不可编辑） */
export function lockText(level: number): string {
  if (level >= 2) return t("browse.lockNoEdit");
  if (level === 1) return t("browse.lockNoDelete");
  return t("browse.lockNone");
}

/** 星级（`{n} 星`；0 就是「0 星」，别写成空） */
export function ratingText(rating: number): string {
  return t("grid.rating").replace("{n}", String(rating));
}

/**
 * 筛选里那两种「空值」的写法：引擎用 `"none"` 表示「无色 / 没标」，
 * 界面上要说成人话（`filter.ts` 里也是这么塞进条件的）。
 */
export function filterValueText(value: string): string {
  return value === "none" ? t("browse.colorNone") : colorText(value);
}
