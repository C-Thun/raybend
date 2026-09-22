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

/**
 * 胶片带的三个作用域：**数据源不同、尺寸偏好各自独立**（人类 2026-09-19 定 import/browse 分开，
 * 2026-09-23 加 editor —— 编辑工作区也有胶片带，但它是自己的尺寸档位）。
 *
 * 组件仍是同一份（`components/ui/viewer/FilmStrip.tsx`），差异只在**这个作用域名**上。
 */
export type FilmStripScope = "import" | "browse" | "editor";

/** 所有作用域（初始化 / 加载 / 清理都按它遍历，别再手写数组字面量）。 */
export const FILM_STRIP_SCOPES: readonly FilmStripScope[] = ["import", "browse", "editor"];

export const FILM_STRIP_PERSIST_DEBOUNCE_MS = 2_000;

/**
 * 每个作用域各自的存储键。
 *
 * 用 `Record<FilmStripScope, string>` 而不是逐项列出：**加了作用域却忘了给键**会在编译期报错
 * （2026-09-23 加 editor 时正是这条把漏掉的一处当场揪出来）。
 */
export type FilmStripPreferenceKeys = Record<FilmStripScope, string>;

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
    editor: DEFAULT_FILM_STRIP_STEP,
  });
  const revisions: Record<FilmStripScope, number> = { import: 0, browse: 0, editor: 0 };
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
      // 顺序读三个键即可：每次都是一条本地 SQLite 读，并行省不下什么，循环更直白
      //（早先是 `Promise.all(scopes.map(async …))` —— 那个回调没有返回值，
      //  lint 的「数组回调必须有 return」规则盯着它）。
      for (const scope of FILM_STRIP_SCOPES) {
        const revision = revisions[scope];
        try {
          const raw = await deps.getSetting(deps.keys[scope]);
          // 用户已经滚过就不让迟到的数据库结果覆盖手上的新值。
          if (revisions[scope] !== revision) continue;
          const next = parseFilmStripStep(raw);
          setSteps((current) => ({ ...current, [scope]: next }));
        } catch (error: unknown) {
          report(error, scope, "load");
        }
      }
    },
    dispose: () => {
      for (const scope of FILM_STRIP_SCOPES) {
        const timer = timers[scope];
        if (timer !== undefined) cancel(timer);
        delete timers[scope];
      }
    },
  };
}
