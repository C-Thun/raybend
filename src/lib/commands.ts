/**
 * 命令与快捷键的**纯逻辑**（`plans/M2-W3.md` §2.2/§2.4）。
 *
 * 这一层只认数据：`CommandSpec[]` + 覆盖表 ⇒ 有效键位 + 冲突报告。
 * 不认识 store、不认识 DOM、不 import 任何 feature（`lib/` 的规矩），所以：
 *
 * * **命令面板**、**标题栏菜单**、**快捷键分发器**、**设置界面**四处读的都是这一份判定；
 * * 单测能把「什么算冲突」钉死，而不是靠界面手感。
 *
 * ## 作用域（冲突检测的地基）
 *
 * | 作用域 | 什么时候成立 |
 * | --- | --- |
 * | `global` | 永远（与任何面同时成立） |
 * | `tiles` | 网格面：导入网格与浏览网格（键位与语义本来一样） |
 * | `viewer` | 看图面：单张看图与对比（同一时刻只挂载一个） |
 *
 * 于是「同一条键绑两条命令」分两种：
 * * 作用域**相交**（含任一为 `global`）⇒ **冲突**（真的会互相盖住）；
 * * 作用域不相交（`tiles` vs `viewer`）⇒ **允许**，只是提示一句 ——
 *   现状本来就是这样：`0`–`5` 在网格里打星、在你看图时是缩放与适配。
 */

import { formatChord, isReservedChord, parseChord, type Chord } from "./key-chords.ts";

/** 命令的作用域（见文件头的表） */
export type CommandScope = "global" | "tiles" | "viewer";

/** 命令分组（命令面板的分节、菜单之外的分组） */
export type CommandGroup =
  | "file"
  | "edit"
  | "view"
  | "window"
  | "help"
  | "mark"
  | "navigate"
  | "viewer"
  | "import";

/** 进哪个标题栏菜单（不填 = 只在命令面板里出现） */
export type CommandMenu = "file" | "edit" | "view" | "window" | "help";

export interface CommandSpec {
  /** 稳定 id（**契约**：改文案不改 id；导入导出的文件里存的就是它） */
  id: string;
  /** i18n key（`lib/` 不许 import i18n，所以这里只是字符串；类型收窄在 features 层） */
  titleKey: string;
  group: CommandGroup;
  /** 进哪个菜单；不填就只出现在命令面板 */
  menu?: CommandMenu;
  scope: CommandScope;
  /** 默认键位串（`Mod+K` / `P` / `Delete` …）。不填 = 默认不绑键 */
  defaultKey?: string;
  /**
   * **此刻**适不适用（在导入流？在看图？）。省略 = 永远适用。
   *
   * 与 `scope` 的分工：`scope` 是静态归属（冲突检测用），`when` 是运行时判定
   * （分发器靠它决定接不接这个键、面板靠它决定显不显示）。两者**都要**给：
   * 只给 `scope` 的话，「同一条键在导入网格与浏览网格都该生效」这类事就只能靠猜。
   */
  when?: () => boolean;
  /** 菜单项是否可用（默认 `true`）；命令面板里不显示不可用的命令 */
  enabled?: () => boolean;
  /** 破坏性动作（删除 / 退出）：绑到危险单键上时给警告 */
  dangerous?: boolean;
  /** 做这件事（**由组装层注入**：命令不认识 store） */
  run: () => void | Promise<void>;
}

/** 覆盖表：`null` = 显式解绑（"我就是要这个功能没有键"） */
export type ShortcutOverrides = Readonly<Record<string, string | null>>;

/** 一条命令此刻**不可用**的原因（两个判定各对应一个） */
export type CommandUnavailableReason = "when" | "enabled";

/**
 * 一条命令此刻能不能用 —— **全应用只有这一个判定**。
 *
 * ```text
 *  when 为假   → 这个场景里没有它（例：不在浏览网格里）
 *  enabled 为假 → 场景对了但条件不够（例：没有选中照片）
 * ```
 *
 * 四个消费方读同一份：**分发器**（接不接这个键）、**命令面板**（亮着还是暗着）、
 * **标题栏菜单**（能不能点）、**快捷键设置**（不判定 —— 键位永远可改）。
 *
 * 为什么必须收成一条（2026-09-23 人类报「F11 / 全屏看图在 Ctrl+K 里搜不到」）：
 * 之前四个界面里有三套不同口径 —— 面板按 `when` **过滤**（命令直接消失）、
 * 菜单只看 `enabled`、分发器两个都看。同一件事三种说法，必然有一处说错，
 * 而且「说错」的表现是「命令找不到」——用户根本无法区分是没登记还是被过滤。
 * 现在：**列表永远列全，能不能用由这一条说**。
 */
export function availabilityOf(
  command: CommandSpec,
): { available: true } | { available: false; reason: CommandUnavailableReason } {
  if (command.when?.() === false) return { available: false, reason: "when" };
  if (command.enabled?.() === false) return { available: false, reason: "enabled" };
  return { available: true };
}

export interface BindingIssue {
  kind: "duplicate" | "shared" | "reserved" | "risky" | "invalid";
  /** 要不要拦住保存/导入 */
  blocking: boolean;
  /** 显示用键位（格式化后的） */
  chord: string;
  commandIds: readonly string[];
}

/** 两个作用域会不会同时成立 */
export function scopeOverlaps(a: CommandScope, b: CommandScope): boolean {
  if (a === b) return true;
  return a === "global" || b === "global";
}

/**
 * 命令绑的**原始**键位写法（覆盖表优先；`null` = 没绑/显式解绑）。
 *
 * 为什么要留原始写法：`Mod+K` 一格式化成 `Ctrl+K` 就丢了「这是平台主修饰键」这层意思，
 * 再解析回来在 macOS 上就错了（那里 `Mod` 是 ⌘）。所以**匹配走原始写法**，
 * 显示才用格式化后的。
 */
export function rawChordOf(
  command: CommandSpec,
  overrides: ShortcutOverrides,
): string | null {
  if (Object.prototype.hasOwnProperty.call(overrides, command.id)) {
    return overrides[command.id] ?? null;
  }
  return command.defaultKey ?? null;
}

/** 单条命令的有效键位（**显示用**的格式化串；解析不出来的当没绑） */
export function chordOf(
  command: CommandSpec,
  overrides: ShortcutOverrides,
): string | null {
  const raw = rawChordOf(command, overrides);
  if (raw === null) return null;
  const parsed = parseChord(raw);
  return parsed.ok ? parsed.text : null;
}

/** 解析成 `Chord`（**用原始写法**解析，保留 `Mod` 的平台含义；拿不到就 `null`） */
export function parsedChordOf(
  command: CommandSpec,
  overrides: ShortcutOverrides,
): Chord | null {
  const raw = rawChordOf(command, overrides);
  if (raw === null) return null;
  const parsed = parseChord(raw);
  return parsed.ok ? parsed.chord : null;
}

/** 全部命令的有效键位（`命令 id → 键位串`），没绑的不在表里 */
export function effectiveBindings(
  commands: readonly CommandSpec[],
  overrides: ShortcutOverrides = {},
): Map<string, string> {
  const out = new Map<string, string>();
  for (const command of commands) {
    const chord = chordOf(command, overrides);
    if (chord !== null) out.set(command.id, chord);
  }
  return out;
}

/** 危险单键（有系统语义、误触代价大） */
const RISKY_SINGLE_KEYS = new Set(["escape", "enter", "tab", "space", "backspace"]);

/**
 * 冲突 / 提示报告。
 *
 * 判据见 `plans/M2-W3.md` §2.4：相交作用域的同键 = 冲突（拦）；不相交 = 提示（放行）；
 * 保留键 = 拦；破坏性命令绑危险单键 = 提示。
 */
export function detectConflicts(
  commands: readonly CommandSpec[],
  overrides: ShortcutOverrides = {},
): BindingIssue[] {
  const issues: BindingIssue[] = [];
  const byChord = new Map<string, { chord: Chord; text: string; commands: CommandSpec[] }>();

  for (const command of commands) {
    // 覆盖表里写坏的键位（导入的历史文件 / 手改存储）单独报，不让它参与分组
    if (Object.prototype.hasOwnProperty.call(overrides, command.id)) {
      const raw = overrides[command.id];
      if (raw !== null && raw !== undefined && !parseChord(raw).ok) {
        issues.push({
          kind: "invalid",
          blocking: true,
          chord: raw,
          commandIds: [command.id],
        });
        continue;
      }
    }
    const parsed = parsedChordOf(command, overrides);
    if (parsed === null) continue;
    const text = formatChord(parsed);
    if (isReservedChord(text)) {
      issues.push({
        kind: "reserved",
        blocking: true,
        chord: text,
        commandIds: [command.id],
      });
      continue;
    }
    if (
      command.dangerous === true &&
      parsed.mod === false &&
      parsed.ctrl === false &&
      parsed.meta === false &&
      parsed.alt === false &&
      RISKY_SINGLE_KEYS.has(parsed.key)
    ) {
      issues.push({
        kind: "risky",
        blocking: false,
        chord: text,
        commandIds: [command.id],
      });
    }
    const key = `${text}`;
    const bucket = byChord.get(key);
    if (bucket === undefined) {
      byChord.set(key, { chord: parsed, text, commands: [command] });
    } else {
      bucket.commands.push(command);
    }
  }

  for (const { text, commands: same } of byChord.values()) {
    if (same.length < 2) continue;
    // 两两看作用域：只要有一对相交，这条键就是真冲突
    let overlapping = false;
    for (let i = 0; i < same.length && !overlapping; i += 1) {
      for (let j = i + 1; j < same.length; j += 1) {
        if (scopeOverlaps(same[i].scope, same[j].scope)) {
          overlapping = true;
          break;
        }
      }
    }
    issues.push({
      kind: overlapping ? "duplicate" : "shared",
      blocking: overlapping,
      chord: text,
      commandIds: same.map((command) => command.id),
    });
  }

  return issues;
}

/** 只看会拦住保存的那些 */
export function blockingIssues(issues: readonly BindingIssue[]): BindingIssue[] {
  return issues.filter((issue) => issue.blocking);
}
