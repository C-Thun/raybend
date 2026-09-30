import type { AssetIdentity } from "../../api/organization.ts";

export const PHOTO_DRAG_TYPE = "application/x-raybend-photos";

export function parsePhotoDrag(value: string): AssetIdentity[] {
  try {
    const parsed: unknown = JSON.parse(value);
    if (!Array.isArray(parsed) || parsed.length > 5000) return [];
    const photos: AssetIdentity[] = [];
    for (const item of parsed) {
      if (typeof item !== "object" || item === null) return [];
      const candidate = item as Record<string, unknown>;
      if (typeof candidate.repositoryId !== "string" || !candidate.repositoryId ||
          !Number.isSafeInteger(candidate.assetId) || (candidate.assetId as number) <= 0) return [];
      photos.push({ repositoryId: candidate.repositoryId, assetId: candidate.assetId as number });
    }
    return photos;
  } catch { return []; }
}
