/**
 * 冲突 / 导入问题的**文案渲染**（`features/commands/`）。
 *
 * 为什么单独一份：`lib/commands.ts` 与 `lib/shortcuts.ts` 只能给出**结构化**结果
 * （`BindingIssue` / `ImportProblem`）—— `lib/` 不许 import i18n（分层规矩），
 * 而这些话又必须跟着界面语言走。于是：**判定在 lib、措辞在这里**。
 *
 * 实现用**查表**而不是 `switch`：表被 `satisfies Record<..., MessageKey>` 钉住 ——
 * lib 那边新增一种 kind 而不在这里补文案，**编译期就报错**（switch 少了分支同样会报，
 * 但一张表更好读，也不会被「没有 default」的规则误伤）。
 */

import type { BindingIssue } from "../../lib/commands.ts";
import type { ImportProblem } from "../../lib/shortcuts.ts";
import { t, type MessageKey } from "../../i18n/index.ts";

/** `kind → 文案 key`（带替换位：见下面的 render） */
const ISSUE_KEYS = {
  duplicate: "shortcuts.issue.duplicate",
  shared: "shortcuts.issue.shared",
  reserved: "shortcuts.issue.reserved",
  risky: "shortcuts.issue.risky",
  invalid: "shortcuts.issue.invalid",
} as const satisfies Record<BindingIssue["kind"], MessageKey>;

const IMPORT_KEYS = {
  notJson: "shortcuts.import.notJson",
  notObject: "shortcuts.import.notObject",
  version: "shortcuts.import.version",
  missingShortcuts: "shortcuts.import.missingShortcuts",
  unknownCommand: "shortcuts.import.unknownCommand",
  notString: "shortcuts.import.notString",
  badChord: "shortcuts.import.badChord",
  reserved: "shortcuts.import.reserved",
  conflict: "shortcuts.import.conflict",
} as const satisfies Record<ImportProblem["kind"], MessageKey>;

/** 把 `{name}` 替换位填上（语言包里的占位符统一是这个写法） */
function fill(text: string, values: Record<string, string>): string {
  let out = text;
  for (const [key, value] of Object.entries(values)) {
    // 不用 `replaceAll`：本仓的 TS lib 目标不到 ES2021（`pnpm typecheck` 会报）
    out = out.split(`{${key}}`).join(value);
  }
  return out;
}

/** 一条冲突 / 提示 → 人话 */
export function issueText(issue: BindingIssue): string {
  return fill(t(ISSUE_KEYS[issue.kind]), {
    chord: issue.chord,
    n: String(issue.commandIds.length),
    id: issue.commandIds[0] ?? "",
  });
}

/** 一条导入问题 → 人话 */
export function problemText(problem: ImportProblem): string {
  const values: Record<string, string> = { id: "", value: "" };
  if ("id" in problem) values.id = problem.id;
  if ("value" in problem) values.value = problem.value;
  if (problem.kind === "version") {
    values.found = problem.found;
    values.expected = String(problem.expected);
  }
  if (problem.kind === "conflict") {
    values.chord = problem.chord;
    values.n = String(problem.commandIds.length);
  }
  return fill(t(IMPORT_KEYS[problem.kind]), values);
}
