/** 平台 adapter：桌面优先原生快照，WebView/browser 补齐；失败不阻断首屏。 */
import { isTauriRuntime } from "./tauri-env.ts";
import { withTimeout } from "../lib/timeout.ts";
import { sanitizeSystemPreferences, type SystemPreferencesSnapshot } from "../lib/system-preferences.ts";

export interface SystemPreferencesAdapter {
  read: () => Promise<SystemPreferencesSnapshot>;
}

export interface BrowserSystemProbes {
  matchMedia: (query: string) => { matches: boolean } | undefined;
  language: () => string | undefined;
}

/** 媒体查询和语言分别捕获异常，某一项不可用不能丢掉另一项。 */
export function browserSystemPreferencesAdapter(probes: BrowserSystemProbes = {
  matchMedia: (query) => globalThis.matchMedia?.(query),
  language: () => globalThis.navigator?.language,
}): SystemPreferencesAdapter {
  return { read: async () => {
    let theme: SystemPreferencesSnapshot["theme"] = null;
    let language: string | null = null;
    try {
      if (probes.matchMedia("(prefers-color-scheme: dark)")?.matches) theme = "dark";
      else if (probes.matchMedia("(prefers-color-scheme: light)")?.matches) theme = "light";
    } catch { /* 平台不提供媒体查询时保留未知，由默认值处理。 */ }
    try { language = probes.language() ?? null; } catch { /* 同上，独立降级。 */ }
    return sanitizeSystemPreferences({ theme, language });
  } };
}

function desktopSystemPreferencesAdapter(): SystemPreferencesAdapter {
  return { read: async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    return sanitizeSystemPreferences(await invoke("system_preferences"));
  } };
}

export async function readSystemPreferences(options: {
  adapter?: SystemPreferencesAdapter;
  fallbackAdapter?: SystemPreferencesAdapter;
  timeoutMs?: number;
} = {}): Promise<SystemPreferencesSnapshot> {
  const fallback = options.fallbackAdapter ?? browserSystemPreferencesAdapter();
  const adapter = options.adapter ?? (isTauriRuntime() ? desktopSystemPreferencesAdapter() : fallback);
  const probe = async (source: SystemPreferencesAdapter): Promise<SystemPreferencesSnapshot> => {
    try {
      return sanitizeSystemPreferences(await withTimeout(
        source.read(), options.timeoutMs ?? 800, "SYSTEM_PREFERENCES_TIMEOUT",
      ));
    } catch {
      return { theme: null, language: null };
    }
  };
  // 两条探测互相独立，并行保证总等待时间仍在一个超时窗口内。
  const [native, portable] = await Promise.all([
    probe(adapter), adapter === fallback ? Promise.resolve(null) : probe(fallback),
  ]);
  return {
    theme: native.theme ?? portable?.theme ?? null,
    language: native.language ?? portable?.language ?? null,
  };
}
