/** 合并正在执行期间的变更范围；一轮结束后继续处理，库切换丢弃旧库待处理项。 */
export function createCatalogRefresh(run: (repositoryId: string, scopes: readonly string[]) => Promise<void>) {
  let pendingRepository: string | null = null;
  const scopes = new Set<string>();
  let pending = false;
  let refreshActive = false;
  let running: Promise<void> | null = null;
  let disposed = false;
  const pump = async (): Promise<void> => {
    // request 必须先登记 running，才开始执行可能同步发回新 request 的 run。
    await Promise.resolve();
    try {
      while (pending && !disposed) {
        const repository = pendingRepository;
        const batch = refreshActive ? [] : [...scopes].slice(0, 66);
        refreshActive = false;
        scopes.clear();
        pending = scopes.size > 0;
        if (repository !== null) await run(repository, batch);
      }
    } finally { running = null; }
  };
  return {
    request(repositoryId: string, changed: readonly string[] = []): Promise<void> {
      if (disposed) return Promise.resolve();
      if (pendingRepository !== repositoryId) { scopes.clear(); refreshActive = false; pendingRepository = repositoryId; }
      if (changed.length === 0) refreshActive = true;
      if (!refreshActive) for (const scope of changed) {
        scopes.add(scope);
        if (scopes.size > 66) { scopes.clear(); refreshActive = true; break; }
      }
      pending = true;
      running ??= pump();
      return running;
    },
    dispose(): void { disposed = true; pending = false; scopes.clear(); },
  };
}
