/**
 * `repositories` 模块的唯一对外出口（`ARCHITECTURE.md` §2）。
 */

export { RepositoryList } from "./RepositoryList.tsx";
export type { RepositoryListProps } from "./RepositoryList.tsx";
export { RepositoryFooter } from "./RepositoryFooter.tsx";
export type { RepositoryFooterProps } from "./RepositoryFooter.tsx";
export { CreateRepositoryDialog } from "./CreateRepositoryDialog.tsx";
export type { CreateRepositoryDialogProps } from "./CreateRepositoryDialog.tsx";
export { evaluateCreate, probeMatches } from "./create-logic.ts";
export type { CreateGate, CreateHint } from "./create-logic.ts";
