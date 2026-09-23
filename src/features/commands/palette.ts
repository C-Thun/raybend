/**
 * 命令面板的**行模型**（纯函数，好测；`CommandPalette.tsx` 只负责画）。
 *
 * 抽出来的理由与 `availabilityOf` 同一个：面板的「列什么、什么顺序、什么状态」
 * 是产品口径，不该埋在 JSX 里靠手感验。2026-09-23 人类报的「F11 搜不到」
 * 有两个根因，两个都在这条链上：
 *
 * 1. **以前按 `when` 过滤**（不适用的命令直接消失）——现在**全列**，
 *    能不能用由 `availabilityOf` 说（灰掉 + 说明），永远搜得到；
 * 2. **键位不在搜索索引里**——现在把当前键位与原始写法一起喂给
 *    `rankCommands` 的 `keywords`，搜 `F11` / `Ctrl+K` / `Mod+K` 都能中。
 */

import { rankCommands } from "../../lib/command-match.ts";
import { availabilityOf, type CommandSpec } from "../../lib/commands.ts";

/** 面板里的一行：命令 + 展示字段 + 此刻可用性 */
export interface PaletteRow {
  command: CommandSpec;
  /** 已本地化标题 */
  title: string;
  /** 已本地化分组名 */
  group: string;
  /** 当前有效键位（已格式化）；`null` = 没绑（界面显示 `—`） */
  chord: string | null;
  /** 此刻能不能用（不能用时面板灰掉并说明原因） */
  availability: ReturnType<typeof availabilityOf>;
}

export interface PaletteRowSources {
  commands: readonly CommandSpec[];
  /** 输入框里的查询（空 = 最近使用优先） */
  query: string;
  /** 最近用过的命令 id（`lib/shortcuts.ts`） */
  recent: readonly string[];
  /** 标题本地化（`t(command.titleKey)`；认不出可以退回 key 本身） */
  titleOf: (command: CommandSpec) => string;
  /** 分组本地化（同上） */
  groupOf: (command: CommandSpec) => string;
  /** 当前有效键位（**显示用**，`chordOf`）；没绑返回 `null` */
  chordOf: (command: CommandSpec) => string | null;
  /** 原始键位写法（`Mod+K`；拿不到返回 `null`）——只进搜索词，不显示 */
  rawChordOf: (command: CommandSpec) => string | null;
}

/**
 * PageUp / PageDown 的**步长**：一页能放几行。
 *
 * 纯函数（量到的高度当入参）—— 这样「翻一页到底跳几行」有单测钉住，
 * 而不是靠手感；量不到高度（列表还没挂上 / 高度为 0）时退回 `fallback`
 * （保守值，总比「按了不动」强）。
 *
 * 除不尽时**向下取整**：宁可少跳一行，也不要把下一屏的头一行跳过去。
 */
export function pageStep(args: {
  listHeight: number;
  rowHeight: number;
  fallback?: number;
}): number {
  const fallback = args.fallback ?? 10;
  if (!Number.isFinite(args.listHeight) || !Number.isFinite(args.rowHeight)) return fallback;
  if (args.listHeight <= 0 || args.rowHeight <= 0) return fallback;
  return Math.max(1, Math.floor(args.listHeight / args.rowHeight));
}

/**
 * 造面板行：**全量命令** → 按查询排序 → 带回展示字段与可用性。
 *
 * ⚠️ **不过滤 `when`**（与菜单「暗着但可见」同一条纪律，`DESIGN.md` §12.11）：
 * 过滤的代价是「用户根本不知道有这条命令」；灰掉的代价只是多看一眼。
 */
export function buildPaletteRows(sources: PaletteRowSources): PaletteRow[] {
  const rows: PaletteRow[] = sources.commands.map((command) => ({
    command,
    title: sources.titleOf(command),
    group: sources.groupOf(command),
    chord: sources.chordOf(command),
    availability: availabilityOf(command),
  }));

  const searchable = rows.map((row) => ({
    row,
    id: row.command.id,
    title: row.title,
    group: row.group,
    // 键位两种写法都收：显示的是 `Ctrl+K`，而习惯 `Mod` 写法的人会搜 `Mod+K`
    keywords: [row.chord, sources.rawChordOf(row.command)].filter(
      (word): word is string => word !== null && word !== "",
    ),
  }));

  return rankCommands(sources.query, searchable, sources.recent).map((item) => item.row);
}
