/**
 * import / browse 各自的胶片带尺寸偏好。
 *
 * 这里负责「两份响应式值 + 2 秒防抖」，但不认识 Tauri / app.db：数据库键与读写函数
 * 由组装层注入。这样 FilmStrip 仍然只是受控视图，浏览器冒烟也能复用同一套逻辑。
 */

import { createSignal } from "solid-js";

import {
  DEFAULT_FILM_STRIP_STEP,
  clampFilmStripStep,
} from "./film-strip-size.ts";

export type FilmStripScope = "import" | "browse";

export const FILM_STRIP_PERSIST_DEBOUNCE_MS = 2_000;

export interface FilmStripPreferenceKeys {
  import: string;
  browse: string;
}

export interface FilmStripPreferenceDeps {
  getSetting: (key: string) => Promise<string | null>;
  setSetting: (key: string, value: string) => Promise<void>;
  keys: FilmStripPreferenceKeys;
  debounceMs?: number;
  schedule?: (run: () => void, delayMs: number) => unknown;
  cancel?: (handle: unknown) => void;
  onError?: (error: unknown, scope: FilmStripScope, operation: "load" | "save") => void;
}

export interface FilmStripPreferenceStore {
  step: (scope: FilmStripScope) => number;
  setStep: (scope: FilmStripScope, step: number) => void;
  load: () => Promise<void>;
  dispose: () => void;
}

export function parseFilmStripStep(raw: unknown): number {
  if (typeof raw !== "string" || raw.trim() === "") return DEFAULT_FILM_STRIP_STEP;
  return clampFilmStripStep(Number(raw));
}

export function createFilmStripPreferenceStore(
  deps: FilmStripPreferenceDeps,
): FilmStripPreferenceStore {
  const [steps, setSteps] = createSignal<Record<FilmStripScope, number>>({
    import: DEFAULT_FILM_STRIP_STEP,
    browse: DEFAULT_FILM_STRIP_STEP,
  });
  const revisions: Record<FilmStripScope, number> = { import: 0, browse: 0 };
  const timers: Partial<Record<FilmStripScope, unknown>> = {};
  const delay = Math.max(0, deps.debounceMs ?? FILM_STRIP_PERSIST_DEBOUNCE_MS);
  const schedule = deps.schedule ?? ((run, delayMs) => globalThis.setTimeout(run, delayMs));
  const cancel = deps.cancel ?? ((handle) => globalThis.clearTimeout(handle as number));

  const report = (
    error: unknown,
    scope: FilmStripScope,
    operation: "load" | "save",
  ): void => {
    deps.onError?.(error, scope, operation);
  };

  const saveLater = (scope: FilmStripScope): void => {
    const previous = timers[scope];
    if (previous !== undefined) cancel(previous);
    timers[scope] = schedule(() => {
      delete timers[scope];
      void deps
        .setSetting(deps.keys[scope], String(steps()[scope]))
        .catch((error: unknown) => report(error, scope, "save"));
    }, delay);
  };

  return {
    step: (scope) => steps()[scope],
    setStep: (scope, rawStep) => {
      const next = clampFilmStripStep(rawStep);
      if (steps()[scope] === next) return;
      revisions[scope] += 1;
      setSteps((current) => ({ ...current, [scope]: next }));
      saveLater(scope);
    },
    load: async () => {
      await Promise.all(
        (["import", "browse"] as const).map(async (scope) => {
          const revision = revisions[scope];
          try {
            const raw = await deps.getSetting(deps.keys[scope]);
            // 用户已经滚过就不让迟到的数据库结果覆盖手上的新值。
            if (revisions[scope] !== revision) return;
            const next = parseFilmStripStep(raw);
            setSteps((current) => ({ ...current, [scope]: next }));
          } catch (error: unknown) {
            report(error, scope, "load");
          }
        }),
      );
    },
    dispose: () => {
      for (const scope of ["import", "browse"] as const) {
        const timer = timers[scope];
        if (timer !== undefined) cancel(timer);
        delete timers[scope];
      }
    },
  };
}
