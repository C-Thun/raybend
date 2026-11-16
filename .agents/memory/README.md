# Agent memory framework v1

This repository keeps agent memory in a fixed shape so that every session can find
a fact in one place instead of re-deriving it, and so the memory does not grow into
an unreadable pile. Read this file **before writing memory**; it is the rulebook,
while `AGENTS.md` at the repository root is the entry point and index.

## Where things live

| Location | Answers | Lifetime |
| --- | --- | --- |
| `specs/PLAN.md` | What are we going to do, what is still open | rewritten as work moves |
| `specs/<unit>.md` | One unit of work: requirement, approach, acceptance | written before work, status updated at close |
| `.agents/memory/` | What is known, decided, and why | long-lived; append and compact only |
| `.agents/implementations/` | What was changed, when, with what evidence | one record per change, never rewritten |
| `.agents/prompts/` | The operator's own words (optional) | read-only input |

## Memory files

| File | Owns | Never holds |
| --- | --- | --- |
| `FINISHED.md` | Closed work: summary + evidence pointer + historical appendix | spec bodies, progress lists |
| `FUTURE.md` | Deferred, rejected, waiting-for-a-trigger | schedule promises |
| `ARCHITECTURE.md` | Layering, version baseline, decision records, investigation archive | visual specs, progress |
| `DESIGN.md` | Visual single source of truth: tokens, palette, spacing, component looks | implementation details |
| `TERMS.md` | Glossary and the rules that tell look-alike terms apart | visual specs |
| `REVIEW.md` | Review ledger: judgement, trade-off, destination | a second copy of a spec |
| `DEBUGGING.md` | Runtime diagnosis runbook | code facts |
| `FUNCTION-<MODULE>.md` | One module's functional rules | copies of another module's rules |

No file is created before it has content. Files not listed here are not memory.

## Write rules

1. **One fact, one home.** Detail lives in the file that owns it; everywhere else
   is a pointer (`path#section`). If you are about to restate a fact, write a
   pointer instead.
2. **AGENTS.md holds only what every session needs**: invariants, commands,
   routing, the index. Anything that can be looked up does not belong there.
3. **Record conclusions and constraints, not narration.** No chat transcripts, no
   "first I did X, then Y".
4. **Append and compact; never rewrite history to look better.** Change records
   are written once. The single exception: a directory move may update the path
   strings inside them, and that must be listed in the migration report.
5. **Date every stale-able claim** (versions, measurements, "currently"). An
   unmeasured claim is `inference` or `TBD`, never a fact.
6. **Keep the operator's decisions verbatim** (quote + date + who). They are the
   most expensive thing to reconstruct.
7. **Progress lives in the todo tool.** No second progress table in any document.
8. **Specs precede work; records follow it.** Close a spec honestly (status line);
   never delete it — archive it and leave a link when superseded.
9. **Compress before you append.** Over budget means compact first, then add.
10. **No secrets, credentials, or personal paths** in memory. Evidence you would
    not publish does not belong here.

## Budgets

| What | Limit | Why |
| --- | --- | --- |
| `AGENTS.md` | soft 12000 chars, hard 20000 | paid on **every** request |
| one memory file | 80000 chars | navigability, not tokens |
| change records | 200 files | roll up and archive past this |

The audit reports overruns. Overrunning is not an emergency; growing without a
compaction plan is.

## Compaction recipes

| File | How to shrink it |
| --- | --- |
| `specs/PLAN.md` | Move closed items to `FINISHED.md` as a 3-8 line summary + pointer |
| `FINISHED.md` | Keep milestone summaries + pointers; move whole eras to `.agents/memory/archive/<year>/` |
| `FUTURE.md` | Every entry needs a trigger; adopted → `specs/PLAN.md` + one-line pointer; unreachable → mark dropped with a date |
| `ARCHITECTURE.md` | Decisions are append-only (context / decision / consequence / date); supersede, do not delete |
| `FUNCTION-<MODULE>.md` | Split by sub-domain past budget; move dating measurements into a measurement appendix |
| `REVIEW.md` | One section per round; the overview table is the only summary; fold old rounds into an appendix |
| change records | Roll up per milestone into `FINISHED.md`, then move originals to `.agents/implementations/archive/<year>/` |

## Audit

`/ask-init check` (or the `framework_audit` tool during an init run) reports:

- structure: missing required files, index ↔ file mismatches, `AGENTS.override.md` shadowing
- framework files: managed text that is missing or behind the installed template, a missing or malformed `AGENTS.md` workflow block, and hand edits (`/ask-init` refreshes only what is safe)
- paths: references to `memory` or `implementations` without the `.agents/` prefix; an error while a legacy directory still exists (`E-PATH-01`), a warning once it is gone (`W-PATH-01`)
- duplication: the same section heading in two files (one fact, two homes)
- order: sections appended after the historical appendix
- budgets: `AGENTS.md` and memory-file sizes
- state: `PLAN.md` ↔ specs drift, stale `todos/`, `FUTURE.md` entries with no trigger
- evidence: `FINISHED.md` entries without a change-record pointer

Codes are stable (`E-…`, `W-…`, `I-…`), so reports can be compared across runs.

## Upgrading the framework

`.agents/framework/state.toml` records the framework version. When this file's
version is newer than the state file, `/ask-init` plans an upgrade: deterministic
steps run in code, prose changes are handed to the model as tasks. A project whose
state version is newer than the installed tool is never migrated — upgrade the
tool instead.

Text-only changes (the workflow block, the signpost, this contract) do not bump
the framework version. Run `/ask-init` (or `/ask-init refresh`) after upgrading
the package to bring them up to date, then `/reload` if `AGENTS.md` changed.
Re-running `/ask-init` is safe: it reports `current` when nothing drifted and
leaves files you edited by hand alone.
