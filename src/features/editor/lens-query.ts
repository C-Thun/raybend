/** One request state for lens selection. An idle/failed request is never represented as loading. */
import { createSignal } from 'solid-js';
import type { LensMatch } from '../../api/types.ts';

export interface LensQueryState {
  status: 'idle' | 'loading' | 'ready' | 'error';
  data: LensMatch | null;
  error: string | null;
  failure: 'request' | 'timeout' | 'unavailable' | null;
}
export interface LensQuery {
  state(): LensQueryState;
  select(repositoryId: string | null, assetId: number | null): void;
  refresh(): Promise<LensMatch | null>;
  dispose(): void;
}
const idle = (): LensQueryState => ({ status: 'idle', data: null, error: null, failure: null });

export function createLensQuery(
  load: (repositoryId: string, assetId: number) => Promise<LensMatch | null>,
  timeoutMs = 15000,
): LensQuery {
  const [state, setState] = createSignal<LensQueryState>(idle());
  let target: {repositoryId: string; assetId: number} | null = null;
  let revision = 0;
  let pending: Promise<LensMatch | null> | null = null;
  let cancel: (() => void) | null = null;
  const select = (repositoryId: string | null, assetId: number | null): void => {
    revision++;
    cancel?.();
    cancel = null;
    pending = null;
    target = repositoryId !== null && assetId !== null ? {repositoryId, assetId} : null;
    setState(idle());
  };
  const refresh = (): Promise<LensMatch | null> => {
    if (pending !== null) return pending;
    if (target === null) return Promise.resolve(null);
    const current = ++revision;
    const {repositoryId, assetId} = target;
    setState(previous => ({...previous, status: 'loading', error: null, failure: null}));
    const task = async (): Promise<LensMatch | null> => {
      let timer: ReturnType<typeof setTimeout> | undefined;
      let result: LensMatch | null;
      const timeout = Symbol('timeout');
      try {
        result = await Promise.race([
          load(repositoryId, assetId),
          new Promise<never>((_, reject) => { cancel = () => reject(new Error("cancelled")); }),
          new Promise<never>((_, reject) => { timer = setTimeout(() => reject(timeout), timeoutMs); }),
        ]);
      } catch (error) {
        if (current === revision) setState(previous => ({...previous, status: 'error',
          error: error === timeout ? null : String(error), failure: error === timeout ? 'timeout' : 'request'}));
        return null;
      } finally {
        clearTimeout(timer);
      }
      if (current !== revision) return null;
      if (result === null || !result.ready) {
        setState(previous => ({...previous, status: 'error', error: null, failure: 'unavailable'}));
        return null;
      }
      // Publish outside the transport catch: a UI exception must not be called an IPC failure.
      setState({status: 'ready', data: result, error: null, failure: null});
      return result;
    };
    pending = task().finally(() => { if (current === revision) { pending = null; cancel = null; } });
    return pending;
  };
  return {state, select, refresh, dispose: () => select(null, null)};
}
