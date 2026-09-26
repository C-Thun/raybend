/**
 * 快捷键设置（`specs/M2-W3.md` §4 阶段 4）。
 *
 * ## 交互
 *
 * 一行 = 一条命令：左边标题、右边当前键位（点它进入**捕获态**，按一下新键就改）、
 * 再右边是「恢复默认」（只有被改过的行才有）。
 * 捕获态下：`Esc` 取消、`Backspace`/`Delete` **解绑**（这条命令不要键）。
 *
 * ## 为什么是「草稿 + 保存」而不是改一下就生效
 *
 * 冲突检测是**整表**的判定（改 A 可能撞上 B）。边改边生效的话，撞上的那一瞬间
 * 界面已经处于一个「两条命令同键」的状态了 —— 那时候该执行谁？所以：
 * 改动先进草稿，**全表无阻断冲突才允许保存**，保存后立刻生效（不需要重启）。
 *
 * ## 导入导出
 *
 * JSON 文件（`lib/shortcuts.ts` 的 `exportShortcuts` / `parseShortcutsFile`）。
 * 走浏览器自己的下载 / 文件选择：这样 `pnpm dev` 的浏览器与打包后的 WebView2 都能用，
 * 不需要为它加一条 IPC（真要换成系统对话框 + Rust 写盘，改的只有这一处）。
 */

import { Dialog as ArkDialog } from "@ark-ui/solid";
import { createEffect, createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import { Portal } from "solid-js/web";

import { Button } from "../../components/ui/Button.tsx";
import { blockingIssues, detectConflicts, rawChordOf, type CommandSpec } from "../../lib/commands.ts";
import { chordFromEvent, formatChord, parseChord } from "../../lib/key-chords.ts";
import {
  clearShortcutOverride,
  exportShortcuts,
  parseShortcutsFile,
  resetAllShortcuts,
  shortcutOverrides,
  setShortcutOverride,
  type ImportProblem,
} from "../../lib/shortcuts.ts";
import { issueText, problemText } from "./messages.ts";
import { t, type MessageKey } from "../../i18n/index.ts";

export interface ShortcutSettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  commands: readonly CommandSpec[];
  /** 保存成功后给一句提示（组装层接 toast） */
  onSaved?: () => void;
}

/** 分组显示顺序（与命令面板同一套分组，顺序按这里） */
const GROUP_ORDER = ["file", "edit", "view", "window", "help", "viewer", "mark", "navigate", "import"] as const;

export function ShortcutSettingsDialog(props: ShortcutSettingsDialogProps) {
  /** 草稿：打开时从单例拷一份，保存时才写回 */
  const [draft, setDraft] = createSignal<Record<string, string | null>>({});
  /** 正在捕获哪一行（命令 id；`null` = 没在捕获） */
  const [capturing, setCapturing] = createSignal<string | null>(null);
  /** 导入失败的问题清单（逐条列出来） */
  const [importProblems, setImportProblems] = createSignal<readonly ImportProblem[]>([]);
  let fileInput: HTMLInputElement | undefined;

  createEffect(() => {
    if (props.open) {
      setDraft({ ...shortcutOverrides() });
      setCapturing(null);
      setImportProblems([]);
    }
  });

  const issues = createMemo(() => detectConflicts(props.commands, draft()));
  const blocking = createMemo(() => blockingIssues(issues()));
  const notes = createMemo(() => issues().filter((issue) => !issue.blocking));

  const groupLabel = (group: string): string => {
    const key = `cmd.group.${group}` as MessageKey;
    const text = t(key);
    return text === key ? group : text;
  };

  const groups = createMemo(() => {
    const order = new Map(GROUP_ORDER.map((group, index) => [group, index]));
    const names = [...new Set(props.commands.map((command) => command.group))];
    names.sort((a, b) => (order.get(a as never) ?? 99) - (order.get(b as never) ?? 99));
    return names.map((name) => ({
      name,
      label: groupLabel(name),
      commands: props.commands.filter((command) => command.group === name),
    }));
  });

  /** 某条命令在草稿里的显示键位（草稿优先，其次默认） */
  const shownChord = (command: CommandSpec): string => {
    const raw = Object.prototype.hasOwnProperty.call(draft(), command.id)
      ? (draft()[command.id] ?? null)
      : (command.defaultKey ?? null);
    if (raw === null) return "—";
    const parsed = parseChord(raw);
    return parsed.ok ? formatChord(parsed.chord) : raw;
  };

  const isOverridden = (command: CommandSpec): boolean =>
    Object.prototype.hasOwnProperty.call(draft(), command.id);

  const setChord = (command: CommandSpec, chord: string | null): void => {
    setDraft((current) => ({ ...current, [command.id]: chord }));
  };

  /** 捕获态：window 级监听（捕获阶段，抢在分发器与 Ark 之前） */
  const onCaptureKeyDown = (event: KeyboardEvent): void => {
    const id = capturing();
    if (id === null) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      setCapturing(null);
      return;
    }
    if (event.key === "Backspace" || event.key === "Delete") {
      const command = props.commands.find((item) => item.id === id);
      if (command !== undefined) setChord(command, null);
      setCapturing(null);
      return;
    }
    const chord = chordFromEvent(event);
    if (chord === null) return; // 只按了修饰键：等真正的键
    const command = props.commands.find((item) => item.id === id);
    if (command !== undefined) setChord(command, chord);
    setCapturing(null);
  };

  createEffect(() => {
    if (capturing() === null) return;
    window.addEventListener("keydown", onCaptureKeyDown, true);
    onCleanup(() => window.removeEventListener("keydown", onCaptureKeyDown, true));
  });

  const save = (): void => {
    if (blocking().length > 0) return;
    // 逐条写回单例（`setShortcutOverride` / `clearShortcutOverride` 都各自落盘）
    const ids = new Set([...Object.keys(shortcutOverrides()), ...Object.keys(draft())]);
    for (const id of ids) {
      const next = draft()[id];
      if (next === undefined) clearShortcutOverride(id);
      else setShortcutOverride(id, next);
    }
    props.onSaved?.();
    props.onOpenChange(false);
  };

  const exportToFile = (): void => {
    const text = exportShortcuts({ overrides: draft(), recent: [] });
    const blob = new Blob([text], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = "raybend-shortcuts.json";
    link.click();
    URL.revokeObjectURL(url);
  };

  const importFromFile = async (file: File | undefined): Promise<void> => {
    if (file === undefined) return;
    const text = await file.text();
    const result = parseShortcutsFile(text, props.commands);
    if (!result.ok) {
      setImportProblems(result.problems);
      return;
    }
    setImportProblems([]);
    setDraft({ ...result.state.overrides });
  };

  return (
    <ArkDialog.Root
      open={props.open}
      onOpenChange={(details) => props.onOpenChange(details.open)}
      lazyMount
      unmountOnExit
      role="dialog"
    >
      <Portal>
        <ArkDialog.Backdrop class="fixed inset-0 bg-scrim" />
        <ArkDialog.Positioner class="fixed inset-0 flex items-center justify-center p-4">
          <ArkDialog.Content
            data-shortcuts-dialog="open"
            class="flex max-h-[80vh] w-full max-w-2xl flex-col gap-3 rounded-ui bg-surface-layer p-(--dialog-pad) outline-none"
          >
            <ArkDialog.Title class="text-[17px] leading-6 font-semibold text-fg-1">
              {t("shortcuts.title")}
            </ArkDialog.Title>
            <p class="text-[13px] text-fg-2">{t("shortcuts.description")}</p>

            {/* 阻断冲突：红字逐条列出来（保存会被禁用） */}
            <Show when={blocking().length > 0}>
              <div
                data-shortcuts-conflicts="blocking"
                class="rounded-ui bg-surface-bar px-2 py-1.5 text-fs-1 text-danger"
              >
                <For each={blocking()}>{(issue) => <p>{issueText(issue)}</p>}</For>
              </div>
            </Show>
            {/* 非阻断提示（同一按键、不同上下文）：一句话带过 */}
            <Show when={notes().length > 0 && blocking().length === 0}>
              <p data-shortcuts-conflicts="note" class="text-fs-1 text-fg-3">
                {t("shortcuts.sharedNote").replace("{n}", String(notes().length))}
              </p>
            </Show>
            <Show when={importProblems().length > 0}>
              <div
                data-shortcuts-import-problems="open"
                class="rounded-ui bg-surface-bar px-2 py-1.5 text-fs-1 text-danger"
              >
                <For each={importProblems()}>{(problem) => <p>{problemText(problem)}</p>}</For>
              </div>
            </Show>

            <div class="min-h-0 flex-1 overflow-y-auto" data-shortcuts-list="open">
              <For each={groups()}>
                {(group) => (
                  <>
                    <h3 class="px-1 pt-3 pb-1 text-fs-1 font-semibold text-fg-2">{group.label}</h3>
                    <For each={group.commands}>
                      {(command) => (
                        <div
                          data-shortcut-row={command.id}
                          class="flex items-center gap-2 rounded-ui px-1 py-1 hover:bg-state-hover"
                        >
                          <span class="min-w-0 flex-1 truncate text-fs-2 text-fg-1">
                            {t(command.titleKey as MessageKey)}
                          </span>
                          <button
                            type="button"
                            data-shortcut-key={command.id}
                            onClick={() => setCapturing(command.id)}
                            class={[
                              "min-w-24 shrink-0 rounded-ui px-2 py-0.5 text-fs-1",
                              capturing() === command.id
                                ? "bg-state-selected text-fg-1"
                                : "bg-surface-bar text-fg-2 hover:text-fg-1",
                            ].join(" ")}
                          >
                            {capturing() === command.id ? t("shortcuts.pressKey") : shownChord(command)}
                          </button>
                          <Show when={isOverridden(command)}>
                            <button
                              type="button"
                              data-shortcut-reset={command.id}
                              onClick={() => {
                                setDraft((current) => {
                                  const next = { ...current };
                                  delete next[command.id];
                                  return next;
                                });
                              }}
                              class="shrink-0 text-fs-0 text-fg-3 hover:text-fg-1"
                            >
                              {t("shortcuts.reset")}
                            </button>
                          </Show>
                          <span class="w-10 shrink-0 text-fs-0 text-fg-3">
                            {/* 默认键位（被改过时给个对照） */}
                            {isOverridden(command)
                              ? (rawChordOf(command, {}) ?? "—")
                              : ""}
                          </span>
                        </div>
                      )}
                    </For>
                  </>
                )}
              </For>
            </div>

            {/* 内建键位：不经过注册表、不可改（诚实说明，免得用户以为漏了） */}
            <p class="text-fs-0 text-fg-3">{t("shortcuts.builtinNote")}</p>

            <div class="flex items-center gap-2">
              <input
                ref={(element) => {
                  fileInput = element;
                }}
                type="file"
                accept="application/json,.json"
                class="hidden"
                onChange={(event) => {
                  void importFromFile(event.currentTarget.files?.[0]);
                  event.currentTarget.value = "";
                }}
              />
              <Button variant="secondary" onClick={() => fileInput?.click()}>
                {t("shortcuts.import")}
              </Button>
              <Button variant="secondary" onClick={exportToFile}>
                {t("shortcuts.export")}
              </Button>
              <Button
                variant="secondary"
                onClick={() => {
                  resetAllShortcuts();
                  setDraft({});
                }}
              >
                {t("shortcuts.resetAll")}
              </Button>
              <span class="min-w-0 flex-1" />
              <Button variant="secondary" onClick={() => props.onOpenChange(false)}>
                {t("common.cancel")}
              </Button>
              <Button
                variant="primary"
                disabled={blocking().length > 0}
                onClick={save}
              >
                {t("common.save")}
              </Button>
            </div>
          </ArkDialog.Content>
        </ArkDialog.Positioner>
      </Portal>
    </ArkDialog.Root>
  );
}
