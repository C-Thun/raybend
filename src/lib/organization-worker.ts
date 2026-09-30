/** 有界、可重入的自动桶补扫驱动。进度游标由后端事务保存。 */
export function createOrganizationWorker(deps: {
  repositories: () => string[];
  reconcile: (ids: string[]) => Promise<{ processedBatches: number; failedRepositories: string[] }>;
  onChanged?: () => void;
  onError?: (error: unknown) => void;
  onPartialFailure?: (repositoryIds: string[]) => void;
}) {
  let disposed = false;
  let running = false;
  let requested = false;
  let lastFailed = "";

  async function drain(): Promise<void> {
    if (running || disposed) return;
    running = true;
    let changed = false;
    try {
      while (!disposed) {
        requested = false;
        const ids = deps.repositories();
        if (ids.length === 0) break;
        const result = await deps.reconcile(ids);
        const failed = [...result.failedRepositories].sort().join("\u0000");
        if (failed !== lastFailed) {
          lastFailed = failed;
          if (failed) deps.onPartialFailure?.(result.failedRepositories);
        }
        if (result.processedBatches > 0) changed = true;
        if (result.processedBatches === 0 && !requested) break;
        // Allow UI updates and catalog writes between 256-photo batches.
        await new Promise<void>((resolve) => setTimeout(resolve, 0));
      }
    } catch (error) { deps.onError?.(error); }
    finally {
      running = false;
      if (changed && !disposed) deps.onChanged?.();
      if (requested && !disposed) void drain();
    }
  }

  return {
    wake() { requested = true; void drain(); },
    dispose() { disposed = true; },
  };
}
