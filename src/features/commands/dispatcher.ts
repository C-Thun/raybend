/**
 * 命令分发器（`plans/M2-W3.md` §2.5）—— 全应用**唯一**的快捷键监听。
 *
 * ## 它做什么
 *
 * 一次 `keydown` → 按注册表顺序找「键位匹配 + `when()` 成立」的第一条命令 → 执行。
 * 匹配用原始键位写法（保留 `Mod` 的平台含义），所以 macOS 上 `Mod+K` 就是 ⌘K。
 *
 * ## 它不做什么
 *
 * * **不接**输入框 / 模态里的键（复用 `lib/viewer-keys.ts` 的 `shouldHandleKey`）；
 * * **不接**元素自己的键盘行为（列表 roving focus、`Tile` 的 Enter/Space、Ark 组件内部）；
 * * **不接管**没在注册表里绑键的东西（例如 `Enter` 在单张 view 返回 —— 那是看图件自己的语义；
 *   compare 的 Enter 已登记为切换胶片带范围的命令）。
 *
 * ## 为什么「第一个匹配」就够
 *
 * 冲突检测（`lib/commands.ts`）保证同一作用域里不会出现两条同键命令；
 * 不同作用域靠 `when()` 互斥。万一存储被外部改坏（两条都成立），
 * 这里按注册表顺序取第一条，并把另一条打进 `console.warn` —— 不静默。
 */

import { createMemo } from "solid-js";

import { chordMatches, detectPlatform, type ChordPlatform } from "../../lib/key-chords.ts";
import { parsedChordOf, type CommandSpec, type ShortcutOverrides } from "../../lib/commands.ts";
import { rememberCommand } from "../../lib/shortcuts.ts";
import { shouldHandleKey } from "../../lib/viewer-keys.ts";

export interface CommandDispatcherOptions {
  /** 注册表（组装层建的；顺序 = 命令面板的默认顺序） */
  commands: () => readonly CommandSpec[];
  /** 覆盖表（快捷键设置改的就是它） */
  overrides: () => ShortcutOverrides;
  /** 额外拦截：弹窗开着、正在输入、已经被别人处理过…… */
  blocked?: (event: KeyboardEvent) => boolean;
  /** 平台（测试注入用） */
  platform?: () => ChordPlatform;
  /** 执行留痕（冒烟与调试用） */
  onRun?: (command: CommandSpec) => void;
}

export interface CommandDispatcher {
  /** 处理一次按键；返回是否被用掉（测试直接调它） */
  handle: (event: KeyboardEvent) => boolean;
  /** 挂上 window 监听（组装层 `onMount` 调一次） */
  attach: () => void;
  dispose: () => void;
}

export function createCommandDispatcher(options: CommandDispatcherOptions): CommandDispatcher {
  const platform = options.platform ?? detectPlatform;
  /*
   * 命令 + 有效键位：只在注册表或覆盖表变时重算（每次按键都重解析 60 条没必要）。
   * `parsedChordOf` 用**原始写法**解析，`Mod` 的平台含义因此保留到匹配那一刻。
   */
  const bound = createMemo(() =>
    options.commands().map((command) => ({
      command,
      chord: parsedChordOf(command, options.overrides()),
    })),
  );

  const handle = (event: KeyboardEvent): boolean => {
    if (event.defaultPrevented) return false;
    if (options.blocked?.(event) === true) return false;
    if (!shouldHandleKey(event.target as HTMLElement | null, false)) return false;

    const matches: CommandSpec[] = [];
    for (const entry of bound()) {
      if (entry.chord === null) continue;
      if (!chordMatches(entry.chord, event, platform())) continue;
      if (entry.command.when?.() === false) continue;
      if (entry.command.enabled?.() === false) continue;
      matches.push(entry.command);
      // 第一个就执行：见文件头「为什么第一个匹配就够」
      break;
    }
    const command = matches[0];
    if (command === undefined) return false;

    event.preventDefault();
    rememberCommand(command.id);
    options.onRun?.(command);
    void command.run();
    return true;
  };

  const onKeyDown = (event: KeyboardEvent): void => {
    handle(event);
  };

  return {
    handle,
    attach: () => window.addEventListener("keydown", onKeyDown),
    dispose: () => window.removeEventListener("keydown", onKeyDown),
  };
}
