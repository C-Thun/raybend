import { createEffect, createSignal, For, onCleanup, Show, untrack } from "solid-js";
import { aiModelStatus, aiTasks, aiTaskAction, aiStart, type AiRange, type AiModelStatus, type AiTask } from "../../api/photo-ai.ts";
import type { AssetIdentity } from "../../api/organization.ts";
import { Button } from "../../components/ui/Button.tsx";
import { Dialog } from "../../components/ui/Dialog.tsx";
import { t, locale, type MessageKey } from "../../i18n/index.ts";
export function AiDialog(props: { open: boolean; mode: "recognize" | "tasks"; rerun: boolean; photos: AssetIdentity[]; repositoryId: string | null; directory: string | null; repositories: readonly { id: string; name: string; online: boolean }[]; onClose: () => void; onModels: () => void; onStarted: () => void }) {
  const [model, setModel] = createSignal<AiModelStatus | null>(null);
  const [tasks, setTasks] = createSignal<AiTask[]>([]);
  const [error, setError] = createSignal<string | null>(null);
  const [scope, setScope] = createSignal("selected");
  const [repository, setRepository] = createSignal("");
  const [rerun, setRerun] = createSignal(false);
  const [recursive, setRecursive] = createSignal(true);
  const [selection, setSelection] = createSignal<AssetIdentity[]>([]);
  const [location, setLocation] = createSignal<{ repositoryId: string | null; directory: string | null }>({ repositoryId: null, directory: null });
  const [actionBusy, setActionBusy] = createSignal(false);
  const refresh = async () => { try { setTasks(await aiTasks()); } catch (reason) { setError(String(reason)); } };
  createEffect(() => {
    if (!props.open) return;
    const mode = props.mode;
    let active = true; setError(null); setModel(null);
    untrack(() => {
      setSelection(props.photos.map((photo) => ({ ...photo })));
      setLocation({ repositoryId: props.repositoryId, directory: props.directory });
      setRerun(props.rerun); setRecursive(true);
      setScope(props.photos.length > 0 ? "selected" : props.directory !== null ? "directory" : "repository");
      setRepository(props.repositoryId ?? props.repositories.find((item) => item.online)?.id ?? "");
    });
    void mode;
    void aiModelStatus().then((status) => { if (active) setModel(status); }).catch((reason: unknown) => { if (active) setError(String(reason)); });
    const read = () => void aiTasks().then((rows) => { if (active) setTasks(rows); }).catch((reason: unknown) => { if (active) setError(String(reason)); });
    read(); const timer = setInterval(read, 2000);
    onCleanup(() => { active = false; clearInterval(timer); });
  });
  const action = async (id: number, value: "pause" | "resume" | "cancel" | "retry") => {
    setActionBusy(true); setError(null);
    try { await aiTaskAction(id, value); await refresh(); } catch (reason) { setError(String(reason)); } finally { setActionBusy(false); }
  };
  const start = async () => {
    if (actionBusy() || model()?.state !== "ready") return;
    const current = location();
    const range: AiRange | null = scope() === "selected" ? selection().length > 0 ? { kind: "selected", photos: selection() } : null
      : scope() === "directory" ? current.repositoryId !== null && current.directory !== null ? { kind: "directory", repositoryId: current.repositoryId, directory: current.directory, recursive: recursive() } : null
      : repository() ? { kind: "repository", repositoryId: repository() } : null;
    if (range === null) return;
    setActionBusy(true); setError(null);
    try { await aiStart(range, rerun(), locale() === "zh-CN"); props.onStarted(); }
    catch (reason) { setError(String(reason)); }
    finally { setActionBusy(false); }
  };
  const stateKey = (state: string): MessageKey => ({ pending: "ai.pending", preparing: "ai.preparing", running: "ai.running", paused: "ai.paused", cancelled: "ai.cancelled", failed: "ai.failed", done: "ai.done" } as Record<string, MessageKey>)[state] ?? "ai.failed";
  return <Dialog open={props.open} onOpenChange={(open) => { if (!open && !actionBusy()) props.onClose(); }} title={t(props.mode === "tasks" ? "ai.tasks" : "ai.recognize")} footer={<div class="flex justify-end gap-2"><Button variant="secondary" disabled={actionBusy()} onClick={props.onClose}>{t("common.cancel")}</Button><Show when={props.mode === "recognize"}><Button disabled={actionBusy() || model()?.state !== "ready"} title={model()?.state !== "ready" ? t(model()?.state === "unpublished" ? "ai.unpublished" : "ai.missing") : undefined} onClick={() => void start()}>{t("ai.start")}</Button></Show></div>}>
    <div class="space-y-4">
      <Show when={props.mode === "recognize"}>
        <fieldset class="space-y-2"><legend class="mb-2 text-fs-1 text-fg-2">{t("ai.range")}</legend>
          <For each={["selected", "directory", "repository"]}>{(value) => <label class="flex items-center gap-2 rounded-ui bg-surface-bar p-2 text-fs-1 text-fg-1"><input type="radio" name="ai-scope" value={value} checked={scope() === value} disabled={value === "selected" ? selection().length === 0 : value === "directory" ? !location().repositoryId || location().directory === null : props.repositories.every((item) => !item.online)} onChange={() => setScope(value)} />{t(value === "selected" ? "ai.selected" : value === "directory" ? "ai.directory" : "ai.repository")}<Show when={value === "selected"}><span class="text-fg-3">{t("ai.count").replace("{n}", String(selection().length))}</span></Show><Show when={value === "directory"}><span class="truncate text-fg-3">{location().directory}</span></Show></label>}</For>
        </fieldset>
        <Show when={scope() === "repository"}><select class="w-full rounded-ui bg-surface-bar p-2 text-fs-1 text-fg-1" aria-label={t("ai.repository")} value={repository()} onChange={(event) => setRepository(event.currentTarget.value)}><For each={props.repositories.filter((item) => item.online)}>{(item) => <option value={item.id}>{item.name}</option>}</For></select></Show>
        <Show when={scope() === "directory"}><label class="flex gap-2 text-fs-1 text-fg-2"><input type="checkbox" checked={recursive()} onChange={(event) => setRecursive(event.currentTarget.checked)} />{t("ai.subdirectories")}</label></Show>
        <p class="text-fs-1 text-fg-3">{t("ai.freeze")}</p>
        <label class="flex gap-2 text-fs-1 text-fg-2"><input type="checkbox" checked={!rerun()} onChange={(event) => setRerun(!event.currentTarget.checked)} />{t("ai.skipKnown")}</label>
        <p class="text-fs-1 text-fg-3">{t("ai.replace")}</p>
        <Show when={model()?.state !== "ready"}><div class="space-y-2 rounded-ui bg-surface-bar p-3"><p class="text-fs-1 text-fg-2">{t(model()?.state === "unpublished" ? "ai.unpublished" : "ai.missing")}</p><Button variant="secondary" size="sm" onClick={props.onModels}>{t("ai.manageModel")}</Button></div></Show>
      </Show>
      <Show when={props.mode === "tasks"}>
        <Button variant="ghost" size="sm" onClick={() => void refresh()}>{t("ai.refresh")}</Button>
        <Show when={tasks().length > 0} fallback={<p class="py-8 text-center text-fs-1 text-fg-3">{t("ai.noTasks")}</p>}>
          <div class="max-h-96 space-y-3 overflow-y-auto"><For each={tasks()}>{(task) => <article class="space-y-2 rounded-ui bg-surface-bar p-3"><div class="flex justify-between gap-3 text-fs-1 text-fg-1"><span class="truncate">{task.label}</span><span>{t(stateKey(task.state))}</span></div><progress class="h-1.5 w-full accent-brand" max={Math.max(1, task.total)} value={task.done + task.failed + task.skipped} aria-label={task.label} /><p class="text-fs-0 text-fg-3">{t("ai.done")} {task.done} · {t("ai.skipped")} {task.skipped} · {t("ai.failed")} {task.failed} · {t("ai.waiting")} {task.waiting} / {task.total}</p><Show when={task.error}><p class="text-fs-0 text-danger">{task.error}</p></Show><div class="flex gap-2"><Show when={["pending", "running", "preparing"].includes(task.state)}><Button variant="secondary" size="sm" disabled={actionBusy()} onClick={() => void action(task.id, "pause")}>{t("ai.pause")}</Button></Show><Show when={task.state === "paused"}><Button variant="secondary" size="sm" disabled={actionBusy() || model()?.state !== "ready"} onClick={() => void action(task.id, "resume")}>{t("ai.resume")}</Button></Show><Show when={task.state === "failed"}><Button variant="secondary" size="sm" disabled={actionBusy()} onClick={() => void action(task.id, "retry")}>{t("ai.retry")}</Button></Show><Show when={["pending", "running", "preparing", "paused"].includes(task.state)}><Button variant="secondary" size="sm" disabled={actionBusy()} onClick={() => void action(task.id, "cancel")}>{t("ai.cancel")}</Button></Show></div></article>}</For></div>
        </Show><p class="text-fs-0 text-fg-3">{t("ai.closeRuns")}</p>
      </Show>
      <Show when={error()}>{(message) => <p class="text-fs-1 text-danger" role="alert">{message()}</p>}</Show>
    </div>
  </Dialog>;
}
