/** 最后打开的库与库内目录（设备级启动恢复）。 */

export interface BrowseSession {
  repositoryId: string | null;
  scopePath: string | null;
}

export const BROWSE_SESSION_STORAGE_KEY = "raybend.browse-session.v1";
export const DEFAULT_BROWSE_SESSION: BrowseSession = {
  repositoryId: null,
  scopePath: null,
};

export interface BrowseSessionStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

function storage(): BrowseSessionStorage | undefined {
  try {
    return globalThis.localStorage;
  } catch {
    return undefined;
  }
}

function validScopePath(value: unknown): value is string {
  if (typeof value !== "string" || value === "" || value.includes("\\") || value.includes("\0")) {
    return false;
  }
  if (value !== "photos" && !value.startsWith("photos/")) return false;
  return value.split("/").every((segment) => segment !== "" && segment !== "." && segment !== "..");
}

export function sanitizeBrowseSession(raw: unknown): BrowseSession {
  if (typeof raw !== "object" || raw === null) return { ...DEFAULT_BROWSE_SESSION };
  const record = raw as Record<string, unknown>;
  const repositoryId =
    typeof record.repositoryId === "string" && record.repositoryId.trim() !== ""
      ? record.repositoryId
      : null;
  const scopePath =
    repositoryId !== null && validScopePath(record.scopePath) ? record.scopePath : null;
  return { repositoryId, scopePath };
}

export function readBrowseSession(
  source: BrowseSessionStorage | undefined = storage(),
): BrowseSession {
  if (!source) return { ...DEFAULT_BROWSE_SESSION };
  try {
    const raw = source.getItem(BROWSE_SESSION_STORAGE_KEY);
    return raw === null || raw === ""
      ? { ...DEFAULT_BROWSE_SESSION }
      : sanitizeBrowseSession(JSON.parse(raw));
  } catch {
    return { ...DEFAULT_BROWSE_SESSION };
  }
}

export function writeBrowseSession(
  next: BrowseSession,
  target: BrowseSessionStorage | undefined = storage(),
): void {
  if (!target) return;
  try {
    target.setItem(BROWSE_SESSION_STORAGE_KEY, JSON.stringify(sanitizeBrowseSession(next)));
  } catch {
    // 启动恢复是便利功能，存储不可用时静默退化。
  }
}
