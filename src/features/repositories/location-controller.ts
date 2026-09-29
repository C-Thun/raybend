import type { RepositoryView } from "../../api/types.ts";

/** 原生选择器和 IPC 都可能晚于弹窗关闭；只把结果送回仍然相同的上下文。 */
export function createLocationController(deps: {
  target: () => string | null;
  pick: () => Promise<string | null>;
  add: (id: string, path: string) => Promise<RepositoryView>;
  remove: (id: string, path: string) => Promise<RepositoryView>;
  busy: (value: boolean) => void;
  error: (value: unknown | null) => void;
}) {
  let epoch = 0, running = false;
  const reset = () => { epoch++; running = false; deps.busy(false); deps.error(null); };
  async function run(path?: string): Promise<void> {
    const id = deps.target();
    if (id === null || running) return;
    const ticket = epoch;
    const current = () => epoch === ticket && deps.target() === id;
    running = true; deps.busy(true); deps.error(null);
    try {
      const selected = path ?? await deps.pick();
      if (!current() || selected === null) return;
      if (!selected.trim()) { deps.error({ code: "invalid_path" }); return; }
      const result = path === undefined ? await deps.add(id, selected) : await deps.remove(id, selected);
      if (current() && !result.online && result.connection?.state === "unavailable")
        deps.error({ code: result.connection.reason ?? "not_found" });
    } catch (error) {
      if (current()) deps.error(error);
    } finally {
      if (current()) { running = false; deps.busy(false); }
    }
  }
  return { reset, choose: () => run(), remove: (path: string) => run(path) };
}
