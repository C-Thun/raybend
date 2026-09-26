/** A display key is distinct from a file path; it never changes the catalog/latest. */
import type { VariantSnapshot } from './export-model.ts';
export interface DisplayVariant {
  repositoryId: string;
  reference: VariantSnapshot['reference'];
  captured?: VariantSnapshot;
}
export function displayVariantKey(value: DisplayVariant): string {
  return JSON.stringify(['raybend.display.v1', value]);
}
export function readDisplayVariantKey(key: string): DisplayVariant | null {
  if (!key.startsWith('[')) return null;
  try {
    const [tag, value] = JSON.parse(key);
    return tag === 'raybend.display.v1' && typeof value?.repositoryId === 'string' &&
      Number.isSafeInteger(value?.reference?.assetId) && value.reference.assetId > 0 &&
      typeof value.reference.variant === 'string' ? value : null;
  } catch { return null; }
}
/** latest can equal a neutral source or a named issue; snapshot must address that real draft. */
export function displayedReference(assetId: number, choice: string, latest: string | {issue: number}) {
  return {assetId, variant: choice !== 'latest' ? choice : typeof latest === 'string' ? latest : `issue:${latest.issue}`};
}
