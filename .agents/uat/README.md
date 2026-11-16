# Acceptance ledger (UAT)

This directory owns one question: **which user-visible behaviours have been
accepted, and on which evidence?** It is the bridge between
`.agents/implementations/` (prose records of what changed) and the operator's
acceptance decisions.

`ledger.jsonl` is append-only and committed to git. Nothing here is a task
list, a plan, or a second copy of a spec.

## Where it lives

The ledger belongs to the **main checkout only**. Sessions running in a linked
git worktree write their change records and ledger events to the main
checkout, so the acceptance history never splits.

## Events

One JSON object per line. Every event carries `v`, `id`, `ts`, `type`; readers
deduplicate by `id` (two branches may both append, and both sides are kept).

| type | written by | meaning |
| --- | --- | --- |
| `candidate` | `record_change` / `/ask-uat` fallback | a change record that may need acceptance, plus the one-sentence behaviour it exposes |
| `batch` | `/ask-uat` | one clustering run: the acceptance items reviewed together |
| `item` | `uat_item` | one acceptance item: title, 2-3 sentence summary, test steps, evidence records |
| `link` | `uat_item` | new evidence attached to an existing item (the N:M bridge) |
| `check` / `uncheck` | the browser page | the operator's acceptance decision, written immediately |
| `amend` / `merge` / `drop` | `uat_item` | revise an item, merge duplicates, retire abandoned work |

## Rules

1. **One item, many records; one record, many items.** A record is evidence, not
   an acceptance unit. Items are created by clustering on the UAT side, never by
   an implementation session — parallel sessions cannot see each other, so
   item identity must have a single writer.
2. **No submit.** Checking an item writes a `check` event immediately. Closing
   the page changes nothing. Unchecking is the same event stream in reverse.
3. **New evidence reopens an item.** When a `link` (or a new record in an item)
   is newer than the item's last `check`, the item counts as unchecked again.
   "Checked" therefore means "checked against the current evidence set".
4. **Nothing is deleted.** `drop` retires abandoned work; the event stays.
5. **Records stay prose.** The ledger stores the behaviour sentence and the
   test hint; the change record keeps the narrative for the agent to read. Do
   not parse prose to reconstruct the ledger.

## Commands

- `/ask-uat` — cluster pending change records into acceptance items, then open
  the page
- `/ask-uat open` — reopen the page without a model turn
- `/ask-uat off` — stop the page server
- `/ask-init refresh` — refresh framework-owned files (this contract included)
