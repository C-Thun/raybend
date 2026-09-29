/** 系统事实与首次默认值的纯转换；平台探测集中在 api 的 adapter。 */
import type { ThemeMode } from "./appearance.ts";

export interface SystemPreferencesSnapshot {
  theme: ThemeMode | null;
  /** 系统首选显示语言；后续备用语言不影响默认界面。 */
  language: string | null;
}

export interface SystemDefaults {
  theme: ThemeMode;
  locale: "zh-CN" | "en-US";
}

export function sanitizeSystemPreferences(value: unknown): SystemPreferencesSnapshot {
  if (!value || typeof value !== "object") return { theme: null, language: null };
  const raw = value as Record<string, unknown>;
  return {
    theme: raw.theme === "dark" || raw.theme === "light" ? raw.theme : null,
    language: typeof raw.language === "string" && raw.language.trim() !== ""
      ? raw.language.trim() : null,
  };
}

export function systemDefaults(value: unknown): SystemDefaults {
  const snapshot = sanitizeSystemPreferences(value);
  return {
    theme: snapshot.theme ?? "dark",
    locale: /^zh(?:$|[-_])/i.test(snapshot.language ?? "") ? "zh-CN" : "en-US",
  };
}
