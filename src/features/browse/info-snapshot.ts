/** Keep one coherent right-panel snapshot until both async reads have settled. */
import type { AssetItem, FileExif } from "../../api/types.ts";
import type { IssueLibrary } from "../../api/issues.ts";

export interface BrowseInfoSnapshot {
  context: string;
  item: AssetItem;
  file: FileExif | null;
  library: IssueLibrary | null;
  error: string | null;
}

export function retainBrowseInfo(
  previous: BrowseInfoSnapshot | null,
  input: {
    context: string;
    item: AssetItem | null;
    path: string | null;
    metadata: { path: string; file: FileExif | null } | null;
    issuesReady: boolean;
    library: IssueLibrary | null;
    error: string | null;
  },
): BrowseInfoSnapshot | null {
  if (input.item === null || input.path === null) return null;
  if (input.metadata?.path !== input.path || !input.issuesReady) {
    return previous?.context === input.context ? previous : null;
  }
  return {
    context: input.context, item: input.item, file: input.metadata.file,
    library: input.library, error: input.error,
  };
}
